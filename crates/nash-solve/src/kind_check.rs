//! Check the completed inference graph with ordinary Haskell 98 kinds.
//!
//! Keep every expression root: discarded intermediate types must also have
//! kind Type. Predicate-only variables are checked through the predicate store.
use std::collections::{BTreeMap, HashMap};

use bumpalo::Bump;
use nash_can::kinds::{Infer, K, KindEnv, Mismatch};
use nash_constrain::error::{Error, KindProblem};
use nash_constrain::{Content, FlatType, UnionFind, Variable};
use nash_region::Region;

use crate::preds::{Body, Store};

#[derive(Clone, Copy)]
pub(crate) struct Contract<'a> {
    pub variable: Variable,
    /// Scheme or instantiated use that supplied this requirement.
    pub owner: Variable,
    pub kind: &'a nash_ast::Kind<'a>,
    pub region: Region,
}

/// Recorded contracts with an index by the class of their variable.
#[derive(Default)]
pub(crate) struct Contracts<'a> {
    all: Vec<Contract<'a>>,
    /// Positions in `all`, ascending, so a class keeps contract order.
    by_class: HashMap<Variable, Vec<usize>>,
    /// Number of union-find merges already applied to `by_class`.
    merged: usize,
}

/// Contracts whose index agrees with the current union-find classes.
#[derive(Clone, Copy)]
struct Indexed<'c, 'a>(&'c Contracts<'a>);

impl<'a> Contracts<'a> {
    fn indexed(&mut self, uf: &UnionFind<'a>) -> Indexed<'_, 'a> {
        for &(loser, winner) in &uf.merges()[self.merged..] {
            if let Some(moved) = self.by_class.remove(&loser) {
                let kept = self.by_class.entry(winner).or_default();
                kept.extend(moved);
                kept.sort_unstable();
            }
        }
        self.merged = uf.merges().len();
        Indexed(self)
    }

    pub(crate) fn len(&self) -> usize {
        self.all.len()
    }

    pub(crate) fn push(&mut self, uf: &mut UnionFind<'a>, contract: Contract<'a>) {
        self.indexed(uf);
        self.by_class
            .entry(uf.find(contract.variable))
            .or_default()
            .push(self.all.len());
        self.all.push(contract);
    }

    /// Contracts among the first `recorded` on the class of `variable`, oldest first.
    pub(crate) fn on_class(
        &mut self,
        uf: &mut UnionFind<'a>,
        variable: Variable,
        recorded: usize,
    ) -> Vec<Contract<'a>> {
        let class = uf.find(variable);
        self.indexed(uf)
            .on(class)
            .take_while(|(position, _)| *position < recorded)
            .map(|(_, contract)| *contract)
            .collect()
    }
}

impl<'c, 'a> Indexed<'c, 'a> {
    fn on(self, class: Variable) -> impl Iterator<Item = (usize, &'c Contract<'a>)> {
        self.0
            .by_class
            .get(&class)
            .into_iter()
            .flatten()
            .map(move |&position| (position, &self.0.all[position]))
    }
}

pub(crate) struct Failure<'a> {
    pub error: Error<'a>,
    pub variable: Variable,
}

struct Check<'a, 'env> {
    infer: Infer<'a>,
    env: &'env KindEnv<'a>,
    variables: BTreeMap<Variable, &'a K<'a>>,
    contracts: Indexed<'env, 'a>,
    failed: Option<Variable>,
}

impl<'a, 'env> Check<'a, 'env> {
    fn new(bump: &'a Bump, env: &'env KindEnv<'a>, contracts: Indexed<'env, 'a>) -> Self {
        Self {
            infer: Infer::new(bump),
            env,
            variables: BTreeMap::new(),
            contracts,
            failed: None,
        }
    }

    fn variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        variable: Variable,
    ) -> Result<&'a K<'a>, Mismatch<'a>> {
        let result = self.check_variable(uf, variable);
        if result.is_err() {
            self.failed.get_or_insert(variable);
        }
        result
    }

    fn check_variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        variable: Variable,
    ) -> Result<&'a K<'a>, Mismatch<'a>> {
        let variable = uf.find(variable);
        if let Some(kind) = self.variables.get(&variable) {
            return Ok(*kind);
        }
        let kind = self.infer.fresh();
        self.variables.insert(variable, kind);
        // Poison does not change inside one check.
        for (_, contract) in self.contracts.on(variable) {
            if !crate::recovery::is_poisoned(uf, [contract.owner]) {
                self.infer
                    .unify(self.infer.from_kind(contract.kind), kind)?;
            }
        }
        let actual = match uf.get(variable).content.clone() {
            Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => return Ok(kind),
            Content::PartialAlias {
                home, name, args, ..
            } => self.named(
                uf,
                nash_ast::QualifiedName { home, name },
                args.into_iter().map(|(_, v)| v),
            )?,
            Content::Alias {
                home,
                name,
                args,
                real,
                ..
            } => {
                let named = self.named(
                    uf,
                    nash_ast::QualifiedName { home, name },
                    args.into_iter().map(|(_, v)| v),
                )?;
                let real = self.variable(uf, real)?;
                self.infer.unify(named, real)?;
                named
            }
            Content::Structure(flat) => match flat {
                FlatType::App1(home, name, args) => {
                    self.named(uf, nash_ast::QualifiedName { home, name }, args.into_iter())?
                }
                FlatType::AppV1(head, args) => {
                    let head = self.variable(uf, head)?;
                    let args = args
                        .into_iter()
                        .map(|v| self.variable(uf, v))
                        .collect::<Result<Vec<_>, _>>()?;
                    self.infer.apply(head, &args)?
                }
                FlatType::Fun1(a, b) => {
                    self.values(uf, [a, b])?;
                    &K::Type
                }
                FlatType::Tuple1(a, b, rest) => {
                    self.values(uf, [a, b].into_iter().chain(rest))?;
                    &K::Type
                }
                FlatType::Record1(fields) => {
                    self.values(uf, fields.into_values())?;
                    &K::Type
                }
            },
        };
        self.infer.unify(kind, actual)?;
        Ok(kind)
    }

    fn named(
        &mut self,
        uf: &mut UnionFind<'a>,
        name: nash_ast::QualifiedName<'a>,
        args: impl Iterator<Item = Variable>,
    ) -> Result<&'a K<'a>, Mismatch<'a>> {
        let head = self.infer.from_kind(self.env.constructor(name).kind());
        let args = args
            .map(|v| self.variable(uf, v))
            .collect::<Result<Vec<_>, _>>()?;
        self.infer.apply(head, &args)
    }

    fn values(
        &mut self,
        uf: &mut UnionFind<'a>,
        variables: impl IntoIterator<Item = Variable>,
    ) -> Result<(), Mismatch<'a>> {
        for variable in variables {
            let kind = self.variable(uf, variable)?;
            self.infer.unify(&K::Type, kind).inspect_err(|_| {
                self.failed.get_or_insert(variable);
            })?;
        }
        Ok(())
    }

    fn predicate(&mut self, uf: &mut UnionFind<'a>, body: &Body<'a>) -> Result<(), Mismatch<'a>> {
        match body {
            Body::Trait { trait_, args, .. } => {
                let kinds = self
                    .env
                    .traits
                    .get(trait_)
                    .expect("canonical trait kind metadata");
                assert_eq!(kinds.len(), args.len());
                for (kind, variable) in kinds.iter().zip(args) {
                    let actual = self.variable(uf, *variable)?;
                    self.infer
                        .unify(self.infer.from_kind(kind), actual)
                        .inspect_err(|_| {
                            self.failed.get_or_insert(*variable);
                        })?;
                }
            }
            Body::Apply { head, args } => {
                let head_variable = *head;
                let head = self.variable(uf, *head)?;
                let args = args
                    .iter()
                    .map(|v| self.variable(uf, *v))
                    .collect::<Result<Vec<_>, _>>()?;
                // Partial applications may still have an arrow result kind.
                self.infer.apply(head, &args).inspect_err(|_| {
                    self.failed.get_or_insert(head_variable);
                })?;
            }
        }
        Ok(())
    }

    fn error(&mut self, region: Region, mismatch: Mismatch<'a>) -> Error<'a> {
        let reason = match mismatch {
            Mismatch::Infinite => KindProblem::Infinite,
            Mismatch::Shapes { expected, actual } => KindProblem::Mismatch {
                expected: self.infer.default_and_zonk(expected),
                actual: self.infer.default_and_zonk(actual),
            },
        };
        Error::BadKind {
            region,
            name: "type application",
            args: &[],
            reason,
        }
    }
}

pub(crate) fn check<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    env: &KindEnv<'a>,
    roots: &[(Variable, Region)],
    predicates: &Store<'a>,
    contracts: &mut Contracts<'a>,
    dependencies: &mut crate::recovery::Dependencies,
) -> Vec<Error<'a>> {
    // Restart after each failure: kind assignments made by a failed check must
    // not affect another diagnostic. Poison the failing nested type, then let
    // directed containment skip its parents while retaining sibling roots.
    let mut errors = Vec::new();
    let contracts = contracts.indexed(uf);
    loop {
        let mut check = Check::new(bump, env, contracts);
        let result = (|| {
            for contract in &contracts.0.all {
                if !crate::recovery::is_poisoned(uf, [contract.owner, contract.variable]) {
                    check
                        .variable(uf, contract.variable)
                        .map_err(|mismatch| (contract.region, mismatch))?;
                }
            }
            for &(variable, region) in roots {
                if !crate::recovery::is_poisoned(uf, [variable]) {
                    check
                        .values(uf, [variable])
                        .map_err(|mismatch| (region, mismatch))?;
                }
            }
            for (index, predicate) in predicates.iter().enumerate() {
                if !crate::recovery::is_poisoned(uf, predicate.body.roots()) {
                    let id = nash_constrain::type_::PredId(index as u32);
                    let region = predicates
                        .use_site(id)
                        .map_or(Region::zero(), |site| site.region);
                    check
                        .predicate(uf, &predicate.body)
                        .map_err(|mismatch| (region, mismatch))?;
                }
            }
            Ok(())
        })();
        let Err((region, mismatch)) = result else {
            break;
        };
        errors.push(check.error(region, mismatch));
        dependencies.invalidate(
            uf,
            [check
                .failed
                .expect("kind failure identifies a type variable")],
        );
        while dependencies.propagate(uf) {}
    }
    errors
}

/// Freeze only quantified variables. Captured type variables remain shared.
pub(crate) fn freeze<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    env: &KindEnv<'a>,
    root: nash_region::Located<Variable>,
    predicates: &[Body<'a>],
    quantified: &[Variable],
    contracts: &mut Contracts<'a>,
) -> Result<Vec<Contract<'a>>, Box<Failure<'a>>> {
    let region = root.region;
    let contracts = contracts.indexed(uf);
    let mut check = Check::new(bump, env, contracts);
    let result = (|| {
        check.values(uf, [root.value])?;
        for body in predicates {
            check.predicate(uf, body)?;
        }
        let mut result = Vec::new();
        for &variable in quantified {
            let kind = check.variable(uf, variable)?;
            result.push(Contract {
                variable,
                owner: root.value,
                kind: check.infer.default_and_zonk(kind),
                region,
            });
        }
        Ok(result)
    })();
    result.map_err(|mismatch| {
        Box::new(Failure {
            variable: check
                .failed
                .expect("kind failure identifies a type variable"),
            error: check.error(region, mismatch),
        })
    })
}
