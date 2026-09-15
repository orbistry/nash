use super::*;
use nash_ast::{Expr as CanExpr, NodeId};
use nash_constrain::error::{Context, MaybeName, PContext, SubContext};

mod calls;
mod forms;

pub(super) struct PendingConversion {
    node: NodeId,
    rank: usize,
    request: ConversionRequest,
}

struct ConversionRequest {
    site: nash_ast::ConversionSite,
    source: Variable,
    target: Variable,
    region: nash_region::Region,
}

pub(super) struct PendingSerialisable {
    rank: usize,
    variable: Variable,
    region: nash_region::Region,
    top_level: bool,
    purpose: SerialisablePurpose,
}

#[derive(Clone, Copy)]
enum SerialisablePurpose {
    Comparison,
    Format,
    Data,
}

impl<'a> Solver<'a, '_> {
    pub(super) fn queue_serialisable(
        &mut self,
        rank: usize,
        variable: Variable,
        region: nash_region::Region,
        top_level: bool,
    ) {
        self.pending_serialisable.push(PendingSerialisable {
            rank,
            variable,
            region,
            top_level,
            purpose: SerialisablePurpose::Data,
        });
    }

    pub(super) fn finish_conversions(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        errors: &mut Vec<Error<'a>>,
    ) {
        for pending in std::mem::take(&mut self.pending_conversions) {
            if pending.rank < rank {
                self.pending_conversions.push(pending);
                continue;
            }
            match self.plan_conversion(uf, pending.rank, pending.request) {
                Ok(plan) => {
                    self.conversions.insert(pending.node, plan);
                }
                Err(error) => errors.push(error),
            }
        }
        self.finish_serialisable(uf, rank, errors);
    }

    fn finish_serialisable(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        errors: &mut Vec<Error<'a>>,
    ) {
        for pending in std::mem::take(&mut self.pending_serialisable) {
            if pending.rank < rank {
                self.pending_serialisable.push(pending);
                continue;
            }
            let mut allocated = Vec::new();
            let mut todo = vec![(pending.variable, pending.top_level)];
            let mut seen = BTreeSet::new();
            let mut illegal = None;
            while let Some((variable, top_level)) = todo.pop() {
                let variable = crate::representation::subject(
                    uf,
                    &self.tables.kinds,
                    variable,
                    &mut allocated,
                );
                if !seen.insert((uf.find(variable), top_level)) {
                    continue;
                }
                match uf.get(variable).content.clone() {
                    Content::Structure(FlatType::Fun1(argument, result)) => {
                        if !top_level {
                            illegal = Some(variable);
                            break;
                        }
                        todo.extend([(argument, true), (result, true)]);
                    }
                    Content::Structure(FlatType::Function1(arguments, result)) => {
                        if !top_level {
                            illegal = Some(variable);
                            break;
                        }
                        todo.extend(arguments.into_iter().map(|argument| (argument, true)));
                        todo.push((result, true));
                    }
                    Content::Structure(FlatType::App1(home, name, args)) => {
                        if !top_level
                            && home == nash_ast::primitives::builtin_home()
                            && name == "bls_mlr"
                        {
                            illegal = Some(variable);
                            break;
                        }
                        todo.extend(args.into_iter().map(|argument| (argument, false)));
                    }
                    Content::Structure(FlatType::AppV1(head, args)) => {
                        todo.push((head, top_level));
                        todo.extend(args.into_iter().map(|argument| (argument, false)));
                    }
                    Content::Structure(FlatType::Tuple1(first, second, rest)) => {
                        todo.extend([(first, false), (second, false)]);
                        todo.extend(rest.into_iter().map(|argument| (argument, false)));
                    }
                    Content::Structure(FlatType::Record1(fields)) => {
                        todo.extend(fields.into_values().map(|field| (field, false)));
                    }
                    Content::Alias { args, real, .. } => {
                        todo.extend(args.into_iter().map(|(_, variable)| (variable, false)));
                        todo.push((real, top_level));
                    }
                    Content::PartialAlias { args, .. } => {
                        todo.extend(args.into_iter().map(|(_, variable)| (variable, false)));
                    }
                    Content::FlexVar(_) | Content::RigidVar(_) | Content::Error => {}
                }
            }
            self.introduce(uf, pending.rank, &allocated);
            if let Some(illegal) = illegal {
                errors.push(match pending.purpose {
                    SerialisablePurpose::Format => Error::IllegalTraceArgument {
                        region: pending.region,
                        typ: to_error_type(self.bump, uf, pending.variable),
                    },
                    SerialisablePurpose::Comparison => Error::IllegalComparison {
                        region: pending.region,
                        typ: to_error_type(self.bump, uf, pending.variable),
                    },
                    SerialisablePurpose::Data => Error::IllegalDataType {
                        region: pending.region,
                        typ: to_error_type(self.bump, uf, illegal),
                    },
                });
            }
        }
    }

    /// Assignment annotations permit a Data cast; named function signatures do
    /// not. Decide from inference types, including aliases, rather than syntax.
    // Errors move directly into the solver's diagnostic vector. Keep that
    // existing representation instead of allocating a box for each rejection.
    #[allow(clippy::result_large_err)]
    fn plan_conversion(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        request: ConversionRequest,
    ) -> Result<nash_ast::ConversionKind, Error<'a>> {
        use nash_ast::{ConversionKind, ConversionSite};

        let mut allocated = Vec::new();
        let source =
            crate::representation::subject(uf, &self.tables.kinds, request.source, &mut allocated);
        let target =
            crate::representation::subject(uf, &self.tables.kinds, request.target, &mut allocated);
        self.introduce(uf, rank, &allocated);
        if let ConversionSite::LambdaSignature(arity) = request.site {
            if let (
                Content::Structure(FlatType::Function1(source_args, source_result)),
                Content::Structure(FlatType::Function1(target_args, target_result)),
            ) = (
                uf.get(source).content.clone(),
                uf.get(target).content.clone(),
            ) && source_args.len() == arity
                && target_args.len() == arity
            {
                for (source, target) in source_args.into_iter().zip(target_args) {
                    self.plan_conversion(
                        uf,
                        rank,
                        ConversionRequest {
                            site: ConversionSite::FunctionResult,
                            source,
                            target,
                            region: request.region,
                        },
                    )?;
                }
                self.plan_conversion(
                    uf,
                    rank,
                    ConversionRequest {
                        site: ConversionSite::LambdaResult,
                        source: source_result,
                        target: target_result,
                        region: request.region,
                    },
                )?;
                return Ok(ConversionKind::Identity);
            }
            let mut source_result = source;
            let mut target_result = target;
            let mut remaining = arity;
            while remaining > 0 {
                let source_function = match &uf.get(source_result).content {
                    Content::Structure(FlatType::Fun1(arg, ret)) => Some((*arg, *ret)),
                    _ => None,
                };
                let target_function = match &uf.get(target_result).content {
                    Content::Structure(FlatType::Fun1(arg, ret)) => Some((*arg, *ret)),
                    _ => None,
                };
                let (Some((source_arg, source_ret)), Some((target_arg, target_ret))) =
                    (source_function, target_function)
                else {
                    break;
                };
                self.plan_conversion(
                    uf,
                    rank,
                    ConversionRequest {
                        site: ConversionSite::FunctionResult,
                        source: source_arg,
                        target: target_arg,
                        region: request.region,
                    },
                )?;
                source_result = source_ret;
                target_result = target_ret;
                remaining -= 1;
            }
            if remaining == 0 {
                self.plan_conversion(
                    uf,
                    rank,
                    ConversionRequest {
                        site: ConversionSite::LambdaResult,
                        source: source_result,
                        target: target_result,
                        region: request.region,
                    },
                )?;
                return Ok(ConversionKind::Identity);
            }
        }
        let source_is_data = matches!(&uf.get(source).content,
            Content::Structure(FlatType::App1(home, "Data", args))
                if *home == nash_ast::primitives::builtin_home() && args.is_empty());
        let target_is_data = matches!(&uf.get(target).content,
            Content::Structure(FlatType::App1(home, "Data", args))
                if *home == nash_ast::primitives::builtin_home() && args.is_empty());
        if request.site == ConversionSite::ValidatorDatum
            && !matches!(&uf.get(target).content,
                Content::Structure(FlatType::App1(home, "data_option", args))
                    if *home == nash_ast::primitives::builtin_home() && args.len() == 1)
        {
            return Err(Error::InvalidDataCast {
                region: request.region,
                reason: "A spending handler's datum parameter must have an Option type.",
                typ: to_error_type(self.bump, uf, target),
            });
        }
        if source_is_data
            && matches!(
                request.site,
                ConversionSite::ValidatorParameter | ConversionSite::ValidatorRedeemer
            )
        {
            self.formed_at(uf, rank, &[request.source, request.target], request.region);
            return Ok(if target_is_data {
                ConversionKind::Identity
            } else if request.site == ConversionSite::ValidatorParameter {
                ConversionKind::FromDataShallow
            } else {
                ConversionKind::ValidateData
            });
        }
        if source_is_data && request.site == ConversionSite::ValidatorMintPolicy {
            self.formed_at(uf, rank, &[request.source, request.target], request.region);
            return Ok(ConversionKind::FromDataBytesView);
        }
        if source_is_data
            && matches!(
                request.site,
                ConversionSite::ValidatorDatum
                    | ConversionSite::ValidatorPurpose
                    | ConversionSite::ValidatorContext
            )
        {
            self.formed_at(uf, rank, &[request.source, request.target], request.region);
            return Ok(if target_is_data {
                ConversionKind::Identity
            } else {
                ConversionKind::ViewData
            });
        }
        let assignment = matches!(
            request.site,
            ConversionSite::AnnotatedBinding
                | ConversionSite::ModuleConstant
                | ConversionSite::LambdaResult
                | ConversionSite::CallArgument
                | ConversionSite::RecordUpdateField
                | ConversionSite::ExpectBinding
                | ConversionSite::ExpectPattern
                | ConversionSite::CastTest
                | ConversionSite::CastPattern
        );
        let decoding = matches!(
            request.site,
            ConversionSite::ExpectBinding
                | ConversionSite::ExpectPattern
                | ConversionSite::CastTest
                | ConversionSite::CastPattern
                | ConversionSite::ValidatorParameter
                | ConversionSite::ValidatorRedeemer
                | ConversionSite::ValidatorDatum
                | ConversionSite::ValidatorPurpose
                | ConversionSite::ValidatorMintPolicy
                | ConversionSite::ValidatorContext
        );
        // Root variables, functions and String unify normally: permission to
        // cast never selects a serializable type for an unknown source.
        let root_castable = |content: &Content<'a>| {
            matches!(
                content,
                Content::Alias { .. } | Content::Structure(FlatType::Tuple1(..))
            ) || matches!(content, Content::Structure(FlatType::App1(home, name, _))
                if *home != nash_ast::primitives::builtin_home() || *name != "string")
        };
        if assignment && target_is_data && !source_is_data && root_castable(&uf.get(source).content)
        {
            if !self.conversion_type_known(uf, rank, source, false) {
                return Err(Error::InvalidDataCast {
                    region: request.region,
                    reason: "This value contains a type that cannot be encoded as Data.",
                    typ: to_error_type(self.bump, uf, source),
                });
            }
            self.formed_at(uf, rank, &[request.source, request.target], request.region);
            return Ok(if request.site == ConversionSite::LambdaResult {
                ConversionKind::Identity
            } else {
                ConversionKind::ToData
            });
        }
        if decoding && source_is_data && !target_is_data && root_castable(&uf.get(target).content) {
            let mut allocated = Vec::new();
            let opaque = crate::representation::contains_opaque(
                uf,
                &self.tables.kinds,
                target,
                &mut allocated,
            );
            self.introduce(uf, rank, &allocated);
            if opaque
                && matches!(
                    request.site,
                    ConversionSite::ExpectBinding
                        | ConversionSite::ExpectPattern
                        | ConversionSite::CastTest
                        | ConversionSite::CastPattern
                )
            {
                return Err(Error::InvalidDataCast {
                    region: request.region,
                    reason: "A Data cast cannot construct an opaque type or a type containing one.",
                    typ: to_error_type(self.bump, uf, target),
                });
            }
            if !self.conversion_type_known(uf, rank, target, false) {
                return Err(Error::InvalidDataCast {
                    region: request.region,
                    reason: "This type cannot be decoded from Data.",
                    typ: to_error_type(self.bump, uf, target),
                });
            }
            self.formed_at(uf, rank, &[request.source, request.target], request.region);
            return Ok(match request.site {
                ConversionSite::ValidatorParameter => ConversionKind::FromDataShallow,
                _ => ConversionKind::ValidateData,
            });
        }
        self.formed_at(uf, rank, &[request.source, request.target], request.region);
        match self.unify(uf, request.source, request.target) {
            unify::Answer::Ok(variables) => {
                self.introduce(uf, rank, &variables);
                Ok(ConversionKind::Identity)
            }
            unify::Answer::Err(variables, source, target) => {
                self.introduce(uf, rank, &variables);
                Err(Error::BadExpr(
                    request.region,
                    Category::Foreign("type ascription"),
                    source,
                    Expected::NoExpectation(target),
                ))
            }
        }
    }

    fn conversion_annotation(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        rtv: &BTreeMap<&'a str, Variable>,
        typ: &'a Located<CanType<'a>>,
        rigid: bool,
    ) -> Variable {
        let mut names = BTreeSet::new();
        nash_can::types::collect_free_vars(&typ.value, &mut names);
        let mut variables = rtv.clone();
        for name in names {
            if variables.contains_key(name) {
                continue;
            }
            let variable = if rigid {
                self.annotation_scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name).copied())
                    .unwrap_or_else(|| {
                        let variable = self.register(uf, rank, Content::RigidVar(name));
                        if let Some(scope) = self.annotation_scopes.last_mut() {
                            scope.insert(name, variable);
                        }
                        variable
                    })
            } else {
                self.fresh(uf, rank)
            };
            variables.insert(name, variable);
        }
        self.src_type_to_var(uf, rank, &variables, typ)
    }

    pub(super) fn push_annotation_scope(&mut self) {
        self.annotation_scopes.push(BTreeMap::new());
    }

    pub(super) fn pop_annotation_scope(&mut self) {
        self.annotation_scopes
            .pop()
            .expect("annotation scope is balanced");
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_annotation_scope(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        value: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        self.push_annotation_scope();
        let state = self.infer_expr(uf, env, rank, state, rtv, value, expected);
        self.pop_annotation_scope();
        state
    }

    fn conversion_type_known(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        variable: Variable,
        monomorphic: bool,
    ) -> bool {
        let mut pending = vec![variable];
        let mut seen = BTreeSet::new();
        let mut allocated = Vec::new();
        let mut valid = true;
        while let Some(variable) = pending.pop() {
            let variable =
                crate::representation::subject(uf, &self.tables.kinds, variable, &mut allocated);
            if !seen.insert(uf.find(variable)) {
                continue;
            }
            match uf.get(variable).content.clone() {
                Content::FlexVar(_) if !monomorphic => {}
                Content::FlexVar(_)
                | Content::RigidVar(_)
                | Content::Structure(FlatType::Fun1(..) | FlatType::Function1(..))
                | Content::PartialAlias { .. }
                | Content::Structure(FlatType::AppV1(..)) => {
                    valid = false;
                    break;
                }
                Content::Structure(FlatType::App1(home, name, args)) => {
                    if home == nash_ast::primitives::builtin_home() && name == "bls_mlr" {
                        valid = false;
                        break;
                    }
                    pending.extend(args);
                }
                Content::Structure(FlatType::Tuple1(first, second, rest)) => {
                    pending.extend([first, second]);
                    pending.extend(rest);
                }
                Content::Structure(FlatType::Record1(fields)) => {
                    pending.extend(fields.into_values())
                }
                Content::Alias { args, real, .. } => {
                    pending.extend(args.into_iter().map(|(_, value)| value));
                    pending.push(real);
                }
                Content::Error => {}
            }
        }
        self.introduce(uf, rank, &allocated);
        valid
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_expr(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let variable = match expected {
            Expected::NoExpectation(var)
            | Expected::FromContext(_, _, var)
            | Expected::FromAnnotation(_, _, _, _, var) => var,
        };
        self.expr_types.push(NodeTypeRecord {
            node: NodeId::expr(expr),
            variable,
            owner: self.owners.last().copied(),
        });
        // These are exactly the old `exists` boundaries. Their wanted flush
        // must remain even though variables are now introduced eagerly.
        let existential = matches!(
            &expr.value,
            CanExpr::List(_)
                | CanExpr::Binop { .. }
                | CanExpr::Lambda { .. }
                | CanExpr::Call { .. }
                | CanExpr::Function { .. }
                | CanExpr::SurfaceCall { .. }
                | CanExpr::Pipe { .. }
                | CanExpr::Case { .. }
                | CanExpr::Accessor(_)
                | CanExpr::Access { .. }
                | CanExpr::FieldOrModule { .. }
                | CanExpr::Update { .. }
                | CanExpr::RecordUpdate { .. }
                | CanExpr::TupleIndex { .. }
                | CanExpr::Match { .. }
                | CanExpr::LetValue { .. }
                | CanExpr::Equal { .. }
                | CanExpr::Format { .. }
                | CanExpr::Record { .. }
                | CanExpr::Tuple { .. }
                | CanExpr::DataTuple { .. }
                | CanExpr::If { .. }
                | CanExpr::Convert { .. }
                | CanExpr::Pair { .. }
                | CanExpr::DataList { .. }
        );
        let wanted_start = self.wanted.len();
        let mut state = self.infer_expr_inner(uf, env, rank, state, rtv, expr, expected);
        if existential {
            self.retry_fields(uf, rank, &mut state.errors);
            state = self.resolve_wanted(uf, rank, state, wanted_start, None, false);
        }
        state
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_expr_inner(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        match &expr.value {
            CanExpr::TypeScope { .. }
            | CanExpr::ModuleConstantCheck { .. }
            | CanExpr::RunnableCheck { .. }
            | CanExpr::Constant(_) => {
                self.infer_scoped_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::TupleIndex { .. }
            | CanExpr::LetValue { .. }
            | CanExpr::Match { .. }
            | CanExpr::Equal { .. }
            | CanExpr::Format { .. }
            | CanExpr::TraceLabel { .. } => {
                self.infer_operations_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::Convert { .. } => {
                self.infer_conversion_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::Pair { .. } | CanExpr::DataList { .. } => {
                self.infer_collections_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::VarLocal(_)
            | CanExpr::VarTopLevel(_)
            | CanExpr::VarForeign { .. }
            | CanExpr::VarConstructor { .. }
            | CanExpr::VarMethod { .. }
            | CanExpr::VarOperator { .. }
            | CanExpr::Str(_)
            | CanExpr::Bytes(_)
            | CanExpr::Int(_)
            | CanExpr::List(_)
            | CanExpr::Binop { .. } => {
                self.infer_values_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::Callable { .. }
            | CanExpr::Function { .. }
            | CanExpr::SurfaceCall { .. }
            | CanExpr::Pipe { .. }
            | CanExpr::Lambda { .. }
            | CanExpr::Call { .. } => {
                self.infer_functions_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::If { .. }
            | CanExpr::Case { .. }
            | CanExpr::Let { .. }
            | CanExpr::LetRec { .. }
            | CanExpr::LetDestruct { .. } => {
                self.infer_branches_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::Accessor(_)
            | CanExpr::Access { .. }
            | CanExpr::FieldOrModule { .. }
            | CanExpr::Update { .. }
            | CanExpr::RecordUpdate { .. }
            | CanExpr::Record { .. } => {
                self.infer_records_form(uf, env, rank, state, rtv, expr, expected)
            }
            CanExpr::Assert(_)
            | CanExpr::Fail(_)
            | CanExpr::Todo(_)
            | CanExpr::Trace { .. }
            | CanExpr::Comptime(_)
            | CanExpr::Unit
            | CanExpr::DataTuple { .. }
            | CanExpr::Tuple { .. } => {
                self.infer_effects_form(uf, env, rank, state, rtv, expr, expected)
            }
        }
    }
}

fn direct_get_name<'a>(expr: &Located<CanExpr<'a>>) -> MaybeName<'a> {
    match &expr.value {
        CanExpr::VarMethod { method, .. } => MaybeName::FuncName(method),
        CanExpr::VarLocal(name) => MaybeName::FuncName(name),
        CanExpr::VarTopLevel(reference) | CanExpr::VarForeign { reference, .. } => {
            MaybeName::FuncName(reference.name)
        }
        CanExpr::VarConstructor { reference, .. } => MaybeName::CtorName(reference.name),
        CanExpr::VarOperator { symbol, .. } => MaybeName::OpName(symbol),
        _ => MaybeName::NoName,
    }
}

fn direct_get_access_name<'a>(expr: &Located<CanExpr<'a>>) -> Option<&'a str> {
    match &expr.value {
        CanExpr::VarLocal(name) => Some(name),
        CanExpr::VarTopLevel(reference) | CanExpr::VarForeign { reference, .. } => {
            Some(reference.name)
        }
        _ => None,
    }
}

impl<'a> Solver<'a, '_> {
    /// An annotation is canonical syntax, not a shared unification structure.
    /// Reinstantiate it at each branch equation so one failed branch cannot
    /// poison another branch's structural expectation. Its rigid variables
    /// still come from the same lexical substitution.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_annotated_expr(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        expr: &Located<CanExpr<'a>>,
        expected: Expected<'a, &'a Located<CanType<'a>>>,
    ) -> State<'a> {
        let Expected::FromAnnotation(name, annotation_region, arity, _, typ) = expected else {
            unreachable!("annotated expression has its canonical expectation")
        };
        // These forms recurse directly through canonical expectations instead
        // of entering infer_expr. Retain their original arena node identities.
        if matches!(
            expr.value,
            CanExpr::If { .. }
                | CanExpr::Case { .. }
                | CanExpr::Let { .. }
                | CanExpr::LetRec { .. }
                | CanExpr::LetDestruct { .. }
                | CanExpr::Trace { .. }
                | CanExpr::TraceLabel { .. }
                | CanExpr::Comptime(_)
        ) {
            let variable = self.src_type_to_var(uf, rank, rtv, typ);
            self.expr_types.push(NodeTypeRecord {
                node: NodeId::expr(expr),
                variable,
                owner: self.owners.last().copied(),
            });
        }
        let region = expr.region;
        match &expr.value {
            CanExpr::TraceLabel {
                label,
                arguments,
                body,
                ..
            } => {
                let string = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "string", Vec::new()),
                );
                for argument in *arguments {
                    state = self.infer_annotation_scope(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        argument,
                        Expected::NoExpectation(string),
                    );
                }
                state = self.infer_annotated_expr(uf, env, rank, state, rtv, body, expected);
                self.infer_annotation_scope(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    label,
                    Expected::NoExpectation(string),
                )
            }
            CanExpr::Trace { message, body } => {
                let string = self.structure(
                    uf,
                    rank,
                    FlatType::App1(nash_ast::primitives::builtin_home(), "string", vec![]),
                );
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    message,
                    Expected::NoExpectation(string),
                );
                self.infer_annotated_expr(uf, env, rank, state, rtv, body, expected)
            }
            CanExpr::Comptime(inner) => {
                self.infer_annotated_expr(uf, env, rank, state, rtv, inner, expected)
            }
            CanExpr::If {
                branches,
                final_else,
            } => {
                for branch in *branches {
                    let boolean = self.structure(
                        uf,
                        rank,
                        FlatType::App1(nash_ast::primitives::builtin_home(), "bool", Vec::new()),
                    );
                    state = self.infer_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        branch.condition,
                        Expected::FromContext(region, Context::IfCondition, boolean),
                    );
                }
                for (index, branch) in branches
                    .iter()
                    .map(|branch| branch.then_branch)
                    .chain(std::iter::once(*final_else))
                    .enumerate()
                {
                    state = self.infer_annotated_expr(
                        uf,
                        env,
                        rank,
                        state,
                        rtv,
                        branch,
                        Expected::FromAnnotation(
                            name,
                            annotation_region,
                            arity,
                            SubContext::TypedIfBranch(index),
                            typ,
                        ),
                    );
                }
                state
            }
            CanExpr::Case {
                scrutinee,
                branches,
            } => {
                let start = self.wanted.len();
                let matched = self.fresh(uf, rank);
                state = self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    scrutinee,
                    Expected::NoExpectation(matched),
                );
                for (index, branch) in branches.iter().enumerate() {
                    let scope = self.infer_patterns(
                        uf,
                        env,
                        rank,
                        state,
                        &[(
                            branch.pattern,
                            PExpected::FromContext(region, PContext::CaseMatch(index), matched),
                        )],
                    );
                    state = self.infer_annotated_expr(
                        uf,
                        &scope.env,
                        rank,
                        scope.state,
                        rtv,
                        branch.body,
                        Expected::FromAnnotation(
                            name,
                            annotation_region,
                            arity,
                            SubContext::TypedCaseBranch(index),
                            typ,
                        ),
                    );
                    state = self.close_locals(uf, state, scope.locals);
                }
                self.retry_fields(uf, rank, &mut state.errors);
                self.resolve_wanted(uf, rank, state, start, None, false)
            }
            CanExpr::Let { definition, body } => {
                let scope = self.infer_definition(uf, env, rank, state, rtv, definition, true);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetRec { definitions, body } => {
                let scope = self.infer_group(uf, env, rank, state, rtv, definitions);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            CanExpr::LetDestruct {
                pattern,
                value,
                body,
            } => {
                let scope = self.infer_destruct(uf, env, rank, state, rtv, region, pattern, value);
                state = self.infer_annotated_expr(
                    uf,
                    &scope.env,
                    rank,
                    scope.state,
                    rtv,
                    body,
                    expected,
                );
                self.close_locals(uf, state, scope.locals)
            }
            _ => {
                let variable = self.src_type_to_var(uf, rank, rtv, typ);
                self.infer_expr(
                    uf,
                    env,
                    rank,
                    state,
                    rtv,
                    expr,
                    expected.type_replace(variable),
                )
            }
        }
    }
}
