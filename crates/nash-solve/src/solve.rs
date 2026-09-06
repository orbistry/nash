//! Port of Elm's `Type.Solve`: solve a constraint tree with rank-based
//! generalization, producing an annotation per top-level value.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bumpalo::Bump;
use nash_ast::Type as CanType;
use nash_can::Annotations;
use nash_constrain::error::{Category, Error, Expected, PExpected};
use nash_constrain::type_::{
    self, Constraint, Content, Descriptor, FlatType, Mark, NO_MARK, NO_RANK, OUTERMOST_RANK, Type,
};
use nash_constrain::{UnionFind, Variable};
use nash_region::Located;

use crate::annotation::to_error_type;
use crate::occurs;
use crate::preds::{Origin, Predicate, Store, UseSite};
use crate::unify;

// RUN SOLVER

pub fn run<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    constraint: &Constraint<'a>,
    tables: &nash_can::environment::Tables<'a>,
) -> Result<(Annotations<'a>, crate::SolvedTypes<'a>), Vec<Error<'a>>> {
    let mut solver = Solver {
        bump,
        tables,
        pools: vec![Vec::new(); 8],
        copied: Vec::new(),
        predicates: Store::default(),
        wanted: Vec::new(),
        givens: Vec::new(),
        schemes: Vec::new(),
        recursive_uses: Vec::new(),
        uses: Vec::new(),
        owners: Vec::new(),
        resolution_work: std::collections::HashMap::new(),
    };

    let state = solver.solve(
        uf,
        &Env::new(),
        OUTERMOST_RANK,
        State {
            env: Env::new(),
            mark: NO_MARK.next(),
            errors: Vec::new(),
        },
        constraint,
    );

    if state.errors.is_empty() {
        solver.finish(uf, &state.env)
    } else {
        // Elm accumulates errors by prepending; match its final order.
        let mut errors = state.errors;
        errors.reverse();
        Err(errors)
    }
}

// SOLVER

#[derive(Clone, Copy)]
struct Binding<'a> {
    variable: Variable,
    context: &'a [type_::PredId],
    definition: Option<nash_ast::NodeId>,
    context_is_final: bool,
    /// Early recursive annotations own these variables even while checking
    /// their bodies temporarily changes the variables' ranks.
    declared_quantifiers: &'a [Variable],
}

struct SchemeRecord<'a> {
    site: type_::Binder<'a>,
    binding: Binding<'a>,
    /// Captures may become generalized later in an enclosing definition.
    /// Freeze the variables owned by this scheme at its own boundary.
    quantified: Vec<Variable>,
    binder: nash_ast::NodeId,
    parent: Option<nash_ast::NodeId>,
}

enum UseSource {
    Local {
        definition: nash_ast::NodeId,
        copies: Vec<(Variable, Variable)>,
    },
    Foreign {
        variables: Vec<Variable>,
    },
}

struct UseRecord<'a> {
    site: UseSite<'a>,
    owner: Option<nash_ast::NodeId>,
    source: UseSource,
    predicates: Vec<type_::PredId>,
}

type Env<'a> = BTreeMap<&'a str, Binding<'a>>;

struct State<'a> {
    env: Env<'a>,
    mark: Mark,
    errors: Vec<Error<'a>>,
}

fn add_error<'a>(mut state: State<'a>, error: Error<'a>) -> State<'a> {
    state.errors.push(error);
    state
}

struct Solver<'a, 'tables> {
    bump: &'a Bump,
    tables: &'tables nash_can::environment::Tables<'a>,
    pools: Vec<Vec<Variable>>,
    copied: Vec<(Variable, Variable)>,
    predicates: Store<'a>,
    wanted: Vec<(usize, type_::PredId)>,
    givens: Vec<GivenFrame<'a>>,
    schemes: Vec<SchemeRecord<'a>>,
    recursive_uses: Vec<usize>,
    uses: Vec<UseRecord<'a>>,
    owners: Vec<nash_ast::NodeId>,
    resolution_work: std::collections::HashMap<nash_ast::NodeId, usize>,
}

struct GivenFrame<'a> {
    binder: nash_ast::NodeId,
    predicates: Vec<Given<'a>>,
}

#[derive(Clone)]
struct Given<'a> {
    trait_: nash_ast::QualifiedName<'a>,
    args: Vec<Variable>,
    index: usize,
    path: Vec<usize>,
}

impl<'a> Solver<'a, '_> {
    fn scope(&self, mut owner: nash_ast::NodeId) -> (nash_ast::NodeId, usize) {
        let mut depth = 0;
        loop {
            let scheme = self
                .schemes
                .iter()
                .find(|scheme| scheme.site.node() == owner)
                .expect("body owner has a recorded scheme");
            match scheme.parent {
                Some(parent) => {
                    owner = parent;
                    depth += 1;
                }
                None => return (scheme.binder, depth),
            }
        }
    }

    fn use_variables(
        &self,
        uf: &mut UnionFind<'a>,
        use_: &UseRecord<'a>,
        quantified: &[Variable],
    ) -> Vec<Variable> {
        match &use_.source {
            UseSource::Foreign { variables } => variables.clone(),
            UseSource::Local { copies, .. } => quantified
                .iter()
                .map(|var| {
                    if copies.is_empty() {
                        return *var;
                    }
                    copies
                        .iter()
                        .find_map(|(original, copy)| {
                            uf.equivalent(*original, *var).then_some(*copy)
                        })
                        .expect("scheme quantifier was copied with its type and context")
                })
                .collect(),
        }
    }

    /// Track evidence arguments through local calls, including helpers which
    /// introduce their own givens. A closed proof contributes no dependency.
    fn growing_evidence(&self) -> BTreeSet<type_::PredId> {
        use crate::preds::Solution;
        use std::collections::{HashMap, HashSet};
        let binders: HashMap<_, _> = self
            .schemes
            .iter()
            .map(|scheme| (scheme.site.node(), scheme.binder))
            .collect();
        let mut edges: HashMap<_, Vec<_>> = HashMap::new();
        let mut wrapped = Vec::new();
        for use_ in &self.uses {
            let UseSource::Local { definition, .. } = use_.source else {
                continue;
            };
            let binder = binders[&definition];
            for (index, root) in use_.predicates.iter().enumerate() {
                let target = (binder, index);
                let mut pending = vec![(*root, false)];
                let mut seen = BTreeSet::new();
                while let Some((id, under_impl)) = pending.pop() {
                    if !seen.insert((id, under_impl)) {
                        continue;
                    }
                    match &self.predicates.get(id).solution {
                        Some(Solution::Impl { subs, .. }) => {
                            pending.extend(subs.iter().map(|id| (*id, true)))
                        }
                        Some(
                            Solution::Given { binder, index }
                            | Solution::Super { binder, index, .. },
                        ) => {
                            let source = (*binder, *index);
                            edges.entry(target).or_default().push(source);
                            if under_impl {
                                wrapped.push((target, source, *root));
                            }
                        }
                        None => {}
                    }
                }
            }
        }
        let mut growing = BTreeSet::new();
        for (target, source, root) in wrapped {
            let mut pending = vec![source];
            let mut seen = HashSet::new();
            while let Some(slot) = pending.pop() {
                if slot == target {
                    growing.insert(root);
                    break;
                }
                if seen.insert(slot) {
                    pending.extend(edges.get(&slot).into_iter().flatten().copied());
                }
            }
        }
        growing
    }

    fn finish(
        &self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
    ) -> Result<(Annotations<'a>, crate::SolvedTypes<'a>), Vec<Error<'a>>> {
        use crate::solved::{Instance, Scheme, SolvedTypes};
        use std::collections::HashMap;
        let growing = self.growing_evidence();
        let mut errors = Vec::new();
        for use_ in &self.uses {
            for root in &use_.predicates {
                if growing.contains(root) {
                    let pred = self.predicates.get(*root);
                    let args: Vec<_> = pred
                        .args
                        .iter()
                        .map(|var| to_error_type(self.bump, uf, *var))
                        .collect();
                    errors.push(Error::PolymorphicRecursion {
                        region: use_.site.region,
                        name: use_.site.name,
                        trait_: pred.trait_,
                        args: self.bump.alloc_slice_copy(&args),
                    });
                }
            }
            let mut pending = use_.predicates.clone();
            let mut seen = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let pred = self.predicates.get(id);
                match &pred.solution {
                    Some(crate::preds::Solution::Impl { subs, .. }) => pending.extend(subs),
                    Some(_) => {}
                    None => {
                        let args = pred
                            .args
                            .iter()
                            .map(|var| to_error_type(self.bump, uf, *var))
                            .collect::<Vec<_>>();
                        errors.push(Error::UnresolvedConstraint {
                            region: use_.site.region,
                            name: use_.site.name,
                            trait_: pred.trait_,
                            args: self.bump.alloc_slice_copy(&args),
                        });
                    }
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut solved = SolvedTypes::default();
        let mut orders = HashMap::new();
        let mut rendered_uses = HashMap::new();
        let mut scopes = Vec::new();
        for scheme in &self.schemes {
            let root = self.scope(scheme.site.node()).0;
            if !scopes.contains(&root) {
                scopes.push(root);
            }
        }
        // Quantifier order must be frozen with its rendered scheme, before
        // another body can assign names to its own instantiations.
        for scope in &scopes {
            let mut members: Vec<_> = self
                .schemes
                .iter()
                .filter(|scheme| self.scope(scheme.site.node()).0 == *scope)
                .collect();
            members.sort_by_key(|scheme| self.scope(scheme.site.node()).1);
            let mut roots = Vec::new();
            for scheme in &members {
                roots.push(scheme.binding.variable);
                for id in scheme.binding.context {
                    roots.extend(&self.predicates.get(*id).args);
                }
            }
            for use_ in &self.uses {
                if self
                    .scope(use_.owner.expect("use belongs to a definition"))
                    .0
                    != *scope
                {
                    continue;
                }
                let quantified = match use_.source {
                    UseSource::Local { definition, .. } => self
                        .schemes
                        .iter()
                        .find(|scheme| scheme.site.node() == definition)
                        .unwrap()
                        .quantified
                        .as_slice(),
                    UseSource::Foreign { .. } => &[],
                };
                roots.extend(self.use_variables(uf, use_, quantified));
                let mut pending = use_.predicates.clone();
                while let Some(id) = pending.pop() {
                    if let Some(crate::preds::Solution::Impl {
                        type_vars, subs, ..
                    }) = &self.predicates.get(id).solution
                    {
                        roots.extend(type_vars);
                        pending.extend(subs);
                    }
                }
            }
            crate::annotation::prepare_scope(self.bump, uf, &roots);
            for scheme in members {
                let id = scheme.site.node();
                let context: Vec<_> = scheme
                    .binding
                    .context
                    .iter()
                    .map(|id| {
                        let pred = self.predicates.get(*id);
                        (pred.trait_, pred.args.as_slice())
                    })
                    .collect();
                let annotation = crate::annotation::to_scheme_annotation(
                    self.bump,
                    uf,
                    scheme.binding.variable,
                    &context,
                    &scheme.quantified,
                );
                orders.insert(
                    id,
                    crate::annotation::ordered_quantifiers(uf, &scheme.quantified),
                );
                solved.schemes.insert(
                    id,
                    Scheme {
                        annotation,
                        binder: scheme.binder,
                    },
                );
            }
            for use_ in &self.uses {
                if self
                    .scope(use_.owner.expect("use belongs to a definition"))
                    .0
                    != *scope
                {
                    continue;
                }
                let keys = match &use_.source {
                    UseSource::Local { definition, .. } => self
                        .schemes
                        .iter()
                        .find(|scheme| scheme.site.node() == *definition)
                        .unwrap()
                        .quantified
                        .clone(),
                    UseSource::Foreign { variables } => variables.clone(),
                };
                let variables = self.use_variables(uf, use_, &keys);
                let types: Vec<_> = keys
                    .into_iter()
                    .zip(variables)
                    .map(|(key, var)| (key, crate::annotation::to_solved_type(self.bump, uf, var)))
                    .collect();
                let evidence = &*self
                    .bump
                    .alloc_slice_fill_iter(use_.predicates.iter().map(|id| self.evidence(uf, *id)));
                assert!(
                    rendered_uses
                        .insert(use_.site.node, (types, evidence))
                        .is_none(),
                    "one instance per original use node"
                );
            }
        }
        // Callees in other scopes may have been rendered later. Reorder the
        // already-rendered caller types using the callee's frozen identities.
        for use_ in &self.uses {
            let (types, evidence) = rendered_uses.remove(&use_.site.node).unwrap();
            let type_args: Vec<_> = match use_.source {
                UseSource::Local { definition, .. } => orders[&definition]
                    .iter()
                    .map(|var| {
                        types
                            .iter()
                            .find(|(key, _)| key == var)
                            .expect("rendered quantifier argument")
                            .1
                    })
                    .collect(),
                UseSource::Foreign { .. } => types.into_iter().map(|(_, typ)| typ).collect(),
            };
            solved.instances.insert(
                use_.site.node,
                Instance {
                    type_args: self.bump.alloc_slice_fill_iter(type_args),
                    evidence,
                },
            );
        }
        let annotations = env
            .iter()
            .map(|(name, binding)| {
                (
                    *name,
                    solved.schemes[&binding.definition.expect("exported definition")].annotation,
                )
            })
            .collect();
        Ok((annotations, solved))
    }

    fn evidence(&self, uf: &mut UnionFind<'a>, id: type_::PredId) -> nash_ast::Evidence<'a> {
        use crate::preds::Solution;
        use nash_ast::Evidence;
        match self
            .predicates
            .get(id)
            .solution
            .as_ref()
            .expect("validated use evidence")
        {
            Solution::Given { binder, index } => Evidence::Given {
                binder: *binder,
                index: u16::try_from(*index).expect("context slot fits evidence index"),
            },
            Solution::Super {
                binder,
                index,
                path,
            } => {
                let mut evidence = Evidence::Given {
                    binder: *binder,
                    index: u16::try_from(*index).expect("context slot fits evidence index"),
                };
                for index in path {
                    evidence = Evidence::Super {
                        of: self.bump.alloc(evidence),
                        index: u16::try_from(*index).expect("superclass slot fits evidence index"),
                    };
                }
                evidence
            }
            Solution::Impl {
                impl_,
                type_vars,
                subs,
            } => Evidence::Impl {
                impl_: *impl_,
                type_args: self.bump.alloc_slice_fill_iter(
                    type_vars
                        .iter()
                        .map(|var| crate::annotation::to_solved_type(self.bump, uf, *var)),
                ),
                args: self
                    .bump
                    .alloc_slice_fill_iter(subs.iter().map(|id| self.evidence(uf, *id))),
            },
        }
    }

    fn expand_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        predicates: &mut Vec<Given<'a>>,
    ) {
        // Canonicalization rejects superclass cycles. Breadth-first
        // expansion keeps explicit givens first and chooses short paths.
        let mut cursor = 0;
        while cursor < predicates.len() {
            let current = predicates[cursor].clone();
            cursor += 1;
            let Some(info) = self.tables.traits.get(&current.trait_).copied() else {
                continue;
            };
            let vars = info
                .parameters
                .iter()
                .copied()
                .zip(current.args.iter().copied())
                .collect();
            for (index, superclass) in info.supers.iter().enumerate() {
                let args: Vec<_> = superclass
                    .args
                    .iter()
                    .map(|arg| self.src_type_to_var(uf, rank, &vars, arg))
                    .collect();
                if predicates.iter().any(|existing| {
                    existing.trait_ == superclass.trait_
                        && crate::preds::same_args(uf, &existing.args, &args)
                }) {
                    continue;
                }
                let mut path = current.path.clone();
                path.push(index);
                predicates.push(Given {
                    trait_: superclass.trait_,
                    args,
                    index: current.index,
                    path,
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn solve_header(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        constraint: &Constraint<'a>,
        given: &[type_::Pred<'a>],
        binder: Option<type_::Binder<'a>>,
        annotated: bool,
    ) -> State<'a> {
        let depth = self.enter_givens(uf, rank, given, binder);
        let start = self.wanted.len();
        let owner_depth = self.owners.len();
        if let Some(binder) = binder {
            self.owners.push(binder.node());
        }
        let mut state = self.solve(uf, env, rank, state, constraint);
        state = self.resolve_wanted(uf, rank, state, start, binder, annotated);
        self.givens.truncate(depth);
        self.owners.truncate(owner_depth);
        state
    }

    fn enter_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        given: &[type_::Pred<'a>],
        binder: Option<type_::Binder<'a>>,
    ) -> usize {
        let depth = self.givens.len();
        if let Some(binder) = binder.filter(|_| !given.is_empty()) {
            let mut predicates: Vec<Given<'a>> = given
                .iter()
                .enumerate()
                .map(|(index, pred)| Given {
                    trait_: pred.trait_,
                    args: pred
                        .args
                        .iter()
                        .map(|arg| self.type_to_variable(uf, rank, arg))
                        .collect(),
                    index,
                    path: Vec::new(),
                })
                .collect();
            self.expand_givens(uf, rank, &mut predicates);
            self.givens.push(GivenFrame {
                binder: binder.node(),
                predicates,
            });
        }
        depth
    }

    fn resolve_wanted(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        mut state: State<'a>,
        start: usize,
        binder: Option<type_::Binder<'a>>,
        annotated: bool,
    ) -> State<'a> {
        let report_errors = state.errors.is_empty();
        let report_missing = annotated && report_errors;
        let mut queue: VecDeque<_> = self
            .wanted
            .split_off(start)
            .into_iter()
            .map(|(rank, id)| (rank, id, self.predicates.depth(id)))
            .collect();
        while let Some((wanted_rank, id, resolution_depth)) = queue.pop_front() {
            let wanted = self.predicates.get(id);
            let solution = self.givens.iter().rev().find_map(|frame| {
                frame.predicates.iter().find_map(|given| {
                    (given.trait_ == wanted.trait_
                        && crate::preds::same_args(uf, &given.args, &wanted.args))
                    .then_some((frame.binder, given.index, given.path.clone()))
                })
            });
            if let Some((binder, index, path)) = solution {
                self.predicates.solve_given(uf, id, binder, index, path);
            } else if binder.is_none()
                || (!self.givens.is_empty()
                    && crate::resolve::has_outer_flex(uf, &wanted.args, rank))
            {
                // Synthetic existential scopes are not definition boundaries.
                // Finish surrounding equalities before choosing an impl, and
                // let enclosing givens see captured variables at their final type.
                self.wanted.push((wanted_rank, id));
            } else if report_missing
                && let Some(binder) = binder
                && let Some(site) = self.predicates.use_site(id)
                // Constructor-headed impls cannot discharge a bare rigid head.
                // Core Lift also has a compiler rule; its Big proof belongs
                // to kind-aware resolution, so leave that requirement pending.
                && !(wanted.trait_ == nash_ast::primitives::lift_trait()
                    && self.tables.has_reflexive_lift())
                && wanted.args.iter().any(|arg| {
                    matches!(uf.get(*arg).content, Content::RigidVar(_))
                })
            {
                let args: Vec<_> = wanted
                    .args
                    .iter()
                    .map(|arg| to_error_type(self.bump, uf, *arg))
                    .collect();
                state.errors.push(Error::MissingConstraint {
                    region: site.region,
                    name: site.name,
                    trait_: wanted.trait_,
                    args: self.bump.alloc_slice_copy(&args),
                    binder: binder.name(),
                });
            } else if report_errors
                && let Some(binder) = binder
                && !(wanted.trait_ == nash_ast::primitives::lift_trait()
                    && self.tables.has_reflexive_lift())
            {
                let site = self
                    .predicates
                    .use_site(id)
                    .expect("wanteds originate at uses");
                let work = self.resolution_work.entry(binder.node()).or_default();
                *work += 1;
                if *work > 16_384 || resolution_depth >= 128 {
                    state.errors.push(Error::ImplResolutionLimit {
                        region: site.region,
                        name: site.name,
                        trait_: wanted.trait_,
                    });
                    break;
                }
                match crate::resolve::select(
                    self.bump,
                    self.tables,
                    uf,
                    wanted.trait_,
                    &wanted.args,
                ) {
                    crate::resolve::Selection::Deferred => self.wanted.push((wanted_rank, id)),
                    crate::resolve::Selection::Missing => {
                        let args: Vec<_> = wanted
                            .args
                            .iter()
                            .map(|arg| to_error_type(self.bump, uf, *arg))
                            .collect();
                        let available: Vec<_> = self
                            .tables
                            .impls
                            .keys()
                            .filter(|key| key.trait_ == wanted.trait_)
                            .map(|key| key.heads)
                            .collect();
                        state.errors.push(Error::MissingImpl {
                            region: site.region,
                            name: site.name,
                            trait_: wanted.trait_,
                            args: self.bump.alloc_slice_copy(&args),
                            available: self.bump.alloc_slice_copy(&available),
                        });
                    }
                    crate::resolve::Selection::Impl {
                        info,
                        key,
                        substitution,
                    } => {
                        let type_vars = substitution.iter().map(|(_, var)| *var).collect();
                        let vars = substitution.into_iter().collect();
                        let mut subs = Vec::new();
                        for (index, context) in info.context.iter().enumerate() {
                            let args = context
                                .args
                                .iter()
                                .map(|arg| self.src_type_to_var(uf, rank, &vars, arg))
                                .collect();
                            let sub = self.predicates.push(
                                uf,
                                Predicate {
                                    trait_: context.trait_,
                                    args,
                                    origin: Origin::Sub { parent: id, index },
                                    solution: None,
                                },
                            );
                            subs.push(sub);
                            queue.push_back((wanted_rank, sub, resolution_depth + 1));
                        }
                        self.predicates.solve(
                            uf,
                            id,
                            crate::preds::Solution::Impl {
                                impl_: nash_ast::ImplRef {
                                    home: info.home,
                                    key,
                                },
                                type_vars,
                                subs,
                            },
                        );
                    }
                }
            } else {
                self.wanted.push((wanted_rank, id));
            }
        }
        state
    }

    fn solve(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        constraint: &Constraint<'a>,
    ) -> State<'a> {
        match constraint {
            Constraint::True => state,

            Constraint::SaveTheEnvironment => State {
                env: env.clone(),
                ..state
            },

            Constraint::Equal(region, category, tipe, expectation) => {
                let actual = self.type_to_variable(uf, rank, tipe);
                let expected = self.expected_to_variable(uf, rank, expectation);
                match unify::unify(self.bump, uf, actual, expected) {
                    unify::Answer::Ok(vars) => {
                        self.introduce(uf, rank, &vars);
                        state
                    }
                    unify::Answer::Err(vars, actual_type, expected_type) => {
                        self.introduce(uf, rank, &vars);
                        add_error(
                            state,
                            Error::BadExpr(
                                *region,
                                *category,
                                actual_type,
                                expectation.type_replace(expected_type),
                            ),
                        )
                    }
                }
            }

            Constraint::Local(region, node, name, expectation) => {
                let binding = *env
                    .get(name)
                    .expect("constraint generator only references bound locals");
                let actual = self.instantiate_binding(
                    uf,
                    rank,
                    binding,
                    UseSite {
                        node: *node,
                        region: *region,
                        name,
                    },
                );
                let expected = self.expected_to_variable(uf, rank, expectation);
                match unify::unify(self.bump, uf, actual, expected) {
                    unify::Answer::Ok(vars) => {
                        self.introduce(uf, rank, &vars);
                        state
                    }
                    unify::Answer::Err(vars, actual_type, expected_type) => {
                        self.introduce(uf, rank, &vars);
                        add_error(
                            state,
                            Error::BadExpr(
                                *region,
                                Category::Local(name),
                                actual_type,
                                expectation.type_replace(expected_type),
                            ),
                        )
                    }
                }
            }

            Constraint::Foreign(region, node, name, annotation, expectation) => {
                let actual = self.src_type_to_variable(
                    uf,
                    rank,
                    UseSite {
                        node: *node,
                        region: *region,
                        name,
                    },
                    annotation,
                );
                let expected = self.expected_to_variable(uf, rank, expectation);
                match unify::unify(self.bump, uf, actual, expected) {
                    unify::Answer::Ok(vars) => {
                        self.introduce(uf, rank, &vars);
                        state
                    }
                    unify::Answer::Err(vars, actual_type, expected_type) => {
                        self.introduce(uf, rank, &vars);
                        add_error(
                            state,
                            Error::BadExpr(
                                *region,
                                Category::Foreign(name),
                                actual_type,
                                expectation.type_replace(expected_type),
                            ),
                        )
                    }
                }
            }

            Constraint::Pattern(region, category, tipe, expectation) => {
                let actual = self.type_to_variable(uf, rank, tipe);
                let expected = self.pattern_expectation_to_variable(uf, rank, expectation);
                match unify::unify(self.bump, uf, actual, expected) {
                    unify::Answer::Ok(vars) => {
                        self.introduce(uf, rank, &vars);
                        state
                    }
                    unify::Answer::Err(vars, actual_type, expected_type) => {
                        self.introduce(uf, rank, &vars);
                        add_error(
                            state,
                            Error::BadPattern(
                                *region,
                                *category,
                                actual_type,
                                expectation.type_replace(expected_type),
                            ),
                        )
                    }
                }
            }

            Constraint::And(constraints) => constraints
                .iter()
                .fold(state, |state, sub| self.solve(uf, env, rank, state, sub)),

            Constraint::Let {
                declarations,
                given,
                binder,
                definitions,
                rigid_vars,
                flex_vars,
                header,
                header_con,
                body_con,
            } => {
                let wanted_start = self.wanted.len();
                let annotated = definitions.iter().any(|def| def.context.is_some());
                if definitions.is_empty()
                    && rigid_vars.is_empty()
                    && matches!(body_con, Constraint::True)
                {
                    self.introduce(uf, rank, flex_vars);
                    let declared = self.declared_contexts(uf, rank, definitions, declarations);
                    let state1 = self
                        .solve_header(uf, env, rank, state, header_con, given, *binder, annotated);
                    self.record_definitions(uf, rank, definitions, &declared, &[], *binder);
                    state1
                } else if definitions.is_empty() && rigid_vars.is_empty() && flex_vars.is_empty() {
                    let declared = self.declared_contexts(uf, rank, definitions, declarations);
                    let state1 = self
                        .solve_header(uf, env, rank, state, header_con, given, *binder, annotated);
                    self.record_definitions(uf, rank, definitions, &declared, &[], *binder);
                    let locals: Vec<(&'a str, Located<Variable>)> = header
                        .iter()
                        .map(|(name, loc_type)| {
                            let var = self.type_to_variable(uf, rank, loc_type.value);
                            (*name, Located::at(loc_type.region, var))
                        })
                        .collect();
                    let mut new_env = env.clone();
                    for (name, loc) in &locals {
                        new_env.entry(name).or_insert(Binding {
                            declared_quantifiers: &[],
                            variable: loc.value,
                            context: declared.get(name).copied().unwrap_or(&[]),
                            context_is_final: !declarations
                                .iter()
                                .any(|def| def.site.name().value == *name && def.context.is_none()),
                            definition: definitions
                                .iter()
                                .chain(declarations.iter())
                                .find(|def| def.site.name().value == *name)
                                .map(|def| def.site.node())
                                .or_else(|| {
                                    binder
                                        .filter(|b| matches!(b, type_::Binder::Pattern { .. }))
                                        .map(type_::Binder::node)
                                }),
                        });
                    }
                    let state2 = self.solve(uf, &new_env, rank, state1, body_con);
                    locals.into_iter().fold(state2, |state, (name, loc)| {
                        self.check_occurs(uf, state, name, loc)
                    })
                } else {
                    // work in the next pool to localize header
                    let next_rank = rank + 1;
                    if next_rank >= self.pools.len() {
                        let pools_length = self.pools.len();
                        self.pools.resize(pools_length * 2, Vec::new());
                    }

                    // introduce variables
                    let vars: Vec<Variable> =
                        rigid_vars.iter().chain(flex_vars.iter()).copied().collect();
                    for var in &vars {
                        uf.modify(*var, |desc| desc.rank = next_rank);
                    }
                    self.pools[next_rank] = vars;

                    // run solver in next pool
                    let locals: Vec<(&'a str, Located<Variable>)> = header
                        .iter()
                        .map(|(name, loc_type)| {
                            let var = self.type_to_variable(uf, next_rank, loc_type.value);
                            (*name, Located::at(loc_type.region, var))
                        })
                        .collect();
                    let declared = self.declared_contexts(uf, next_rank, definitions, declarations);
                    let mut state1 = self.solve_header(
                        uf, env, next_rank, state, header_con, given, *binder, annotated,
                    );

                    let young_mark = state1.mark;
                    let visit_mark = young_mark.next();
                    let final_mark = visit_mark.next();

                    // pop pool
                    self.generalize(uf, young_mark, visit_mark, next_rank);
                    self.pools[next_rank] = Vec::new();

                    // check that things went well
                    if state1.errors.is_empty() {
                        for rigid in rigid_vars.iter() {
                            if uf.get(*rigid).rank != NO_RANK {
                                let owner = binder
                                    .map(|name| (name.name().region, name.name().value))
                                    .or_else(|| {
                                        header.first().map(|(name, typ)| (typ.region, *name))
                                    });
                                state1.errors.push(Error::AnnotationVariableEscapes {
                                    region: owner
                                        .map_or_else(nash_region::Region::zero, |(region, _)| {
                                            region
                                        }),
                                    name: owner.map(|(_, name)| name),
                                    variable: to_error_type(self.bump, uf, *rigid),
                                });
                            }
                        }
                    }

                    if state1.errors.is_empty()
                        && let Some(binder) = *binder
                    {
                        let depth = self.enter_givens(uf, rank, given, Some(binder));
                        loop {
                            let (errors, defaulted) = self.check_ambiguity(
                                uf,
                                rank,
                                wanted_start,
                                definitions,
                                binder.name(),
                            );
                            state1.errors.extend(errors);
                            if !defaulted || !state1.errors.is_empty() {
                                break;
                            }
                            state1 = self.resolve_wanted(
                                uf,
                                next_rank,
                                state1,
                                wanted_start,
                                Some(binder),
                                annotated,
                            );
                            if !state1.errors.is_empty() {
                                break;
                            }
                        }
                        self.givens.truncate(depth);
                    }
                    let context = if !definitions.is_empty()
                        && definitions.iter().all(|def| def.context.is_none())
                    {
                        self.retain_wanted(
                            uf,
                            rank,
                            wanted_start,
                            binder.expect("inferred definition binder").node(),
                        )
                    } else {
                        &[]
                    };

                    let mut new_env = env.clone();
                    self.record_definitions(uf, rank, definitions, &declared, context, *binder);
                    for (name, loc) in &locals {
                        new_env.entry(name).or_insert(Binding {
                            declared_quantifiers: if declarations
                                .iter()
                                .any(|def| def.site.name().value == *name && def.context.is_some())
                            {
                                rigid_vars
                            } else {
                                &[]
                            },
                            variable: loc.value,
                            context: declared.get(name).copied().unwrap_or(context),
                            context_is_final: true,
                            definition: definitions
                                .iter()
                                .chain(declarations.iter())
                                .find(|def| def.site.name().value == *name)
                                .map(|def| def.site.node())
                                .or_else(|| {
                                    binder
                                        .filter(|b| matches!(b, type_::Binder::Pattern { .. }))
                                        .map(type_::Binder::node)
                                }),
                        });
                    }
                    let temp_state = State {
                        env: state1.env,
                        mark: final_mark,
                        errors: state1.errors,
                    };
                    let new_state = self.solve(uf, &new_env, rank, temp_state, body_con);

                    locals.into_iter().fold(new_state, |state, (name, loc)| {
                        self.check_occurs(uf, state, name, loc)
                    })
                }
            }
        }
    }

    // EXPECTATIONS TO VARIABLE

    fn expected_to_variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        expectation: &Expected<'a, &'a Type<'a>>,
    ) -> Variable {
        let tipe = match expectation {
            Expected::NoExpectation(tipe) => tipe,
            Expected::FromContext(_, _, tipe) => tipe,
            Expected::FromAnnotation(_, _, _, tipe) => tipe,
        };
        self.type_to_variable(uf, rank, tipe)
    }

    fn pattern_expectation_to_variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        expectation: &PExpected<'a, &'a Type<'a>>,
    ) -> Variable {
        let tipe = match expectation {
            PExpected::NoExpectation(tipe) => tipe,
            PExpected::FromContext(_, _, tipe) => tipe,
        };
        self.type_to_variable(uf, rank, tipe)
    }

    // OCCURS CHECK

    fn check_occurs(
        &mut self,
        uf: &mut UnionFind<'a>,
        state: State<'a>,
        name: &'a str,
        located_variable: Located<Variable>,
    ) -> State<'a> {
        let variable = located_variable.value;
        if occurs::occurs(uf, variable) {
            let error_type = to_error_type(self.bump, uf, variable);
            uf.modify(variable, |desc| desc.content = Content::Error);
            add_error(
                state,
                Error::InfiniteType {
                    region: located_variable.region,
                    name,
                    overall_type: error_type,
                },
            )
        } else {
            state
        }
    }

    // GENERALIZE

    /// Every variable has rank less than or equal to the maxRank of the
    /// pool. This sorts variables into the young and old pools accordingly.
    fn generalize(
        &mut self,
        uf: &mut UnionFind<'a>,
        young_mark: Mark,
        visit_mark: Mark,
        young_rank: usize,
    ) {
        let young_vars = self.pools[young_rank].clone();
        let rank_table = pool_to_rank_table(uf, young_mark, young_rank, young_vars);

        // get the ranks right for each entry.
        // start at low ranks so that we only have to pass
        // over the information once.
        for (rank, table) in rank_table.iter().enumerate() {
            for var in table {
                adjust_rank(uf, young_mark, visit_mark, rank, *var);
            }
        }

        // For variables that have rank lower than youngRank, register them
        // in the appropriate old pool if they are not redundant.
        for vars in &rank_table[..young_rank] {
            for var in vars {
                if !uf.redundant(*var) {
                    let rank = uf.get(*var).rank;
                    self.pools[rank].push(*var);
                }
            }
        }

        // For variables with rank youngRank
        //   If rank < youngRank: register in oldPool
        //   otherwise generalize
        for var in &rank_table[young_rank] {
            if !uf.redundant(*var) {
                let rank = uf.get(*var).rank;
                if rank < young_rank {
                    self.pools[rank].push(*var);
                } else {
                    uf.modify(*var, |desc| desc.rank = NO_RANK);
                }
            }
        }
    }

    // REGISTER VARIABLES

    fn introduce(&mut self, uf: &mut UnionFind<'a>, rank: usize, variables: &[Variable]) {
        self.pools[rank].extend_from_slice(variables);
        for var in variables {
            uf.modify(*var, |desc| desc.rank = rank);
        }
    }

    // TYPE TO VARIABLE

    fn type_to_variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        tipe: &Type<'a>,
    ) -> Variable {
        match tipe {
            Type::PartialAliasN {
                home,
                name,
                args,
                remaining,
                body,
            } => {
                let args = args
                    .iter()
                    .map(|(name, typ)| (*name, self.type_to_variable(uf, rank, typ)))
                    .collect();
                self.register(
                    uf,
                    rank,
                    Content::PartialAlias {
                        home: *home,
                        name,
                        args,
                        remaining: remaining.to_vec(),
                        body,
                    },
                )
            }
            Type::VarN(var) => *var,

            Type::AppVarN(head, args) => {
                let head = self.type_to_variable(uf, rank, head);
                let args = args
                    .iter()
                    .map(|arg| self.type_to_variable(uf, rank, arg))
                    .collect();
                self.register(uf, rank, Content::Structure(FlatType::AppV1(head, args)))
            }

            Type::AppN { home, name, args } => {
                let arg_vars: Vec<Variable> = args
                    .iter()
                    .map(|arg| self.type_to_variable(uf, rank, arg))
                    .collect();
                self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::App1(*home, name, arg_vars)),
                )
            }

            Type::FunN(a, b) => {
                let a_var = self.type_to_variable(uf, rank, a);
                let b_var = self.type_to_variable(uf, rank, b);
                self.register(uf, rank, Content::Structure(FlatType::Fun1(a_var, b_var)))
            }

            Type::AliasN {
                home,
                name,
                args,
                real,
                body,
            } => {
                let arg_vars: Vec<(&'a str, Variable)> = args
                    .iter()
                    .map(|(arg_name, arg_type)| {
                        (*arg_name, self.type_to_variable(uf, rank, arg_type))
                    })
                    .collect();
                let alias_var = self.type_to_variable(uf, rank, real);
                self.register(
                    uf,
                    rank,
                    Content::Alias {
                        home: *home,
                        name,
                        args: arg_vars,
                        real: alias_var,
                        body,
                    },
                )
            }

            Type::RecordN { fields, ext } => {
                let field_vars: BTreeMap<&'a str, Variable> = fields
                    .iter()
                    .map(|(name, field_type)| (*name, self.type_to_variable(uf, rank, field_type)))
                    .collect();
                let ext_var = self.type_to_variable(uf, rank, ext);
                self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::Record1(field_vars, ext_var)),
                )
            }

            Type::EmptyRecordN => {
                self.register(uf, rank, Content::Structure(FlatType::EmptyRecord1))
            }

            Type::UnitN => self.register(uf, rank, Content::Structure(FlatType::Unit1)),

            Type::TupleN(a, b, maybe_c) => {
                let a_var = self.type_to_variable(uf, rank, a);
                let b_var = self.type_to_variable(uf, rank, b);
                let c_var = maybe_c.map(|c| self.type_to_variable(uf, rank, c));
                self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::Tuple1(a_var, b_var, c_var)),
                )
            }
        }
    }

    fn register(&mut self, uf: &mut UnionFind<'a>, rank: usize, content: Content<'a>) -> Variable {
        let var = uf.fresh(Descriptor {
            preds: Vec::new(),
            content,
            rank,
            mark: NO_MARK,
            copy: None,
        });
        self.pools[rank].push(var);
        var
    }

    // SOURCE TYPE TO VARIABLE

    fn src_type_to_variable(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        site: UseSite<'a>,
        annotation: &nash_ast::Annotation<'a>,
    ) -> Variable {
        // Elm's freeVars is a `Map Name ()`, so creation is name-sorted.
        let mut sorted_names: Vec<&'a str> = annotation.free_vars.to_vec();
        sorted_names.sort_unstable();

        let flex_vars: BTreeMap<&'a str, Variable> = sorted_names
            .into_iter()
            .map(|name| {
                let content = Content::FlexVar(Some(name));
                let var = uf.fresh(Descriptor {
                    preds: Vec::new(),
                    content,
                    rank,
                    mark: NO_MARK,
                    copy: None,
                });
                (name, var)
            })
            .collect();
        self.pools[rank].extend(flex_vars.values().copied());

        let typ = self.src_type_to_var(uf, rank, &flex_vars, annotation.typ);
        let mut predicates = Vec::new();
        for (index, predicate) in annotation.context.iter().enumerate() {
            let args = predicate
                .args
                .iter()
                .map(|arg| self.src_type_to_var(uf, rank, &flex_vars, arg))
                .collect();
            let id = self.predicates.push(
                uf,
                Predicate {
                    trait_: predicate.trait_,
                    args,
                    solution: None,
                    origin: Origin::Use { site, index },
                },
            );
            self.wanted.push((rank, id));
            predicates.push(id);
        }
        self.uses.push(UseRecord {
            site,
            owner: self.owners.last().copied(),
            source: UseSource::Foreign {
                variables: annotation
                    .free_vars
                    .iter()
                    .map(|name| flex_vars[name])
                    .collect(),
            },
            predicates,
        });
        typ
    }

    fn src_type_to_var(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        flex_vars: &BTreeMap<&'a str, Variable>,
        src_type: &Located<CanType<'a>>,
    ) -> Variable {
        nash_constrain::instantiate::canonical_to_variable(
            uf,
            rank,
            &mut self.pools[rank],
            flex_vars,
            src_type,
        )
    }

    // COPY

    fn record_definitions(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        definitions: &[type_::Definition<'a>],
        declared: &BTreeMap<&'a str, &'a [type_::PredId]>,
        inferred: &'a [type_::PredId],
        binder: Option<type_::Binder<'a>>,
    ) {
        for definition in definitions {
            let binding = Binding {
                declared_quantifiers: &[],
                variable: self.type_to_variable(uf, rank, definition.typ),
                context: declared
                    .get(definition.site.name().value)
                    .copied()
                    .unwrap_or(inferred),
                definition: Some(definition.site.node()),
                context_is_final: true,
            };
            let mut pending = vec![binding.variable];
            for id in binding.context {
                pending.extend(&self.predicates.get(*id).args);
            }
            let quantified = Self::type_variables(uf, pending)
                .into_iter()
                .filter(|var| uf.get(*var).rank == NO_RANK)
                .collect();
            self.schemes.push(SchemeRecord {
                site: definition.site,
                binding,
                quantified,
                binder: binder.unwrap_or(definition.site).node(),
                parent: self.owners.last().copied(),
            });
        }
        let pending = std::mem::take(&mut self.recursive_uses);
        for use_index in pending {
            let use_ = &self.uses[use_index];
            let site = use_.site;
            let UseSource::Local { definition, .. } = use_.source else {
                unreachable!("recursive local use")
            };
            let Some(scheme) = self
                .schemes
                .iter()
                .find(|scheme| scheme.site.node() == definition)
            else {
                self.recursive_uses.push(use_index);
                continue;
            };
            for (index, id) in scheme.binding.context.iter().enumerate() {
                let pred = self.predicates.get(*id);
                let id = self.predicates.push(
                    uf,
                    Predicate {
                        trait_: pred.trait_,
                        args: pred.args.clone(),
                        origin: Origin::Use { site, index },
                        solution: None,
                    },
                );
                // Untyped recursive calls use the group's monomorphic type
                // variables and pass its final context through unchanged.
                self.predicates
                    .solve_given(uf, id, scheme.binder, index, Vec::new());
                self.uses[use_index].predicates.push(id);
            }
        }
    }

    fn declared_contexts(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        definitions: &[type_::Definition<'a>],
        declarations: &[type_::Definition<'a>],
    ) -> BTreeMap<&'a str, &'a [type_::PredId]> {
        let mut contexts = BTreeMap::new();
        for definition in definitions.iter().chain(declarations) {
            let Some(context) = definition.context else {
                continue;
            };
            let mut ids = Vec::new();
            for (index, pred) in context.iter().enumerate() {
                let args = pred
                    .args
                    .iter()
                    .map(|arg| self.type_to_variable(uf, rank, arg))
                    .collect();
                ids.push(self.predicates.push(
                    uf,
                    Predicate {
                        trait_: pred.trait_,
                        args,
                        solution: None,
                        origin: Origin::Annotation {
                            binder: definition.site.node(),
                            index,
                        },
                    },
                ));
            }
            contexts.insert(
                definition.site.name().value,
                &*self.bump.alloc_slice_copy(&ids),
            );
        }
        contexts
    }

    fn instantiate_binding(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        binding: Binding<'a>,
        site: UseSite<'a>,
    ) -> Variable {
        let mut roots = vec![binding.variable];
        if let Some(scheme) = self
            .schemes
            .iter()
            .find(|scheme| Some(scheme.site.node()) == binding.definition)
        {
            // A destructured name instantiates its whole aggregate scheme,
            // including quantifiers absent from this selected component.
            roots.push(scheme.binding.variable);
        }
        let context_offset = roots.len();
        for id in binding.context {
            roots.extend_from_slice(&self.predicates.get(*id).args);
        }
        let (copies, pairs) =
            self.make_scheme_copies(uf, rank, &roots, binding.declared_quantifiers);
        let mut predicates = Vec::new();
        let mut offset = context_offset;
        for (index, id) in binding.context.iter().enumerate() {
            let predicate = self.predicates.get(*id);
            let end = offset + predicate.args.len();
            let predicate = Predicate {
                trait_: predicate.trait_,
                args: copies[offset..end].to_vec(),
                solution: None,
                origin: Origin::Use { site, index },
            };
            let id = self.predicates.push(uf, predicate);
            self.wanted.push((rank, id));
            predicates.push(id);
            offset = end;
        }
        if let Some(definition) = binding.definition {
            if !binding.context_is_final {
                self.recursive_uses.push(self.uses.len());
            }
            self.uses.push(UseRecord {
                site,
                owner: self.owners.last().copied(),
                source: UseSource::Local {
                    definition,
                    copies: pairs,
                },
                predicates,
            });
        }
        copies[0]
    }

    fn check_ambiguity(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        start: usize,
        definitions: &[type_::Definition<'a>],
        binder: &'a Located<&'a str>,
    ) -> (Vec<Error<'a>>, bool) {
        let roots: Vec<_> = definitions
            .iter()
            .map(|def| self.type_to_variable(uf, rank, def.typ))
            .collect();
        let reachable = Self::type_variables(uf, roots);
        let mut ambiguous: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (_, id) in &self.wanted[start..] {
            let variables = Self::type_variables(uf, self.predicates.get(*id).args.clone());
            for var in variables {
                if uf.get(var).rank == NO_RANK && !reachable.contains(&var) {
                    ambiguous.entry(var).or_default().push(*id);
                }
            }
        }
        if !ambiguous.is_empty() {
            let mut roots: Vec<_> = reachable.iter().copied().collect();
            for (var, ids) in &ambiguous {
                roots.push(*var);
                for id in ids {
                    roots.extend(&self.predicates.get(*id).args);
                }
            }
            crate::annotation::prepare_scope(self.bump, uf, &roots);
        }
        let mut defaulted = false;
        let mut unresolved = Vec::new();
        for (var, ids) in ambiguous {
            let mut distinct = Vec::new();
            for id in &ids {
                let pred = self.predicates.get(*id);
                if !distinct.iter().any(|other| {
                    let other = self.predicates.get(*other);
                    pred.trait_ == other.trait_
                        && crate::preds::same_args(uf, &pred.args, &other.args)
                }) {
                    distinct.push(*id);
                }
            }
            let defaults: BTreeMap<_, _> = distinct
                .iter()
                .filter_map(|id| {
                    let pred = self.predicates.get(*id);
                    if pred.args.len() == 1 && uf.equivalent(pred.args[0], var) {
                        type_::literal_default(pred.trait_).map(|typ| (pred.trait_, typ))
                    } else {
                        None
                    }
                })
                .collect();
            if defaults.len() == 1 && matches!(uf.get(var).content, Content::FlexVar(_)) {
                let typ = self.bump.alloc(defaults.into_values().next().unwrap());
                let target = self.type_to_variable(uf, rank, typ);
                if matches!(
                    unify::unify(self.bump, uf, var, target),
                    unify::Answer::Ok(_)
                ) {
                    defaulted = true;
                    continue;
                }
            }
            unresolved.push((var, ids, distinct));
        }
        // A default can unlock an impl which supplies a literal constraint
        // for another hidden variable. Retry before declaring ambiguity.
        if defaulted {
            return (Vec::new(), true);
        }
        let mut errors = Vec::new();
        let mut rejected = BTreeSet::new();
        for (var, ids, distinct) in unresolved {
            let predicates: Vec<_> = distinct
                .iter()
                .map(|id| {
                    let pred = self.predicates.get(*id);
                    let args: Vec<_> = pred
                        .args
                        .iter()
                        .map(|arg| to_error_type(self.bump, uf, *arg))
                        .collect();
                    nash_constrain::error::AmbiguousPredicate {
                        trait_: pred.trait_,
                        args: self.bump.alloc_slice_copy(&args),
                    }
                })
                .collect();
            errors.push(Error::AmbiguousType {
                region: binder.region,
                name: binder.value,
                variable: to_error_type(self.bump, uf, var),
                predicates: self.bump.alloc_slice_fill_iter(predicates),
            });
            rejected.extend(ids);
        }
        for id in &rejected {
            self.predicates.detach(uf, *id);
        }
        self.wanted.retain(|(_, id)| !rejected.contains(id));
        (errors, defaulted)
    }

    fn type_variables(uf: &mut UnionFind<'a>, mut pending: Vec<Variable>) -> BTreeSet<Variable> {
        let mut seen = BTreeSet::new();
        let mut variables = BTreeSet::new();
        while let Some(var) = pending.pop() {
            let var = uf.find(var);
            if !seen.insert(var) {
                continue;
            }
            match &uf.get(var).content {
                Content::FlexVar(_) | Content::RigidVar(_) => {
                    variables.insert(var);
                }
                Content::Structure(FlatType::App1(_, _, args)) => pending.extend(args),
                Content::Structure(FlatType::AppV1(head, args)) => {
                    pending.push(*head);
                    pending.extend(args);
                }
                Content::Structure(FlatType::Fun1(a, b)) => pending.extend([a, b]),
                Content::Structure(FlatType::Tuple1(a, b, c)) => {
                    pending.extend([a, b]);
                    pending.extend(c);
                }
                Content::Structure(FlatType::Record1(fields, ext)) => {
                    pending.extend(fields.values());
                    pending.push(*ext);
                }
                Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
                    pending.extend(args.iter().map(|(_, var)| var))
                }
                _ => {}
            }
        }
        variables
    }

    fn retain_wanted(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        start: usize,
        binder: nash_ast::NodeId,
    ) -> &'a [type_::PredId] {
        let pending = self.wanted.split_off(start);
        let mut retained = Vec::new();
        for (_, id) in pending {
            let variables = Self::type_variables(uf, self.predicates.get(id).args.clone());
            let generalized = variables.iter().any(|var| uf.get(*var).rank == NO_RANK);
            let outer = variables.iter().any(|var| uf.get(*var).rank != NO_RANK);
            if generalized || !outer {
                retained.push(id);
            } else {
                self.wanted.push((rank, id));
            }
        }
        self.reduce_context(uf, rank, binder, retained)
    }

    fn reduce_context(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        binder: nash_ast::NodeId,
        mut retained: Vec<type_::PredId>,
    ) -> &'a [type_::PredId] {
        // Resolution queues can reorder children and deferred requirements.
        // IDs preserve creation order, which defines the context slot order.
        retained.sort_unstable();
        let closures: Vec<_> = retained
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let pred = self.predicates.get(*id);
                let mut closure = vec![Given {
                    trait_: pred.trait_,
                    args: pred.args.clone(),
                    index,
                    path: Vec::new(),
                }];
                self.expand_givens(uf, rank, &mut closure);
                closure
            })
            .collect();
        let survivors: Vec<_> = retained
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let pred = self.predicates.get(*id);
                let implied = closures.iter().enumerate().any(|(other, closure)| {
                    other != index
                        && closure.iter().any(|given| {
                            (other < index || !given.path.is_empty())
                                && given.trait_ == pred.trait_
                                && crate::preds::same_args(uf, &given.args, &pred.args)
                        })
                });
                (!implied).then_some(index)
            })
            .collect();
        for id in &retained {
            let pred = self.predicates.get(*id);
            let (index, path) = survivors
                .iter()
                .enumerate()
                .find_map(|(slot, survivor)| {
                    closures[*survivor].iter().find_map(|given| {
                        (given.trait_ == pred.trait_
                            && crate::preds::same_args(uf, &given.args, &pred.args))
                        .then(|| (slot, given.path.clone()))
                    })
                })
                .expect("acyclic superclass reduction leaves an evidence root");
            self.predicates.solve_given(uf, *id, binder, index, path);
        }
        self.bump
            .alloc_slice_fill_iter(survivors.into_iter().map(|index| retained[index]))
    }

    /// Copy all roots of one scheme (type and context) together, retaining
    /// sharing within the use and restoring every touched original afterward.
    /// The pairs also identify generalized variables for instance type args.
    #[cfg(test)]
    fn make_copies(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        roots: &[Variable],
    ) -> (Vec<Variable>, Vec<(Variable, Variable)>) {
        self.make_scheme_copies(uf, rank, roots, &[])
    }

    fn make_scheme_copies(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        roots: &[Variable],
        quantified: &[Variable],
    ) -> (Vec<Variable>, Vec<(Variable, Variable)>) {
        debug_assert!(self.copied.is_empty());
        let roots = roots
            .iter()
            .map(|root| self.make_copy_help(uf, rank, *root, quantified))
            .collect();
        let copied = std::mem::take(&mut self.copied);
        for (original, _) in &copied {
            uf.modify(*original, |desc| {
                desc.copy = None;
                desc.mark = NO_MARK;
            });
        }
        (roots, copied)
    }

    fn make_copy_help(
        &mut self,
        uf: &mut UnionFind<'a>,
        max_rank: usize,
        variable: Variable,
        quantified: &[Variable],
    ) -> Variable {
        let desc = uf.get(variable).clone();

        if let Some(copy) = desc.copy {
            return copy;
        }

        if desc.rank != NO_RANK && !quantified.iter().any(|var| uf.equivalent(*var, variable)) {
            return variable;
        }

        let make_descriptor = |content: Content<'a>| Descriptor {
            preds: Vec::new(),
            content,
            rank: max_rank,
            mark: NO_MARK,
            copy: None,
        };

        let copy = uf.fresh(make_descriptor(desc.content.clone()));
        self.pools[max_rank].push(copy);
        self.copied.push((variable, copy));

        // Link the original variable to the new variable. This lets us
        // avoid making multiple copies of the variable we are instantiating.
        //
        // Need to do this before recursively copying to avoid looping.
        uf.set(
            variable,
            Descriptor {
                preds: desc.preds.clone(),
                content: desc.content.clone(),
                rank: desc.rank,
                mark: NO_MARK,
                copy: Some(copy),
            },
        );

        // Now we recursively copy the content of the variable. We have
        // already marked the variable as copied, so we will not repeat this
        // work or crawl this variable again.
        match desc.content {
            Content::PartialAlias {
                home,
                name,
                args,
                remaining,
                body,
            } => {
                let args = args
                    .iter()
                    .map(|(name, var)| (*name, self.make_copy_help(uf, max_rank, *var, quantified)))
                    .collect();
                uf.set(
                    copy,
                    make_descriptor(Content::PartialAlias {
                        home,
                        name,
                        args,
                        remaining,
                        body,
                    }),
                );
                copy
            }
            Content::Structure(term) => {
                let new_term = self.copy_flat_type(uf, max_rank, term, quantified);
                uf.set(copy, make_descriptor(Content::Structure(new_term)));
                copy
            }

            Content::FlexVar(_) => copy,

            Content::RigidVar(name) => {
                uf.set(copy, make_descriptor(Content::FlexVar(Some(name))));
                copy
            }

            Content::Alias {
                home,
                name,
                args,
                real,
                body,
            } => {
                let new_args: Vec<(&'a str, Variable)> = args
                    .iter()
                    .map(|(arg_name, arg_var)| {
                        (
                            *arg_name,
                            self.make_copy_help(uf, max_rank, *arg_var, quantified),
                        )
                    })
                    .collect();
                let new_real = self.make_copy_help(uf, max_rank, real, quantified);
                uf.set(
                    copy,
                    make_descriptor(Content::Alias {
                        home,
                        name,
                        args: new_args,
                        real: new_real,
                        body,
                    }),
                );
                copy
            }

            Content::Error => copy,
        }
    }

    fn copy_flat_type(
        &mut self,
        uf: &mut UnionFind<'a>,
        max_rank: usize,
        flat_type: FlatType<'a>,
        quantified: &[Variable],
    ) -> FlatType<'a> {
        match flat_type {
            FlatType::AppV1(head, args) => FlatType::AppV1(
                self.make_copy_help(uf, max_rank, head, quantified),
                args.iter()
                    .map(|arg| self.make_copy_help(uf, max_rank, *arg, quantified))
                    .collect(),
            ),
            FlatType::App1(home, name, args) => FlatType::App1(
                home,
                name,
                args.iter()
                    .map(|arg| self.make_copy_help(uf, max_rank, *arg, quantified))
                    .collect(),
            ),

            FlatType::Fun1(a, b) => {
                let a_copy = self.make_copy_help(uf, max_rank, a, quantified);
                let b_copy = self.make_copy_help(uf, max_rank, b, quantified);
                FlatType::Fun1(a_copy, b_copy)
            }

            FlatType::EmptyRecord1 => FlatType::EmptyRecord1,

            FlatType::Record1(fields, ext) => {
                let field_copies: BTreeMap<&'a str, Variable> = fields
                    .iter()
                    .map(|(name, var)| (*name, self.make_copy_help(uf, max_rank, *var, quantified)))
                    .collect();
                let ext_copy = self.make_copy_help(uf, max_rank, ext, quantified);
                FlatType::Record1(field_copies, ext_copy)
            }

            FlatType::Unit1 => FlatType::Unit1,

            FlatType::Tuple1(a, b, maybe_c) => {
                let a_copy = self.make_copy_help(uf, max_rank, a, quantified);
                let b_copy = self.make_copy_help(uf, max_rank, b, quantified);
                let c_copy = maybe_c.map(|c| self.make_copy_help(uf, max_rank, c, quantified));
                FlatType::Tuple1(a_copy, b_copy, c_copy)
            }
        }
    }
}

// GENERALIZE HELPERS

fn pool_to_rank_table<'a>(
    uf: &mut UnionFind<'a>,
    young_mark: Mark,
    young_rank: usize,
    young_inhabitants: Vec<Variable>,
) -> Vec<Vec<Variable>> {
    let mut table = vec![Vec::new(); young_rank + 1];

    // Sort the youngPool variables into buckets by rank.
    for var in young_inhabitants {
        let rank = uf.get(var).rank;
        uf.modify(var, |desc| desc.mark = young_mark);
        table[rank].push(var);
    }

    table
}

// ADJUST RANK

// Adjust variable ranks such that ranks never increase as you move deeper.
// This way the outermost rank is representative of the entire structure.
fn adjust_rank<'a>(
    uf: &mut UnionFind<'a>,
    young_mark: Mark,
    visit_mark: Mark,
    group_rank: usize,
    var: Variable,
) -> usize {
    let desc = uf.get(var);
    let rank = desc.rank;
    let mark = desc.mark;

    if mark == young_mark {
        // Set the variable as marked first because it may be cyclic.
        uf.modify(var, |desc| desc.mark = visit_mark);
        let content = uf.get(var).content.clone();
        let max_rank = adjust_rank_content(uf, young_mark, visit_mark, group_rank, &content);
        uf.modify(var, |desc| {
            desc.rank = max_rank;
            desc.mark = visit_mark;
        });
        max_rank
    } else if mark == visit_mark {
        rank
    } else {
        let min_rank = group_rank.min(rank);
        // TODO how can minRank ever be groupRank?
        uf.modify(var, |desc| {
            desc.rank = min_rank;
            desc.mark = visit_mark;
        });
        min_rank
    }
}

fn adjust_rank_content<'a>(
    uf: &mut UnionFind<'a>,
    young_mark: Mark,
    visit_mark: Mark,
    group_rank: usize,
    content: &Content<'a>,
) -> usize {
    match content {
        Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => group_rank,

        Content::Structure(flat_type) => match flat_type {
            FlatType::AppV1(head, args) => {
                let head_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *head);
                args.iter().fold(head_rank, |rank, arg| {
                    rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *arg))
                })
            }
            FlatType::App1(_, _, args) => args.iter().fold(OUTERMOST_RANK, |rank, arg| {
                rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *arg))
            }),

            FlatType::Fun1(arg, result) => {
                let arg_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *arg);
                let result_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *result);
                arg_rank.max(result_rank)
            }

            // THEORY: an empty record never needs to get generalized
            FlatType::EmptyRecord1 => OUTERMOST_RANK,

            FlatType::Record1(fields, extension) => {
                let ext_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *extension);
                fields.values().fold(ext_rank, |rank, field| {
                    rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *field))
                })
            }

            // THEORY: a unit never needs to get generalized
            FlatType::Unit1 => OUTERMOST_RANK,

            FlatType::Tuple1(a, b, maybe_c) => {
                let a_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *a);
                let b_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *b);
                let ab_rank = a_rank.max(b_rank);
                match maybe_c {
                    None => ab_rank,
                    Some(c) => ab_rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *c)),
                }
            }
        },

        // THEORY: anything in the realVar would be outermostRank
        Content::Alias { args, .. } | Content::PartialAlias { args, .. } => {
            args.iter().fold(OUTERMOST_RANK, |rank, (_, arg_var)| {
                rank.max(adjust_rank(
                    uf, young_mark, visit_mark, group_rank, *arg_var,
                ))
            })
        }
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;
    use nash_constrain::type_::{PredId, make_descriptor};

    #[test]
    fn superclass_givens_record_transitive_paths_and_substitute_arguments() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntype Container 'a = Wrap 'a\ntrait Eq 'a where\n    eq : 'a -> 'a\ntrait Eq 'a => Ord 'a where\n    ord : 'a -> 'a\ntrait Ord 'a => Top 'a where\n    top : 'a -> 'a\ntrait Eq 'b => Select 'a 'b where\n    select : 'a -> 'b -> 'a\nf : Top 'a => 'a -> 'a\nf x = eq x\ng : Select 'a 'b => 'a -> 'b -> 'b\ng x y = eq y\nh : (Top 'a, Eq 'a) => 'a -> 'a\nh x = eq x\n";
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let mut solver = Solver {
            bump: &bump,
            tables: &canonical.tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let result = solver.solve(
            &mut uf,
            &Env::new(),
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            &constraint,
        );
        assert!(result.errors.is_empty());
        assert!(
            solver.wanted.is_empty(),
            "superclass givens must discharge eq uses"
        );
        let solutions: Vec<_> = solver
            .predicates
            .iter()
            .filter_map(|pred| {
                if matches!(pred.origin, Origin::Use { .. }) {
                    Some(pred.solution.as_ref().unwrap())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(solutions.len(), 3);
        assert!(solutions.iter().any(|solution| matches!(solution, crate::preds::Solution::Super { index: 0, path, .. } if path == &[0, 0])));
        assert!(solutions.iter().any(|solution| matches!(solution, crate::preds::Solution::Super { index: 0, path, .. } if path == &[0])));
        assert!(
            solutions
                .iter()
                .any(|solution| matches!(solution, crate::preds::Solution::Given { index: 1, .. })),
            "explicit Eq evidence precedes its superclass projection"
        );
    }

    #[test]
    fn givens_discharge_body_uses_without_escaping_their_scope() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nf : Keep 'a => 'a -> 'a\nf x = keep x\ng : Keep () => ()\ng = keep ()\nh = keep ()\n";
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let mut solver = Solver {
            bump: &bump,
            tables: &nash_can::environment::Tables::default(),
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let result = solver.solve(
            &mut uf,
            &Env::new(),
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            &constraint,
        );
        assert!(
            matches!(&result.errors[..], [Error::MissingImpl { region, .. }] if region.start.line == 8)
        );
        assert!(solver.givens.is_empty());
        let uses: Vec<_> = solver
            .predicates
            .iter()
            .filter_map(|pred| {
                if let Origin::Use { site, .. } = &pred.origin {
                    Some((site.region.start.line, pred.solution.is_some()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(uses.len(), 3);
        for (line, solved) in uses {
            assert_eq!(solved, line != 8, "only f and g have enclosing givens");
        }
        assert!(result.env["h"].context.is_empty());
    }

    #[test]
    fn scheme_records_freeze_local_quantifiers_before_outer_generalization() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\nouter x =\n    let\n        local y = (x, y)\n    in\n    local ()\n";
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let mut solver = Solver {
            bump: &bump,
            tables: &canonical.tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let result = solver.solve(
            &mut uf,
            &Env::new(),
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            &constraint,
        );
        assert!(result.errors.is_empty());
        let local = solver
            .schemes
            .iter()
            .find(|scheme| scheme.site.name().value == "local")
            .unwrap();
        let outer = solver
            .schemes
            .iter()
            .find(|scheme| scheme.site.name().value == "outer")
            .unwrap();
        assert_eq!(local.quantified.len(), 1);
        assert_eq!(outer.quantified.len(), 1);
        assert!(
            !uf.equivalent(local.quantified[0], outer.quantified[0]),
            "captured x is not quantified by local"
        );
        let Content::Structure(FlatType::Fun1(arg, result)) =
            uf.get(local.binding.variable).content
        else {
            panic!("local function")
        };
        let Content::Structure(FlatType::Tuple1(capture, value, None)) = uf.get(result).content
        else {
            panic!("local result")
        };
        assert!(uf.equivalent(arg, local.quantified[0]));
        assert!(uf.equivalent(value, local.quantified[0]));
        assert!(uf.equivalent(capture, outer.quantified[0]));
        let annotation = crate::annotation::to_scheme_annotation(
            &bump,
            &mut uf,
            local.binding.variable,
            &[],
            &local.quantified,
        );
        assert_eq!(
            annotation.free_vars.len(),
            1,
            "the serialized scheme excludes its captured variable"
        );
    }

    #[test]
    fn nested_impl_solutions_preserve_substitution_and_child_origins() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep () where\n    keep x = x\nimpl Keep 'a => Keep (List 'a) where\n    keep xs = xs\nvalue = keep [[()]]\n";
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let mut solver = Solver {
            bump: &bump,
            tables: &canonical.tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let result = solver.solve(
            &mut uf,
            &Env::new(),
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            &constraint,
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(result.env["value"].context.is_empty());
        assert!(solver.wanted.is_empty());
        let root = solver.predicates.iter().find(|pred| {
            matches!(pred.origin, Origin::Use { site, .. } if site.region.start.line == 8)
        }).unwrap();
        let crate::preds::Solution::Impl {
            impl_: outer,
            type_vars,
            subs,
        } = root.solution.as_ref().unwrap()
        else {
            panic!("outer impl")
        };
        assert_eq!(type_vars.len(), 1);
        assert!(matches!(
            uf.get(type_vars[0]).content,
            Content::Structure(FlatType::App1(_, "List", _))
        ));
        assert_eq!(subs.len(), 1);
        let child = solver.predicates.get(subs[0]);
        assert!(matches!(child.origin, Origin::Sub { index: 0, .. }));
        assert_eq!(
            solver
                .predicates
                .use_site(subs[0])
                .unwrap()
                .region
                .start
                .line,
            8
        );
        let crate::preds::Solution::Impl {
            impl_: inner,
            type_vars,
            subs,
        } = child.solution.as_ref().unwrap()
        else {
            panic!("inner impl")
        };
        assert_eq!(
            outer.key, inner.key,
            "the same impl may recur at a smaller type"
        );
        assert!(matches!(
            uf.get(type_vars[0]).content,
            Content::Structure(FlatType::Unit1)
        ));
        assert_eq!(subs.len(), 1);
        assert!(
            matches!(solver.predicates.get(subs[0]).solution.as_ref(), Some(crate::preds::Solution::Impl { type_vars, subs, .. }) if type_vars.is_empty() && subs.is_empty())
        );
    }

    #[test]
    fn retained_impl_children_and_recursive_uses_reference_final_context_slots() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntrait Base 'a where\n    base : 'a -> 'a\ntrait Base 'a => Strong 'a where\n    strong : 'a -> 'a\ntrait Strong 'a => Top 'a where\n    top : 'a -> 'a\nimpl Base 'a => Base (List 'a) where\n    base xs = xs\nf x = (base [let local y = g y in local x], top x)\ng x = case f x of\n    (xs, y) -> y\nh x = (f x, f x)\n";
        let parsed = nash_parse::Parser::new(&bump, source.as_bytes())
            .module()
            .unwrap();
        let canonical =
            nash_can::canonicalize(&bump, nash_can::Context::default(), &parsed).unwrap();
        let mut group_binder = None;
        let mut h_binder = None;
        let mut decls = canonical.module.decls;
        loop {
            let (definition, next, recursive) = match decls {
                nash_ast::Decls::Declare { definition, next } => (definition, next, false),
                nash_ast::Decls::DeclareRec {
                    definition, next, ..
                } => (definition, next, true),
                nash_ast::Decls::Empty => break,
            };
            let name = match definition {
                nash_ast::Def::Def { name, .. } | nash_ast::Def::TypedDef { name, .. } => name,
            };
            if recursive {
                group_binder = Some(nash_ast::NodeId::def(name));
            }
            if name.value == "h" {
                h_binder = Some(nash_ast::NodeId::def(name));
            }
            decls = next;
        }
        let group_binder = group_binder.unwrap();
        let h_binder = h_binder.unwrap();
        let mut uf = UnionFind::new();
        let constraint = nash_constrain::constrain(&bump, &mut uf, &canonical.module);
        let mut solver = Solver {
            bump: &bump,
            tables: &canonical.tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let result = solver.solve(
            &mut uf,
            &Env::new(),
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            &constraint,
        );
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert!(solver.wanted.is_empty());
        assert_eq!(result.env["f"].context, result.env["g"].context);
        assert!(solver.recursive_uses.is_empty());
        let local = solver
            .schemes
            .iter()
            .find(|scheme| scheme.site.name().value == "local")
            .unwrap();
        assert_ne!(
            local.binder, group_binder,
            "the pending g call survives checking the local helper"
        );
        assert_eq!(result.env["f"].context.len(), 1);
        assert_eq!(
            result.env["h"].context.len(),
            1,
            "{:?}",
            result.env["h"]
                .context
                .iter()
                .map(|id| solver.predicates.get(*id))
                .collect::<Vec<_>>()
        );
        let mut children = 0;
        let mut h_uses = 0;
        let mut recursive_uses = 0;
        for pred in solver.predicates.iter() {
            match &pred.origin {
                Origin::Sub { parent, index } => {
                    children += 1;
                    assert_eq!(*index, 0);
                    let parent = solver.predicates.get(*parent);
                    assert!(matches!(
                        parent.solution,
                        Some(crate::preds::Solution::Impl { .. })
                    ));
                    assert!(
                        matches!(&pred.solution, Some(crate::preds::Solution::Super { binder, index: 0, path }) if *binder == group_binder && path == &[0, 0])
                    );
                }
                Origin::Use { site, .. } if site.region.start.line == 13 => {
                    h_uses += 1;
                    assert_eq!(
                        pred.solution,
                        Some(crate::preds::Solution::Given {
                            binder: h_binder,
                            index: 0
                        })
                    );
                }
                Origin::Use { site, .. } if matches!(site.name, "f" | "g") => {
                    recursive_uses += 1;
                    assert_eq!(
                        pred.solution,
                        Some(crate::preds::Solution::Given {
                            binder: group_binder,
                            index: 0,
                        })
                    );
                }
                _ => {}
            }
        }
        assert_eq!(children, 1);
        assert_eq!(
            recursive_uses, 2,
            "recursive calls receive the final group context"
        );
        assert_eq!(
            h_uses, 2,
            "each use copies the reduced scheme and receives its own evidence"
        );
        let (_, solved) = solver.finish(&mut uf, &result.env).unwrap();
        for use_ in &solver.uses {
            let instance = &solved.instances[&use_.site.node];
            assert_eq!(instance.evidence.len(), use_.predicates.len());
            if matches!(use_.site.name, "f" | "g") {
                let expected_binder = if use_.site.region.start.line == 13 {
                    h_binder
                } else {
                    group_binder
                };
                assert!(matches!(
                    instance.evidence,
                    [nash_ast::Evidence::Given { binder, index: 0 }]
                        if *binder == expected_binder
                ));
            }
            if use_.site.name == "base" {
                let [nash_ast::Evidence::Impl { args, .. }] = instance.evidence else {
                    panic!("list Base use must publish impl evidence")
                };
                let [nash_ast::Evidence::Super { of, index: 0 }] = *args else {
                    panic!("Base evidence must project from Strong")
                };
                let nash_ast::Evidence::Super { of, index: 0 } = of else {
                    panic!("Strong evidence must project from Top")
                };
                assert!(matches!(of, nash_ast::Evidence::Given { binder, index: 0 }
                    if *binder == group_binder));
            }
        }
    }

    #[test]
    fn foreign_context_uses_the_same_fresh_variables_as_its_type() {
        use nash_ast::{Annotation, Expr, ModuleName, NodeId, Pred, QualifiedName};
        let bump = Bump::new();
        let mut solver = Solver {
            bump: &bump,
            tables: &nash_can::environment::Tables::default(),
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let mut uf = UnionFind::new();
        let a = bump.alloc(Located::at_zero(CanType::Var("a")));
        let trait_ = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Main",
            },
            name: "Keep",
        };
        let annotation = Annotation {
            free_vars: &["a"],
            context: bump.alloc_slice_fill_iter([Pred {
                trait_,
                args: bump.alloc_slice_copy(&[&*a, &*a]),
            }]),
            typ: bump.alloc(Located::at_zero(CanType::Lambda { from: a, to: a })),
        };
        let sites = [
            Located::at_zero(Expr::VarLocal("first")),
            Located::at_zero(Expr::VarLocal("second")),
        ];
        let mut vars = Vec::new();
        for site in &sites {
            let site = UseSite {
                node: NodeId::expr(site),
                region: site.region,
                name: "keep",
            };
            let typ = solver.src_type_to_variable(&mut uf, 2, site, &annotation);
            let Content::Structure(FlatType::Fun1(arg, result)) = uf.get(typ).content else {
                panic!("function type")
            };
            assert_eq!(arg, result);
            let (rank, id) = *solver.wanted.last().unwrap();
            let predicate = solver.predicates.get(id);
            assert_eq!(rank, 2);
            assert_eq!(predicate.trait_, trait_);
            assert_eq!(predicate.args, [arg, arg]);
            let Origin::Use {
                site: origin,
                index,
            } = &predicate.origin
            else {
                panic!("use origin")
            };
            assert_eq!(origin.node, site.node);
            assert_eq!(*index, 0);
            assert_eq!(
                uf.get(arg).preds,
                [id],
                "repeated arguments attach the ID once"
            );
            vars.push(arg);
        }
        assert_eq!(solver.wanted.len(), 2);
        assert!(!uf.equivalent(vars[0], vars[1]));
    }

    #[test]
    fn scheme_roots_share_copies_but_separate_uses_do_not() {
        let bump = Bump::new();
        let mut solver = Solver {
            bump: &bump,
            tables: &nash_can::environment::Tables::default(),
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Vec::new(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
        };
        let mut uf = UnionFind::new();
        let result = uf.fresh(make_descriptor(Content::RigidVar("a")));
        let context_only = uf.fresh(make_descriptor(Content::FlexVar(Some("b"))));
        let outer = uf.fresh(make_descriptor(Content::FlexVar(Some("outer"))));
        uf.modify(outer, |desc| desc.rank = OUTERMOST_RANK);
        uf.modify(context_only, |desc| desc.preds.push(PredId(0)));
        let tuple = uf.fresh(make_descriptor(Content::Structure(FlatType::Tuple1(
            result,
            context_only,
            Some(outer),
        ))));
        let roots = [result, tuple, context_only];
        let (first, first_pairs) = solver.make_copies(&mut uf, 2, &roots);
        let (second, second_pairs) = solver.make_copies(&mut uf, 2, &roots);
        assert_eq!(first_pairs.len(), 3);
        assert_eq!(second_pairs.len(), 3);
        for copies in [&first, &second] {
            let Content::Structure(FlatType::Tuple1(a, b, Some(c))) = uf.get(copies[1]).content
            else {
                panic!("copied context root");
            };
            assert_eq!(a, copies[0]);
            assert_eq!(b, copies[2]);
            assert_eq!(c, outer, "outer variables must stay shared");
            assert!(matches!(uf.get(a).content, Content::FlexVar(Some("a"))));
        }
        for (first, second) in first.iter().zip(&second) {
            assert!(!uf.equivalent(*first, *second));
        }
        for original in roots {
            assert!(uf.get(original).copy.is_none());
        }
        assert_eq!(uf.get(context_only).preds, [PredId(0)]);
        assert!(
            first_pairs.contains(&(result, first[0])),
            "record rigid type-argument copies"
        );
    }
}
