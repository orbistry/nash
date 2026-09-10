//! Infer directly from canonical definitions. Only union-find variables cross scopes.
use super::*;
use nash_ast::{Decls, Def, Expr, Module, Pattern};
use nash_constrain::error::{Context, PCategory, PContext, SubContext};
use type_::Binder;

type Rtv<'a> = BTreeMap<&'a str, Variable>;
type Locals<'a> = Vec<(&'a str, Located<Variable>)>;
type Arguments<'a> = Vec<(&'a Located<Pattern<'a>>, PExpected<'a, Variable>)>;

#[derive(Clone, Copy)]
pub(super) struct Definition<'a> {
    pub site: Binder<'a>,
    pub typ: Variable,
    pub context: Option<&'a [Body<'a>]>,
}

pub(super) struct ScopeResult<'a> {
    pub env: Env<'a>,
    pub state: State<'a>,
    pub locals: Locals<'a>,
}

struct PreparedDefinition<'a> {
    definition: Definition<'a>,
    arguments: Arguments<'a>,
    source: &'a Def<'a>,
    expected: Expected<'a, Variable>,
    rtv: Rtv<'a>,
    rigids: &'a [Variable],
}

impl<'a> Solver<'a, '_> {
    pub(super) fn fresh(&mut self, uf: &mut UnionFind<'a>, rank: usize) -> Variable {
        self.register(uf, rank, Content::FlexVar(None))
    }

    pub(super) fn structure(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        typ: FlatType<'a>,
    ) -> Variable {
        let typ = type_::normalize_application(uf, typ);
        self.register(uf, rank, Content::Structure(typ))
    }

    pub(super) fn field(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        mut state: State<'a>,
        field: DeferredField<'a>,
    ) -> State<'a> {
        if let Some((_, typ)) = field.field {
            self.dependencies.field(typ, field.record);
        }
        self.fields.push(field);
        self.retry_fields(uf, rank, &mut state.errors);
        state
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn equal(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        state: State<'a>,
        region: Region,
        category: Category<'a>,
        actual: Variable,
        expectation: Expected<'a, Variable>,
    ) -> State<'a> {
        let expected = expectation_type(expectation);
        if let Expected::FromContext(_, Context::CallArity(_, arity), _) = expectation {
            let mut result = expected;
            let mut inputs = vec![actual];
            for _ in 0..arity {
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
            unify::Answer::Err(vars, actual, expected) => {
                self.introduce(uf, rank, &vars);
                add_error(
                    state,
                    Error::BadExpr(region, category, actual, expectation.type_replace(expected)),
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_equal(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        state: State<'a>,
        region: Region,
        category: PCategory<'a>,
        actual: Variable,
        expectation: PExpected<'a, Variable>,
    ) -> State<'a> {
        let expected = match expectation {
            PExpected::NoExpectation(t) | PExpected::FromContext(_, _, t) => t,
        };
        self.formed_at(uf, rank, &[actual, expected], region);
        match self.unify(uf, actual, expected) {
            unify::Answer::Ok(vars) => {
                self.introduce(uf, rank, &vars);
                state
            }
            unify::Answer::Err(vars, actual, expected) => {
                self.introduce(uf, rank, &vars);
                add_error(
                    state,
                    Error::BadPattern(region, category, actual, expectation.type_replace(expected)),
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn local(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        region: Region,
        node: nash_ast::NodeId,
        name: &'a str,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let binding = *env.get(name).expect("canonical local is bound");
        let actual = self.instantiate_binding(uf, rank, binding, UseSite { node, region, name });
        self.equal(
            uf,
            rank,
            state,
            region,
            Category::Local(name),
            actual,
            expected,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn foreign(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        state: State<'a>,
        region: Region,
        node: nash_ast::NodeId,
        name: &'a str,
        annotation: &'a nash_ast::Annotation<'a>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let actual =
            self.src_type_to_variable(uf, rank, UseSite { node, region, name }, annotation);
        self.equal(
            uf,
            rank,
            state,
            region,
            Category::Foreign(name),
            actual,
            expected,
        )
    }

    fn young_pool(&mut self, rank: usize) -> usize {
        let young = rank + 1;
        if young >= self.pools.len() {
            self.pools.resize(self.pools.len() * 2, Vec::new());
        }
        assert!(
            self.pools[young].is_empty(),
            "young pool belongs to one active scope"
        );
        young
    }

    fn generalize_scope(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        mut state: State<'a>,
    ) -> State<'a> {
        self.retry_fields(uf, rank, &mut state.errors);
        self.finish_fields(uf, rank, &mut state.errors);
        self.generalize(uf, state.mark, state.mark.next(), rank);
        self.pools[rank].clear();
        state.mark = state.mark.next().next();
        state
    }

    pub(super) fn close_locals(
        &mut self,
        uf: &mut UnionFind<'a>,
        state: State<'a>,
        locals: Locals<'a>,
    ) -> State<'a> {
        locals.into_iter().fold(state, |state, (name, loc)| {
            self.check_occurs(uf, state, name, loc)
        })
    }

    pub(super) fn infer_patterns(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        args: &[(&Located<Pattern<'a>>, PExpected<'a, Variable>)],
    ) -> ScopeResult<'a> {
        let owns = args
            .iter()
            .any(|(pattern, _)| super::patterns::pattern_owns_vars(pattern));
        let pattern_rank = if owns { self.young_pool(rank) } else { rank };
        let start = self.wanted.len();
        let mut headers = BTreeMap::new();
        for (pattern, expected) in args.iter().rev() {
            state = self.infer_pattern(uf, pattern_rank, state, pattern, *expected, &mut headers);
        }
        self.finish_pattern_scope(uf, env, rank, pattern_rank, state, start, headers)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_pattern_scope(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        pattern_rank: usize,
        mut state: State<'a>,
        start: usize,
        headers: BTreeMap<&'a str, Located<Variable>>,
    ) -> ScopeResult<'a> {
        self.retry_fields(uf, pattern_rank, &mut state.errors);
        state = self.resolve_wanted(uf, pattern_rank, state, start, None, false);
        if pattern_rank != rank {
            state = self.generalize_scope(uf, pattern_rank, state);
        }
        let locals: Locals<'a> = headers.into_iter().collect();
        let mut env = env.clone();
        for (name, loc) in &locals {
            env.entry(name).or_insert(Binding {
                variable: loc.value,
                context: &[],
                definition: None,
                context_is_final: true,
                declared_quantifiers: &[],
            });
        }
        ScopeResult { env, state, locals }
    }

    fn prepare_definition(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        rtv: &Rtv<'a>,
        def: &'a Def<'a>,
    ) -> PreparedDefinition<'a> {
        let mut rtv = rtv.clone();
        let mut rigids = Vec::new();
        let (name, arguments, result, context, annotation_region) = match def {
            Def::Def { name, args, .. } => {
                let arguments: Arguments<'a> = args
                    .iter()
                    .map(|pattern| (*pattern, PExpected::NoExpectation(self.fresh(uf, rank))))
                    .collect();
                (*name, arguments, self.fresh(uf, rank), None, None)
            }
            Def::TypedDef {
                name,
                free_vars,
                context,
                args,
                typ,
                annotation,
                ..
            } => {
                let mut names: Vec<_> = free_vars
                    .iter()
                    .copied()
                    .filter(|name| !rtv.contains_key(name))
                    .collect();
                names.sort_unstable();
                for name in names {
                    let var = self.register(uf, rank, Content::RigidVar(name));
                    rigids.push(var);
                    rtv.insert(name, var);
                }
                let arguments: Arguments<'a> = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let typ = self.src_type_to_var(uf, rank, &rtv, arg.typ);
                        (
                            arg.pattern,
                            PExpected::FromContext(
                                arg.pattern.region,
                                PContext::TypedArg(name.value, index, annotation.region),
                                typ,
                            ),
                        )
                    })
                    .collect();
                let result = self.src_type_to_var(uf, rank, &rtv, typ);
                let context: Vec<_> = context
                    .iter()
                    .map(|pred| self.canonical_predicate(uf, rank, &rtv, *pred))
                    .collect();
                (
                    *name,
                    arguments,
                    result,
                    Some(&*self.bump.alloc_slice_fill_iter(context)),
                    Some(annotation.region),
                )
            }
        };
        let typ = arguments
            .iter()
            .rev()
            .fold(result, |result, (_, expected)| {
                let arg = match expected {
                    PExpected::NoExpectation(t) | PExpected::FromContext(_, _, t) => *t,
                };
                self.structure(uf, rank, FlatType::Fun1(arg, result))
            });
        let expected = if let Some(annotation_region) = annotation_region {
            Expected::FromAnnotation(
                name.value,
                annotation_region,
                arguments.len(),
                SubContext::TypedBody,
                result,
            )
        } else {
            Expected::NoExpectation(result)
        };
        PreparedDefinition {
            definition: Definition {
                site: Binder::Named(name),
                typ,
                context,
            },
            arguments,
            source: def,
            expected,
            rtv,
            rigids: self.bump.alloc_slice_copy(&rigids),
        }
    }

    fn infer_prepared_body(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        prepared: &PreparedDefinition<'a>,
    ) -> State<'a> {
        match prepared.source {
            Def::Def { body, .. } => {
                let scope = self.infer_patterns(uf, env, rank, state, &prepared.arguments);
                let state = self.infer_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    &prepared.rtv,
                    body,
                    prepared.expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            Def::TypedDef {
                name,
                args,
                body,
                typ,
                annotation,
                ..
            } => {
                let scope = self.infer_typed_patterns(
                    uf,
                    env,
                    rank,
                    state,
                    name.value,
                    annotation.region,
                    args,
                    &prepared.rtv,
                );
                let state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    &prepared.rtv,
                    body,
                    prepared.expected.type_replace(*typ),
                );
                self.close_locals(uf, state, scope.locals)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_typed_patterns(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        name: &'a str,
        annotation_region: Region,
        args: &[nash_ast::TypedPattern<'a>],
        rtv: &Rtv<'a>,
    ) -> ScopeResult<'a> {
        let owns = args
            .iter()
            .any(|arg| super::patterns::pattern_owns_vars(arg.pattern));
        let pattern_rank = if owns { self.young_pool(rank) } else { rank };
        let start = self.wanted.len();
        let mut headers = BTreeMap::new();
        for (index, arg) in args.iter().enumerate().rev() {
            state = self.infer_canonical_pattern(
                uf,
                pattern_rank,
                state,
                arg.pattern,
                PExpected::FromContext(
                    arg.pattern.region,
                    PContext::TypedArg(name, index, annotation_region),
                    arg.typ,
                ),
                rtv,
                &mut headers,
            );
        }
        self.finish_pattern_scope(uf, env, rank, pattern_rank, state, start, headers)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_bindings(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        definitions: &[Definition<'a>],
        declarations: &[Definition<'a>],
        rigids: &'a [Variable],
        locals: Locals<'a>,
        declared: &BTreeMap<&'a str, &'a [type_::PredId]>,
        binder: Option<Binder<'a>>,
        given: &[Body<'a>],
        wanted_start: usize,
        errors_before: usize,
    ) -> ScopeResult<'a> {
        let young = rank + 1;
        state = self.generalize_scope(uf, young, state);
        for rigid in rigids {
            if uf.get(*rigid).rank != NO_RANK && !crate::recovery::is_poisoned(uf, [*rigid]) {
                let owner = binder
                    .map(|b| (b.name().region, b.name().value))
                    .or_else(|| locals.first().map(|(name, loc)| (loc.region, *name)));
                state.errors.push(Error::AnnotationVariableEscapes {
                    region: owner.map_or_else(Region::zero, |(r, _)| r),
                    name: owner.map(|(_, n)| n),
                    variable: to_error_type(self.bump, uf, *rigid),
                });
            }
        }
        if let Some(binder) = binder {
            let depth = self.enter_givens(uf, rank, given, Some(binder));
            loop {
                let (errors, defaulted) =
                    self.check_ambiguity(uf, rank, wanted_start, definitions, binder.name());
                state.errors.extend(errors);
                if !defaulted {
                    break;
                }
                state = self.resolve_wanted(
                    uf,
                    young,
                    state,
                    wanted_start,
                    Some(binder),
                    definitions.iter().any(|d| d.context.is_some()),
                );
            }
            self.givens.truncate(depth);
        }
        let context = if !definitions.is_empty() && definitions.iter().all(|d| d.context.is_none())
        {
            self.retain_wanted(
                uf,
                rank,
                wanted_start,
                binder.expect("inferred binding owner").node(),
            )
        } else {
            &[]
        };
        if state.errors.len() > errors_before
            && let Some(binder) = binder
        {
            self.fail_definition(uf, binder.node());
        }
        self.record_definitions(uf, rank, definitions, declared, context, binder);
        let mut env = env.clone();
        for (name, loc) in &locals {
            env.entry(name).or_insert(Binding {
                variable: loc.value,
                context: declared.get(name).copied().unwrap_or(context),
                definition: definitions
                    .iter()
                    .chain(declarations)
                    .find(|d| d.site.name().value == *name)
                    .map(|d| d.site.node())
                    .or_else(|| {
                        binder
                            .filter(|b| matches!(b, Binder::Pattern { .. }))
                            .map(Binder::node)
                    }),
                context_is_final: true,
                declared_quantifiers: if declarations
                    .iter()
                    .any(|d| d.site.name().value == *name && d.context.is_some())
                {
                    rigids
                } else {
                    &[]
                },
            });
        }
        ScopeResult { env, state, locals }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_definition(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &Rtv<'a>,
        def: &'a Def<'a>,
        bind_name: bool,
    ) -> ScopeResult<'a> {
        let young = self.young_pool(rank);
        let prepared = self.prepare_definition(uf, young, rtv, def);
        self.check_prepared(uf, env, rank, state, prepared, bind_name)
    }

    fn check_prepared(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        prepared: PreparedDefinition<'a>,
        bind_name: bool,
    ) -> ScopeResult<'a> {
        let young = rank + 1;
        let errors_before = state.errors.len();
        let start = self.wanted.len();
        let definition = prepared.definition;
        let binder = Some(definition.site);
        let declared = self.declared_contexts(uf, young, &[definition], &[]);
        let given = definition.context.unwrap_or(&[]);
        let depth = self.enter_givens(uf, young, given, binder);
        let owners = self.owners.len();
        self.owners.push(definition.site.node());
        let mut state = self.infer_prepared_body(uf, env, young, state, &prepared);
        self.retry_fields(uf, young, &mut state.errors);
        state = self.resolve_wanted(
            uf,
            young,
            state,
            start,
            binder,
            definition.context.is_some(),
        );
        if state.errors.len() > errors_before {
            self.fail_definition(uf, definition.site.node());
        }
        self.givens.truncate(depth);
        self.owners.truncate(owners);
        let name = definition.site.name();
        let locals = if bind_name {
            vec![(name.value, Located::at(name.region, definition.typ))]
        } else {
            Vec::new()
        };
        self.finish_bindings(
            uf,
            env,
            rank,
            state,
            &[definition],
            &[],
            prepared.rigids,
            locals,
            &declared,
            binder,
            given,
            start,
            errors_before,
        )
    }

    pub(super) fn infer_module(
        &mut self,
        uf: &mut UnionFind<'a>,
        module: &Module<'a>,
        state: State<'a>,
    ) -> State<'a> {
        self.infer_decls(uf, &Env::new(), OUTERMOST_RANK, state, module.decls, module)
    }

    fn infer_decls(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        decls: &Decls<'a>,
        module: &Module<'a>,
    ) -> State<'a> {
        let (scope, next) = match decls {
            Decls::Declare { definition, next } => (
                self.infer_definition(uf, env, rank, state, &Rtv::new(), definition, true),
                *next,
            ),
            Decls::DeclareRec {
                definition,
                following,
                next,
            } => {
                let mut defs = vec![*definition];
                defs.extend_from_slice(following);
                (
                    self.infer_group(uf, env, rank, state, &Rtv::new(), &defs),
                    *next,
                )
            }
            Decls::Empty => {
                let mut state = state;
                for definition in module
                    .traits
                    .iter()
                    .flat_map(|t| t.value.methods.iter().filter_map(|m| m.default))
                    .chain(
                        module
                            .impls
                            .iter()
                            .flat_map(|i| i.value.methods.iter().copied()),
                    )
                {
                    let scope =
                        self.infer_definition(uf, env, rank, state, &Rtv::new(), definition, false);
                    state = self.close_locals(uf, scope.state, scope.locals);
                }
                state.env = env.clone();
                return state;
            }
        };
        let state = self.infer_decls(uf, &scope.env, rank, scope.state, next, module);
        self.close_locals(uf, state, scope.locals)
    }
}

pub(super) fn expectation_type(expected: Expected<'_, Variable>) -> Variable {
    match expected {
        Expected::NoExpectation(t)
        | Expected::FromContext(_, _, t)
        | Expected::FromAnnotation(_, _, _, _, t) => t,
    }
}

impl<'a> Solver<'a, '_> {
    pub(super) fn infer_group(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &Rtv<'a>,
        defs: &[&'a Def<'a>],
    ) -> ScopeResult<'a> {
        let owns_rigids = defs.iter().any(|def| match def {
            Def::TypedDef { free_vars, .. } => free_vars.iter().any(|name| !rtv.contains_key(name)),
            _ => false,
        });
        let declared_rank = if owns_rigids {
            self.young_pool(rank)
        } else {
            rank
        };
        let mut typed = Vec::new();
        for def in defs
            .iter()
            .filter(|def| matches!(def, Def::TypedDef { .. }))
        {
            typed.push((*def, self.prepare_definition(uf, declared_rank, rtv, def)));
        }
        let declarations: Vec<_> = typed.iter().map(|(_, p)| p.definition).collect();
        let rigid_vars: Vec<_> = typed
            .iter()
            .rev()
            .flat_map(|(_, p)| p.rigids.iter().copied())
            .collect();
        let rigid_vars = &*self.bump.alloc_slice_copy(&rigid_vars);
        let declared = self.declared_contexts(uf, declared_rank, &[], &declarations);
        if owns_rigids {
            state = self.generalize_scope(uf, declared_rank, state);
        }
        let mut declared_env = env.clone();
        let mut locals = BTreeMap::new();
        for declaration in &declarations {
            let name = declaration.site.name();
            locals.insert(name.value, Located::at(name.region, declaration.typ));
            declared_env.entry(name.value).or_insert(Binding {
                variable: declaration.typ,
                context: declared.get(name.value).copied().unwrap_or(&[]),
                definition: Some(declaration.site.node()),
                context_is_final: true,
                declared_quantifiers: rigid_vars,
            });
        }
        let inferred_defs: Vec<_> = defs
            .iter()
            .filter(|def| matches!(def, Def::Def { .. }))
            .copied()
            .collect();
        let mut result = if inferred_defs.is_empty() {
            ScopeResult {
                env: declared_env,
                state,
                locals: Vec::new(),
            }
        } else {
            let young = self.young_pool(rank);
            let prepared: Vec<_> = inferred_defs
                .iter()
                .map(|def| self.prepare_definition(uf, young, rtv, def))
                .collect();
            let definitions: Vec<_> = prepared.iter().map(|p| p.definition).collect();
            let binder = Some(definitions[0].site);
            let mut recursive_env = declared_env.clone();
            let mut inferred_locals = BTreeMap::new();
            for definition in &definitions {
                let name = definition.site.name();
                inferred_locals.insert(name.value, Located::at(name.region, definition.typ));
                recursive_env.entry(name.value).or_insert(Binding {
                    variable: definition.typ,
                    context: &[],
                    definition: Some(definition.site.node()),
                    context_is_final: false,
                    declared_quantifiers: &[],
                });
            }
            let errors_before = state.errors.len();
            let start = self.wanted.len();
            let owners = self.owners.len();
            self.owners.push(definitions[0].site.node());
            for prepared in prepared.iter().rev() {
                state = self.infer_prepared_body(uf, &recursive_env, young, state, prepared);
            }
            // The recursive headers are checked inside the group before its
            // generalization, as well as after the group's continuation.
            state = self.close_locals(
                uf,
                state,
                inferred_locals
                    .iter()
                    .map(|(name, loc)| (*name, *loc))
                    .collect(),
            );
            self.retry_fields(uf, young, &mut state.errors);
            state = self.resolve_wanted(uf, young, state, start, binder, false);
            self.owners.truncate(owners);
            self.finish_bindings(
                uf,
                &declared_env,
                rank,
                state,
                &definitions,
                &[],
                &[],
                inferred_locals.into_iter().collect(),
                &BTreeMap::new(),
                binder,
                &[],
                start,
                errors_before,
            )
        };
        for (def, prepared) in typed.into_iter().rev() {
            let young = self.young_pool(rank);
            self.introduce(uf, young, prepared.rigids);
            let mut body = self.prepare_definition(uf, young, &prepared.rtv, def);
            body.rigids = prepared.rigids;
            let checked = self.check_prepared(uf, &result.env, rank, result.state, body, false);
            result.state = self.close_locals(uf, checked.state, checked.locals);
        }
        locals.extend(result.locals);
        result.locals = locals.into_iter().collect();
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_destruct(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &Rtv<'a>,
        region: Region,
        pattern: &'a Located<Pattern<'a>>,
        expr: &'a Located<Expr<'a>>,
    ) -> ScopeResult<'a> {
        let young = self.young_pool(rank);
        let typ = self.fresh(uf, young);
        let binder = Binder::Pattern {
            node: nash_ast::NodeId::pattern(pattern),
            name: self.bump.alloc(Located::at(region, "<destructure>")),
        };
        let definition = Definition {
            site: binder,
            typ,
            context: None,
        };
        let mut headers = BTreeMap::new();
        let errors_before = state.errors.len();
        let start = self.wanted.len();
        let owners = self.owners.len();
        self.owners.push(binder.node());
        let state = self.infer_pattern(
            uf,
            young,
            state,
            pattern,
            PExpected::NoExpectation(typ),
            &mut headers,
        );
        let mut state = self.infer_expr(
            uf,
            env,
            young,
            state,
            rtv,
            expr,
            Expected::FromContext(region, Context::Destructure, typ),
        );
        self.retry_fields(uf, young, &mut state.errors);
        state = self.resolve_wanted(uf, young, state, start, Some(binder), false);
        self.owners.truncate(owners);
        self.finish_bindings(
            uf,
            env,
            rank,
            state,
            &[definition],
            &[],
            &[],
            headers.into_iter().collect(),
            &BTreeMap::new(),
            Some(binder),
            &[],
            start,
            errors_before,
        )
    }
}

#[cfg(test)]
mod preparation_tests {
    use super::*;
    use nash_ast::{Annotation, ModuleName, NodeId, Pred, QualifiedName, Type as CanType};

    #[test]
    fn apply_head_and_arguments_share_the_signature_substitution() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let f = solver.fresh(&mut uf, 2);
        let a = solver.fresh(&mut uf, 2);
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

    #[test]
    fn prepared_rigids_preserve_order_captures_and_predicate_substitution() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let captured = solver.register(&mut uf, 1, Content::RigidVar("captured"));
        let scope = BTreeMap::from([("captured", captured)]);
        let z = &*bump.alloc(Located::at_zero(CanType::Var("z")));
        let cap = &*bump.alloc(Located::at_zero(CanType::Var("captured")));
        let a = &*bump.alloc(Located::at_zero(CanType::Var("a")));
        let result = &*bump.alloc(Located::at_zero(CanType::App {
            head: z,
            args: bump.alloc_slice_copy(&[cap, a]),
        }));
        let definition = bump.alloc(Def::TypedDef {
            name: bump.alloc(Located::at_zero("test")),
            free_vars: &["z", "captured", "a"],
            context: bump.alloc_slice_copy(&[Pred::Apply {
                head: z,
                args: bump.alloc_slice_copy(&[cap, a]),
            }]),
            annotation: result,
            args: &[],
            body: bump.alloc(Located::at_zero(Expr::Unit)),
            typ: result,
        });
        let prepared = solver.prepare_definition(&mut uf, 2, &scope, definition);
        assert_eq!(prepared.rigids.len(), 2);
        assert!(matches!(
            uf.get(prepared.rigids[0]).content,
            Content::RigidVar("a")
        ));
        assert!(matches!(
            uf.get(prepared.rigids[1]).content,
            Content::RigidVar("z")
        ));
        assert_eq!(prepared.rtv["captured"], captured);
        assert_eq!(uf.get(captured).rank, 1);
        assert!(prepared.rigids.iter().all(|var| uf.get(*var).rank == 2));
        let [Body::Apply { head, args }] = prepared.definition.context.unwrap() else {
            panic!("Apply context")
        };
        assert_eq!(*head, prepared.rigids[1]);
        assert_eq!(args, &[captured, prepared.rigids[0]]);
        let Content::Structure(FlatType::AppV1(type_head, type_args)) =
            &uf.get(prepared.definition.typ).content
        else {
            panic!("signature application")
        };
        assert_eq!(type_head, head);
        assert_eq!(type_args, args);
        let other = solver.prepare_definition(&mut uf, 2, &scope, definition);
        assert_eq!(other.rtv["captured"], captured);
        assert_ne!(other.rtv["a"], prepared.rtv["a"]);
        assert_ne!(other.rtv["z"], prepared.rtv["z"]);
    }

    #[test]
    fn literals_and_patterns_keep_original_nodes_and_ordered_predicates() {
        let bump = Bump::new();
        let mut tables = nash_can::environment::Tables::default();
        for trait_ in [
            type_::literal_trait("FromInt"),
            type_::literal_trait("FromString"),
            type_::literal_trait("FromBytes"),
            type_::eq_trait(),
        ] {
            tables.kinds.traits.insert(trait_, &[&nash_ast::Kind::Type]);
        }
        for (expression, pattern, trait_name) in [
            (Expr::Int(7), Pattern::Int(7), "FromInt"),
            (
                Expr::Bytes(&[0, 255]),
                Pattern::Bytes(&[0, 255]),
                "FromBytes",
            ),
            (Expr::Str("nash"), Pattern::Str("nash"), "FromString"),
        ] {
            let expression = bump.alloc(Located::at_zero(expression));
            let pattern = bump.alloc(Located::at_zero(pattern));
            let mut solver = Solver::new(&bump, &tables);
            let mut uf = UnionFind::new();
            let value = solver.fresh(&mut uf, 2);
            let state = State {
                env: Env::new(),
                mark: NO_MARK.next(),
                errors: Vec::new(),
            };
            let state = solver.infer_expr(
                &mut uf,
                &Env::new(),
                2,
                state,
                &Rtv::new(),
                expression,
                Expected::NoExpectation(value),
            );
            assert!(state.errors.is_empty());
            assert_eq!(solver.uses.len(), 1);
            let use_ = &solver.uses[0];
            assert_eq!(use_.site.node, NodeId::expr(expression));
            assert_eq!(use_.predicates.len(), 1);
            let expression_predicate = &solver.predicates.get(use_.predicates[0]).body;
            let trait_ = expression_predicate.trait_ref().unwrap();
            assert_eq!(trait_.name, trait_name);
            assert_eq!(trait_.home.package, Some(nash_ast::primitives::CORE));
            let value = solver.fresh(&mut uf, 2);
            let mut headers = BTreeMap::new();
            let state = solver.infer_pattern(
                &mut uf,
                2,
                state,
                pattern,
                PExpected::NoExpectation(value),
                &mut headers,
            );
            assert!(state.errors.is_empty());
            assert_eq!(
                solver.uses.len(),
                2,
                "one evidence use per literal expression/pattern"
            );
            let use_ = &solver.uses[1];
            assert_eq!(use_.site.node, NodeId::pattern(pattern));
            let predicates: Vec<_> = use_
                .predicates
                .iter()
                .map(|id| &solver.predicates.get(*id).body)
                .collect();
            assert_eq!(
                predicates
                    .iter()
                    .map(|pred| pred.trait_ref().unwrap().name)
                    .collect::<Vec<_>>(),
                [trait_name, "Eq"]
            );
            assert_eq!(
                predicates[0].args(),
                predicates[1].args(),
                "literal and Eq use one substitution"
            );
            assert_eq!(predicates[0].args().len(), 1);
        }
    }

    #[test]
    fn operator_and_method_uses_keep_distinct_node_identity_at_the_same_region() {
        let bump = Bump::new();
        let tables = nash_can::environment::Tables::default();
        let mut solver = Solver::new(&bump, &tables);
        let mut uf = UnionFind::new();
        let unit = &*bump.alloc(Located::at_zero(CanType::unit()));
        let method_annotation = bump.alloc(Annotation {
            free_vars: &[],
            context: &[],
            typ: unit,
        });
        let tail = bump.alloc(Located::at_zero(CanType::Lambda {
            from: unit,
            to: unit,
        }));
        let op_annotation = bump.alloc(Annotation {
            free_vars: &[],
            context: &[],
            typ: bump.alloc(Located::at_zero(CanType::Lambda {
                from: unit,
                to: tail,
            })),
        });
        let reference = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Main",
            },
            name: "Combine",
        };
        let local = &*bump.alloc(Located::at_zero(Expr::VarLocal("x")));
        let method = &*bump.alloc(Located::at_zero(Expr::VarMethod {
            trait_: reference,
            method: "combine",
            annotation: method_annotation,
        }));
        let operator = &*bump.alloc(Located::at_zero(Expr::Binop {
            symbol: "+",
            operator_home: reference.home,
            reference,
            annotation: op_annotation,
            left: local,
            right: method,
        }));
        let name = bump.alloc(Located::at_zero("x"));
        let variable = solver.src_type_to_var(&mut uf, 2, &Rtv::new(), unit);
        let env = Env::from([(
            "x",
            Binding {
                variable,
                context: &[],
                definition: Some(NodeId::def(name)),
                context_is_final: true,
                declared_quantifiers: &[],
            },
        )]);
        let state = State {
            env: Env::new(),
            mark: NO_MARK.next(),
            errors: Vec::new(),
        };
        let state = solver.infer_expr(
            &mut uf,
            &env,
            2,
            state,
            &Rtv::new(),
            operator,
            Expected::NoExpectation(variable),
        );
        assert!(state.errors.is_empty());
        let nodes: Vec<_> = solver.uses.iter().map(|use_| use_.site.node).collect();
        assert_eq!(
            nodes,
            [
                NodeId::expr(operator),
                NodeId::expr(local),
                NodeId::expr(method)
            ]
        );
        assert_ne!(nodes[0], nodes[1]);
        assert_ne!(nodes[0], nodes[2]);
        assert_ne!(nodes[1], nodes[2]);
        assert!(
            solver
                .uses
                .iter()
                .all(|use_| use_.site.region == Region::zero())
        );
    }
}
