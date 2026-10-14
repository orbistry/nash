//! Elm's constraint solver: walks the constraint tree that `nash-constrain`
//! generates, with rank-based generalization. It produces annotations,
//! definition schemes, and use-site evidence.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bumpalo::Bump;
use nash_ast::Type as CanType;
use nash_ast::head::{DEPTH_LIMIT, WORK_LIMIT};
use nash_can::Annotations;
use nash_constrain::error::{Category, Error, Expected, PExpected};
use nash_constrain::type_::{
    self, Constraint, Content, Descriptor, FlatType, Mark, NO_MARK, NO_RANK, OUTERMOST_RANK, Type,
};
use nash_constrain::{UnionFind, Variable};
use nash_region::{Located, Region};

use crate::annotation::to_error_type;
use crate::occurs;
use crate::preds::{Body, Origin, Predicate, Store, UseSite};
use crate::unify;

// RUN SOLVER

pub fn run<'a>(
    bump: &'a Bump,
    uf: &mut UnionFind<'a>,
    module: &nash_ast::Module<'a>,
    tables: &nash_can::environment::Tables<'a>,
) -> Result<(Annotations<'a>, crate::SolvedTypes<'a>), Vec<Error<'a>>> {
    let constraint = nash_constrain::constrain(bump, uf, module);
    let mut solver = Solver::new(bump, tables);

    let state = solver.solve(
        uf,
        &Env::new(),
        OUTERMOST_RANK,
        State {
            env: Env::new(),
            mark: NO_MARK.next(),
            errors: Vec::new(),
        },
        &constraint,
    );

    solver.finish_state(uf, state)
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

/// Recorded schemes with a lookup by defining node.
#[derive(Default)]
struct Schemes<'a> {
    records: Vec<SchemeRecord<'a>>,
    /// First record of each site node.
    by_site: std::collections::HashMap<nash_ast::NodeId, usize>,
}

impl<'a> Schemes<'a> {
    fn push(&mut self, record: SchemeRecord<'a>) {
        self.by_site
            .entry(record.site.node())
            .or_insert(self.records.len());
        self.records.push(record);
    }

    fn at(&self, site: nash_ast::NodeId) -> Option<&SchemeRecord<'a>> {
        self.by_site.get(&site).map(|index| &self.records[*index])
    }

    fn iter(&self) -> std::slice::Iter<'_, SchemeRecord<'a>> {
        self.records.iter()
    }
}

struct NodeTypeRecord {
    node: nash_ast::NodeId,
    variable: Variable,
    owner: Option<nash_ast::NodeId>,
}

struct UseRecord<'a> {
    site: UseSite<'a>,
    variable: Variable,
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

/// The subject and class of a representation predicate such as `Big 'a`.
fn representation_requirement<'a>(
    body: &Body<'a>,
) -> Option<(Variable, nash_ast::primitives::ReprTrait)> {
    let Body::Trait { trait_, args, .. } = body else {
        return None;
    };
    let required = nash_ast::primitives::ReprTrait::of(*trait_)?;
    Some((args[0], required))
}

/// Why representation coverage could not prove a wanted predicate.
enum CoverageProblem<'a> {
    Limit,
    /// Some cell cannot prove this predicate: a context of a covered impl,
    /// or, at the limit, the wanted trait at a rigid subject.
    MissingContext(Body<'a>),
    /// Some representation cell selects no impl.
    Uncovered,
}

#[derive(Clone, Copy)]
struct DeferredField<'a> {
    test_scope: bool,
    region: nash_region::Region,
    context: type_::FieldContext<'a>,
    record: Variable,
    field: Option<(&'a str, Variable)>,
}

struct Solver<'a, 'tables> {
    bump: &'a Bump,
    tables: &'tables nash_can::environment::Tables<'a>,
    test_scope: bool,
    pools: Vec<Vec<Variable>>,
    copied: Vec<(Variable, Variable)>,
    predicates: Store<'a>,
    wanted: Vec<(usize, type_::PredId)>,
    givens: Vec<GivenFrame<'a>>,
    schemes: Schemes<'a>,
    recursive_uses: Vec<usize>,
    uses: Vec<UseRecord<'a>>,
    expr_types: Vec<NodeTypeRecord>,
    pattern_types: Vec<NodeTypeRecord>,
    owners: Vec<nash_ast::NodeId>,
    resolution_work: std::collections::HashMap<type_::PredId, usize>,
    has_poison: bool,
    dependencies: crate::recovery::Dependencies,
    failed_lineages: BTreeSet<type_::PredId>,
    failed_predicates: BTreeSet<type_::PredId>,
    failed_definitions: std::collections::HashSet<nash_ast::NodeId>,
    value_roots: Vec<(Variable, nash_region::Region)>,
    kind_contracts: crate::kind_check::Contracts<'a>,
    kind_errors: Vec<Error<'a>>,
    fields: Vec<DeferredField<'a>>,
}

/// A `Let` definition with its type and context converted to variables, for
/// the Nash-only declared contexts, scheme records, and ambiguity check.
#[derive(Clone, Copy)]
struct Definition<'a> {
    site: type_::Binder<'a>,
    typ: Variable,
    context: Option<&'a [Body<'a>]>,
}

struct GivenFrame<'a> {
    binder: nash_ast::NodeId,
    predicates: Vec<Given<'a>>,
}

#[derive(Clone)]
struct Given<'a> {
    body: Body<'a>,
    index: Option<usize>,
    path: Vec<usize>,
}

impl<'a, 'tables> Solver<'a, 'tables> {
    fn new(bump: &'a Bump, tables: &'tables nash_can::environment::Tables<'a>) -> Self {
        Self {
            bump,
            tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
        }
    }

    fn finish_state(
        &mut self,
        uf: &mut UnionFind<'a>,
        mut state: State<'a>,
    ) -> Result<(Annotations<'a>, crate::SolvedTypes<'a>), Vec<Error<'a>>> {
        self.retry_fields(uf, OUTERMOST_RANK, &mut state.errors);
        self.finish_fields(uf, OUTERMOST_RANK, &mut state.errors);
        state.errors.append(&mut self.kind_errors);
        if state.errors.is_empty() {
            self.finish(uf, &state.env)
        } else {
            // Elm accumulates errors by prepending; match its final order.
            let mut errors = state.errors;
            errors.extend(self.final_errors(uf));
            errors.reverse();
            Err(errors)
        }
    }
}

// SOLVE

impl<'a> Solver<'a, '_> {
    fn solve(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        constraint: &Constraint<'a>,
    ) -> State<'a> {
        match constraint {
            Constraint::Record {
                region,
                context,
                record,
            } => {
                let record = self.type_to_variable(uf, rank, record);
                self.fields.push(DeferredField {
                    test_scope: self.test_scope,
                    region: *region,
                    context: *context,
                    record,
                    field: None,
                });
                let mut state = state;
                self.retry_fields(uf, rank, &mut state.errors);
                state
            }

            Constraint::Field {
                region,
                context,
                record,
                field,
                field_type,
            } => {
                let record = self.type_to_variable(uf, rank, record);
                let field_type = self.type_to_variable(uf, rank, field_type);
                self.dependencies.field(field_type, record);
                self.fields.push(DeferredField {
                    test_scope: self.test_scope,
                    region: *region,
                    context: *context,
                    record,
                    field: Some((field, field_type)),
                });
                let mut state = state;
                self.retry_fields(uf, rank, &mut state.errors);
                state
            }

            Constraint::True => state,

            Constraint::SaveTheEnvironment => State {
                env: env.clone(),
                ..state
            },

            Constraint::Equal(region, category, tipe, expectation) => {
                let actual = self.type_to_variable(uf, rank, tipe);
                self.equal(uf, rank, state, *region, *category, actual, expectation)
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
                self.equal(
                    uf,
                    rank,
                    state,
                    *region,
                    Category::Local(name),
                    actual,
                    expectation,
                )
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
                self.equal(
                    uf,
                    rank,
                    state,
                    *region,
                    Category::Foreign(name),
                    actual,
                    expectation,
                )
            }

            Constraint::Pattern(region, category, tipe, expectation) => {
                let actual = self.type_to_variable(uf, rank, tipe);
                let expected = self.pattern_expectation_to_variable(uf, rank, expectation);
                self.formed_at(uf, rank, &[actual, expected], *region);
                match self.unify(uf, actual, expected) {
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

            Constraint::SaveType {
                node,
                kind,
                typ,
                constraint,
            } => match kind {
                type_::NodeKind::Expression => {
                    let variable = self.type_to_variable(uf, rank, typ);
                    self.expr_types.push(NodeTypeRecord {
                        node: *node,
                        variable,
                        owner: self.owners.last().copied(),
                    });
                    self.solve(uf, env, rank, state, constraint)
                }
                type_::NodeKind::Pattern => {
                    let variable = self.type_to_variable(uf, rank, typ);
                    self.pattern_types.push(NodeTypeRecord {
                        node: *node,
                        variable,
                        owner: self.owners.last().copied(),
                    });
                    self.solve(uf, env, rank, state, constraint)
                }
            },

            Constraint::TestScope(constraint) => {
                let previous = self.test_scope;
                self.test_scope = true;
                let state = self.solve(uf, env, rank, state, constraint);
                self.test_scope = previous;
                state
            }

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
                let errors_before = state.errors.len();
                let wanted_start = self.wanted.len();
                let annotated = definitions.iter().any(|def| def.context.is_some());
                if definitions.is_empty()
                    && rigid_vars.is_empty()
                    && matches!(body_con, Constraint::True)
                {
                    self.introduce(uf, rank, flex_vars);
                    let given = self.convert_givens(uf, rank, given);
                    self.solve_header(uf, env, rank, state, header_con, given, *binder, annotated)
                } else if definitions.is_empty() && rigid_vars.is_empty() && flex_vars.is_empty() {
                    let decls = self.convert_definitions(uf, rank, declarations);
                    let declared = self.declared_contexts(uf, rank, &[], &decls);
                    let given = self.convert_givens(uf, rank, given);
                    let state1 = self
                        .solve_header(uf, env, rank, state, header_con, given, *binder, annotated);
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
                    let given_bodies = self.convert_givens(uf, next_rank, given);
                    let defs = self.convert_definitions(uf, next_rank, definitions);
                    let decls = self.convert_definitions(uf, next_rank, declarations);
                    let declared = self.declared_contexts(uf, next_rank, &defs, &decls);
                    let mut state1 = self.solve_header(
                        uf,
                        env,
                        next_rank,
                        state,
                        header_con,
                        given_bodies,
                        *binder,
                        annotated,
                    );

                    let young_mark = state1.mark;
                    let visit_mark = young_mark.next();
                    let final_mark = visit_mark.next();

                    self.retry_fields(uf, next_rank, &mut state1.errors);
                    self.finish_fields(uf, next_rank, &mut state1.errors);

                    // pop pool
                    self.generalize(uf, young_mark, visit_mark, next_rank);
                    self.pools[next_rank] = Vec::new();

                    // An unrelated error must not suppress an escaping annotation.
                    let escapes_before = state1.errors.len();
                    for rigid in rigid_vars.iter() {
                        if uf.get(*rigid).rank != NO_RANK
                            && !crate::recovery::is_poisoned(uf, [*rigid])
                        {
                            let owner = binder
                                .map(|name| (name.name().region, name.name().value))
                                .or_else(|| header.first().map(|(name, typ)| (typ.region, *name)));
                            state1.errors.push(Error::AnnotationVariableEscapes {
                                region: owner
                                    .map_or_else(nash_region::Region::zero, |(region, _)| region),
                                name: owner.map(|(_, name)| name),
                                variable: to_error_type(self.bump, uf, *rigid),
                            });
                        }
                    }
                    // A body error leaves a valid annotation usable by callers, as in Elm.
                    // Only an annotation that lets its own variables escape blocks its uses.
                    let all_annotated = !definitions.is_empty()
                        && definitions.iter().all(|def| def.context.is_some());
                    let annotation_failed = state1.errors.len() > escapes_before;

                    if let Some(binder) = *binder {
                        let depth = self.enter_givens(
                            uf,
                            rank,
                            given_bodies,
                            Some(binder),
                            &mut state1.errors,
                        );
                        loop {
                            let (errors, defaulted) =
                                self.check_ambiguity(uf, rank, wanted_start, &defs, binder.name());
                            state1.errors.extend(errors);
                            if !defaulted {
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
                    let failed = if all_annotated {
                        annotation_failed
                    } else {
                        state1.errors.len() > errors_before
                    };
                    if failed && let Some(binder) = *binder {
                        self.fail_definition(uf, binder.node());
                    }
                    self.record_definitions(uf, rank, &defs, &declared, context, *binder);
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

    /// Solve a header constraint in the scope of its givens and its evidence
    /// owner, then retry fields and resolve the predicates wanted in this scope.
    #[allow(clippy::too_many_arguments)]
    fn solve_header(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        constraint: &Constraint<'a>,
        given: &[Body<'a>],
        binder: Option<type_::Binder<'a>>,
        annotated: bool,
    ) -> State<'a> {
        // Attribute new failures to their owning binding. This count does not
        // gate any independent constraint or reset the collected diagnostics.
        let errors_before = state.errors.len();
        let depth = self.enter_givens(uf, rank, given, binder, &mut state.errors);
        let start = self.wanted.len();
        let owner_depth = self.owners.len();
        if let Some(binder) = binder {
            self.owners.push(binder.node());
        }
        let mut state = self.solve(uf, env, rank, state, constraint);
        self.retry_fields(uf, rank, &mut state.errors);
        state = self.resolve_wanted(uf, rank, state, start, binder, annotated);
        // An annotated body keeps its annotation for callers; its failure is
        // decided after generalization.
        if state.errors.len() > errors_before
            && !annotated
            && let Some(binder) = binder
        {
            self.fail_definition(uf, binder.node());
        }
        self.givens.truncate(depth);
        self.owners.truncate(owner_depth);
        state
    }

    /// Unify an actual variable with an expectation, as Elm's `CEqual` does.
    #[allow(clippy::too_many_arguments)]
    fn equal(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        state: State<'a>,
        region: Region,
        category: Category<'a>,
        actual: Variable,
        expectation: &Expected<'a, &'a Type<'a>>,
    ) -> State<'a> {
        let expected = self.expected_to_variable(uf, rank, expectation);
        if let Expected::FromContext(_, nash_constrain::error::Context::CallArity(_, arity), _) =
            expectation
        {
            let mut result = expected;
            let mut inputs = vec![actual];
            for _ in 0..*arity {
                let Content::Structure(FlatType::Fun1(argument, output)) = uf.get(result).content
                else {
                    break;
                };
                inputs.push(argument);
                result = output;
            }
            self.dependencies.computation(result, inputs);
        }
        self.formed_at(uf, rank, &[actual, expected], region);
        match self.unify(uf, actual, expected) {
            unify::Answer::Ok(vars) => {
                self.introduce(uf, rank, &vars);
                state
            }
            unify::Answer::Err(vars, actual_type, expected_type) => {
                self.introduce(uf, rank, &vars);
                add_error(
                    state,
                    Error::BadExpr(
                        region,
                        category,
                        actual_type,
                        expectation.type_replace(expected_type),
                    ),
                )
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
            Expected::FromAnnotation(_, _, _, _, tipe) => tipe,
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

    fn convert_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        given: &[type_::Pred<'a>],
    ) -> &'a [Body<'a>] {
        let bump = self.bump;
        bump.alloc_slice_fill_iter(
            given
                .iter()
                .map(|pred| self.constraint_predicate(uf, rank, *pred)),
        )
    }

    /// Convert each definition type on its own, as Elm converts each type in
    /// each constraint. Its scheme is then the declared type: unification of
    /// the header variable at a later use cannot remove an alias name, which
    /// selects impls in Nash.
    fn convert_definitions(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        definitions: &[type_::Definition<'a>],
    ) -> Vec<Definition<'a>> {
        definitions
            .iter()
            .map(|definition| Definition {
                site: definition.site,
                typ: self.type_to_variable(uf, rank, definition.typ),
                context: definition
                    .context
                    .map(|context| self.convert_givens(uf, rank, context)),
            })
            .collect()
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

            Type::RecordN { fields } => {
                let field_vars: BTreeMap<&'a str, Variable> = fields
                    .iter()
                    .map(|(name, field_type)| (*name, self.type_to_variable(uf, rank, field_type)))
                    .collect();
                self.register(uf, rank, Content::Structure(FlatType::Record1(field_vars)))
            }

            Type::TupleN(a, b, rest) => {
                let a_var = self.type_to_variable(uf, rank, a);
                let b_var = self.type_to_variable(uf, rank, b);
                let c_var = rest
                    .iter()
                    .map(|c| self.type_to_variable(uf, rank, c))
                    .collect();
                self.register(
                    uf,
                    rank,
                    Content::Structure(FlatType::Tuple1(a_var, b_var, c_var)),
                )
            }
        }
    }

    fn constraint_predicate(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        predicate: type_::Pred<'a>,
    ) -> Body<'a> {
        match predicate {
            type_::Pred::Trait {
                trait_,
                args,
                hidden,
            } => Body::Trait {
                trait_,
                hidden,
                args: args
                    .iter()
                    .map(|arg| self.type_to_variable(uf, rank, arg))
                    .collect(),
            },
            type_::Pred::Apply { head, args } => Body::Apply {
                head: self.type_to_variable(uf, rank, head),
                args: args
                    .iter()
                    .map(|arg| self.type_to_variable(uf, rank, arg))
                    .collect(),
            },
        }
    }

    fn structure(&mut self, uf: &mut UnionFind<'a>, rank: usize, typ: FlatType<'a>) -> Variable {
        let typ = type_::normalize_application(uf, typ);
        self.register(uf, rank, Content::Structure(typ))
    }
}

impl<'a> Solver<'a, '_> {
    fn unify(
        &mut self,
        uf: &mut UnionFind<'a>,
        actual: Variable,
        expected: Variable,
    ) -> unify::Answer<'a> {
        self.dependencies.remember(uf, [actual, expected]);
        self.propagate_poison(uf);
        let detached = self.dependencies.detached(uf, [actual, expected]);
        let answer = unify::unify(self.bump, uf, actual, expected);
        self.dependencies.remember(uf, [actual, expected]);
        if matches!(answer, unify::Answer::Err(..)) {
            self.has_poison = true;
            crate::recovery::poison_roots(uf, detached);
            self.propagate_poison(uf);
        } else if matches!(uf.get(actual).content, Content::Error) {
            // A successful merge into an existing Error drops the other side's
            // structure, leaving its children without any type information.
            self.dependencies.strand(uf, [actual, expected]);
        }
        answer
    }

    /// Normalization can remove application heads from the visible type tree.
    /// A use still owns those copied variables and their kind contracts. Carry
    /// poison through its original roots before checking later obligations.
    fn propagate_poison(&self, uf: &mut UnionFind<'a>) {
        if !self.has_poison {
            return;
        }
        loop {
            let mut changed = self.dependencies.propagate(uf);
            for use_ in &self.uses {
                let mut roots = vec![use_.variable];
                for id in &use_.predicates {
                    roots.extend(self.predicates.get(*id).body.roots());
                }
                match &use_.source {
                    UseSource::Local { copies, .. } => {
                        roots.extend(copies.iter().map(|(_, copy)| copy))
                    }
                    UseSource::Foreign { variables } => roots.extend(variables),
                }
                if crate::recovery::is_poisoned(uf, roots.iter().copied())
                    && !crate::recovery::is_poisoned(uf, [use_.variable])
                {
                    changed |= crate::recovery::poison_roots(uf, [use_.variable]);
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn fail_definition(&mut self, uf: &mut UnionFind<'a>, definition: nash_ast::NodeId) {
        self.has_poison = true;
        self.failed_definitions.insert(definition);
        loop {
            let before = self.failed_definitions.len();
            for scheme in self.schemes.iter() {
                if self.failed_definitions.contains(&scheme.site.node())
                    || self.failed_definitions.contains(&scheme.binder)
                {
                    self.failed_definitions.insert(scheme.site.node());
                    crate::recovery::poison_roots(uf, [scheme.binding.variable]);
                }
            }
            for use_ in &self.uses {
                if let UseSource::Local { definition, copies } = &use_.source
                    && self.failed_definitions.contains(definition)
                {
                    crate::recovery::poison(
                        uf,
                        [use_.variable]
                            .into_iter()
                            .chain(copies.iter().map(|(_, copy)| *copy)),
                    );
                    if let Some(owner) = use_.owner {
                        self.failed_definitions.insert(owner);
                    }
                }
            }
            if before == self.failed_definitions.len() {
                break;
            }
        }
    }

    fn predicate_blocked(&self, uf: &mut UnionFind<'a>, id: type_::PredId) -> bool {
        self.failed_predicates.contains(&id)
            || self.failed_lineages.contains(&self.predicates.root(id))
            || crate::recovery::is_poisoned(uf, self.predicates.get(id).body.roots())
    }

    /// Field resolution may expose another receiver, so retry to a fixed point.
    fn retry_fields(&mut self, uf: &mut UnionFind<'a>, rank: usize, errors: &mut Vec<Error<'a>>) {
        loop {
            let pending = std::mem::take(&mut self.fields);
            let before = pending.len();
            for field in pending {
                if !self.try_field(uf, rank, field, errors) {
                    self.fields.push(field);
                }
            }
            if self.fields.len() == before {
                break;
            }
        }
    }

    fn poison_field(&mut self, uf: &mut UnionFind<'a>, field: DeferredField<'a>) {
        self.has_poison = true;
        let result = match (field.context, field.field) {
            (type_::FieldContext::Update { .. }, _) | (_, None) => field.record,
            (_, Some((_, variable))) => variable,
        };
        crate::recovery::poison_roots(uf, [result]);
        self.propagate_poison(uf);
    }

    fn try_field(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        field: DeferredField<'a>,
        errors: &mut Vec<Error<'a>>,
    ) -> bool {
        let before = errors.len();
        let resolved = self.resolve_field(uf, rank, field, errors);
        if errors.len() > before {
            self.poison_field(uf, field);
        }
        resolved
    }

    fn resolve_field(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        field: DeferredField<'a>,
        errors: &mut Vec<Error<'a>>,
    ) -> bool {
        if [field.record]
            .into_iter()
            .chain(field.field.map(|(_, var)| var))
            .any(|variable| matches!(uf.get(variable).content, Content::Error))
        {
            self.poison_field(uf, field);
            return true;
        }
        self.dependencies.remember(
            uf,
            [field.record]
                .into_iter()
                .chain(field.field.map(|(_, var)| var)),
        );
        let mut receiver = field.record;
        let mut seen = BTreeSet::new();
        while seen.insert(uf.find(receiver)) {
            let mut allocated = Vec::new();
            nash_constrain::instantiate::normalize_variable(uf, receiver, &mut allocated);
            self.introduce(uf, rank, &allocated);
            match uf.get(receiver).content.clone() {
                Content::FlexVar(_) => return false,
                Content::Error => {
                    self.poison_field(uf, field);
                    return true;
                }
                Content::Alias { body, real, .. } => {
                    if !matches!(body.value, CanType::Record { .. }) {
                        receiver = real;
                        continue;
                    }
                    let Content::Structure(FlatType::Record1(fields)) =
                        uf.get(real).content.clone()
                    else {
                        break;
                    };
                    let Some((name, field_type)) = field.field else {
                        return true;
                    };
                    let Some(actual) = fields.get(name).copied() else {
                        errors.push(Error::MissingField {
                            region: field.region,
                            context: field.context,
                            field: name,
                            record: to_error_type(self.bump, uf, field.record),
                            available: self.bump.alloc_slice_fill_iter(fields.into_keys()),
                        });
                        return true;
                    };
                    return match self.unify(uf, actual, field_type) {
                        unify::Answer::Ok(vars) => {
                            self.introduce(uf, rank, &vars);
                            true
                        }
                        unify::Answer::Err(vars, actual, expected) => {
                            self.introduce(uf, rank, &vars);
                            errors.push(Error::FieldMismatch {
                                region: field.region,
                                context: field.context,
                                field: name,
                                actual,
                                expected,
                            });
                            true
                        }
                    };
                }
                Content::Structure(FlatType::App1(home, name, args)) => {
                    if home == nash_ast::primitives::primitive_home()
                        && name == "pair"
                        && args.len() == 2
                    {
                        // Pair projections are read-only fields, not record updates.
                        if !matches!(
                            field.context,
                            type_::FieldContext::Access { .. } | type_::FieldContext::Accessor
                        ) {
                            break;
                        }
                        let Some((name, field_type)) = field.field else {
                            break;
                        };
                        let actual = match name {
                            "fst" => args[0],
                            "snd" => args[1],
                            _ => {
                                errors.push(Error::MissingField {
                                    region: field.region,
                                    context: field.context,
                                    field: name,
                                    record: to_error_type(self.bump, uf, field.record),
                                    available: self.bump.alloc_slice_copy(&["fst", "snd"]),
                                });
                                return true;
                            }
                        };
                        match self.unify(uf, actual, field_type) {
                            unify::Answer::Ok(vars) => self.introduce(uf, rank, &vars),
                            unify::Answer::Err(vars, actual, expected) => {
                                self.introduce(uf, rank, &vars);
                                errors.push(Error::FieldMismatch {
                                    region: field.region,
                                    context: field.context,
                                    field: name,
                                    actual,
                                    expected,
                                });
                            }
                        }
                        return true;
                    }
                    let visible = if field.test_scope {
                        &self.tables.test_fields
                    } else {
                        &self.tables.fields
                    };
                    let Some(union) = visible
                        .get(&nash_ast::QualifiedName { home, name })
                        .copied()
                    else {
                        break;
                    };
                    let Some((name, field_type)) = field.field else {
                        return true;
                    };
                    let Some(actual) = union.fields.iter().find(|actual| actual.field == name)
                    else {
                        errors.push(Error::MissingField {
                            region: field.region,
                            context: field.context,
                            field: name,
                            record: to_error_type(self.bump, uf, field.record),
                            available: self.bump.alloc_slice_fill_iter(
                                union.fields.iter().map(|field| field.field),
                            ),
                        });
                        return true;
                    };
                    let substitution = union.parameters.iter().copied().zip(args).collect();
                    let actual = self.src_type_to_var(uf, rank, &substitution, actual.typ);
                    match self.unify(uf, actual, field_type) {
                        unify::Answer::Ok(vars) => self.introduce(uf, rank, &vars),
                        unify::Answer::Err(vars, actual, expected) => {
                            self.introduce(uf, rank, &vars);
                            errors.push(Error::FieldMismatch {
                                region: field.region,
                                context: field.context,
                                field: name,
                                actual,
                                expected,
                            });
                        }
                    }
                    return true;
                }
                Content::Structure(FlatType::AppV1(..)) => return false,
                _ => break,
            }
        }
        errors.push(Error::NotARecord {
            region: field.region,
            context: field.context,
            field: field.field.map(|(name, _)| name),
            record: to_error_type(self.bump, uf, field.record),
        });
        true
    }

    /// A captured receiver keeps its field type in the same outer scope.
    /// No unresolved dependency may enter a generalized scheme.
    fn finish_fields(&mut self, uf: &mut UnionFind<'a>, rank: usize, errors: &mut Vec<Error<'a>>) {
        loop {
            let mut changed = false;
            for field in &self.fields {
                let receiver_rank = uf.get(field.record).rank;
                if receiver_rank != NO_RANK
                    && receiver_rank < rank
                    && let Some((_, field_type)) = field.field
                {
                    let mut variables = Self::type_variables(uf, vec![field_type]);
                    variables.insert(uf.find(field_type));
                    for var in variables {
                        if uf.get(var).rank > receiver_rank {
                            uf.modify(var, |desc| desc.rank = receiver_rank);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let pending = std::mem::take(&mut self.fields);
        for field in pending {
            let receiver_rank = uf.get(field.record).rank;
            if receiver_rank == NO_RANK || receiver_rank >= rank {
                errors.push(Error::AmbiguousRecordAccess {
                    region: field.region,
                    context: field.context,
                    field: field.field.map(|(name, _)| name),
                    record: to_error_type(self.bump, uf, field.record),
                });
                self.poison_field(uf, field);
            } else {
                self.fields.push(field);
            }
        }
    }

    fn scope(&self, mut owner: nash_ast::NodeId) -> (nash_ast::NodeId, usize) {
        let mut depth = 0;
        loop {
            let scheme = self
                .schemes
                .at(owner)
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
            let slots = crate::preds::ContextSlots::new(
                use_.predicates
                    .iter()
                    .map(|id| &self.predicates.get(*id).body),
            );
            for (index, root) in use_.predicates.iter().enumerate() {
                let Some(slot) = slots.slot(index) else {
                    continue;
                };
                let target = (binder, slot);
                let mut pending = vec![(*root, false)];
                let mut seen = BTreeSet::new();
                while let Some((id, under_impl)) = pending.pop() {
                    if !seen.insert((id, under_impl)) {
                        continue;
                    }
                    match &self.predicates.get(id).solution {
                        Some(Solution::Impl { subs, .. } | Solution::Specialize { subs }) => {
                            pending.extend(
                                subs.iter()
                                    .filter(|id| {
                                        self.predicates.get(**id).body.trait_ref().is_some()
                                    })
                                    .map(|id| (*id, true)),
                            )
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
                        Some(
                            Solution::ReflexiveLift { .. }
                            | Solution::StructuralEq { .. }
                            | Solution::Repr { .. }
                            | Solution::Apply { .. },
                        )
                        | None => {}
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

    fn final_errors(&mut self, uf: &mut UnionFind<'a>) -> Vec<Error<'a>> {
        self.propagate_poison(uf);
        let mut errors = crate::kind_check::check(
            self.bump,
            uf,
            &self.tables.kinds,
            &self.value_roots,
            &self.predicates,
            &mut self.kind_contracts,
            &mut self.dependencies,
        );
        if !errors.is_empty() {
            self.has_poison = true;
            self.propagate_poison(uf);
        }
        // Unknown constraints below look up stranded variables.
        self.dependencies.sync(uf);
        let growing = self.growing_evidence();
        for use_ in &self.uses {
            if crate::recovery::is_poisoned(uf, [use_.variable]) {
                continue;
            }
            for root in &use_.predicates {
                if growing.contains(root) && !self.predicate_blocked(uf, *root) {
                    let pred = self.predicates.get(*root);
                    let Body::Trait { trait_, args, .. } = &pred.body else {
                        continue;
                    };
                    let args: Vec<_> = args
                        .iter()
                        .map(|var| to_error_type(self.bump, uf, *var))
                        .collect();
                    errors.push(Error::PolymorphicRecursion {
                        region: use_.site.region,
                        name: use_.site.name,
                        trait_: *trait_,
                        args: self.bump.alloc_slice_copy(&args),
                    });
                }
            }
            let mut pending = use_.predicates.clone();
            let mut seen = BTreeSet::new();
            while let Some(id) = pending.pop() {
                if !seen.insert(id) || self.predicate_blocked(uf, id) {
                    continue;
                }
                let pred = self.predicates.get(id);
                match &pred.solution {
                    Some(
                        crate::preds::Solution::Impl { subs, .. }
                        | crate::preds::Solution::Apply { subs }
                        | crate::preds::Solution::Specialize { subs },
                    ) => pending.extend(subs),
                    Some(_) => {}
                    None if self.predicate_stranded(uf, id) => {}
                    None => {
                        let args = self.bump.alloc_slice_fill_iter(
                            pred.body
                                .args()
                                .iter()
                                .map(|var| to_error_type(self.bump, uf, *var)),
                        );
                        let error = match &pred.body {
                            Body::Trait { trait_, .. } => Error::UnresolvedConstraint {
                                region: use_.site.region,
                                name: use_.site.name,
                                trait_: *trait_,
                                args,
                            },
                            Body::Apply { head, .. } => Error::UnresolvedApplication {
                                region: use_.site.region,
                                name: use_.site.name,
                                head: to_error_type(self.bump, uf, *head),
                                args,
                            },
                        };
                        errors.push(error);
                    }
                }
            }
        }
        errors
    }

    /// A constraint whose open variables lost their type information to a
    /// failed call is unknown because of that failure, not unresolved.
    fn predicate_stranded(&self, uf: &mut UnionFind<'a>, id: type_::PredId) -> bool {
        Self::type_variables(uf, self.predicates.get(id).body.roots().collect())
            .into_iter()
            .any(|variable| self.dependencies.is_stranded(uf, variable))
    }

    fn finish(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
    ) -> Result<(Annotations<'a>, crate::SolvedTypes<'a>), Vec<Error<'a>>> {
        use crate::solved::{Instance, Scheme, SolvedTypes};
        use std::collections::HashMap;
        let errors = self.final_errors(uf);
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut solved = SolvedTypes::default();
        let mut orders = HashMap::new();
        let mut rendered_uses = HashMap::new();
        // Group every record by scope once, scopes in first-seen order.
        let mut scopes = Vec::new();
        let mut scope_index = HashMap::new();
        let mut members: Vec<Vec<&SchemeRecord<'a>>> = Vec::new();
        for scheme in self.schemes.iter() {
            let root = self.scope(scheme.site.node()).0;
            let index = *scope_index.entry(root).or_insert(members.len());
            if index == members.len() {
                scopes.push(root);
                members.push(Vec::new());
            }
            members[index].push(scheme);
        }
        let scope_of = |owner: nash_ast::NodeId| scope_index[&self.scope(owner).0];
        let mut uses: Vec<Vec<&UseRecord<'a>>> = vec![Vec::new(); scopes.len()];
        for use_ in &self.uses {
            uses[scope_of(use_.owner.expect("use belongs to a definition"))].push(use_);
        }
        let mut exprs: Vec<Vec<&NodeTypeRecord>> = vec![Vec::new(); scopes.len()];
        for record in &self.expr_types {
            if let Some(owner) = record.owner {
                exprs[scope_of(owner)].push(record);
            }
        }
        let mut patterns: Vec<Vec<&NodeTypeRecord>> = vec![Vec::new(); scopes.len()];
        for record in &self.pattern_types {
            if let Some(owner) = record.owner {
                patterns[scope_of(owner)].push(record);
            }
        }
        // Quantifier order must be frozen with its rendered scheme, before
        // another body can assign names to its own instantiations.
        for (index, mut members) in members.into_iter().enumerate() {
            let (uses, exprs, patterns) = (&uses[index], &exprs[index], &patterns[index]);
            members.sort_by_key(|scheme| self.scope(scheme.site.node()).1);
            let mut roots = Vec::new();
            for scheme in &members {
                roots.push(scheme.binding.variable);
                for id in scheme.binding.context {
                    roots.extend(self.predicates.get(*id).body.roots());
                }
            }
            for use_ in uses {
                let quantified = match use_.source {
                    UseSource::Local { definition, .. } => {
                        self.schemes.at(definition).unwrap().quantified.as_slice()
                    }
                    UseSource::Foreign { .. } => &[],
                };
                roots.extend(self.use_variables(uf, use_, quantified));
                let mut pending = use_.predicates.clone();
                while let Some(id) = pending.pop() {
                    match &self.predicates.get(id).solution {
                        Some(crate::preds::Solution::Impl {
                            type_vars, subs, ..
                        }) => {
                            roots.extend(type_vars);
                            pending.extend(subs);
                        }
                        Some(
                            crate::preds::Solution::ReflexiveLift { typ }
                            | crate::preds::Solution::StructuralEq { typ }
                            | crate::preds::Solution::Repr { typ, .. },
                        ) => roots.push(*typ),
                        Some(crate::preds::Solution::Apply { subs }) => pending.extend(subs),
                        // Code generation selects again from these types.
                        Some(crate::preds::Solution::Specialize { .. }) => {
                            roots.extend(self.predicates.get(id).body.roots())
                        }
                        _ => {}
                    }
                }
            }
            // Node types must use the same variable names as their owner's
            // schemes and instances. Add them last so existing quantifier
            // ordering remains stable when a body introduces private types.
            roots.extend(exprs.iter().chain(patterns).map(|record| record.variable));
            crate::annotation::prepare_scope(self.bump, uf, &roots);
            for scheme in members {
                let id = scheme.site.node();
                let context: Vec<_> = scheme
                    .binding
                    .context
                    .iter()
                    .map(|id| {
                        let pred = self.predicates.get(*id);
                        pred.body.clone()
                    })
                    .collect();
                let annotation = crate::annotation::to_scheme_annotation(
                    self.bump,
                    uf,
                    scheme.binding.variable,
                    &context,
                    &scheme.quantified,
                );
                let order = crate::annotation::ordered_quantifiers(uf, &scheme.quantified);
                orders.insert(id, order);
                solved.schemes.insert(
                    id,
                    Scheme {
                        annotation,
                        binder: scheme.binder,
                    },
                );
            }
            for use_ in uses {
                let keys = match &use_.source {
                    UseSource::Local { definition, .. } => {
                        self.schemes.at(*definition).unwrap().quantified.clone()
                    }
                    UseSource::Foreign { variables } => variables.clone(),
                };
                let variables = self.use_variables(uf, use_, &keys);
                let types: Vec<_> = keys
                    .into_iter()
                    .zip(variables)
                    .map(|(key, var)| (key, crate::annotation::to_solved_type(self.bump, uf, var)))
                    .collect();
                let evidence = self.evidence_arguments(uf, &use_.predicates);
                assert!(
                    rendered_uses
                        .insert(use_.site.node, (types, evidence))
                        .is_none(),
                    "one instance per original use node"
                );
            }
            for (records, output) in [(exprs, &mut solved.exprs), (patterns, &mut solved.patterns)]
            {
                for record in records {
                    let typ = crate::annotation::to_solved_type(self.bump, uf, record.variable);
                    assert!(
                        output.insert(record.node, typ).is_none(),
                        "one type per original node"
                    );
                }
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

    fn evidence_arguments(
        &self,
        uf: &mut UnionFind<'a>,
        ids: &[type_::PredId],
    ) -> &'a [nash_ast::Evidence<'a>] {
        let slots =
            crate::preds::ContextSlots::new(ids.iter().map(|id| &self.predicates.get(*id).body));
        let mut evidence = Vec::with_capacity(slots.evidence_len());
        for (index, id) in ids.iter().enumerate() {
            if let Some(slot) = slots.slot(index) {
                assert_eq!(slot, evidence.len());
                evidence.push(self.evidence(uf, *id));
            }
        }
        self.bump.alloc_slice_fill_iter(evidence)
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
            Solution::Repr { trait_, typ } => Evidence::Repr {
                trait_: *trait_,
                typ: crate::annotation::to_solved_type(self.bump, uf, *typ),
            },
            Solution::Apply { .. } => unreachable!("Apply has no evidence slot"),
            Solution::StructuralEq { typ } => Evidence::StructuralEq {
                typ: crate::annotation::to_solved_type(self.bump, uf, *typ),
            },
            Solution::ReflexiveLift { typ } => Evidence::ReflexiveLift {
                typ: crate::annotation::to_solved_type(self.bump, uf, *typ),
            },
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
                args: self.evidence_arguments(uf, subs),
            },
            Solution::Specialize { .. } => {
                let Body::Trait { trait_, args, .. } = &self.predicates.get(id).body else {
                    unreachable!("coverage proves trait predicates")
                };
                Evidence::Specialize {
                    trait_: *trait_,
                    args: self.bump.alloc_slice_fill_iter(
                        args.iter()
                            .map(|var| crate::annotation::to_solved_type(self.bump, uf, *var)),
                    ),
                }
            }
        }
    }

    fn canonical_predicate(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        variables: &BTreeMap<&'a str, Variable>,
        predicate: nash_ast::Pred<'a>,
    ) -> Body<'a> {
        let args = predicate
            .args()
            .iter()
            .map(|arg| self.src_type_to_var(uf, rank, variables, arg))
            .collect();
        match predicate {
            nash_ast::Pred::Trait { trait_, .. } => Body::Trait {
                trait_,
                args,
                hidden: false,
            },
            nash_ast::Pred::Implied { trait_, .. } => Body::Trait {
                trait_,
                args,
                hidden: true,
            },
            nash_ast::Pred::Apply { head, .. } => Body::Apply {
                head: self.src_type_to_var(uf, rank, variables, head),
                args,
            },
        }
    }

    fn expand_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        predicates: &mut Vec<Given<'a>>,
    ) {
        // Breadth-first superclass expansion keeps explicit givens first.
        let mut cursor = 0;
        while cursor < predicates.len() {
            let current = predicates[cursor].clone();
            cursor += 1;
            let Body::Trait { trait_, args, .. } = &current.body else {
                continue;
            };
            let Some(info) = self.tables.traits.get(trait_).copied() else {
                continue;
            };
            let variables = info
                .parameters
                .iter()
                .copied()
                .zip(args.iter().copied())
                .collect();
            for (index, superclass) in info.supers.iter().enumerate() {
                let body = self.canonical_predicate(uf, rank, &variables, *superclass);
                if predicates
                    .iter()
                    .any(|existing| existing.body.same(uf, &body))
                {
                    continue;
                }
                let mut path = current.path.clone();
                path.push(index);
                predicates.push(Given {
                    body,
                    index: current.index,
                    path,
                });
            }
        }
    }

    fn enter_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        given: &[Body<'a>],
        binder: Option<type_::Binder<'a>>,
        errors: &mut Vec<Error<'a>>,
    ) -> usize {
        let depth = self.givens.len();
        if let Some(binder) = binder.filter(|_| !given.is_empty()) {
            let bodies = given.to_vec();
            let slots = crate::preds::ContextSlots::new(bodies.iter());
            let mut predicates: Vec<_> = bodies
                .into_iter()
                .enumerate()
                .map(|(index, body)| Given {
                    body,
                    index: slots.slot(index),
                    path: Vec::new(),
                })
                .collect();
            self.expand_givens(uf, rank, &mut predicates);
            // Canonicalization checks each context alone. A local annotation
            // shares the enclosing annotation's type variables, as Elm's `rtv`
            // does, so its representation givens must also agree with every
            // enclosing frame. A frame entered again finds its subject poisoned.
            let requirements = self
                .givens
                .iter()
                .flat_map(|frame| frame.predicates.iter())
                .chain(&predicates)
                .filter_map(|given| representation_requirement(&given.body))
                .collect();
            errors.extend(self.contradictory_representations(
                uf,
                rank,
                requirements,
                binder.name(),
            ));
            self.givens.push(GivenFrame {
                binder: binder.node(),
                predicates,
            });
        }
        depth
    }

    fn requirement_chain(
        &self,
        uf: &mut UnionFind<'a>,
        mut id: type_::PredId,
    ) -> &'a [nash_constrain::error::Requirement<'a>] {
        use nash_constrain::error::Requirement;
        let mut chain = Vec::new();
        loop {
            match self.predicates.get(id).origin {
                Origin::Sub { parent, .. } => {
                    let body = &self.predicates.get(parent).body;
                    let args = self.bump.alloc_slice_fill_iter(
                        body.args()
                            .iter()
                            .map(|arg| to_error_type(self.bump, uf, *arg)),
                    );
                    chain.push(match body {
                        Body::Trait { trait_, .. } => Requirement::Trait {
                            trait_: *trait_,
                            args,
                        },
                        Body::Apply { head, .. } => Requirement::Application {
                            head: to_error_type(self.bump, uf, *head),
                            args,
                        },
                    });
                    id = parent;
                }
                Origin::Formation { typ, .. } => {
                    chain.push(Requirement::Formation(to_error_type(self.bump, uf, typ)));
                    break;
                }
                _ => break,
            }
        }
        chain.reverse();
        self.bump.alloc_slice_fill_iter(chain)
    }

    fn application_context(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        head: Variable,
        args: &[Variable],
    ) -> Option<Vec<Body<'a>>> {
        self.dependencies
            .remember(uf, [head].into_iter().chain(args.iter().copied()));
        let (name, supplied) = crate::representation::application(uf, head, args)?;
        let info = self.tables.kinds.constructor(name);
        let variables: BTreeMap<_, _> = info.parameters().iter().copied().zip(supplied).collect();
        Some(
            info.context(self.bump)
                .iter()
                .filter(|pred| {
                    nash_can::kinds::predicate_variables(**pred)
                        .iter()
                        .all(|name| variables.contains_key(name))
                })
                .map(|pred| self.canonical_predicate(uf, rank, &variables, *pred))
                .collect(),
        )
    }

    fn representation(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        variable: Variable,
    ) -> Option<nash_ast::primitives::Repr> {
        let mut allocated = Vec::new();
        self.dependencies.remember(uf, [variable]);
        let repr = crate::representation::known(uf, &self.tables.kinds, variable, &mut allocated);
        self.introduce(uf, rank, &allocated);
        repr
    }

    fn given_big(&mut self, uf: &mut UnionFind<'a>, rank: usize, variable: Variable) -> bool {
        let wanted = Body::Trait {
            trait_: nash_ast::primitives::ReprTrait::Big.qualified(),
            args: vec![variable],
            hidden: true,
        };
        let mut allocated = Vec::new();
        let found = self.givens.iter().any(|frame| {
            frame.predicates.iter().any(|given| {
                crate::representation::same_predicate(
                    uf,
                    &self.tables.kinds,
                    &given.body,
                    &wanted,
                    &mut allocated,
                )
            })
        });
        self.introduce(uf, rank, &allocated);
        found
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
        use crate::preds::Solution;
        use nash_ast::primitives::{Repr, ReprTrait};
        self.propagate_poison(uf);
        let report_missing = annotated;
        let mut queue: VecDeque<_> = self
            .wanted
            .split_off(start)
            .into_iter()
            .map(|(rank, id)| (rank, id, self.predicates.depth(id)))
            .collect();
        while let Some((wanted_rank, id, resolution_depth)) = queue.pop_front() {
            if self.predicate_blocked(uf, id) {
                self.predicates.detach(uf, id);
                continue;
            }
            if self.predicates.get(id).solution.is_some() {
                continue;
            }
            let body = self.predicates.get(id).body.clone();
            if self.solve_from_givens(uf, rank, id, &body) {
                continue;
            }
            let roots: Vec<_> = body.roots().collect();
            if binder.is_none()
                || (!self.givens.is_empty() && crate::resolve::has_outer_flex(uf, &roots, rank))
            {
                self.wanted.push((wanted_rank, id));
                continue;
            }
            let Body::Trait { trait_, args, .. } = body else {
                let Body::Apply { head, args } = body else {
                    unreachable!()
                };
                if let Some(children) = self.application_context(uf, rank, head, &args) {
                    let mut subs = Vec::new();
                    for (index, body) in children.into_iter().enumerate() {
                        let sub = self.predicates.push(
                            uf,
                            Predicate {
                                body,
                                origin: Origin::Sub { parent: id, index },
                                solution: None,
                            },
                        );
                        subs.push(sub);
                        queue.push_back((wanted_rank, sub, resolution_depth + 1));
                    }
                    self.predicates.solve(uf, id, Solution::Apply { subs });
                } else {
                    self.wanted.push((wanted_rank, id));
                }
                continue;
            };
            let site = self.predicates.use_site(id);
            if let Some(required) = ReprTrait::of(trait_) {
                if let Some(actual) = self.representation(uf, rank, args[0]) {
                    if required.admits().contains(actual) {
                        self.predicates.solve(
                            uf,
                            id,
                            Solution::Repr {
                                trait_: required,
                                typ: args[0],
                            },
                        );
                    } else if let Some(site) = site {
                        self.failed_predicates.insert(id);
                        state.errors.push(Error::MissingImpl {
                            region: site.region,
                            name: site.name,
                            trait_,
                            args: self
                                .bump
                                .alloc_slice_copy(&[to_error_type(self.bump, uf, args[0])]),
                            available: &[],
                            because: self.requirement_chain(uf, id),
                        });
                    }
                    continue;
                }
                let givens: Vec<_> = self
                    .givens
                    .iter()
                    .flat_map(|frame| frame.predicates.iter().map(|given| given.body.clone()))
                    .collect();
                let mut allocated = Vec::new();
                let proven = matches!(
                    crate::resolve::in_class(
                        self.tables,
                        uf,
                        args[0],
                        required.admits(),
                        &givens,
                        &mut allocated
                    ),
                    nash_ast::head::Match::Yes(())
                );
                self.introduce(uf, rank, &allocated);
                if proven {
                    self.predicates.solve(
                        uf,
                        id,
                        Solution::Repr {
                            trait_: required,
                            typ: args[0],
                        },
                    );
                    continue;
                }
                if report_missing
                    && let Some(binder) = binder
                    && let Some(site) = site
                    && matches!(uf.get(args[0]).content, Content::RigidVar(_))
                {
                    self.failed_predicates.insert(id);
                    state.errors.push(Error::MissingConstraint {
                        region: site.region,
                        name: site.name,
                        trait_,
                        args: self
                            .bump
                            .alloc_slice_copy(&[to_error_type(self.bump, uf, args[0])]),
                        binder: binder.name(),
                    });
                } else {
                    self.wanted.push((wanted_rank, id));
                }
                continue;
            }
            let structural_eq = trait_ == nash_ast::primitives::eq_trait()
                && self.tables.has_structural_eq()
                && args.len() == 1;
            let reflexive_lift = trait_ == nash_ast::primitives::lift_trait()
                && self.tables.has_reflexive_lift()
                && args.len() == 2
                && crate::preds::same_args(uf, &args[..1], &args[1..]);
            if reflexive_lift
                || (structural_eq
                    && (self.given_big(uf, rank, args[0])
                        || self.representation(uf, rank, args[0]) == Some(Repr::Big)))
            {
                let solution = if structural_eq {
                    Solution::StructuralEq { typ: args[0] }
                } else {
                    Solution::ReflexiveLift { typ: args[0] }
                };
                self.predicates.solve(uf, id, solution);
                continue;
            }
            // Impls are disjoint only over finite types, and an impl that
            // matches an infinite type matches it again at every depth. Elm
            // reports an infinite type at the occurs check after its `CLet`
            // (`Type/Solve.hs` `occurs`); the binding's check here does the
            // same and poisons the type, which drops this wanted.
            if occurs::reaches_cycle(uf, &args) {
                self.wanted.push((wanted_rank, id));
                continue;
            }
            // A blanket impl can match a rigid argument without narrowing it;
            // its prerequisites are checked against the enclosing givens below.
            let givens: Vec<_> = self
                .givens
                .iter()
                .flat_map(|frame| frame.predicates.iter().map(|given| given.body.clone()))
                .collect();
            let mut allocated = Vec::new();
            let selection =
                crate::resolve::select(self.tables, uf, trait_, &args, &givens, &mut allocated);
            self.introduce(uf, rank, &allocated);
            if matches!(
                selection,
                crate::resolve::Selection::Deferred | crate::resolve::Selection::Missing
            ) {
                let mut allocated = Vec::new();
                let equations = crate::resolve::improvement(
                    self.tables,
                    uf,
                    trait_,
                    &args,
                    &givens,
                    &mut allocated,
                );
                self.introduce(uf, rank, &allocated);
                if let Some(equations) = equations {
                    for (left, right) in equations {
                        let unify::Answer::Ok(fresh) = self.unify(uf, left, right) else {
                            unreachable!("relational probe checked compatible equalities");
                        };
                        self.introduce(uf, rank, &fresh);
                    }
                    queue.push_front((wanted_rank, id, resolution_depth));
                    queue.extend(
                        self.wanted
                            .split_off(start)
                            .into_iter()
                            .map(|(rank, id)| (rank, id, self.predicates.depth(id))),
                    );
                    continue;
                }
            }
            // Representation coverage: a rigid variable at a representation-
            // tested head position keeps ordinary selection from deciding.
            if matches!(
                selection,
                crate::resolve::Selection::Deferred | crate::resolve::Selection::Missing
            ) && let Some(binder) = binder
            {
                let mut remaining = WORK_LIMIT;
                match self.cover(uf, rank, id, trait_, &args, &[], &mut remaining, 0) {
                    Ok(None) => {}
                    Ok(Some(subs)) => {
                        self.predicates.solve(uf, id, Solution::Specialize { subs });
                        continue;
                    }
                    Err(problem) => {
                        let Some(site) = site else {
                            self.wanted.push((wanted_rank, id));
                            continue;
                        };
                        match problem {
                            CoverageProblem::Limit => {
                                state.errors.push(Error::ImplResolutionLimit {
                                    region: site.region,
                                    name: site.name,
                                    trait_,
                                });
                                self.failed_lineages.insert(self.predicates.root(id));
                            }
                            _ if !report_missing => self.wanted.push((wanted_rank, id)),
                            CoverageProblem::MissingContext(Body::Trait {
                                trait_: missing,
                                args: missing_args,
                                ..
                            }) => {
                                let missing_args = self.bump.alloc_slice_fill_iter(
                                    missing_args
                                        .iter()
                                        .map(|arg| to_error_type(self.bump, uf, *arg)),
                                );
                                self.failed_predicates.insert(id);
                                state.errors.push(Error::MissingConstraint {
                                    region: site.region,
                                    name: site.name,
                                    trait_: missing,
                                    args: missing_args,
                                    binder: binder.name(),
                                });
                            }
                            CoverageProblem::MissingContext(Body::Apply { .. })
                            | CoverageProblem::Uncovered => {
                                let args: Vec<_> = args
                                    .iter()
                                    .map(|arg| to_error_type(self.bump, uf, *arg))
                                    .collect();
                                let available: Vec<_> = self
                                    .tables
                                    .impls_for(trait_)
                                    .map(|(key, _)| key.heads)
                                    .collect();
                                self.failed_predicates.insert(id);
                                state.errors.push(Error::MissingImpl {
                                    region: site.region,
                                    name: site.name,
                                    trait_,
                                    args: self.bump.alloc_slice_copy(&args),
                                    available: self.bump.alloc_slice_copy(&available),
                                    because: self.requirement_chain(uf, id),
                                });
                            }
                        }
                        continue;
                    }
                }
            }
            if report_missing
                && !matches!(selection, crate::resolve::Selection::Impl { .. })
                && let Some(binder) = binder
                && let Some(site) = site
                && args
                    .iter()
                    .any(|arg| matches!(uf.get(*arg).content, Content::RigidVar(_)))
            {
                let args: Vec<_> = args
                    .iter()
                    .map(|arg| to_error_type(self.bump, uf, *arg))
                    .collect();
                self.failed_predicates.insert(id);
                state.errors.push(Error::MissingConstraint {
                    region: site.region,
                    name: site.name,
                    trait_,
                    args: self.bump.alloc_slice_copy(&args),
                    binder: binder.name(),
                });
                continue;
            }
            if binder.is_some() {
                let site = site.expect("wanteds originate at uses");
                let work = self
                    .resolution_work
                    .entry(self.predicates.root(id))
                    .or_default();
                *work += 1;
                if *work > WORK_LIMIT || resolution_depth >= DEPTH_LIMIT {
                    state.errors.push(Error::ImplResolutionLimit {
                        region: site.region,
                        name: site.name,
                        trait_,
                    });
                    self.failed_lineages.insert(self.predicates.root(id));
                    continue;
                }
                match selection {
                    crate::resolve::Selection::Deferred => self.wanted.push((wanted_rank, id)),
                    crate::resolve::Selection::Limit => {
                        state.errors.push(Error::ImplResolutionLimit {
                            region: site.region,
                            name: site.name,
                            trait_,
                        });
                        self.failed_lineages.insert(self.predicates.root(id));
                    }
                    crate::resolve::Selection::Missing => {
                        let args: Vec<_> = args
                            .iter()
                            .map(|arg| to_error_type(self.bump, uf, *arg))
                            .collect();
                        let available: Vec<_> = self
                            .tables
                            .impls_for(trait_)
                            .map(|(key, _)| key.heads)
                            .collect();
                        self.failed_predicates.insert(id);
                        state.errors.push(Error::MissingImpl {
                            region: site.region,
                            name: site.name,
                            trait_,
                            args: self.bump.alloc_slice_copy(&args),
                            available: self.bump.alloc_slice_copy(&available),
                            because: self.requirement_chain(uf, id),
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
                            let body = self.canonical_predicate(uf, rank, &vars, *context);
                            let sub = self.predicates.push(
                                uf,
                                Predicate {
                                    body,
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

    /// Whether the givens, with any coverage cell, prove `class` for `node`.
    fn proves_class(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        node: Variable,
        class: nash_ast::primitives::ReprTrait,
        givens: &[Body<'a>],
    ) -> bool {
        let mut allocated = Vec::new();
        let result = crate::resolve::in_class(
            self.tables,
            uf,
            node,
            class.admits(),
            givens,
            &mut allocated,
        );
        self.introduce(uf, rank, &allocated);
        matches!(result, nash_ast::head::Match::Yes(()))
    }

    /// Solve `id` from a given of an active frame, nearest frame first.
    fn solve_from_givens(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        id: type_::PredId,
        body: &Body<'a>,
    ) -> bool {
        let mut allocated = Vec::new();
        let solution = self.givens.iter().rev().find_map(|frame| {
            frame.predicates.iter().find_map(|given| {
                crate::representation::same_predicate(
                    uf,
                    &self.tables.kinds,
                    &given.body,
                    body,
                    &mut allocated,
                )
                .then_some((frame.binder, given.index, given.path.clone()))
            })
        });
        self.introduce(uf, rank, &allocated);
        let Some((binder, index, path)) = solution else {
            return false;
        };
        if let Some(index) = index {
            self.predicates.solve_given(uf, id, binder, index, path);
        } else {
            self.predicates
                .solve(uf, id, crate::preds::Solution::Apply { subs: Vec::new() });
        }
        true
    }

    /// The givens of every active frame, then the coverage cell assumptions.
    fn given_bodies(&self, assumptions: &[Body<'a>]) -> Vec<Body<'a>> {
        self.givens
            .iter()
            .flat_map(|frame| frame.predicates.iter().map(|given| given.body.clone()))
            .chain(assumptions.iter().cloned())
            .collect()
    }

    /// Prove `trait_ args` by representation coverage (inference only).
    /// `Ok(None)`: coverage does not apply. `Ok(Some(subs))`: every admitted
    /// cell selects an impl, and `subs` prove each impl's context under its
    /// cell. The cells are never committed to inference.
    #[allow(clippy::too_many_arguments)]
    fn cover(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        parent: type_::PredId,
        trait_: nash_ast::QualifiedName<'a>,
        args: &[Variable],
        assumptions: &[Body<'a>],
        remaining: &mut usize,
        depth: usize,
    ) -> Result<Option<Vec<type_::PredId>>, CoverageProblem<'a>> {
        if depth >= DEPTH_LIMIT {
            return Err(CoverageProblem::Limit);
        }
        let givens = self.given_bodies(assumptions);
        let mut allocated = Vec::new();
        let coverage = crate::resolve::coverage(
            self.tables,
            uf,
            trait_,
            args,
            &givens,
            &mut allocated,
            remaining,
        );
        self.introduce(uf, rank, &allocated);
        let covered = match coverage {
            crate::resolve::Coverage::NotApplicable => return Ok(None),
            crate::resolve::Coverage::Limit => return Err(CoverageProblem::Limit),
            crate::resolve::Coverage::Unstated(subject) => {
                return Err(CoverageProblem::MissingContext(Body::Trait {
                    trait_,
                    args: if args.len() == 1 {
                        vec![subject]
                    } else {
                        args.to_vec()
                    },
                    hidden: false,
                }));
            }
            crate::resolve::Coverage::Uncovered => return Err(CoverageProblem::Uncovered),
            crate::resolve::Coverage::Complete(covered) => covered,
        };
        let mut subs = Vec::new();
        for covered in covered {
            let mut cell = assumptions.to_vec();
            cell.extend(
                covered
                    .assumptions
                    .iter()
                    .map(|(subject, repr)| Body::Trait {
                        trait_: nash_ast::primitives::ReprTrait::exactly(*repr).qualified(),
                        args: vec![*subject],
                        hidden: true,
                    }),
            );
            let vars: BTreeMap<_, _> = covered.substitution.into_iter().collect();
            for (index, context) in covered.info.context.iter().enumerate() {
                let body = self.canonical_predicate(uf, rank, &vars, *context);
                let sub = self.predicates.push(
                    uf,
                    Predicate {
                        body: body.clone(),
                        origin: Origin::Sub { parent, index },
                        solution: None,
                    },
                );
                if let Err(problem) = self.prove(uf, rank, sub, &cell, remaining, depth + 1) {
                    self.predicates.detach(uf, sub);
                    return Err(match problem {
                        CoverageProblem::Limit => CoverageProblem::Limit,
                        _ => CoverageProblem::MissingContext(body),
                    });
                }
                subs.push(sub);
            }
        }
        Ok(Some(subs))
    }

    /// Prove one predicate of a coverage cell now, without deferral: from a
    /// given, a representation, a selected impl, or coverage again.
    fn prove(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        id: type_::PredId,
        assumptions: &[Body<'a>],
        remaining: &mut usize,
        depth: usize,
    ) -> Result<(), CoverageProblem<'a>> {
        use crate::preds::Solution;
        use nash_ast::primitives::{Repr, ReprTrait};
        nash_ast::head::step(remaining).map_err(|_| CoverageProblem::Limit)?;
        if depth >= DEPTH_LIMIT {
            return Err(CoverageProblem::Limit);
        }
        let body = self.predicates.get(id).body.clone();
        if self.solve_from_givens(uf, rank, id, &body) {
            return Ok(());
        }
        let givens = self.given_bodies(assumptions);
        let (trait_, args) = match body {
            Body::Apply { head, args } => {
                let children = self
                    .application_context(uf, rank, head, &args)
                    .ok_or(CoverageProblem::Uncovered)?;
                let mut subs = Vec::new();
                for (index, child) in children.into_iter().enumerate() {
                    let sub = self.predicates.push(
                        uf,
                        Predicate {
                            body: child,
                            origin: Origin::Sub { parent: id, index },
                            solution: None,
                        },
                    );
                    if let Err(problem) =
                        self.prove(uf, rank, sub, assumptions, remaining, depth + 1)
                    {
                        self.predicates.detach(uf, sub);
                        return Err(problem);
                    }
                    subs.push(sub);
                }
                self.predicates.solve(uf, id, Solution::Apply { subs });
                return Ok(());
            }
            Body::Trait { trait_, args, .. } => (trait_, args),
        };
        if let Some(required) = ReprTrait::of(trait_) {
            let proven = match self.representation(uf, rank, args[0]) {
                Some(actual) => required.admits().contains(actual),
                None => self.proves_class(uf, rank, args[0], required, &givens),
            };
            if !proven {
                return Err(CoverageProblem::Uncovered);
            }
            self.predicates.solve(
                uf,
                id,
                Solution::Repr {
                    trait_: required,
                    typ: args[0],
                },
            );
            return Ok(());
        }
        if trait_ == nash_ast::primitives::lift_trait()
            && self.tables.has_reflexive_lift()
            && args.len() == 2
            && crate::preds::same_args(uf, &args[..1], &args[1..])
        {
            self.predicates
                .solve(uf, id, Solution::ReflexiveLift { typ: args[0] });
            return Ok(());
        }
        if trait_ == nash_ast::primitives::eq_trait()
            && self.tables.has_structural_eq()
            && args.len() == 1
            && (self.representation(uf, rank, args[0]) == Some(Repr::Big)
                || self.proves_class(uf, rank, args[0], ReprTrait::Big, &givens))
        {
            self.predicates
                .solve(uf, id, Solution::StructuralEq { typ: args[0] });
            return Ok(());
        }
        if occurs::reaches_cycle(uf, &args) {
            return Err(CoverageProblem::Uncovered);
        }
        let mut allocated = Vec::new();
        let selection =
            crate::resolve::select(self.tables, uf, trait_, &args, &givens, &mut allocated);
        self.introduce(uf, rank, &allocated);
        match selection {
            crate::resolve::Selection::Impl {
                info,
                key,
                substitution,
            } => {
                let type_vars = substitution.iter().map(|(_, var)| *var).collect();
                let vars: BTreeMap<_, _> = substitution.into_iter().collect();
                let mut subs = Vec::new();
                for (index, context) in info.context.iter().enumerate() {
                    let body = self.canonical_predicate(uf, rank, &vars, *context);
                    let sub = self.predicates.push(
                        uf,
                        Predicate {
                            body,
                            origin: Origin::Sub { parent: id, index },
                            solution: None,
                        },
                    );
                    if let Err(problem) =
                        self.prove(uf, rank, sub, assumptions, remaining, depth + 1)
                    {
                        self.predicates.detach(uf, sub);
                        return Err(problem);
                    }
                    subs.push(sub);
                }
                self.predicates.solve(
                    uf,
                    id,
                    Solution::Impl {
                        impl_: nash_ast::ImplRef {
                            home: info.home,
                            key,
                        },
                        type_vars,
                        subs,
                    },
                );
                Ok(())
            }
            crate::resolve::Selection::Limit => Err(CoverageProblem::Limit),
            crate::resolve::Selection::Deferred | crate::resolve::Selection::Missing => {
                match self.cover(
                    uf,
                    rank,
                    id,
                    trait_,
                    &args,
                    assumptions,
                    remaining,
                    depth + 1,
                )? {
                    Some(subs) => {
                        self.predicates.solve(uf, id, Solution::Specialize { subs });
                        Ok(())
                    }
                    None => Err(CoverageProblem::Uncovered),
                }
            }
        }
    }

    fn formed_at(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        roots: &[Variable],
        region: nash_region::Region,
    ) {
        self.dependencies.remember(uf, roots.iter().copied());
        self.value_roots
            .extend(roots.iter().map(|variable| (*variable, region)));
        let Some(owner) = self.owners.last().copied() else {
            return;
        };
        let site = UseSite {
            node: owner,
            region,
            name: "type application",
        };
        let mut pending = roots.to_vec();
        let mut seen = BTreeSet::new();
        while let Some(variable) = pending.pop() {
            let variable = uf.find(variable);
            if !seen.insert(variable) {
                continue;
            }
            let mut allocated = Vec::new();
            nash_constrain::instantiate::normalize_variable(uf, variable, &mut allocated);
            self.introduce(uf, rank, &allocated);
            let content = uf.get(variable).content.clone();
            let bodies = match &content {
                Content::Structure(FlatType::App1(_, _, args)) => {
                    pending.extend(args);
                    self.application_context(uf, rank, variable, &[])
                        .unwrap_or_default()
                }
                Content::Alias { args, real, .. } => {
                    pending.extend(args.iter().map(|(_, var)| var));
                    pending.push(*real);
                    self.application_context(uf, rank, variable, &[])
                        .unwrap_or_default()
                }
                Content::PartialAlias { args, .. } => {
                    pending.extend(args.iter().map(|(_, var)| var));
                    self.application_context(uf, rank, variable, &[])
                        .unwrap_or_default()
                }
                Content::Structure(FlatType::AppV1(head, args)) => {
                    pending.push(*head);
                    pending.extend(args);
                    self.application_context(uf, rank, *head, args)
                        .unwrap_or_else(|| {
                            vec![Body::Apply {
                                head: *head,
                                args: args.clone(),
                            }]
                        })
                }
                Content::Structure(FlatType::Fun1(from, to)) => {
                    pending.extend([*from, *to]);
                    Vec::new()
                }
                Content::Structure(FlatType::Tuple1(first, second, rest)) => {
                    pending.extend([*first, *second]);
                    pending.extend(rest);
                    Vec::new()
                }
                Content::Structure(FlatType::Record1(fields)) => {
                    pending.extend(fields.values());
                    Vec::new()
                }
                _ => Vec::new(),
            };
            for body in bodies {
                let exists = self.predicates.iter().any(|predicate| matches!(predicate.origin, Origin::Formation { site: previous, .. } if previous.node == owner) && predicate.body.same(uf, &body));
                if exists {
                    continue;
                }
                let id = self.predicates.push(
                    uf,
                    Predicate {
                        body,
                        origin: Origin::Formation {
                            site,
                            typ: variable,
                        },
                        solution: None,
                    },
                );
                self.wanted.push((rank, id));
            }
        }
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
            self.has_poison = true;
            crate::recovery::poison(uf, [variable]);
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
                .args()
                .iter()
                .map(|arg| self.src_type_to_var(uf, rank, &flex_vars, arg))
                .collect();
            let body = match *predicate {
                nash_ast::Pred::Trait { trait_, .. } => Body::Trait {
                    trait_,
                    args,
                    hidden: false,
                },
                nash_ast::Pred::Implied { trait_, .. } => Body::Trait {
                    trait_,
                    args,
                    hidden: true,
                },
                nash_ast::Pred::Apply { head, .. } => Body::Apply {
                    head: self.src_type_to_var(uf, rank, &flex_vars, head),
                    args,
                },
            };
            let id = self.predicates.push(
                uf,
                Predicate {
                    body,
                    solution: None,
                    origin: Origin::Use { site, index },
                },
            );
            self.wanted.push((rank, id));
            predicates.push(id);
        }
        self.freeze_kind_contracts(
            uf,
            typ,
            &predicates,
            &flex_vars.values().copied().collect::<Vec<_>>(),
            site.region,
        );
        self.uses.push(UseRecord {
            site,
            variable: typ,
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

    fn freeze_kind_contracts(
        &mut self,
        uf: &mut UnionFind<'a>,
        root: Variable,
        ids: &[type_::PredId],
        quantified: &[Variable],
        region: nash_region::Region,
    ) {
        self.dependencies.remember(uf, [root]);
        if crate::recovery::is_poisoned(uf, [root]) {
            return;
        }
        let bodies = ids
            .iter()
            .map(|id| self.predicates.get(*id).body.clone())
            .collect::<Vec<_>>();
        match crate::kind_check::freeze(
            self.bump,
            uf,
            &self.tables.kinds,
            Located::at(region, root),
            &bodies,
            quantified,
            &mut self.kind_contracts,
        ) {
            Ok(contracts) => {
                for contract in contracts {
                    self.kind_contracts.push(uf, contract);
                }
            }
            Err(failure) => {
                self.kind_errors.push(failure.error);
                self.has_poison = true;
                self.dependencies.invalidate(uf, [failure.variable]);
                self.propagate_poison(uf);
            }
        }
    }

    fn record_definitions(
        &mut self,
        uf: &mut UnionFind<'a>,
        _rank: usize,
        definitions: &[Definition<'a>],
        declared: &BTreeMap<&'a str, &'a [type_::PredId]>,
        inferred: &'a [type_::PredId],
        binder: Option<type_::Binder<'a>>,
    ) {
        for definition in definitions {
            let binding = Binding {
                declared_quantifiers: &[],
                variable: definition.typ,
                context: declared
                    .get(definition.site.name().value)
                    .copied()
                    .unwrap_or(inferred),
                definition: Some(definition.site.node()),
                context_is_final: true,
            };
            let mut pending = vec![binding.variable];
            for id in binding.context {
                pending.extend(self.predicates.get(*id).body.roots());
            }
            let quantified: Vec<_> = Self::type_variables(uf, pending)
                .into_iter()
                .filter(|var| uf.get(*var).rank == NO_RANK)
                .collect();
            if self.failed_definitions.contains(&definition.site.node())
                || binder.is_some_and(|binder| self.failed_definitions.contains(&binder.node()))
            {
                crate::recovery::poison_roots(uf, [binding.variable]);
            }
            self.freeze_kind_contracts(
                uf,
                binding.variable,
                binding.context,
                &quantified,
                definition.site.name().region,
            );
            self.schemes.push(SchemeRecord {
                site: definition.site,
                binding,
                quantified,
                binder: binder.unwrap_or(definition.site).node(),
                parent: self.owners.last().copied(),
            });
        }
        let failed: Vec<_> = self.failed_definitions.iter().copied().collect();
        for definition in failed {
            self.fail_definition(uf, definition);
        }
        let pending = std::mem::take(&mut self.recursive_uses);
        for use_index in pending {
            let use_ = &self.uses[use_index];
            let site = use_.site;
            let UseSource::Local { definition, .. } = use_.source else {
                unreachable!("recursive local use")
            };
            let Some(scheme) = self.schemes.at(definition) else {
                self.recursive_uses.push(use_index);
                continue;
            };
            let slots = crate::preds::ContextSlots::new(
                scheme
                    .binding
                    .context
                    .iter()
                    .map(|id| &self.predicates.get(*id).body),
            );
            for (index, id) in scheme.binding.context.iter().enumerate() {
                let pred = self.predicates.get(*id);
                let id = self.predicates.push(
                    uf,
                    Predicate {
                        body: pred.body.clone(),
                        origin: Origin::Use { site, index },
                        solution: None,
                    },
                );
                // Untyped recursive calls use the group's monomorphic type
                // variables and pass its final context through unchanged.
                if let Some(slot) = slots.slot(index) {
                    self.predicates
                        .solve_given(uf, id, scheme.binder, slot, Vec::new());
                } else {
                    self.predicates.solve(
                        uf,
                        id,
                        crate::preds::Solution::Apply { subs: Vec::new() },
                    );
                }
                self.uses[use_index].predicates.push(id);
            }
        }
    }

    fn declared_contexts(
        &mut self,
        uf: &mut UnionFind<'a>,
        _rank: usize,
        definitions: &[Definition<'a>],
        declarations: &[Definition<'a>],
    ) -> BTreeMap<&'a str, &'a [type_::PredId]> {
        let mut contexts = BTreeMap::new();
        for definition in definitions.iter().chain(declarations) {
            let Some(context) = definition.context else {
                continue;
            };
            let mut ids = Vec::new();
            for (index, pred) in context.iter().enumerate() {
                let body = pred.clone();
                ids.push(self.predicates.push(
                    uf,
                    Predicate {
                        body,
                        solution: None,
                        origin: Origin::Annotation {
                            binder: definition.site.node(),
                            index,
                        },
                    },
                ));
            }
            let root = definition.typ;
            let mut roots = vec![root];
            for id in &ids {
                roots.extend(self.predicates.get(*id).body.roots());
            }
            let quantified: Vec<_> = Self::type_variables(uf, roots).into_iter().collect();
            self.freeze_kind_contracts(uf, root, &ids, &quantified, definition.site.name().region);
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
        if binding
            .definition
            .is_some_and(|definition| self.failed_definitions.contains(&definition))
        {
            return self.register(uf, rank, Content::Error);
        }
        let mut roots = vec![binding.variable];
        if let Some(scheme) = binding
            .definition
            .and_then(|definition| self.schemes.at(definition))
        {
            // A destructured name instantiates its whole aggregate scheme,
            // including quantifiers absent from this selected component.
            roots.push(scheme.binding.variable);
        }
        let context_offset = roots.len();
        for id in binding.context {
            roots.extend(self.predicates.get(*id).body.roots());
        }
        let (copies, pairs) =
            self.make_scheme_copies(uf, rank, &roots, binding.declared_quantifiers);
        let mut predicates = Vec::new();
        let mut offset = context_offset;
        for (index, id) in binding.context.iter().enumerate() {
            let predicate = self.predicates.get(*id);
            let end = offset + predicate.body.roots().count();
            let mut copied_roots = copies[offset..end].iter().copied();
            let body = predicate
                .body
                .map_variables(|_| copied_roots.next().expect("one copy per predicate root"));
            let predicate = Predicate {
                body,
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
                variable: copies[0],
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

    /// Group representation requirements by subject, as `in_class` compares
    /// them, and report each subject that no representation admits. The
    /// subject is poisoned, so obligations on it are dropped instead of
    /// reaching impl selection. A requirement that another one implies is left
    /// out of the report.
    fn contradictory_representations(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        requirements: Vec<(Variable, nash_ast::primitives::ReprTrait)>,
        binder: &'a Located<&'a str>,
    ) -> Vec<Error<'a>> {
        use nash_ast::primitives::{ReprSet, ReprTrait};
        let mut subjects: Vec<(Variable, Vec<ReprTrait>)> = Vec::new();
        let mut allocated = Vec::new();
        for (argument, required) in requirements {
            let subject =
                crate::representation::subject(uf, &self.tables.kinds, argument, &mut allocated);
            if crate::recovery::is_poisoned(uf, [subject]) {
                continue;
            }
            if let Some((_, found)) = subjects
                .iter_mut()
                .find(|(other, _)| crate::preds::same_args(uf, &[*other], &[subject]))
            {
                if !found.contains(&required) {
                    found.push(required);
                }
            } else {
                subjects.push((subject, vec![required]));
            }
        }
        self.introduce(uf, rank, &allocated);
        let mut errors = Vec::new();
        for (subject, requirements) in subjects {
            let admitted = requirements.iter().fold(ReprSet::ALL, |set, requirement| {
                set.intersect(requirement.admits())
            });
            if !admitted.is_empty() {
                continue;
            }
            let shown: Vec<_> = requirements
                .iter()
                .copied()
                .filter(|requirement| {
                    !requirements.iter().any(|other| {
                        other.admits() != requirement.admits()
                            && other.admits().intersect(requirement.admits()) == other.admits()
                    })
                })
                .collect();
            errors.push(Error::ContradictoryRepresentation {
                region: binder.region,
                name: binder.value,
                typ: to_error_type(self.bump, uf, subject),
                requirements: self.bump.alloc_slice_copy(&shown),
            });
            self.has_poison = true;
            crate::recovery::poison(uf, [subject]);
        }
        self.propagate_poison(uf);
        errors
    }

    fn check_ambiguity(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        start: usize,
        definitions: &[Definition<'a>],
        binder: &'a Located<&'a str>,
    ) -> (Vec<Error<'a>>, bool) {
        // Check the conjunction before defaulting or generalizing. An
        // impossible representation context cannot be exported as a scheme,
        // even when its variable is reachable from the inferred result.
        let mut requirements = Vec::new();
        for (_, id) in &self.wanted[start..] {
            if self.predicate_blocked(uf, *id) {
                continue;
            }
            requirements.extend(representation_requirement(&self.predicates.get(*id).body));
        }
        let mut errors = self.contradictory_representations(uf, rank, requirements, binder);
        let roots: Vec<_> = definitions.iter().map(|def| def.typ).collect();
        let mut reachable = Self::type_variables(uf, roots);
        if definitions
            .iter()
            .all(|definition| definition.context.is_none())
        {
            // Preserve relational hidden variables while the determining argument
            // is generalized. Concrete calls must still solve their hidden types.
            loop {
                let before = reachable.len();
                for (_, id) in &self.wanted[start..] {
                    if self.predicate_blocked(uf, *id) {
                        continue;
                    }
                    let Body::Trait { args, .. } = &self.predicates.get(*id).body else {
                        continue;
                    };
                    if args.len() < 2 {
                        continue;
                    }
                    let sets: Vec<_> = args
                        .iter()
                        .map(|arg| Self::type_variables(uf, vec![*arg]))
                        .collect();
                    if sets.iter().any(|vars| {
                        !vars.is_empty()
                            && vars.is_subset(&reachable)
                            && vars.iter().all(|var| uf.get(*var).rank == NO_RANK)
                    }) {
                        for vars in sets {
                            reachable.extend(vars);
                        }
                    }
                }
                if reachable.len() == before {
                    break;
                }
            }
        }
        self.dependencies.sync(uf);
        let mut ambiguous: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (_, id) in &self.wanted[start..] {
            if self.predicate_blocked(uf, *id) {
                continue;
            }
            let variables =
                Self::type_variables(uf, self.predicates.get(*id).body.roots().collect());
            for var in variables {
                if uf.get(var).rank == NO_RANK
                    && !reachable.contains(&var)
                    && !self.dependencies.is_stranded(uf, var)
                {
                    ambiguous.entry(var).or_default().push(*id);
                }
            }
        }
        if !ambiguous.is_empty() {
            let mut roots: Vec<_> = reachable.iter().copied().collect();
            for (var, ids) in &ambiguous {
                roots.push(*var);
                for id in ids {
                    roots.extend(self.predicates.get(*id).body.roots());
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
                    pred.body.same(uf, &other.body)
                }) {
                    distinct.push(*id);
                }
            }
            let defaults: BTreeMap<_, _> = distinct
                .iter()
                .filter_map(|id| {
                    let pred = self.predicates.get(*id);
                    if let Body::Trait { trait_, args, .. } = &pred.body
                        && args.len() == 1
                        && uf.equivalent(args[0], var)
                    {
                        type_::literal_default(*trait_).map(|typ| (*trait_, typ))
                    } else {
                        None
                    }
                })
                .collect();
            if defaults.len() == 1 && matches!(uf.get(var).content, Content::FlexVar(_)) {
                let typ = defaults.into_values().next().unwrap();
                let target = self.structure(uf, rank, typ);
                if matches!(self.unify(uf, var, target), unify::Answer::Ok(_)) {
                    defaulted = true;
                    continue;
                }
            }
            unresolved.push((var, ids, distinct));
        }
        // A default can unlock an impl which supplies a literal constraint
        // for another hidden variable. Retry before declaring ambiguity.
        if defaulted {
            return (errors, true);
        }
        let mut rejected = BTreeSet::new();
        for (var, ids, distinct) in unresolved {
            let predicates: Vec<_> = distinct
                .iter()
                .filter_map(|id| {
                    let Body::Trait { trait_, args, .. } = &self.predicates.get(*id).body else {
                        return None;
                    };
                    Some(nash_constrain::error::AmbiguousPredicate {
                        trait_: *trait_,
                        args: self.bump.alloc_slice_fill_iter(
                            args.iter().map(|arg| to_error_type(self.bump, uf, *arg)),
                        ),
                    })
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
            self.failed_predicates.insert(*id);
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
                Content::Structure(FlatType::Record1(fields)) => {
                    pending.extend(fields.values());
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
            if self.predicate_blocked(uf, id) {
                self.predicates.detach(uf, id);
                continue;
            }
            let variables =
                Self::type_variables(uf, self.predicates.get(id).body.roots().collect());
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
        retained.sort_unstable();
        retained.dedup();
        let closures: Vec<_> = retained
            .iter()
            .map(|id| {
                let mut closure = vec![Given {
                    body: self.predicates.get(*id).body.clone(),
                    index: None,
                    path: Vec::new(),
                }];
                self.expand_givens(uf, rank, &mut closure);
                closure
            })
            .collect();
        // A retained Big requirement is a proof premise, just like an explicit
        // given. Use it for compiler-owned rules without narrowing a variable.
        let mut derived = BTreeMap::new();
        let mut allocated = Vec::new();
        for id in &retained {
            let Body::Trait { trait_, args, .. } = &self.predicates.get(*id).body else {
                continue;
            };
            let equality = *trait_ == nash_ast::primitives::eq_trait()
                && self.tables.has_structural_eq()
                && args.len() == 1;
            let lift = *trait_ == nash_ast::primitives::lift_trait()
                && self.tables.has_reflexive_lift()
                && args.len() == 2
                && crate::preds::same_args(uf, &args[..1], &args[1..]);
            if lift
                || (equality
                    && closures.iter().flatten().any(|given| {
                        crate::representation::same_predicate(
                            uf,
                            &self.tables.kinds,
                            &given.body,
                            &Body::Trait {
                                trait_: nash_ast::primitives::ReprTrait::Big.qualified(),
                                args: vec![args[0]],
                                hidden: true,
                            },
                            &mut allocated,
                        )
                    }))
            {
                derived.insert(
                    *id,
                    if equality {
                        crate::preds::Solution::StructuralEq { typ: args[0] }
                    } else {
                        crate::preds::Solution::ReflexiveLift { typ: args[0] }
                    },
                );
            }
        }
        self.introduce(uf, rank, &allocated);
        let survivors: Vec<_> = retained
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let body = &self.predicates.get(*id).body;
                let implied = closures.iter().enumerate().any(|(other, closure)| {
                    other != index
                        && closure.iter().any(|given| {
                            (other < index || !given.path.is_empty()) && given.body.same(uf, body)
                        })
                });
                (!implied && !derived.contains_key(id)).then_some(index)
            })
            .collect();
        let slots = crate::preds::ContextSlots::new(
            survivors
                .iter()
                .map(|index| &self.predicates.get(retained[*index]).body),
        );
        for id in &retained {
            if let Some(solution) = derived.remove(id) {
                self.predicates.solve(uf, *id, solution);
                continue;
            }
            let body = &self.predicates.get(*id).body;
            let (index, path) = survivors
                .iter()
                .enumerate()
                .find_map(|(index, survivor)| {
                    closures[*survivor].iter().find_map(|given| {
                        given
                            .body
                            .same(uf, body)
                            .then(|| (index, given.path.clone()))
                    })
                })
                .expect("acyclic superclass reduction leaves a context root");
            if let Some(slot) = slots.slot(index) {
                self.predicates.solve_given(uf, *id, binder, slot, path);
            } else {
                self.predicates
                    .solve(uf, *id, crate::preds::Solution::Apply { subs: Vec::new() });
            }
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
        let roots: Vec<_> = roots
            .iter()
            .map(|root| self.make_copy_help(uf, rank, *root, quantified))
            .collect();
        let copied = std::mem::take(&mut self.copied);
        let recorded = self.kind_contracts.len();
        for (original, copy) in &copied {
            for contract in self.kind_contracts.on_class(uf, *original, recorded) {
                self.kind_contracts.push(
                    uf,
                    crate::kind_check::Contract {
                        variable: *copy,
                        owner: roots[0],
                        ..contract
                    },
                );
            }
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

            FlatType::Record1(fields) => {
                let field_copies: BTreeMap<&'a str, Variable> = fields
                    .iter()
                    .map(|(name, var)| (*name, self.make_copy_help(uf, max_rank, *var, quantified)))
                    .collect();
                FlatType::Record1(field_copies)
            }

            FlatType::Tuple1(a, b, rest) => {
                let a_copy = self.make_copy_help(uf, max_rank, a, quantified);
                let b_copy = self.make_copy_help(uf, max_rank, b, quantified);
                let c_copy = rest
                    .into_iter()
                    .map(|c| self.make_copy_help(uf, max_rank, c, quantified))
                    .collect();
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

            FlatType::Record1(fields) => fields.values().fold(OUTERMOST_RANK, |rank, field| {
                rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *field))
            }),

            FlatType::Tuple1(a, b, rest) => {
                let a_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *a);
                let b_rank = adjust_rank(uf, young_mark, visit_mark, group_rank, *b);
                let ab_rank = a_rank.max(b_rank);
                rest.iter().fold(ab_rank, |rank, c| {
                    rank.max(adjust_rank(uf, young_mark, visit_mark, group_rank, *c))
                })
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

    fn primitive_state<'a>() -> State<'a> {
        State {
            env: Env::new(),
            mark: NO_MARK.next(),
            errors: Vec::new(),
        }
    }

    fn fresh<'a>(solver: &mut Solver<'a, '_>, uf: &mut UnionFind<'a>, rank: usize) -> Variable {
        solver.register(uf, rank, Content::FlexVar(None))
    }

    /// Solve one `Equal` constraint between two existing variables.
    fn equal<'a>(
        solver: &mut Solver<'a, '_>,
        uf: &mut UnionFind<'a>,
        state: State<'a>,
        region: Region,
        category: Category<'a>,
        actual: Variable,
        expected: Variable,
    ) -> State<'a> {
        let bump = solver.bump;
        let constraint = Constraint::Equal(
            region,
            category,
            bump.alloc(Type::VarN(actual)),
            Expected::NoExpectation(bump.alloc(Type::VarN(expected))),
        );
        solver.solve(uf, &Env::new(), OUTERMOST_RANK, state, &constraint)
    }

    fn primitive_nullary<'a>(
        solver: &mut Solver<'a, '_>,
        uf: &mut UnionFind<'a>,
        name: &'a str,
    ) -> Variable {
        solver.structure(
            uf,
            OUTERMOST_RANK,
            FlatType::App1(nash_ast::primitives::primitive_home(), name, Vec::new()),
        )
    }

    fn primitive_unit_function<'a>(
        solver: &mut Solver<'a, '_>,
        uf: &mut UnionFind<'a>,
    ) -> Variable {
        // Each occurrence of the old structural source type was converted afresh,
        // including its two identical unit children.
        let input = primitive_nullary(solver, uf, "unit");
        let output = primitive_nullary(solver, uf, "unit");
        solver.structure(uf, OUTERMOST_RANK, FlatType::Fun1(input, output))
    }

    #[test]
    fn a_partial_constructor_cannot_be_a_value_type() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let actual = primitive_nullary(&mut solver, &mut uf, "list");
        let expected = primitive_nullary(&mut solver, &mut uf, "list");
        let state = equal(
            &mut solver,
            &mut uf,
            primitive_state(),
            nash_region::Region::zero(),
            Category::List,
            actual,
            expected,
        );
        let errors = solver.finish_state(&mut uf, state).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| matches!(error, Error::BadKind { .. })),
            "{errors:#?}"
        );
    }

    #[test]
    fn recovery_collects_final_kind_errors_after_independent_type_failures() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let at = |line| nash_region::Region {
            start: nash_region::Position { line, column: 1 },
            end: nash_region::Position { line, column: 2 },
        };
        let function = primitive_unit_function(&mut solver, &mut uf);
        let unit = primitive_nullary(&mut solver, &mut uf, "unit");
        let mut state = equal(
            &mut solver,
            &mut uf,
            primitive_state(),
            at(1),
            Category::Lambda,
            function,
            unit,
        );
        for line in [2, 3] {
            // Four separate list nodes across these two equalities: sharing the
            // source description never meant sharing its materialized UF graph.
            let actual = primitive_nullary(&mut solver, &mut uf, "list");
            let expected = primitive_nullary(&mut solver, &mut uf, "list");
            state = equal(
                &mut solver,
                &mut uf,
                state,
                at(line),
                Category::List,
                actual,
                expected,
            );
        }
        let errors = solver.finish_state(&mut uf, state).unwrap_err();
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::BadExpr(..)))
                .count(),
            1,
            "{errors:#?}"
        );
        assert_eq!(
            errors
                .iter()
                .filter(|error| matches!(error, Error::BadKind { .. }))
                .count(),
            2,
            "{errors:#?}"
        );
    }

    #[test]
    fn recovery_retains_shared_heads_removed_by_successful_normalization() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let rank = OUTERMOST_RANK;
        let head = fresh(&mut solver, &mut uf, rank);
        let result = fresh(&mut solver, &mut uf, rank);
        let region = nash_region::Region::zero();

        let argument = primitive_nullary(&mut solver, &mut uf, "unit");
        let applied = solver.structure(&mut uf, rank, FlatType::AppV1(head, vec![argument]));
        let mut state = equal(
            &mut solver,
            &mut uf,
            primitive_state(),
            region,
            Category::List,
            result,
            applied,
        );

        let argument = primitive_nullary(&mut solver, &mut uf, "unit");
        let list = solver.structure(
            &mut uf,
            rank,
            FlatType::App1(
                nash_ast::primitives::primitive_home(),
                "list",
                vec![argument],
            ),
        );
        state = equal(
            &mut solver,
            &mut uf,
            state,
            region,
            Category::List,
            result,
            list,
        );

        let function = primitive_unit_function(&mut solver, &mut uf);
        state = equal(
            &mut solver,
            &mut uf,
            state,
            region,
            Category::List,
            result,
            function,
        );

        // Rebuild this entire function graph after the previous failing equality.
        // Only the explicit head/result variables are shared between operations.
        let function = primitive_unit_function(&mut solver, &mut uf);
        state = equal(
            &mut solver,
            &mut uf,
            state,
            region,
            Category::List,
            head,
            function,
        );
        let errors = solver.finish_state(&mut uf, state).unwrap_err();
        assert_eq!(
            errors.len(),
            1,
            "normalization must not sever the dependency from the applied type to its shared head: {errors:#?}"
        );
        assert!(matches!(errors[0], Error::BadExpr(..)), "{errors:#?}");
    }

    #[test]
    fn apply_head_and_arguments_share_the_signature_substitution() {
        use nash_ast::Pred;

        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let f = fresh(&mut solver, &mut uf, 2);
        let a = fresh(&mut solver, &mut uf, 2);
        let scope = BTreeMap::from([("f", f), ("a", a)]);
        let head = &*bump.alloc(Located::at_zero(CanType::Var("f")));
        let arg = &*bump.alloc(Located::at_zero(CanType::Var("a")));
        let args = bump.alloc_slice_copy(&[arg]);
        let application =
            solver.canonical_predicate(&mut uf, 2, &scope, Pred::Apply { head, args });
        let Body::Apply {
            head: actual_head,
            args: actual_args,
        } = application
        else {
            panic!("Apply preserved")
        };
        assert_eq!(actual_head, f);
        assert_eq!(actual_args, [a]);
        let representation = solver.canonical_predicate(
            &mut uf,
            2,
            &scope,
            Pred::Implied {
                trait_: nash_ast::primitives::ReprTrait::Big.qualified(),
                args,
            },
        );
        let Body::Trait {
            hidden,
            args,
            trait_,
        } = representation
        else {
            panic!("representation predicate preserved")
        };
        assert!(hidden);
        assert_eq!(trait_, nash_ast::primitives::ReprTrait::Big.qualified());
        assert_eq!(args, [a]);
        let signature = Located::at_zero(CanType::App {
            head,
            args: bump.alloc_slice_copy(&[arg]),
        });
        let signature = solver.src_type_to_var(&mut uf, 2, &scope, &signature);
        let Content::Structure(FlatType::AppV1(type_head, type_args)) = &uf.get(signature).content
        else {
            panic!("type application preserved")
        };
        assert_eq!(*type_head, actual_head);
        assert_eq!(type_args, &actual_args);
    }

    use nash_constrain::type_::{PredId, make_descriptor};

    fn evidence_name(name: nash_ast::QualifiedName<'_>) -> String {
        let package = name
            .home
            .package
            .map(|package| format!("{}/{}/", package.author, package.project))
            .unwrap_or_default();
        format!("{package}{}.{}", name.home.name, name.name)
    }

    fn evidence_type(typ: &nash_region::Located<nash_ast::Type<'_>>) -> String {
        match &typ.value {
            nash_ast::Type::Var(name) => name.to_string(),
            nash_ast::Type::Named { reference, args } => {
                let args = args
                    .iter()
                    .map(|arg| evidence_type(arg))
                    .collect::<Vec<_>>();
                format!("{}[{}]", evidence_name(*reference), args.join(", "))
            }
            other => format!("{other:?}"),
        }
    }

    fn render_evidence(solver: &Solver<'_, '_>, evidence: &nash_ast::Evidence<'_>) -> String {
        use nash_ast::Evidence;
        match evidence {
            Evidence::Repr { trait_, typ } => {
                format!("Repr {} {}", trait_.name(), evidence_type(typ))
            }
            Evidence::Given { binder, index } => {
                let owner = solver
                    .schemes
                    .iter()
                    .find(|scheme| scheme.site.node() == *binder)
                    .expect("evidence owner must be a retained definition");
                format!(
                    "Given {}@{}:{}#{index}",
                    owner.site.name().value,
                    owner.site.name().region.start.line,
                    owner.site.name().region.start.column
                )
            }
            Evidence::Super { of, index } => {
                format!("Super {index} ({})", render_evidence(solver, of))
            }
            Evidence::StructuralEq { typ } => format!("StructuralEq {}", evidence_type(typ)),
            Evidence::ReflexiveLift { typ } => format!("ReflexiveLift {}", evidence_type(typ)),
            Evidence::Specialize { trait_, args } => format!(
                "Specialize {} [{}]",
                evidence_name(*trait_),
                args.iter()
                    .map(|typ| evidence_type(typ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Evidence::Impl {
                impl_,
                type_args,
                args,
            } => format!(
                "Impl {} [{}] [{}] [{}]",
                evidence_name(impl_.key.trait_),
                impl_
                    .key
                    .heads
                    .iter()
                    .map(|head| match head {
                        nash_ast::Head::Named {
                            reference,
                            args: [],
                        } => evidence_name(*reference),
                        other => format!("{other:?}"),
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                type_args
                    .iter()
                    .map(|typ| evidence_type(typ))
                    .collect::<Vec<_>>()
                    .join(", "),
                args.iter()
                    .map(|arg| render_evidence(solver, arg))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        }
    }

    macro_rules! assert_evidence_snapshot {
        ($source:expr, $solver:expr, $solved:expr) => {{
            let solver = &$solver;
            let solved = &$solved;
            let mut uses = solver.uses.iter().collect::<Vec<_>>();
            uses.sort_by_key(|use_| {
                (
                    use_.site.region.start.line,
                    use_.site.region.start.column,
                    use_.site.name,
                )
            });
            let lines = uses
                .into_iter()
                .map(|use_| {
                    let instance = &solved.instances[&use_.site.node];
                    format!(
                        "{}@{}:{} : [{}] [{}]",
                        use_.site.name,
                        use_.site.region.start.line,
                        use_.site.region.start.column,
                        instance
                            .type_args
                            .iter()
                            .map(|typ| evidence_type(typ))
                            .collect::<Vec<_>>()
                            .join(", "),
                        instance
                            .evidence
                            .iter()
                            .map(|evidence| render_evidence(solver, evidence))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            insta::with_settings!({description => $source, omit_expression => true}, {
                insta::assert_snapshot!(lines);
            });
        }};
    }

    #[test]
    fn recovery_work_limit_does_not_drop_the_next_independent_predicate() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver {
            bump: &bump,
            tables: &tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
        };
        let mut uf = UnionFind::new();
        let name = bump.alloc(Located::at_zero("value"));
        let binder = type_::Binder::Named(name);
        let mut ids = Vec::new();
        for trait_name in ["Expanding", "Missing"] {
            let variable = solver.structure(
                &mut uf,
                OUTERMOST_RANK,
                FlatType::App1(nash_ast::primitives::primitive_home(), "unit", Vec::new()),
            );
            let id = solver.predicates.push(
                &mut uf,
                Predicate {
                    body: Body::Trait {
                        trait_: nash_ast::QualifiedName {
                            home: nash_ast::ModuleName {
                                package: None,
                                name: "Main",
                            },
                            name: trait_name,
                        },
                        args: vec![variable],
                        hidden: false,
                    },
                    origin: Origin::Use {
                        site: UseSite {
                            node: binder.node(),
                            region: name.region,
                            name: trait_name,
                        },
                        index: 0,
                    },
                    solution: None,
                },
            );
            solver.wanted.push((OUTERMOST_RANK, id));
            ids.push(id);
        }
        solver.resolution_work.insert(ids[0], WORK_LIMIT);
        let result = solver.resolve_wanted(
            &mut uf,
            OUTERMOST_RANK,
            State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            },
            0,
            Some(binder),
            false,
        );
        assert!(
            matches!(result.errors.as_slice(), [Error::ImplResolutionLimit { trait_: first, .. }, Error::MissingImpl { trait_: second, .. }] if first.name == "Expanding" && second.name == "Missing"),
            "{:?}",
            result.errors
        );
        assert!(solver.wanted.is_empty());
    }

    #[test]
    fn superclass_givens_record_transitive_paths_and_substitute_arguments() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntype Container 'a = Wrap 'a\ntrait Eq 'a where\n    eq : 'a -> 'a\ntrait Eq 'a => Ord 'a where\n    ord : 'a -> 'a\ntrait Ord 'a => Top 'a where\n    top : 'a -> 'a\ntrait Eq 'b => Select 'a 'b where\n    select : 'a -> 'b -> 'a\nf : Top 'a => 'a -> 'a\nf x = eq x\ng : Select 'a 'b => 'a -> 'b -> 'b\ng x y = eq y\nh : (Top 'a, Eq 'a) => 'a -> 'a\nh x = eq x\n";
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
        let (_, solved) = solver.finish(&mut uf, &result.env).unwrap();
        assert_evidence_snapshot!(source, solver, solved);
    }

    #[test]
    fn givens_discharge_body_uses_without_escaping_their_scope() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nf : Keep 'a => 'a -> 'a\nf x = keep x\ng : Keep () => () -> ()\ng x = keep x\nh : () -> ()\nh x = keep x\n";
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
        let localizer = nash_report::localizer::Localizer::from_module(&parsed, &[]);
        insta::with_settings!({description => source, omit_expression => true, info => &"diagnostic"}, {
            insta::assert_snapshot!(result.errors.iter().map(|error|
                nash_report::render_plain(
                    &nash_report::type_::to_report(&localizer, error),
                    &nash_report::Source::new(source), "Main.nash"
                )
            ).collect::<Vec<_>>().join("\n"));
        });
        assert!(
            matches!(&result.errors[..], [Error::MissingImpl { region, .. }] if region.start.line == 9)
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
            assert_eq!(solved, line != 9, "only f and g have enclosing givens");
        }
        assert!(result.env["h"].context.is_empty());
    }

    #[test]
    fn scheme_records_freeze_local_quantifiers_before_outer_generalization() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\nouter x =\n    let\n        local y = (x, y)\n    in\n    local Primitive.True\n";
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
        let Content::Structure(FlatType::Tuple1(capture, value, ref rest)) = uf.get(result).content
        else {
            panic!("local result")
        };
        assert!(rest.is_empty());
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
        let source = "module Main exposing (..)\ntype Color = Red\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep Color where\n    keep x = x\nimpl Keep 'a => Keep (list 'a) where\n    keep xs = xs\nvalue = keep [[Red]]\n";
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
            matches!(pred.origin, Origin::Use { site, .. } if site.region.start.line == 9 && site.name == "keep")
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
            Content::Structure(FlatType::App1(_, "list", _))
        ));
        assert_eq!(subs.len(), 2);
        assert!(matches!(
            solver.predicates.get(subs[1]).solution,
            Some(crate::preds::Solution::Repr {
                trait_: nash_ast::primitives::ReprTrait::Storable,
                ..
            })
        ));
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
            9
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
            Content::Structure(FlatType::App1(_, "Color", _))
        ));
        assert_eq!(subs.len(), 2);
        assert!(matches!(
            solver.predicates.get(subs[1]).solution,
            Some(crate::preds::Solution::Repr {
                trait_: nash_ast::primitives::ReprTrait::Storable,
                ..
            })
        ));
        assert!(
            matches!(solver.predicates.get(subs[0]).solution.as_ref(), Some(crate::preds::Solution::Impl { type_vars, subs, .. }) if type_vars.is_empty() && subs.is_empty())
        );
        let (_, solved) = solver.finish(&mut uf, &result.env).unwrap();
        assert_evidence_snapshot!(source, solver, solved);
    }

    #[test]
    fn retained_impl_children_and_recursive_uses_reference_final_context_slots() {
        let bump = Bump::new();
        let source = "module Main exposing (..)\ntype box 'a = Box 'a\ntrait Base 'a where\n    base : 'a -> 'a\ntrait Base 'a => Strong 'a where\n    strong : 'a -> 'a\ntrait Strong 'a => Top 'a where\n    top : 'a -> 'a\nimpl Base 'a => Base (box 'a) where\n    base xs = xs\nf x = (base (Box (let local y = g y in local x)), top x, [x])\ng x = case f x of\n    (xs, y, _) -> y\nh x = (f x, f x)\n";
        let parsed = nash_parse::Parser::new(&bump, source).module().unwrap();
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
        assert_eq!(result.env["f"].context.len(), 2);
        assert_eq!(
            result.env["h"].context.len(),
            2,
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
            if pred
                .body
                .trait_ref()
                .and_then(nash_ast::primitives::ReprTrait::of)
                .is_some()
            {
                continue;
            }
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
                        matches!(&pred.solution, Some(crate::preds::Solution::Super { binder, index: 0, path }) if *binder == group_binder && path == &[0, 0]),
                        "solution={:?} context={:?}",
                        pred.solution,
                        result.env["f"]
                            .context
                            .iter()
                            .map(|id| &solver.predicates.get(*id).body)
                            .collect::<Vec<_>>()
                    );
                }
                Origin::Use { site, .. } if site.region.start.line == 14 => {
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
                let expected_binder = if use_.site.region.start.line == 14 {
                    h_binder
                } else {
                    group_binder
                };
                assert!(matches!(
                    instance.evidence,
                    [nash_ast::Evidence::Given { binder, index: 0 }, nash_ast::Evidence::Given { binder: repr_binder, index: 1 }]
                        if *binder == expected_binder && *repr_binder == expected_binder
                ));
            }
            if use_.site.name == "base" {
                let [nash_ast::Evidence::Impl { args, .. }] = instance.evidence else {
                    panic!("box Base use must publish impl evidence")
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
        assert_evidence_snapshot!(source, solver, solved);
    }

    #[test]
    fn foreign_context_uses_the_same_fresh_variables_as_its_type() {
        use nash_ast::{Annotation, Expr, ModuleName, NodeId, Pred, QualifiedName};
        let bump = Bump::new();
        let mut tables = nash_can::environment::Tables::default();
        tables.kinds.traits.insert(
            QualifiedName {
                home: ModuleName {
                    package: None,
                    name: "Main",
                },
                name: "Keep",
            },
            &[&nash_ast::Kind::Type, &nash_ast::Kind::Type],
        );
        let mut solver = Solver {
            bump: &bump,
            tables: &tables,
            pools: vec![Vec::new(); 8],
            copied: Vec::new(),
            predicates: Store::default(),
            wanted: Vec::new(),
            givens: Vec::new(),
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
            context: bump.alloc_slice_fill_iter([Pred::Trait {
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
            assert_eq!(predicate.body.trait_ref(), Some(trait_));
            assert_eq!(predicate.body.args(), [arg, arg]);
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
            schemes: Schemes::default(),
            recursive_uses: Vec::new(),
            uses: Vec::new(),
            expr_types: Vec::new(),
            pattern_types: Vec::new(),
            owners: Vec::new(),
            resolution_work: std::collections::HashMap::new(),
            has_poison: false,
            dependencies: crate::recovery::Dependencies::default(),
            failed_lineages: BTreeSet::new(),
            failed_predicates: BTreeSet::new(),
            failed_definitions: std::collections::HashSet::new(),
            value_roots: Vec::new(),
            kind_contracts: Default::default(),
            kind_errors: Vec::new(),
            fields: Vec::new(),
            test_scope: false,
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
            vec![result, context_only, outer],
        ))));
        let roots = [result, tuple, context_only];
        let (first, first_pairs) = solver.make_copies(&mut uf, 2, &roots);
        let (second, second_pairs) = solver.make_copies(&mut uf, 2, &roots);
        assert_eq!(first_pairs.len(), 3);
        assert_eq!(second_pairs.len(), 3);
        for copies in [&first, &second] {
            let Content::Structure(FlatType::Tuple1(a, b, ref rest)) = uf.get(copies[1]).content
            else {
                panic!("copied context root");
            };
            assert_eq!(a, copies[0]);
            assert_eq!(b, copies[2]);
            assert_eq!(
                rest,
                &[copies[0], copies[2], outer],
                "tail copies must preserve sharing and outer variables"
            );
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
