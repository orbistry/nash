//! Haskell 98 kind inference. Representation requirements are predicates.

use bumpalo::Bump;
use nash_ast::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindVar(u32);

/// Inference variables are local to one checker and never appear in interfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum K<'a> {
    Type,
    Var(KindVar),
    Arrow(&'a K<'a>, &'a K<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch<'a> {
    Shapes {
        expected: &'a K<'a>,
        actual: &'a K<'a>,
    },
    Infinite,
}

pub struct Infer<'a> {
    bump: &'a Bump,
    bindings: Vec<Option<&'a K<'a>>>,
}

impl<'a> Infer<'a> {
    pub fn new(bump: &'a Bump) -> Self {
        Self {
            bump,
            bindings: Vec::new(),
        }
    }

    pub fn fresh(&mut self) -> &'a K<'a> {
        let index = u32::try_from(self.bindings.len()).expect("kind variable count exceeds u32");
        self.bindings.push(None);
        self.bump.alloc(K::Var(KindVar(index)))
    }

    pub fn arrow(&self, from: &'a K<'a>, to: &'a K<'a>) -> &'a K<'a> {
        self.bump.alloc(K::Arrow(from, to))
    }

    pub fn from_kind(&self, kind: &Kind<'_>) -> &'a K<'a> {
        match kind {
            Kind::Type => &K::Type,
            Kind::Arrow(from, to) => self.arrow(self.from_kind(from), self.from_kind(to)),
        }
    }

    fn head(&self, mut kind: &'a K<'a>) -> &'a K<'a> {
        while let K::Var(var) = kind {
            match self.bindings[var.0 as usize] {
                Some(bound) => kind = bound,
                None => break,
            }
        }
        kind
    }

    fn occurs(&self, variable: KindVar, kind: &'a K<'a>) -> bool {
        let mut pending = vec![kind];
        while let Some(kind) = pending.pop() {
            match self.head(kind) {
                K::Var(other) if *other == variable => return true,
                K::Arrow(from, to) => pending.extend([*from, *to]),
                _ => {}
            }
        }
        false
    }

    pub fn unify(&mut self, expected: &'a K<'a>, actual: &'a K<'a>) -> Result<(), Mismatch<'a>> {
        let mut pending = vec![(expected, actual)];
        while let Some((expected, actual)) = pending.pop() {
            let expected = self.head(expected);
            let actual = self.head(actual);
            match (expected, actual) {
                (K::Type, K::Type) => {}
                (K::Var(a), K::Var(b)) if a == b => {}
                (K::Var(variable), kind) | (kind, K::Var(variable)) => {
                    if self.occurs(*variable, kind) {
                        return Err(Mismatch::Infinite);
                    }
                    self.bindings[variable.0 as usize] = Some(kind);
                }
                (K::Arrow(af, at), K::Arrow(bf, bt)) => {
                    pending.push((*at, *bt));
                    pending.push((*af, *bf));
                }
                _ => return Err(Mismatch::Shapes { expected, actual }),
            }
        }
        Ok(())
    }

    pub fn apply(
        &mut self,
        head: &'a K<'a>,
        args: &[&'a K<'a>],
    ) -> Result<&'a K<'a>, Mismatch<'a>> {
        let mut result = head;
        for arg in args {
            let next = self.fresh();
            self.unify(result, self.arrow(arg, next))?;
            result = next;
        }
        Ok(result)
    }

    /// Default only after every use in the inference group has been checked.
    pub fn default_and_zonk(&mut self, kind: &'a K<'a>) -> &'a Kind<'a> {
        match *self.head(kind) {
            K::Type => &Kind::Type,
            K::Var(variable) => {
                self.bindings[variable.0 as usize] = Some(&K::Type);
                &Kind::Type
            }
            K::Arrow(from, to) => {
                let from = self.default_and_zonk(from);
                let to = self.default_and_zonk(to);
                self.bump.alloc(Kind::Arrow(from, to))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconstrained_phantom_defaults_to_type() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let parameter = infer.fresh();
        let constructor = infer.arrow(parameter, &K::Type);
        assert_eq!(
            infer.default_and_zonk(constructor),
            &Kind::Arrow(&Kind::Type, &Kind::Type)
        );
    }

    #[test]
    fn repeated_head_uses_share_argument_kind() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let f = infer.fresh();
        let a = infer.fresh();
        let first = infer.apply(f, &[a]).unwrap();
        infer.unify(first, &K::Type).unwrap();
        let second = infer.apply(f, &[&K::Type]).unwrap();
        infer.unify(second, &K::Type).unwrap();
        assert_eq!(infer.default_and_zonk(a), &Kind::Type);
        assert_eq!(
            infer.default_and_zonk(f),
            &Kind::Arrow(&Kind::Type, &Kind::Type)
        );
    }

    #[test]
    fn self_application_fails_occurs_check() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let f = infer.fresh();
        assert!(matches!(infer.apply(f, &[f]), Err(Mismatch::Infinite)));
    }

    #[test]
    fn indirect_cycle_fails_occurs_check() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let a = infer.fresh();
        let b = infer.fresh();
        infer.unify(a, b).unwrap();
        assert!(matches!(
            infer.unify(b, infer.arrow(&K::Type, a)),
            Err(Mismatch::Infinite)
        ));
    }

    #[test]
    fn equal_arity_does_not_hide_nested_kind_mismatch() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let unary = infer.arrow(&K::Type, &K::Type);
        let higher = infer.arrow(unary, &K::Type);
        assert!(matches!(
            infer.unify(unary, higher),
            Err(Mismatch::Shapes { .. })
        ));
    }

    #[test]
    fn partial_application_preserves_the_remaining_arrow() {
        let bump = Bump::new();
        let mut infer = Infer::new(&bump);
        let unary = infer.arrow(&K::Type, &K::Type);
        let binary = infer.arrow(&K::Type, unary);
        let result = infer.apply(binary, &[&K::Type]).unwrap();
        assert_eq!(
            infer.default_and_zonk(result),
            &Kind::Arrow(&Kind::Type, &Kind::Type)
        );
        assert!(matches!(
            infer.apply(&K::Type, &[&K::Type]),
            Err(Mismatch::Shapes { .. })
        ));
    }
}

use nash_ast::primitives::{self, Repr, ReprTrait};
use nash_ast::{Pred, QualifiedName, Type};
use nash_region::Located;
use std::collections::BTreeMap;

/// Constructor metadata is closed before the next declaration SCC is checked.
#[derive(Clone, Copy, Debug)]
pub enum TypeInfo<'a> {
    Builtin(&'static primitives::Primitive),
    Defined {
        kind: &'a Kind<'a>,
        parameters: &'a [&'a str],
        context: &'a [Pred<'a>],
        /// Transparent aliases use their substituted body instead of a fixed repr.
        repr: Option<Repr>,
        alias: Option<&'a Located<Type<'a>>>,
    },
}

impl<'a> TypeInfo<'a> {
    pub fn kind(self) -> &'a Kind<'a> {
        match self {
            Self::Builtin(p) => p.kind,
            Self::Defined { kind, .. } => kind,
        }
    }
    pub fn parameters(self) -> &'a [&'a str] {
        match self {
            Self::Builtin(p) => &["p0", "p1"][..p.kind.arity()],
            Self::Defined { parameters, .. } => parameters,
        }
    }
    pub fn context(self, bump: &'a Bump) -> &'a [Pred<'a>] {
        match self {
            Self::Defined { context, .. } => context,
            Self::Builtin(p) => {
                bump.alloc_slice_fill_iter(p.context.iter().map(|(index, trait_)| {
                    let arg = &*bump.alloc(Located::at_zero(Type::Var(self.parameters()[*index])));
                    Pred::Implied {
                        trait_: trait_.qualified(),
                        args: bump.alloc_slice_copy(&[arg]),
                    }
                }))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TraitContext<'a> {
    pub parameters: &'a [&'a str],
    pub supers: &'a [Pred<'a>],
}

#[derive(Clone, Debug)]
pub struct KindEnv<'a> {
    pub types: BTreeMap<QualifiedName<'a>, TypeInfo<'a>>,
    pub traits: BTreeMap<QualifiedName<'a>, &'a [&'a Kind<'a>]>,
    pub superclasses: BTreeMap<QualifiedName<'a>, TraitContext<'a>>,
}

impl Default for KindEnv<'_> {
    fn default() -> Self {
        Self::from_interfaces(None)
    }
}

impl<'a> KindEnv<'a> {
    pub fn from_interfaces(interfaces: Option<&BTreeMap<&'a str, crate::Interface<'a>>>) -> Self {
        let mut env = Self {
            types: primitives::PRIMITIVES
                .iter()
                .map(|p| {
                    (
                        QualifiedName {
                            home: primitives::builtin_home(),
                            name: p.name,
                        },
                        TypeInfo::Builtin(p),
                    )
                })
                .collect(),
            superclasses: BTreeMap::new(),
            traits: ReprTrait::ALL
                .into_iter()
                .map(|t| (t.qualified(), &[&Kind::Type][..]))
                .collect(),
        };
        // Literal expressions and literal patterns emit these compiler-owned
        // predicates even before an imported implementation is available.
        for name in ["FromInt", "FromString", "FromBytes"] {
            env.traits.insert(
                QualifiedName {
                    home: primitives::literal_home(),
                    name,
                },
                &[&Kind::Type],
            );
        }
        env.traits.insert(primitives::eq_trait(), &[&Kind::Type]);
        for interface in interfaces
            .into_iter()
            .flat_map(|interfaces| interfaces.values())
        {
            for trait_ in interface.traits {
                env.superclasses.insert(
                    QualifiedName {
                        home: interface.home,
                        name: trait_.name,
                    },
                    TraitContext {
                        parameters: trait_.parameters,
                        supers: trait_.supers,
                    },
                );
                env.traits.insert(
                    QualifiedName {
                        home: interface.home,
                        name: trait_.name,
                    },
                    trait_.kinds,
                );
            }
            for union in interface.unions {
                let name = QualifiedName {
                    home: interface.home,
                    name: union.name,
                };
                // Keep the compiler-owned constructor inventory, including Data's exception.
                env.types.entry(name).or_insert(TypeInfo::Defined {
                    kind: union.kind,
                    parameters: union.parameters,
                    context: union.context,
                    repr: Some(if is_big_name(union.name) {
                        Repr::Big
                    } else {
                        Repr::Term
                    }),
                    alias: None,
                });
            }
            for alias in interface.aliases {
                env.types.insert(
                    QualifiedName {
                        home: interface.home,
                        name: alias.name,
                    },
                    TypeInfo::Defined {
                        kind: alias.kind,
                        parameters: alias.parameters,
                        context: alias.context,
                        repr: record_repr(alias.name, &alias.typ.value),
                        alias: Some(alias.typ),
                    },
                );
            }
        }
        env
    }

    pub fn constructor(&self, name: QualifiedName<'a>) -> TypeInfo<'a> {
        *self
            .types
            .get(&name)
            .expect("type metadata covers every canonical constructor")
    }
}

fn is_big_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

fn record_repr(name: &str, body: &Type<'_>) -> Option<Repr> {
    matches!(body, Type::Record { .. }).then_some(if is_big_name(name) {
        Repr::Big
    } else {
        Repr::Term
    })
}

/// Normalize transparent alias bodies before retaining an unresolved repr
/// predicate, so recursive-context relevance sees hidden variable applications.
pub(crate) fn representation_subject<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    mut typ: &'a Located<Type<'a>>,
) -> &'a Located<Type<'a>> {
    loop {
        let expanded = match &typ.value {
            Type::Alias {
                reference,
                arguments,
                remaining: [],
                target,
            } => {
                let body = match target {
                    nash_ast::AliasType::Open(body) | nash_ast::AliasType::Filled { body, .. } => {
                        *body
                    }
                };
                if record_repr(reference.name, &body.value).is_some() {
                    return typ;
                }
                let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
                crate::types::substitute_type(bump, &substitution, body)
            }
            Type::Named { reference, args } => {
                let TypeInfo::Defined {
                    parameters,
                    alias: Some(body),
                    repr: None,
                    ..
                } = env.constructor(*reference)
                else {
                    return typ;
                };
                if parameters.len() != args.len() {
                    return typ;
                }
                let substitution = parameters
                    .iter()
                    .copied()
                    .zip(args.iter().copied())
                    .collect();
                crate::types::substitute_type(bump, &substitution, body)
            }
            Type::App { head, args } => {
                let applied = crate::types::apply_type(bump, typ.region, head, args);
                if matches!(applied.value, Type::App { .. }) {
                    return applied;
                }
                applied
            }
            _ => return typ,
        };
        typ = expanded;
    }
}

/// The representation query does not bind types or inspect declaration contexts.
/// In particular, a transparent alias must use all of its supplied arguments.
pub fn repr_of<'a>(bump: &'a Bump, env: &KindEnv<'a>, typ: &'a Located<Type<'a>>) -> Option<Repr> {
    match &typ.value {
        Type::Var(_) => None,

        Type::Lambda { .. } | Type::Tuple { .. } => Some(Repr::Term),
        Type::Record { .. } => None,
        Type::App { head, args } => {
            let applied = crate::types::apply_type(bump, typ.region, head, args);
            if matches!(applied.value, Type::App { .. }) {
                None
            } else {
                repr_of(bump, env, applied)
            }
        }
        Type::Alias {
            reference,
            arguments,
            remaining,
            target,
        } => {
            if !remaining.is_empty() {
                return None;
            }
            let body = match target {
                nash_ast::AliasType::Open(body) | nash_ast::AliasType::Filled { body, .. } => *body,
            };
            if let Some(repr) = record_repr(reference.name, &body.value) {
                return Some(repr);
            }
            let substitution = arguments.iter().map(|a| (a.name, a.typ)).collect();
            repr_of(
                bump,
                env,
                crate::types::substitute_type(bump, &substitution, body),
            )
        }
        Type::Named { reference, args } => {
            let info = env.constructor(*reference);
            if args.len() != info.parameters().len() {
                return None;
            }
            match info {
                TypeInfo::Builtin(p) => Some(p.repr),
                TypeInfo::Defined {
                    repr: Some(repr), ..
                } => Some(repr),
                TypeInfo::Defined {
                    parameters,
                    alias: Some(body),
                    ..
                } => {
                    let substitution = parameters
                        .iter()
                        .copied()
                        .zip(args.iter().copied())
                        .collect();
                    repr_of(
                        bump,
                        env,
                        crate::types::substitute_type(bump, &substitution, body),
                    )
                }
                _ => None,
            }
        }
    }
}

/// A type walk shares parameter variables across all uses in a declaration.
/// Constructor variables are shared only within the current declaration SCC.
pub struct TypeChecker<'a, 'env> {
    pub infer: Infer<'a>,
    pub constructors: BTreeMap<QualifiedName<'a>, &'a K<'a>>,
    pub variables: BTreeMap<&'a str, &'a K<'a>>,
    pub traits: BTreeMap<QualifiedName<'a>, Vec<&'a K<'a>>>,
    env: &'env KindEnv<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct TypeMismatch<'a> {
    pub region: nash_region::Region,
    pub mismatch: Mismatch<'a>,
}

impl<'a, 'env> TypeChecker<'a, 'env> {
    pub fn new(bump: &'a Bump, env: &'env KindEnv<'a>) -> Self {
        Self {
            infer: Infer::new(bump),
            constructors: BTreeMap::new(),
            variables: BTreeMap::new(),
            traits: BTreeMap::new(),
            env,
        }
    }

    pub fn variable(&mut self, name: &'a str) -> &'a K<'a> {
        self.variables
            .entry(name)
            .or_insert_with(|| self.infer.fresh())
    }

    pub fn value(&mut self, typ: &'a Located<Type<'a>>) -> Result<(), TypeMismatch<'a>> {
        let kind = self.typ(typ)?;
        self.infer
            .unify(&K::Type, kind)
            .map_err(|mismatch| TypeMismatch {
                region: typ.region,
                mismatch,
            })
    }

    fn application(
        &mut self,
        head: &'a K<'a>,
        args: impl IntoIterator<Item = &'a Located<Type<'a>>>,
    ) -> Result<&'a K<'a>, TypeMismatch<'a>> {
        let mut result = head;
        for arg in args {
            let kind = self.typ(arg)?;
            result = self
                .infer
                .apply(result, &[kind])
                .map_err(|mismatch| TypeMismatch {
                    region: arg.region,
                    mismatch,
                })?;
        }
        Ok(result)
    }

    fn constructor(&self, name: QualifiedName<'a>) -> &'a K<'a> {
        self.constructors
            .get(&name)
            .copied()
            .unwrap_or_else(|| self.infer.from_kind(self.env.constructor(name).kind()))
    }

    pub fn typ(&mut self, typ: &'a Located<Type<'a>>) -> Result<&'a K<'a>, TypeMismatch<'a>> {
        match &typ.value {
            Type::Var(name) => Ok(self.variable(name)),
            Type::Named { reference, args } => {
                let head = self.constructor(*reference);
                self.application(head, args.iter().copied())
            }
            Type::Alias {
                reference,
                arguments,
                ..
            } => {
                let head = self.constructor(*reference);
                self.application(head, arguments.iter().map(|a| a.typ))
            }
            Type::App { head, args } => {
                let kind = self.typ(head)?;
                self.application(kind, args.iter().copied())
            }
            Type::Lambda { from, to } => {
                self.value(from)?;
                self.value(to)?;
                Ok(&K::Type)
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                self.value(first)?;
                self.value(second)?;
                for part in *rest {
                    self.value(part)?;
                }
                Ok(&K::Type)
            }
            Type::Record { fields } => {
                for field in *fields {
                    self.value(field.typ)?;
                }
                Ok(&K::Type)
            }
        }
    }

    pub fn predicate(&mut self, pred: Pred<'a>) -> Result<(), TypeMismatch<'a>> {
        match pred {
            Pred::Trait { trait_, args } | Pred::Implied { trait_, args } => {
                let kinds = self.traits.get(&trait_).cloned().unwrap_or_else(|| {
                    self.env
                        .traits
                        .get(&trait_)
                        .expect("canonical trait has kinds")
                        .iter()
                        .map(|kind| self.infer.from_kind(kind))
                        .collect()
                });
                assert_eq!(kinds.len(), args.len(), "canonical trait has checked arity");
                for (arg, expected) in args.iter().zip(kinds) {
                    let actual = self.typ(arg)?;
                    self.infer
                        .unify(expected, actual)
                        .map_err(|mismatch| TypeMismatch {
                            region: arg.region,
                            mismatch,
                        })?;
                }
            }
            Pred::Apply { head, args } => {
                let kind = self.typ(head)?;
                self.application(kind, args.iter().copied())?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RepresentationFailure<'a> {
    pub typ: &'a Located<Type<'a>>,
    pub required: ReprTrait,
    pub actual: Repr,
}

#[derive(Clone, Copy, Debug)]
pub struct GroupReference<'a> {
    pub name: QualifiedName<'a>,
    pub args: &'a [&'a Located<Type<'a>>],
    pub region: nash_region::Region,
}

/// Formation and context reduction use one path for declarations and signatures.
/// References inside the current SCC are recorded without reading partial contexts.
pub struct Formation<'a, 'env> {
    bump: &'a Bump,
    env: &'env KindEnv<'a>,
    group: &'env std::collections::BTreeSet<QualifiedName<'a>>,
    pub references: Vec<GroupReference<'a>>,
    pub predicates: Vec<Pred<'a>>,
    keys: std::collections::HashSet<nash_ast::PredicateKey<'a>>,
}

impl<'a, 'env> Formation<'a, 'env> {
    pub fn new(
        bump: &'a Bump,
        env: &'env KindEnv<'a>,
        group: &'env std::collections::BTreeSet<QualifiedName<'a>>,
    ) -> Self {
        Self {
            bump,
            env,
            group,
            references: Vec::new(),
            predicates: Vec::new(),
            keys: Default::default(),
        }
    }

    fn retain(&mut self, pred: Pred<'a>) {
        if self.keys.insert(pred.key()) {
            self.predicates.push(pred);
        } else if !pred.hidden()
            && let Some(existing) = self
                .predicates
                .iter_mut()
                .find(|existing| existing.key() == pred.key())
        {
            *existing = pred;
        }
    }

    pub fn require(
        &mut self,
        typ: &'a Located<Type<'a>>,
        required: ReprTrait,
    ) -> Result<(), RepresentationFailure<'a>> {
        self.reduce(Pred::Implied {
            trait_: required.qualified(),
            args: self.bump.alloc_slice_copy(&[typ]),
        })
    }

    pub fn reduce(&mut self, pred: Pred<'a>) -> Result<(), RepresentationFailure<'a>> {
        match pred {
            Pred::Trait { trait_, args } | Pred::Implied { trait_, args } => {
                if let Some(required) = ReprTrait::of(trait_) {
                    let typ = representation_subject(self.bump, self.env, args[0]);
                    if let Some(actual) = repr_of(self.bump, self.env, typ) {
                        if !required.admits().contains(actual) {
                            return Err(RepresentationFailure {
                                typ,
                                required,
                                actual,
                            });
                        }
                        return Ok(());
                    }
                    let args = self.bump.alloc_slice_copy(&[typ]);
                    self.retain(match pred {
                        Pred::Trait { trait_, .. } => Pred::Trait { trait_, args },
                        Pred::Implied { trait_, .. } => Pred::Implied { trait_, args },
                        Pred::Apply { .. } => unreachable!(),
                    });
                    return Ok(());
                }
                self.retain(pred);
            }
            Pred::Apply { head, args } => {
                let applied = crate::types::apply_type(self.bump, head.region, head, args);
                match &applied.value {
                    Type::Named { reference, args } => {
                        self.named(*reference, args, applied.region)?
                    }
                    Type::Alias {
                        reference,
                        arguments,
                        ..
                    } => {
                        let args = self
                            .bump
                            .alloc_slice_fill_iter(arguments.iter().map(|a| a.typ));
                        self.named(*reference, args, applied.region)?;
                    }
                    // Kind checking separately rejects an application of a value kind.
                    Type::App { head, args } => self.retain(Pred::Apply { head, args }),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn named(
        &mut self,
        name: QualifiedName<'a>,
        args: &'a [&'a Located<Type<'a>>],
        region: nash_region::Region,
    ) -> Result<(), RepresentationFailure<'a>> {
        if self.group.contains(&name) {
            self.references.push(GroupReference { name, args, region });
            return Ok(());
        }
        let info = self.env.constructor(name);
        let substitution: BTreeMap<_, _> = info
            .parameters()
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect();
        for pred in info.context(self.bump) {
            if predicate_variables(*pred)
                .iter()
                .all(|name| substitution.contains_key(name))
            {
                self.reduce(substitute_predicate(self.bump, &substitution, *pred))?;
            }
        }
        Ok(())
    }

    pub fn typ(&mut self, typ: &'a Located<Type<'a>>) -> Result<(), RepresentationFailure<'a>> {
        match &typ.value {
            Type::Var(_) => {}
            Type::Named { reference, args } => {
                for arg in *args {
                    self.typ(arg)?;
                }
                self.named(*reference, args, typ.region)?;
            }
            Type::Alias {
                reference,
                arguments,
                ..
            } => {
                for arg in *arguments {
                    self.typ(arg.typ)?;
                }
                let args = self
                    .bump
                    .alloc_slice_fill_iter(arguments.iter().map(|a| a.typ));
                self.named(*reference, args, typ.region)?;
            }
            Type::App { head, args } => {
                self.typ(head)?;
                for arg in *args {
                    self.typ(arg)?;
                }
                self.reduce(Pred::Apply { head, args })?;
            }
            Type::Lambda { from, to } => {
                self.typ(from)?;
                self.typ(to)?;
            }
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                self.typ(first)?;
                self.typ(second)?;
                for part in *rest {
                    self.typ(part)?;
                }
            }
            Type::Record { fields, .. } => {
                for field in *fields {
                    self.typ(field.typ)?;
                }
            }
        }
        Ok(())
    }
}

pub fn substitute_predicate<'a>(
    bump: &'a Bump,
    substitution: &BTreeMap<&'a str, &'a Located<Type<'a>>>,
    pred: Pred<'a>,
) -> Pred<'a> {
    let args = bump.alloc_slice_fill_iter(
        pred.args()
            .iter()
            .map(|arg| crate::types::substitute_type(bump, substitution, arg)),
    );
    match pred {
        Pred::Trait { trait_, .. } => Pred::Trait { trait_, args },
        Pred::Implied { trait_, .. } => Pred::Implied { trait_, args },
        Pred::Apply { head, .. } => Pred::Apply {
            head: crate::types::substitute_type(bump, substitution, head),
            args,
        },
    }
}

pub fn predicate_variables(pred: Pred<'_>) -> std::collections::BTreeSet<&str> {
    let mut result = std::collections::BTreeSet::new();
    let mut pending: Vec<_> = pred.types().collect();
    while let Some(typ) = pending.pop() {
        match &typ.value {
            Type::Var(name) => {
                result.insert(*name);
            }
            Type::Named { args, .. } => pending.extend(args.iter().copied()),
            Type::Alias { arguments, .. } => pending.extend(arguments.iter().map(|a| a.typ)),
            Type::App { head, args } => {
                pending.push(head);
                pending.extend(args.iter().copied());
            }
            Type::Lambda { from, to } => pending.extend([*from, *to]),
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                pending.extend([*first, *second]);
                pending.extend(rest.iter().copied());
            }
            Type::Record { fields } => {
                pending.extend(fields.iter().map(|f| f.typ));
            }
        }
    }
    result
}

pub struct ContextInput<'a> {
    pub name: QualifiedName<'a>,
    pub parameters: &'a [&'a str],
    pub predicates: Vec<Pred<'a>>,
    pub references: Vec<GroupReference<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub enum ContextFailure<'a> {
    Representation(RepresentationFailure<'a>),
    IrregularRecursion {
        reference: GroupReference<'a>,
        parameter: &'a str,
    },
}

/// Close one SCC under substitution. Each (reference, predicate) pair is
/// processed once. New relevant parameters are checked as their predicates
/// enter the queue; there is no iteration limit.
pub fn close_contexts<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    inputs: &[ContextInput<'a>],
) -> Result<Vec<Vec<Pred<'a>>>, ContextFailure<'a>> {
    use std::collections::{BTreeSet, HashSet, VecDeque};
    let indices: BTreeMap<_, _> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| (input.name, i))
        .collect();
    let group: BTreeSet<_> = indices.keys().copied().collect();
    let mut contexts: Vec<Vec<Pred<'a>>> = inputs.iter().map(|_| Vec::new()).collect();
    let mut keys: Vec<HashSet<_>> = inputs.iter().map(|_| HashSet::new()).collect();
    let mut references: Vec<(usize, GroupReference<'a>)> = inputs
        .iter()
        .enumerate()
        .flat_map(|(owner, input)| {
            input
                .references
                .iter()
                .map(move |reference| (owner, *reference))
        })
        .collect();
    let mut queue = VecDeque::new();
    for (owner, input) in inputs.iter().enumerate() {
        for pred in &input.predicates {
            if keys[owner].insert(pred.key()) {
                contexts[owner].push(*pred);
            }
        }
    }
    for (reference_index, (_, reference)) in references.iter().enumerate() {
        for predicate_index in 0..contexts[indices[&reference.name]].len() {
            queue.push_back((reference_index, predicate_index));
        }
    }
    while let Some((reference_index, predicate_index)) = queue.pop_front() {
        let (owner, reference) = references[reference_index];
        let target = indices[&reference.name];
        let pred = contexts[target][predicate_index];
        let substitution: BTreeMap<_, _> = inputs[target]
            .parameters
            .iter()
            .copied()
            .zip(reference.args.iter().copied())
            .collect();
        let variables = predicate_variables(pred);
        if contains_variable_application(pred) {
            for parameter in &variables {
                if let Some(typ) = substitution.get(parameter)
                    && !matches!(typ.value, Type::Var(_))
                {
                    return Err(ContextFailure::IrregularRecursion {
                        reference,
                        parameter,
                    });
                }
            }
        }
        // Partial applications enforce only predicates whose complete free
        // parameter set has been supplied. A later Apply supplies the rest.
        if !variables.iter().all(|name| substitution.contains_key(name)) {
            continue;
        }
        let mut formation = Formation::new(bump, env, &group);
        formation
            .reduce(substitute_predicate(bump, &substitution, pred))
            .map_err(ContextFailure::Representation)?;
        for pred in formation.predicates {
            if keys[owner].insert(pred.key()) {
                let index = contexts[owner].len();
                contexts[owner].push(pred);
                for (reference_index, (_, reference)) in references.iter().enumerate() {
                    if indices[&reference.name] == owner {
                        queue.push_back((reference_index, index));
                    }
                }
            }
        }
        for new_reference in formation.references {
            // A reduced Apply can reveal another group reference. Its semantic
            // application key ignores source regions, just like context keys.
            let new_key = reference_key(bump, new_reference);
            if references.iter().any(|(existing_owner, existing)| {
                *existing_owner == owner && reference_key(bump, *existing) == new_key
            }) {
                continue;
            }
            let reference_index = references.len();
            for predicate_index in 0..contexts[indices[&new_reference.name]].len() {
                queue.push_back((reference_index, predicate_index));
            }
            references.push((owner, new_reference));
        }
    }
    Ok(contexts)
}

fn reference_key<'a>(bump: &'a Bump, reference: GroupReference<'a>) -> nash_ast::PredicateKey<'a> {
    Pred::Apply {
        head: bump.alloc(Located::at(
            reference.region,
            Type::Named {
                reference: reference.name,
                args: &[],
            },
        )),
        args: reference.args,
    }
    .key()
}

fn contains_variable_application(pred: Pred<'_>) -> bool {
    if matches!(pred, Pred::Apply { .. }) {
        return true;
    }
    let mut pending: Vec<_> = pred.types().collect();
    while let Some(typ) = pending.pop() {
        match &typ.value {
            Type::App { .. } => return true,
            Type::Named { args, .. } => pending.extend(args.iter().copied()),
            Type::Alias { arguments, .. } => pending.extend(arguments.iter().map(|a| a.typ)),
            Type::Lambda { from, to } => pending.extend([*from, *to]),
            Type::Tuple {
                first,
                second,
                rest,
            } => {
                pending.extend([*first, *second]);
                pending.extend(rest.iter().copied());
            }
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|f| f.typ)),
            Type::Var(_) => {}
        }
    }
    false
}

#[cfg(test)]
mod formation_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn var<'a>(bump: &'a Bump, name: &'a str) -> &'a Located<Type<'a>> {
        bump.alloc(Located::at_zero(Type::Var(name)))
    }
    fn name(name: &str) -> QualifiedName<'_> {
        QualifiedName {
            home: primitives::builtin_home(),
            name,
        }
    }
    fn named<'a>(
        bump: &'a Bump,
        name_: &'a str,
        args: &'a [&'a Located<Type<'a>>],
    ) -> &'a Located<Type<'a>> {
        bump.alloc(Located::at_zero(Type::Named {
            reference: name(name_),
            args,
        }))
    }

    #[test]
    fn partial_application_waits_for_every_free_parameter() {
        let bump = Bump::new();
        let mut env = KindEnv::default();
        let pair = name("custom");
        let f = var(&bump, "f");
        let a = var(&bump, "a");
        env.types.insert(
            pair,
            TypeInfo::Defined {
                kind: &Kind::Arrow(
                    &Kind::Arrow(&Kind::Type, &Kind::Type),
                    &Kind::Arrow(&Kind::Type, &Kind::Type),
                ),
                parameters: &["f", "a"],
                context: bump.alloc_slice_copy(&[Pred::Apply {
                    head: f,
                    args: bump.alloc_slice_copy(&[a]),
                }]),
                repr: Some(Repr::Term),
                alias: None,
            },
        );
        let group = BTreeSet::new();
        let mut formation = Formation::new(&bump, &env, &group);
        let list = named(&bump, "list", &[]);
        formation
            .typ(named(&bump, "custom", bump.alloc_slice_copy(&[list])))
            .unwrap();
        assert!(formation.predicates.is_empty());
        let function = &*bump.alloc(Located::at_zero(Type::Lambda { from: a, to: a }));
        let result = formation.typ(named(
            &bump,
            "custom",
            bump.alloc_slice_copy(&[list, function]),
        ));
        assert!(matches!(
            result,
            Err(RepresentationFailure {
                required: ReprTrait::Storable,
                actual: Repr::Term,
                ..
            })
        ));
    }

    #[test]
    fn partial_builtin_enforces_already_supplied_parameters() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let group = BTreeSet::new();
        let mut formation = Formation::new(&bump, &env, &group);
        let a = var(&bump, "a");
        formation
            .typ(named(&bump, "pair", bump.alloc_slice_copy(&[a])))
            .unwrap();
        assert_eq!(formation.predicates.len(), 1);
        assert_eq!(
            formation.predicates[0].trait_ref(),
            Some(ReprTrait::Storable.qualified())
        );
    }

    #[test]
    fn transparent_alias_representation_uses_supplied_argument() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let body = var(&bump, "a");
        let int = named(&bump, "int", &[]);
        let alias = bump.alloc(Located::at_zero(Type::Alias {
            reference: name("identity"),
            arguments: bump.alloc_slice_fill_iter([nash_ast::AliasArgument {
                name: "a",
                typ: int,
            }]),
            remaining: &[],
            target: nash_ast::AliasType::Open(body),
        }));
        assert_eq!(repr_of(&bump, &env, alias), Some(Repr::Const));
    }

    #[test]
    fn formation_checks_nested_arguments_even_when_outer_repr_is_known() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let group = BTreeSet::new();
        let mut formation = Formation::new(&bump, &env, &group);
        let a = var(&bump, "a");
        let function = &*bump.alloc(Located::at_zero(Type::Lambda { from: a, to: a }));
        let list = named(&bump, "List", bump.alloc_slice_copy(&[function]));
        assert_eq!(repr_of(&bump, &env, list), Some(Repr::Big));
        assert!(matches!(
            formation.typ(list),
            Err(RepresentationFailure {
                required: ReprTrait::Big,
                actual: Repr::Term,
                ..
            })
        ));
    }

    #[test]
    fn repeated_variable_head_formation_is_deduplicated() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let group = BTreeSet::new();
        let mut formation = Formation::new(&bump, &env, &group);
        let app = bump.alloc(Located::at_zero(Type::App {
            head: var(&bump, "f"),
            args: bump.alloc_slice_copy(&[var(&bump, "a")]),
        }));
        formation.typ(app).unwrap();
        formation.typ(app).unwrap();
        assert_eq!(formation.predicates.len(), 1);
        assert!(matches!(formation.predicates[0], Pred::Apply { .. }));
    }

    #[test]
    fn context_permutation_closes_after_more_than_256_steps() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let parameters =
            bump.alloc_slice_fill_iter((0..23).map(|i| &*bump.alloc_str(&format!("p{i}"))));
        let vars: Vec<_> = parameters.iter().map(|p| var(&bump, p)).collect();
        let mut permutation = Vec::new();
        for (start, size) in [(0, 5), (5, 7), (12, 11)] {
            for i in 0..size {
                permutation.push(vars[start + (i + 1) % size]);
            }
        }
        let input = ContextInput {
            name: name("r"),
            parameters,
            predicates: vec![Pred::Apply {
                head: vars[0],
                args: bump.alloc_slice_copy(&[vars[5], vars[12]]),
            }],
            references: vec![GroupReference {
                name: name("r"),
                args: bump.alloc_slice_copy(&permutation),
                region: nash_region::Region::zero(),
            }],
        };
        let contexts = close_contexts(&bump, &env, &[input]).unwrap();
        assert_eq!(contexts[0].len(), 385);
    }

    #[test]
    fn recursive_argument_growth_is_rejected_for_applied_relevant_parameter() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let f = var(&bump, "f");
        let a = var(&bump, "a");
        let input = ContextInput {
            name: name("r"),
            parameters: &["f", "a"],
            predicates: vec![Pred::Apply {
                head: f,
                args: bump.alloc_slice_copy(&[a]),
            }],
            references: vec![GroupReference {
                name: name("r"),
                args: bump
                    .alloc_slice_copy(&[f, named(&bump, "list", bump.alloc_slice_copy(&[a]))]),
                region: nash_region::Region::zero(),
            }],
        };
        assert!(matches!(
            close_contexts(&bump, &env, &[input]),
            Err(ContextFailure::IrregularRecursion { parameter: "a", .. })
        ));
    }

    #[test]
    fn wrapped_non_applied_parameter_does_not_make_recursion_irregular() {
        let bump = Bump::new();
        let env = KindEnv::default();
        let a = var(&bump, "a");
        let input = ContextInput {
            name: name("Nest"),
            parameters: &["a"],
            predicates: vec![Pred::Implied {
                trait_: ReprTrait::Big.qualified(),
                args: bump.alloc_slice_copy(&[a]),
            }],
            references: vec![GroupReference {
                name: name("Nest"),
                args: bump.alloc_slice_copy(&[named(&bump, "List", bump.alloc_slice_copy(&[a]))]),
                region: nash_region::Region::zero(),
            }],
        };
        let contexts = close_contexts(&bump, &env, &[input]).unwrap();
        assert_eq!(contexts[0].len(), 1);
    }
}

pub struct DeclarationKinds<'a>(BTreeMap<&'a str, (&'a Kind<'a>, &'a [Pred<'a>])>);

impl<'a> DeclarationKinds<'a> {
    pub fn kind(&self, name: &str) -> &'a Kind<'a> {
        self.0[name].0
    }
    pub fn context(&self, name: &str) -> &'a [Pred<'a>] {
        self.0[name].1
    }
}

#[derive(Clone, Copy)]
enum Declaration<'a> {
    Union(crate::module::PreUnion<'a>),
    Alias(crate::module::PreAlias<'a>),
}

impl<'a> Declaration<'a> {
    fn name(self) -> &'a str {
        match self {
            Self::Union(u) => u.name.value,
            Self::Alias(a) => a.name.value,
        }
    }
    fn parameters(self) -> &'a [&'a str] {
        match self {
            Self::Union(u) => u.parameters,
            Self::Alias(a) => a.parameters,
        }
    }
    fn context(self) -> &'a [Pred<'a>] {
        match self {
            Self::Union(u) => u.context,
            Self::Alias(a) => a.context,
        }
    }
    fn bodies(
        self,
    ) -> Vec<(
        &'a Located<Type<'a>>,
        crate::error::KindContext<'a>,
        Option<ReprTrait>,
    )> {
        use crate::error::KindContext;
        match self {
            Self::Union(u) => u
                .ctors
                .iter()
                .flat_map(|ctor| {
                    ctor.arguments.iter().enumerate().map(move |(index, typ)| {
                        let index = u16::try_from(index).expect("constructor arity fits u16");
                        if is_big_name(u.name.value) {
                            (
                                *typ,
                                KindContext::BigField {
                                    union: u.name.value,
                                    ctor: ctor.name,
                                    index,
                                },
                                Some(ReprTrait::Big),
                            )
                        } else {
                            (
                                *typ,
                                KindContext::LittleField {
                                    union: u.name.value,
                                    ctor: ctor.name,
                                    index,
                                },
                                None,
                            )
                        }
                    })
                })
                .collect(),
            Self::Alias(a) => match &a.typ.value {
                Type::Record { fields, .. } => fields
                    .iter()
                    .map(|field| {
                        (
                            field.typ,
                            KindContext::RecordField {
                                alias: a.name.value,
                                field: field.field,
                                big: is_big_name(a.name.value),
                            },
                            is_big_name(a.name.value).then_some(ReprTrait::Big),
                        )
                    })
                    .collect(),
                _ => vec![(
                    a.typ,
                    KindContext::AliasCasing {
                        alias: a.name.value,
                        big: is_big_name(a.name.value),
                    },
                    Some(if is_big_name(a.name.value) {
                        ReprTrait::Big
                    } else {
                        ReprTrait::Little
                    }),
                )],
            },
        }
    }
}

fn kind_error<'a>(
    infer: &mut Infer<'a>,
    mismatch: TypeMismatch<'a>,
    context: &'a crate::error::KindContext<'a>,
) -> crate::Error<'a> {
    match mismatch.mismatch {
        Mismatch::Infinite => crate::Error::KindInfinite {
            region: mismatch.region,
            context,
        },
        Mismatch::Shapes { expected, actual } => crate::Error::KindMismatch {
            region: mismatch.region,
            context,
            expected: infer.default_and_zonk(expected),
            actual: infer.default_and_zonk(actual),
        },
    }
}

fn representation_error<'a>(
    failure: RepresentationFailure<'a>,
    context: &'a crate::error::KindContext<'a>,
) -> crate::Error<'a> {
    crate::Error::RepresentationMismatch {
        region: failure.typ.region,
        context,
        required: failure.required,
        actual: failure.actual,
    }
}

/// Infer and close one SCC before making its metadata visible to dependents.
pub(crate) fn infer_declarations<'a>(
    bump: &'a Bump,
    env: &mut KindEnv<'a>,
    home: nash_ast::ModuleName<'a>,
    unions: &[crate::module::PreUnion<'a>],
    aliases: &[crate::module::PreAlias<'a>],
) -> Result<DeclarationKinds<'a>, Vec<crate::Error<'a>>> {
    let declarations: Vec<_> = unions
        .iter()
        .copied()
        .map(Declaration::Union)
        .chain(aliases.iter().copied().map(Declaration::Alias))
        .collect();
    let nodes: Vec<_> = declarations
        .iter()
        .copied()
        .map(|declaration| {
            let mut deps = Vec::new();
            let mut pending: Vec<_> = declaration
                .bodies()
                .into_iter()
                .map(|(typ, _, _)| typ)
                .collect();
            while let Some(typ) = pending.pop() {
                match &typ.value {
                    Type::Named { reference, args } => {
                        if reference.home == home {
                            deps.push(reference.name);
                        }
                        pending.extend(args.iter().copied());
                    }
                    Type::Alias {
                        reference,
                        arguments,
                        ..
                    } => {
                        if reference.home == home {
                            deps.push(reference.name);
                        }
                        pending.extend(arguments.iter().map(|a| a.typ));
                    }
                    Type::App { head, args } => {
                        pending.push(head);
                        pending.extend(args.iter().copied());
                    }
                    Type::Lambda { from, to } => pending.extend([*from, *to]),
                    Type::Tuple {
                        first,
                        second,
                        rest,
                    } => {
                        pending.extend([*first, *second]);
                        pending.extend(rest.iter().copied());
                    }
                    Type::Record { fields, .. } => pending.extend(fields.iter().map(|f| f.typ)),
                    Type::Var(_) => {}
                }
            }
            crate::scc::Node {
                key: declaration.name(),
                value: declaration,
                deps,
            }
        })
        .collect();
    let dependencies: BTreeMap<_, _> = nodes
        .iter()
        .map(|node| (node.key, node.deps.clone()))
        .collect();
    let mut failed = std::collections::BTreeSet::new();
    let mut errors = Vec::new();
    let mut result = BTreeMap::new();
    for component in crate::scc::strongly_connected_components(nodes) {
        let group = match component {
            crate::scc::Scc::Acyclic(declaration) => vec![declaration],
            crate::scc::Scc::Cyclic(group) => group,
        };
        if group.iter().any(|declaration| {
            dependencies[declaration.name()]
                .iter()
                .any(|name| failed.contains(name))
        }) {
            failed.extend(group.iter().copied().map(Declaration::name));
            continue;
        }
        let outcome = (|| -> Result<(), Vec<crate::Error<'a>>> {
            let mut checker = TypeChecker::new(bump, env);
            let mut parameter_kinds = BTreeMap::new();
            for declaration in &group {
                let parameters: BTreeMap<_, _> = declaration
                    .parameters()
                    .iter()
                    .map(|name| (*name, checker.infer.fresh()))
                    .collect();
                let mut kind = &K::Type;
                for parameter in declaration.parameters().iter().rev() {
                    kind = checker.infer.arrow(parameters[parameter], kind);
                }
                checker.constructors.insert(
                    QualifiedName {
                        home,
                        name: declaration.name(),
                    },
                    kind,
                );
                parameter_kinds.insert(declaration.name(), parameters);
            }
            for declaration in &group {
                checker.variables = parameter_kinds[declaration.name()].clone();
                for (typ, context, _) in declaration.bodies() {
                    if let Err(mismatch) = checker.value(typ) {
                        return Err(vec![kind_error(
                            &mut checker.infer,
                            mismatch,
                            bump.alloc(context),
                        )]);
                    }
                }
                for pred in declaration.context() {
                    if let Err(mismatch) = checker.predicate(*pred) {
                        return Err(vec![kind_error(
                            &mut checker.infer,
                            mismatch,
                            &crate::error::KindContext::TypeAnnotation,
                        )]);
                    }
                }
            }
            let mut closed_kinds = BTreeMap::new();
            for (name, kind) in checker.constructors.clone() {
                closed_kinds.insert(name, checker.infer.default_and_zonk(kind));
            }
            drop(checker);
            // Publish shapes for representation lookup while formation defers every
            // context lookup inside this group.
            for declaration in &group {
                let name = QualifiedName {
                    home,
                    name: declaration.name(),
                };
                let (repr, alias) = match declaration {
                    Declaration::Union(_) => (
                        Some(if is_big_name(name.name) {
                            Repr::Big
                        } else {
                            Repr::Term
                        }),
                        None,
                    ),
                    Declaration::Alias(a) => (record_repr(name.name, &a.typ.value), Some(a.typ)),
                };
                env.types.insert(
                    name,
                    TypeInfo::Defined {
                        kind: closed_kinds[&name],
                        parameters: declaration.parameters(),
                        context: &[],
                        repr,
                        alias,
                    },
                );
            }
            let group_names = closed_kinds.keys().copied().collect();
            let mut inputs = Vec::new();
            for declaration in &group {
                let mut formation = Formation::new(bump, env, &group_names);
                for (typ, context, required) in declaration.bodies() {
                    let context = bump.alloc(context);
                    formation
                        .typ(typ)
                        .map_err(|failure| vec![representation_error(failure, context)])?;
                    if let Some(required) = required {
                        formation
                            .require(typ, required)
                            .map_err(|failure| vec![representation_error(failure, context)])?;
                    }
                }
                for pred in declaration.context() {
                    // A bound written around a record alias body describes
                    // that alias's nominal representation, not an anonymous
                    // record constraint to export to its callers.
                    if let Declaration::Alias(alias) = declaration
                        && let Some(actual) = record_repr(alias.name.value, &alias.typ.value)
                        && let Some(required) = pred.trait_ref().and_then(ReprTrait::of)
                        && pred.key()
                            == (Pred::Implied {
                                trait_: required.qualified(),
                                args: bump.alloc_slice_copy(&[alias.typ]),
                            })
                            .key()
                    {
                        if !required.admits().contains(actual) {
                            return Err(vec![representation_error(
                                RepresentationFailure {
                                    typ: alias.typ,
                                    required,
                                    actual,
                                },
                                &crate::error::KindContext::TypeAnnotation,
                            )]);
                        }
                        continue;
                    }
                    formation.reduce(*pred).map_err(|failure| {
                        vec![representation_error(
                            failure,
                            &crate::error::KindContext::TypeAnnotation,
                        )]
                    })?;
                }
                inputs.push(ContextInput {
                    name: QualifiedName {
                        home,
                        name: declaration.name(),
                    },
                    parameters: declaration.parameters(),
                    predicates: formation.predicates,
                    references: formation.references,
                });
            }
            let contexts = close_contexts(bump, env, &inputs).map_err(|failure| {
                vec![match failure {
                    ContextFailure::Representation(failure) => {
                        representation_error(failure, &crate::error::KindContext::TypeAnnotation)
                    }
                    ContextFailure::IrregularRecursion {
                        reference,
                        parameter,
                    } => crate::Error::IrregularRecursion {
                        region: reference.region,
                        constructor: reference.name,
                        parameter,
                    },
                }]
            })?;
            for (declaration, context) in group.iter().zip(contexts) {
                let name = QualifiedName {
                    home,
                    name: declaration.name(),
                };
                check_representation_consistency(&context)?;
                let context = &*bump.alloc_slice_fill_iter(context);
                let TypeInfo::Defined {
                    context: stored, ..
                } = env.types.get_mut(&name).expect("group metadata installed")
                else {
                    unreachable!()
                };
                *stored = context;
                result.insert(name.name, (closed_kinds[&name], context));
            }
            Ok(())
        })();
        if let Err(group_errors) = outcome {
            failed.extend(group.iter().copied().map(Declaration::name));
            errors.extend(group_errors);
        }
    }
    if errors.is_empty() {
        Ok(DeclarationKinds(result))
    } else {
        Err(errors)
    }
}

fn check_representation_consistency<'a>(
    predicates: &[Pred<'a>],
) -> Result<(), Vec<crate::Error<'a>>> {
    let mut admitted = BTreeMap::new();
    for pred in predicates {
        let Some(trait_) = pred.trait_ref().and_then(ReprTrait::of) else {
            continue;
        };
        let arg = pred.args()[0];
        if let Type::Var(name) = arg.value {
            let previous = admitted.entry(name).or_insert(primitives::ReprSet::ALL);
            *previous = previous.intersect(trait_.admits());
            if previous.is_empty() {
                return Err(vec![crate::Error::ContradictoryRepresentation {
                    region: arg.region,
                    variable: name,
                }]);
            }
        }
    }
    Ok(())
}

pub fn check_annotation<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    name: &'a str,
    annotation: &'a nash_ast::Annotation<'a>,
) -> Result<&'a nash_ast::Annotation<'a>, Vec<crate::Error<'a>>> {
    let context = &*bump.alloc(crate::error::KindContext::Annotation { name });
    let mut checker = TypeChecker::new(bump, env);
    checker
        .value(annotation.typ)
        .map_err(|mismatch| vec![kind_error(&mut checker.infer, mismatch, context)])?;
    for pred in annotation.context {
        checker
            .predicate(*pred)
            .map_err(|mismatch| vec![kind_error(&mut checker.infer, mismatch, context)])?;
    }
    let group = Default::default();
    let mut formation = Formation::new(bump, env, &group);
    // Explicit predicates take precedence over hidden formation predicates with
    // the same semantic key, so a written context remains visible.
    for pred in annotation.context {
        for typ in pred.types() {
            formation
                .typ(typ)
                .map_err(|failure| vec![representation_error(failure, context)])?;
        }
        formation
            .reduce(*pred)
            .map_err(|failure| vec![representation_error(failure, context)])?;
    }
    formation
        .typ(annotation.typ)
        .map_err(|failure| vec![representation_error(failure, context)])?;
    check_representation_consistency(&formation.predicates)?;
    // A class premise also promises its superclasses. Reject a context such
    // as BigOnly int when BigOnly has Big as a superclass, even if unused.
    let mut validation = Formation::new(bump, env, &group);
    let mut pending = formation.predicates.clone();
    let mut seen = std::collections::HashSet::new();
    while let Some(pred) = pending.pop() {
        if !seen.insert(pred.key()) {
            continue;
        }
        validation
            .reduce(pred)
            .map_err(|failure| vec![representation_error(failure, context)])?;
        if let Some(info) = pred
            .trait_ref()
            .and_then(|name| env.superclasses.get(&name))
        {
            let substitution = info
                .parameters
                .iter()
                .copied()
                .zip(pred.args().iter().copied())
                .collect();
            pending.extend(
                info.supers
                    .iter()
                    .map(|sup| substitute_predicate(bump, &substitution, *sup)),
            );
        }
    }
    check_representation_consistency(&validation.predicates)?;
    Ok(bump.alloc(nash_ast::Annotation {
        typ: annotation.typ,
        free_vars: annotation.free_vars,
        context: bump.alloc_slice_fill_iter(formation.predicates),
    }))
}

pub(crate) fn infer_traits<'a>(
    bump: &'a Bump,
    env: &mut KindEnv<'a>,
    home: nash_ast::ModuleName<'a>,
    traits: &[crate::traits::PreTrait<'a>],
) -> Result<BTreeMap<&'a str, &'a [&'a Kind<'a>]>, Vec<crate::Error<'a>>> {
    for trait_ in traits {
        env.superclasses.insert(
            QualifiedName {
                home,
                name: trait_.source.value.name.value,
            },
            TraitContext {
                parameters: trait_.parameters,
                supers: trait_.supers,
            },
        );
    }
    let nodes = traits
        .iter()
        .map(|trait_| crate::scc::Node {
            key: trait_.source.value.name.value,
            value: trait_,
            deps: trait_
                .supers
                .iter()
                .chain(trait_.methods.iter().flat_map(|m| m.annotation.context))
                .filter_map(|p| p.trait_ref())
                .filter(|name| name.home == home)
                .map(|name| name.name)
                .collect(),
        })
        .collect();
    let mut result = BTreeMap::new();
    for component in crate::scc::strongly_connected_components(nodes) {
        let group = match component {
            crate::scc::Scc::Acyclic(t) => vec![t],
            crate::scc::Scc::Cyclic(group) => group,
        };
        let mut checker = TypeChecker::new(bump, env);
        for trait_ in &group {
            let kinds = trait_
                .parameters
                .iter()
                .map(|_| checker.infer.fresh())
                .collect();
            checker.traits.insert(
                QualifiedName {
                    home,
                    name: trait_.source.value.name.value,
                },
                kinds,
            );
        }
        for trait_ in &group {
            let name = QualifiedName {
                home,
                name: trait_.source.value.name.value,
            };
            let parameters: BTreeMap<_, _> = trait_
                .parameters
                .iter()
                .copied()
                .zip(checker.traits[&name].iter().copied())
                .collect();
            checker.variables = parameters.clone();
            for pred in trait_.supers {
                checker.predicate(*pred).map_err(|mismatch| {
                    vec![kind_error(
                        &mut checker.infer,
                        mismatch,
                        &crate::error::KindContext::TypeAnnotation,
                    )]
                })?;
            }
            for method in trait_.methods {
                checker.variables = parameters.clone();
                let context = &*bump.alloc(crate::error::KindContext::Annotation {
                    name: method.name.value,
                });
                checker
                    .value(method.annotation.typ)
                    .map_err(|mismatch| vec![kind_error(&mut checker.infer, mismatch, context)])?;
                for pred in method.annotation.context {
                    checker.predicate(*pred).map_err(|mismatch| {
                        vec![kind_error(&mut checker.infer, mismatch, context)]
                    })?;
                }
            }
        }
        let closed: Vec<_> = checker
            .traits
            .clone()
            .into_iter()
            .map(|(name, kinds)| {
                (
                    name,
                    &*bump.alloc_slice_fill_iter(
                        kinds
                            .into_iter()
                            .map(|kind| checker.infer.default_and_zonk(kind)),
                    ),
                )
            })
            .collect();
        drop(checker);
        for (name, kinds) in closed {
            env.traits.insert(name, kinds);
            result.insert(name.name, kinds);
        }
    }
    Ok(result)
}

pub(crate) fn check_impl<'a>(
    bump: &'a Bump,
    env: &KindEnv<'a>,
    trait_: &crate::environment::TraitInfo<'a>,
    heads: &[&'a Located<Type<'a>>],
    context: &'a [Pred<'a>],
) -> Result<&'a [Pred<'a>], Vec<crate::Error<'a>>> {
    let mut checker = TypeChecker::new(bump, env);
    let name = QualifiedName {
        home: trait_.home,
        name: trait_.name,
    };
    for (index, (head, expected)) in heads.iter().zip(trait_.kinds).enumerate() {
        let position = &*bump.alloc(crate::error::KindContext::ImplHead {
            trait_: name,
            index: index as u16,
        });
        let actual = checker
            .typ(head)
            .map_err(|mismatch| vec![kind_error(&mut checker.infer, mismatch, position)])?;
        let expected = checker.infer.from_kind(expected);
        if let Err(mismatch) = checker.infer.unify(expected, actual) {
            return Err(vec![kind_error(
                &mut checker.infer,
                TypeMismatch {
                    region: head.region,
                    mismatch,
                },
                position,
            )]);
        }
    }
    for pred in context {
        checker.predicate(*pred).map_err(|mismatch| {
            vec![kind_error(
                &mut checker.infer,
                mismatch,
                &crate::error::KindContext::TypeAnnotation,
            )]
        })?;
    }
    let group = Default::default();
    let mut formation = Formation::new(bump, env, &group);
    for pred in context {
        for typ in pred.types() {
            formation.typ(typ).map_err(|failure| {
                vec![representation_error(
                    failure,
                    &crate::error::KindContext::TypeAnnotation,
                )]
            })?;
        }
        formation.reduce(*pred).map_err(|failure| {
            vec![representation_error(
                failure,
                &crate::error::KindContext::TypeAnnotation,
            )]
        })?;
    }
    for head in heads {
        formation.typ(head).map_err(|failure| {
            vec![representation_error(
                failure,
                &crate::error::KindContext::TypeAnnotation,
            )]
        })?;
    }
    check_representation_consistency(&formation.predicates)?;
    Ok(bump.alloc_slice_fill_iter(formation.predicates))
}

/// The compiler-owned interface is built from the same constructor inventory
/// used by formation checks. Data's foreign constructor fields bypass casing.
pub fn builtin_interface(bump: &Bump) -> crate::Interface<'_> {
    use crate::interface::{InterfaceTrait, InterfaceUnion, InterfaceValue, UnionVisibility};
    let env = KindEnv::default();
    crate::Interface {
        home: primitives::builtin_home(),
        impls: &[],
        aliases: &[],
        binops: &[],
        traits: bump.alloc_slice_fill_iter(ReprTrait::ALL.into_iter().map(|trait_| {
            InterfaceTrait {
                name: trait_.name(),
                parameters: &["a"],
                kinds: &[&Kind::Type],
                methods: &[],
                exported: true,
                supers: bump.alloc_slice_fill_iter(trait_.supers().iter().map(|superclass| {
                    Pred::Trait {
                        trait_: superclass.qualified(),
                        args: bump
                            .alloc_slice_copy(&[&*bump.alloc(Located::at_zero(Type::Var("a")))]),
                    }
                })),
            }
        })),
        unions: bump.alloc_slice_fill_iter(primitives::PRIMITIVES.iter().map(|primitive| {
            let info = TypeInfo::Builtin(primitive);
            InterfaceUnion {
                name: primitive.name,
                kind: primitive.kind,
                context: info.context(bump),
                parameters: info.parameters(),
                ctors: primitive.ctors,
                alternatives: primitive.ctors.len() as u16,
                options: if primitive.name == "bool" {
                    nash_ast::CtorOpts::Enum
                } else {
                    nash_ast::CtorOpts::Normal
                },
                visibility: UnionVisibility::Open,
            }
        })),
        values: bump.alloc_slice_fill_iter(primitives::BUILTINS.iter().map(|builtin| {
            InterfaceValue {
                name: builtin.name,
                annotation: check_annotation(
                    bump,
                    &env,
                    builtin.name,
                    bump.alloc(nash_ast::Annotation {
                        free_vars: builtin.free_vars,
                        typ: builtin.typ,
                        context: builtin.context,
                    }),
                )
                .expect("compiler-owned builtin signatures are well formed"),
            }
        })),
    }
}
