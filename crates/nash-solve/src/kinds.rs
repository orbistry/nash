//! Shared value-kind inference over the solver's type equivalence classes.

use std::collections::{BTreeMap, BTreeSet};

use bumpalo::Bump;
use nash_ast::{BaseKind, Kind, KindSet, QualifiedName, ValueKinds};
use nash_can::kinds::{Infer, K, KindEnv, Mismatch};
use nash_constrain::{Content, FlatType, UnionFind, Variable};

pub use nash_constrain::error::KindProblem as Error;

/// Kind variables are shared by type identity, including before type variables
/// are unified. Reconciliation joins their kind roots after a type UF merge.
#[derive(Clone)]
pub struct State<'a> {
    bump: &'a Bump,
    infer: Infer<'a>,
    roots: BTreeMap<Variable, &'a K<'a>>,
}

impl<'a> State<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Self {
            bump,
            infer: Infer::new(bump),
            roots: BTreeMap::new(),
        }
    }

    fn reconcile(&mut self, uf: &mut UnionFind<'a>) -> Result<(), Error<'a>> {
        let mut roots = BTreeMap::new();
        for (variable, kind) in self.roots.clone() {
            let representative = uf.find(variable);
            if let Some(previous) = roots.insert(representative, kind) {
                self.unify(previous, kind)?;
            }
        }
        self.roots = roots;
        Ok(())
    }

    fn synchronize(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
    ) -> Result<BTreeSet<Variable>, Error<'a>> {
        self.reconcile(uf)?;
        let variables: Vec<_> = self.roots.keys().copied().collect();
        let mut seen = BTreeSet::new();
        for variable in variables {
            self.infer_type(uf, env, variable, &mut seen)?;
        }
        Ok(seen)
    }

    fn error(&mut self, mismatch: Mismatch<'a>) -> Error<'a> {
        match mismatch {
            Mismatch::Shapes { expected, actual } => Error::Mismatch {
                expected: self.infer.generalize(expected),
                actual: self.infer.generalize(actual),
            },
            Mismatch::Infinite(_) => Error::Infinite,
        }
    }

    fn unify(&mut self, expected: &'a K<'a>, actual: &'a K<'a>) -> Result<(), Error<'a>> {
        self.infer
            .unify(expected, actual)
            .map_err(|error| self.error(error))
    }

    /// Apply one complete signature at once; its roots can share kind variables.
    pub fn require(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        signature: ValueKinds<'a>,
        variables: &[Variable],
    ) -> Result<(), Error<'a>> {
        let mut query = self.clone();
        query.require_mut(uf, env, signature, variables)?;
        *self = query;
        Ok(())
    }

    fn require_mut(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        signature: ValueKinds<'a>,
        variables: &[Variable],
    ) -> Result<(), Error<'a>> {
        assert_eq!(signature.kinds.len(), variables.len());
        let mut seen = self.synchronize(uf, env)?;
        let expected = self.infer.instantiate_values(&signature);
        for (expected, variable) in expected.into_iter().zip(variables) {
            let actual = self.infer_type(uf, env, *variable, &mut seen)?;
            self.unify(expected, actual)?;
        }
        self.check_record_kinds(uf)
    }

    // Structural records do not yet identify a nominal carrier. Their kinds
    // can participate in representation-independent uses, but cannot be narrowed
    // or equated with a distinct carrier's kind until plan 04 supplies that
    // identity. Check the whole binder to detect both restrictions and equality.
    fn check_record_kinds(&mut self, uf: &mut UnionFind<'a>) -> Result<(), Error<'a>> {
        let roots: Vec<_> = self
            .roots
            .iter()
            .filter_map(|(variable, kind)| {
                matches!(
                    uf.get(*variable).content,
                    Content::Structure(FlatType::Record1(..) | FlatType::EmptyRecord1)
                )
                .then_some(*kind)
            })
            .collect();
        let signature = normalize(self.bump, self.infer.generalize_values(&roots));
        if signature.bounds.len() != roots.len()
            || signature.bounds.iter().any(|bound| *bound != KindSet::ANY)
            || signature.kinds.iter().enumerate().any(|(index, kind)| !matches!(kind, Kind::Var(variable) if usize::from(*variable) == index))
        {
            return Err(Error::AnonymousRecord);
        }
        Ok(())
    }

    /// Prove a use without restricting the enclosing annotation's promises.
    /// Compare the whole rigid binder so new equalities between roots count as
    /// restrictions too. Failed queries leave the inference state unchanged.
    pub fn require_preserving(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        signature: ValueKinds<'a>,
        variables: &[Variable],
        rigid_variables: &[Variable],
    ) -> Result<(), Error<'a>> {
        let mut query = self.clone();
        let declared = query.generalize_mut(uf, env, rigid_variables)?;
        query.require_mut(uf, env, signature, variables)?;
        let required = query.generalize_mut(uf, env, rigid_variables)?;
        if declared != required {
            return Err(Error::Rigid { declared, required });
        }
        *self = query;
        Ok(())
    }

    /// Serialize roots under one binder, in the caller's quantifier order.
    pub fn generalize(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        variables: &[Variable],
    ) -> Result<ValueKinds<'a>, Error<'a>> {
        let mut query = self.clone();
        let signature = query.generalize_mut(uf, env, variables)?;
        *self = query;
        Ok(signature)
    }

    fn generalize_mut(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        variables: &[Variable],
    ) -> Result<ValueKinds<'a>, Error<'a>> {
        let mut seen = self.synchronize(uf, env)?;
        let roots: Result<Vec<_>, _> = variables
            .iter()
            .map(|var| self.infer_type(uf, env, *var, &mut seen))
            .collect();
        let roots = roots?;
        self.check_record_kinds(uf)?;
        let signature = self.infer.generalize_values(&roots);
        Ok(normalize(self.bump, signature))
    }

    fn infer_type(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        variable: Variable,
        seen: &mut BTreeSet<Variable>,
    ) -> Result<&'a K<'a>, Error<'a>> {
        let variable = uf.find(variable);
        let root = *self
            .roots
            .entry(variable)
            .or_insert_with(|| self.infer.fresh_k(KindSet::ALL));
        if !seen.insert(variable) {
            return Ok(root);
        }
        let actual = match uf.get(variable).content.clone() {
            Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => return Ok(root),
            Content::PartialAlias {
                home, name, args, ..
            }
            | Content::Alias {
                home, name, args, ..
            } => {
                let head = self
                    .infer
                    .instantiate(&env.scheme(QualifiedName { home, name }));
                self.apply(uf, env, head, args.into_iter().map(|(_, var)| var), seen)?
            }
            Content::Structure(FlatType::App1(home, name, args)) => {
                let head = self
                    .infer
                    .instantiate(&env.scheme(QualifiedName { home, name }));
                self.apply(uf, env, head, args, seen)?
            }
            Content::Structure(FlatType::AppV1(head, args)) => {
                let head = self.infer_type(uf, env, head, seen)?;
                self.apply(uf, env, head, args, seen)?
            }
            Content::Structure(FlatType::Fun1(from, to)) => {
                self.value(uf, env, from, seen)?;
                self.value(uf, env, to, seen)?;
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            Content::Structure(FlatType::Tuple1(first, second, third)) => {
                self.value(uf, env, first, seen)?;
                self.value(uf, env, second, seen)?;
                for third in third {
                    self.value(uf, env, third, seen)?;
                }
                self.bump.alloc(K::Base(BaseKind::Term))
            }
            Content::Structure(FlatType::Unit1) => self.bump.alloc(K::Base(BaseKind::Const)),
            Content::Structure(FlatType::Record1(fields, extension)) => {
                for field in fields.values() {
                    self.value(uf, env, *field, seen)?;
                }
                if matches!(
                    uf.get(extension).content,
                    Content::Structure(FlatType::Record1(..))
                ) {
                    self.value(uf, env, extension, seen)?;
                }
                self.infer.fresh_k(KindSet::ANY)
            }
            Content::Structure(FlatType::EmptyRecord1) => self.infer.fresh_k(KindSet::ANY),
        };
        self.unify(root, actual)?;
        Ok(root)
    }

    fn value(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        variable: Variable,
        seen: &mut BTreeSet<Variable>,
    ) -> Result<(), Error<'a>> {
        // Plan 04 owns the representation of structural records. Their fields
        // are values, but a row tail is not a value-kind variable. Do not invent
        // a representation kind for the record carrier during value inference.
        let representative = uf.find(variable);
        match uf.get(variable).content.clone() {
            Content::Structure(FlatType::Record1(fields, extension)) => {
                if !seen.insert(representative) {
                    return Ok(());
                }
                for field in fields.values() {
                    self.value(uf, env, *field, seen)?;
                }
                if matches!(
                    uf.get(extension).content,
                    Content::Structure(FlatType::Record1(..))
                ) {
                    self.value(uf, env, extension, seen)?;
                }
                return Ok(());
            }
            Content::Structure(FlatType::EmptyRecord1) => return Ok(()),
            _ => {}
        }
        let kind = self.infer_type(uf, env, variable, seen)?;
        let any = self.infer.fresh_k(KindSet::ANY);
        self.unify(any, kind)?;
        Ok(())
    }

    pub fn observe_value(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        variable: Variable,
    ) -> Result<(), Error<'a>> {
        let mut query = self.clone();
        let mut seen = query.synchronize(uf, env)?;
        query.value(uf, env, variable, &mut seen)?;
        query.check_record_kinds(uf)?;
        *self = query;
        Ok(())
    }

    fn apply(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &KindEnv<'a>,
        mut head: &'a K<'a>,
        args: impl IntoIterator<Item = Variable>,
        seen: &mut BTreeSet<Variable>,
    ) -> Result<&'a K<'a>, Error<'a>> {
        for arg in args {
            let (parameter, result) = self.infer.apply(head).map_err(|error| self.error(error))?;
            let actual = self.infer_type(uf, env, arg, seen)?;
            self.unify(parameter, actual)?;
            head = result;
        }
        Ok(head)
    }
}

// Singleton base bounds and concrete base kinds express the same promise.
fn normalize<'a>(bump: &'a Bump, signature: ValueKinds<'a>) -> ValueKinds<'a> {
    fn root<'a>(
        bump: &'a Bump,
        kind: &'a Kind<'a>,
        source: &[KindSet],
        variables: &mut BTreeMap<u16, u16>,
        bounds: &mut Vec<KindSet>,
    ) -> &'a Kind<'a> {
        match kind {
            Kind::Base(_) => kind,
            Kind::Arrow(from, to) => {
                let from = root(bump, from, source, variables, bounds);
                let to = root(bump, to, source, variables, bounds);
                bump.alloc(Kind::Arrow(from, to))
            }
            Kind::Var(index) => {
                let bound = source[*index as usize];
                let base = match bound {
                    KindSet::BIG => Some(BaseKind::Big),
                    KindSet::CONST => Some(BaseKind::Const),
                    KindSet::TERM => Some(BaseKind::Term),
                    _ => None,
                };
                if let Some(base) = base {
                    return bump.alloc(Kind::Base(base));
                }
                let index = *variables.entry(*index).or_insert_with(|| {
                    let index = bounds.len().try_into().expect("kind binder exceeds u16");
                    bounds.push(bound);
                    index
                });
                bump.alloc(Kind::Var(index))
            }
        }
    }
    let mut variables = BTreeMap::new();
    let mut bounds = Vec::new();
    let kinds = bump.alloc_slice_fill_iter(
        signature
            .kinds
            .iter()
            .map(|kind| root(bump, kind, signature.bounds, &mut variables, &mut bounds)),
    );
    ValueKinds {
        kinds,
        bounds: bump.alloc_slice_fill_iter(bounds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nash_ast::{Kind, primitives};
    use nash_constrain::type_::make_descriptor;

    fn bounded<'a>(bump: &'a Bump, bound: KindSet) -> ValueKinds<'a> {
        ValueKinds {
            bounds: bump.alloc_slice_copy(&[bound]),
            kinds: bump.alloc_slice_copy(&[&*bump.alloc(Kind::Var(0))]),
        }
    }

    #[test]
    fn anonymous_carriers_allow_only_representation_independent_requirements() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let mut uf = UnionFind::new();
        let first = uf.fresh(make_descriptor(Content::Structure(FlatType::EmptyRecord1)));
        let second = uf.fresh(make_descriptor(Content::Structure(FlatType::EmptyRecord1)));
        let mut state = State::new(&bump);
        for bound in [KindSet::ALL, KindSet::ANY] {
            state
                .require(&mut uf, &env, bounded(&bump, bound), &[first])
                .unwrap();
        }
        let before = state.generalize(&mut uf, &env, &[first]).unwrap();
        for bound in [
            KindSet::STORABLE,
            KindSet::LITTLE,
            KindSet::BIG,
            KindSet::CONST,
            KindSet::TERM,
        ] {
            assert!(matches!(
                state.require(&mut uf, &env, bounded(&bump, bound), &[first]),
                Err(Error::AnonymousRecord)
            ));
            assert_eq!(state.generalize(&mut uf, &env, &[first]).unwrap(), before);
        }
        let shared = ValueKinds {
            bounds: &[KindSet::ANY],
            kinds: &[&Kind::Var(0), &Kind::Var(0)],
        };
        state
            .require(&mut uf, &env, shared, &[first, first])
            .unwrap();
        assert!(matches!(
            state.require(&mut uf, &env, shared, &[first, second]),
            Err(Error::AnonymousRecord)
        ));
        state
            .require(&mut uf, &env, bounded(&bump, KindSet::ANY), &[second])
            .unwrap();
        uf.union(
            first,
            second,
            make_descriptor(Content::Structure(FlatType::EmptyRecord1)),
        );
        state
            .require(&mut uf, &env, shared, &[first, second])
            .unwrap();
        // A resolved nominal type no longer has an unknown record carrier.
        uf.modify(first, |desc| {
            desc.content =
                Content::Structure(FlatType::App1(primitives::builtin_home(), "int", vec![]))
        });
        state
            .require(&mut uf, &env, bounded(&bump, KindSet::CONST), &[first])
            .unwrap();
    }

    #[test]
    fn type_merges_reconcile_preexisting_kind_restrictions() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let mut uf = UnionFind::new();
        let first = uf.fresh(make_descriptor(Content::FlexVar(None)));
        let second = uf.fresh(make_descriptor(Content::FlexVar(None)));
        let mut state = State::new(&bump);
        state
            .require(&mut uf, &env, bounded(&bump, KindSet::STORABLE), &[first])
            .unwrap();
        let term = ValueKinds {
            bounds: &[],
            kinds: &[&Kind::Base(BaseKind::Term)],
        };
        state.require(&mut uf, &env, term, &[second]).unwrap();
        uf.union(first, second, make_descriptor(Content::FlexVar(None)));
        assert!(matches!(
            state.generalize(&mut uf, &env, &[first]),
            Err(Error::Mismatch { .. })
        ));
    }

    #[test]
    fn applied_heads_share_argument_kinds_but_separate_uses_are_fresh() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let mut uf = UnionFind::new();
        let signature = ValueKinds {
            bounds: &[KindSet::ALL],
            kinds: &[
                &Kind::Arrow(&Kind::Var(0), &Kind::Base(BaseKind::Const)),
                &Kind::Var(0),
            ],
        };
        let mut state = State::new(&bump);
        let mut uses = Vec::new();
        for _ in 0..2 {
            let f = uf.fresh(make_descriptor(Content::FlexVar(None)));
            let a = uf.fresh(make_descriptor(Content::FlexVar(None)));
            state.require(&mut uf, &env, signature, &[f, a]).unwrap();
            uses.push((f, a));
        }
        uf.modify(uses[0].0, |desc| {
            desc.content =
                Content::Structure(FlatType::App1(primitives::builtin_home(), "list", vec![]))
        });
        assert_eq!(
            state
                .generalize(&mut uf, &env, &[uses[0].1])
                .unwrap()
                .bounds,
            &[KindSet::STORABLE]
        );
        assert_eq!(
            state
                .generalize(&mut uf, &env, &[uses[1].1])
                .unwrap()
                .bounds,
            &[KindSet::ALL]
        );
        let term = uf.fresh(make_descriptor(Content::Structure(FlatType::Fun1(
            uses[1].1, uses[1].1,
        ))));
        let invalid = uf.fresh(make_descriptor(Content::Structure(FlatType::AppV1(
            uses[0].0,
            vec![term],
        ))));
        assert!(matches!(
            state.generalize(&mut uf, &env, &[invalid]),
            Err(Error::Mismatch { .. })
        ));
    }

    #[test]
    fn rigid_requirements_are_proved_without_narrowing_the_binder() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let mut uf = UnionFind::new();
        let a = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let b = uf.fresh(make_descriptor(Content::RigidVar("b")));
        let mut state = State::new(&bump);
        let any = bounded(&bump, KindSet::ANY);
        state.require(&mut uf, &env, any, &[a]).unwrap();
        state.require(&mut uf, &env, any, &[b]).unwrap();
        let before = state.generalize(&mut uf, &env, &[a, b]).unwrap();
        assert!(matches!(
            state.require_preserving(
                &mut uf,
                &env,
                bounded(&bump, KindSet::STORABLE),
                &[a],
                &[a, b]
            ),
            Err(Error::Rigid { .. })
        ));
        let same = ValueKinds {
            bounds: &[KindSet::ANY],
            kinds: &[&Kind::Var(0), &Kind::Var(0)],
        };
        assert!(matches!(
            state.require_preserving(&mut uf, &env, same, &[a, b], &[a, b]),
            Err(Error::Rigid { .. })
        ));
        assert_eq!(before, state.generalize(&mut uf, &env, &[a, b]).unwrap());
        state
            .require_preserving(&mut uf, &env, any, &[a], &[a, b])
            .unwrap();
        let big = uf.fresh(make_descriptor(Content::RigidVar("big")));
        state
            .require(&mut uf, &env, bounded(&bump, KindSet::BIG), &[big])
            .unwrap();
        let concrete_big = ValueKinds {
            bounds: &[],
            kinds: &[&Kind::Base(BaseKind::Big)],
        };
        state
            .require_preserving(&mut uf, &env, concrete_big, &[big], &[a, b, big])
            .unwrap();
        let concrete_const = ValueKinds {
            bounds: &[],
            kinds: &[&Kind::Base(BaseKind::Const)],
        };
        let Err(Error::Mismatch { expected, actual }) =
            state.require_preserving(&mut uf, &env, concrete_const, &[big], &[a, b, big])
        else {
            panic!("incompatible base kinds")
        };
        // These are closed schemes, usable after the failed query is dropped.
        assert_eq!(expected.kind, &Kind::Base(BaseKind::Const));
        assert_eq!(actual.kind, &Kind::Base(BaseKind::Big));
    }
}
