//! Check runnable and module-constant type boundaries before generalization.
use super::*;
use nash_ast::{Expr, Type};

impl<'a> Solver<'a, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_module_constant(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        value: &Located<Expr<'a>>,
        region: Region,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let actual = self.fresh(uf, rank);
        state = self.infer_expr(
            uf,
            env,
            rank,
            state,
            rtv,
            value,
            Expected::NoExpectation(actual),
        );
        self.finish_conversions(uf, rank, &mut state.errors);
        self.retry_fields(uf, rank, &mut state.errors);
        let mut allocated = Vec::new();
        let subject =
            crate::representation::subject(uf, &self.tables.kinds, actual, &mut allocated);
        let function = matches!(
            uf.get(subject).content,
            Content::Structure(FlatType::Fun1(..) | FlatType::Function1(..))
        );
        let monomorphic = !function || self.concrete_type(uf, subject, true, &mut allocated);
        self.introduce(uf, rank, &allocated);
        if !monomorphic {
            return add_error(
                state,
                Error::PolymorphicModuleConstant {
                    region,
                    typ: to_error_type(self.bump, uf, actual),
                },
            );
        }
        self.equal(
            uf,
            rank,
            state,
            region,
            Category::CallResult(nash_constrain::error::MaybeName::NoName),
            actual,
            expected,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn infer_runnable(
        &mut self,
        uf: &mut UnionFind<'a>,
        env: &Env<'a>,
        rank: usize,
        mut state: State<'a>,
        rtv: &BTreeMap<&'a str, Variable>,
        generator: Option<&Located<Expr<'a>>>,
        argument_type: Option<&'a Located<Type<'a>>>,
        return_type: Option<&'a Located<Type<'a>>>,
        function: &Located<Expr<'a>>,
        benchmark: bool,
        region: Region,
        expected: Expected<'a, Variable>,
    ) -> State<'a> {
        let mut arguments = Vec::new();
        let mut variables = rtv.clone();
        let mut names = BTreeSet::new();
        for annotation in argument_type.into_iter().chain(return_type) {
            nash_can::types::collect_free_vars(&annotation.value, &mut names);
        }
        for name in names {
            if !variables.contains_key(name) {
                let variable = self.register(uf, rank, Content::RigidVar(name));
                variables.insert(name, variable);
            }
        }
        if let Some(generator) = generator {
            let via = self.fresh(uf, rank);
            state = self.infer_annotation_scope(
                uf,
                env,
                rank,
                state,
                rtv,
                generator,
                Expected::NoExpectation(via),
            );
            self.finish_conversions(uf, rank, &mut state.errors);
            self.retry_fields(uf, rank, &mut state.errors);
            let mut allocated = Vec::new();
            let target = self.runnable_target(uf, via, benchmark, &mut allocated);
            self.introduce(uf, rank, &allocated);
            let target = match target {
                Ok(target) => target,
                Err(message) => {
                    return self.runnable_error(uf, state, generator.region, via, message);
                }
            };
            // The target must be concrete before the annotation or body gets a
            // chance to constrain it (the pinned runnable boundary rule).
            let mut allocated = Vec::new();
            let valid = self.concrete_type(uf, target, false, &mut allocated);
            self.introduce(uf, rank, &allocated);
            if !valid {
                return self.runnable_error(
                    uf,
                    state,
                    generator.region,
                    target,
                    "A runnable generator must produce a concrete type containing no functions.",
                );
            }
            let prng = self.runnable_primitive(uf, rank, "data_prng", Vec::new());
            let tuple = self.structure(uf, rank, FlatType::Tuple1(prng, target, Vec::new()));
            let tuple = self.runnable_primitive(uf, rank, "data_tuple", vec![tuple]);
            let option = self.runnable_primitive(uf, rank, "data_option", vec![tuple]);
            let fuzzer = self.structure(uf, rank, FlatType::Function1(vec![prng], option));
            let signature = if benchmark {
                let size = self.runnable_primitive(uf, rank, "int", Vec::new());
                self.structure(uf, rank, FlatType::Function1(vec![size], fuzzer))
            } else {
                fuzzer
            };
            state = self.equal(
                uf,
                rank,
                state,
                generator.region,
                Category::CallResult(nash_constrain::error::MaybeName::NoName),
                via,
                Expected::NoExpectation(signature),
            );
            if let Some(annotation) = argument_type {
                let annotated = self.src_type_to_var(uf, rank, &variables, annotation);
                state = self.equal(
                    uf,
                    rank,
                    state,
                    annotation.region,
                    Category::CallResult(nash_constrain::error::MaybeName::NoName),
                    target,
                    Expected::NoExpectation(annotated),
                );
            }
            arguments.push(target);
        }
        let result = match return_type {
            Some(annotation) => self.src_type_to_var(uf, rank, &variables, annotation),
            None => self.fresh(uf, rank),
        };
        let function_type = self.structure(uf, rank, FlatType::Function1(arguments, result));
        state = self.infer_expr(
            uf,
            env,
            rank,
            state,
            &variables,
            function,
            Expected::NoExpectation(function_type),
        );
        if benchmark {
            self.queue_serialisable(rank, result, region, true);
        }
        if !benchmark {
            let mut allocated = Vec::new();
            let subject =
                crate::representation::subject(uf, &self.tables.kinds, result, &mut allocated);
            self.introduce(uf, rank, &allocated);
            let valid = matches!(uf.get(subject).content,
                Content::Structure(FlatType::App1(home, "bool" | "unit", ref args))
                    if home == nash_ast::primitives::builtin_home() && args.is_empty());
            if !valid {
                if matches!(uf.get(subject).content, Content::FlexVar(_)) {
                    let boolean = self.runnable_primitive(uf, rank, "bool", Vec::new());
                    state = self.equal(
                        uf,
                        rank,
                        state,
                        region,
                        Category::CallResult(nash_constrain::error::MaybeName::NoName),
                        result,
                        Expected::NoExpectation(boolean),
                    );
                } else if !matches!(uf.get(subject).content, Content::Error) {
                    return self.runnable_error(
                        uf,
                        state,
                        region,
                        result,
                        "A test body must return Bool or Void.",
                    );
                }
            }
        }
        self.equal(
            uf,
            rank,
            state,
            region,
            Category::CallResult(nash_constrain::error::MaybeName::NoName),
            function_type,
            expected,
        )
    }

    fn runnable_primitive(
        &mut self,
        uf: &mut UnionFind<'a>,
        rank: usize,
        name: &'a str,
        arguments: Vec<Variable>,
    ) -> Variable {
        self.structure(
            uf,
            rank,
            FlatType::App1(nash_ast::primitives::builtin_home(), name, arguments),
        )
    }

    fn runnable_target(
        &self,
        uf: &mut UnionFind<'a>,
        via: Variable,
        benchmark: bool,
        allocated: &mut Vec<Variable>,
    ) -> Result<Variable, &'static str> {
        let subject = |uf: &mut UnionFind<'a>, value, allocated: &mut Vec<Variable>| {
            crate::representation::subject(uf, &self.tables.kinds, value, allocated)
        };
        let mut via = subject(uf, via, allocated);
        if benchmark {
            let Content::Structure(FlatType::Function1(arguments, result)) =
                uf.get(via).content.clone()
            else {
                return Err("A benchmark requires a sampler: fn(Int) -> Fuzzer<a>.");
            };
            if arguments.len() != 1 {
                return Err("A benchmark sampler must accept exactly one Int size argument.");
            }
            let size = subject(uf, arguments[0], allocated);
            if !matches!(uf.get(size).content, Content::Structure(FlatType::App1(home, "int", ref args))
                if home == nash_ast::primitives::builtin_home() && args.is_empty())
            {
                return Err("A benchmark sampler's size parameter must already have type Int.");
            }
            via = subject(uf, result, allocated);
        }
        let Content::Structure(FlatType::Function1(_, result)) = uf.get(via).content.clone() else {
            return Err("A runnable requires a fuzzer: fn(PRNG) -> Option<(PRNG, a)>.");
        };
        let option = subject(uf, result, allocated);
        let Content::Structure(FlatType::App1(home, "data_option", args)) =
            uf.get(option).content.clone()
        else {
            return Err("A fuzzer must return Option<(PRNG, a)>.");
        };
        if home != nash_ast::primitives::builtin_home() || args.len() != 1 {
            return Err("A fuzzer must return Option<(PRNG, a)>.");
        }
        let tuple = subject(uf, args[0], allocated);
        let Content::Structure(FlatType::App1(home, "data_tuple", args)) =
            uf.get(tuple).content.clone()
        else {
            return Err("A fuzzer's Some payload must be the tuple (PRNG, a).");
        };
        if home != nash_ast::primitives::builtin_home() || args.len() != 1 {
            return Err("A fuzzer's Some payload must be the tuple (PRNG, a).");
        }
        let tuple = subject(uf, args[0], allocated);
        match uf.get(tuple).content.clone() {
            Content::Structure(FlatType::Tuple1(_, target, rest)) if rest.is_empty() => Ok(target),
            _ => Err("A fuzzer's Some payload must contain exactly a PRNG and a generated value."),
        }
    }

    fn concrete_type(
        &self,
        uf: &mut UnionFind<'a>,
        target: Variable,
        allow_functions: bool,
        allocated: &mut Vec<Variable>,
    ) -> bool {
        let mut pending = vec![target];
        let mut seen = BTreeSet::new();
        while let Some(variable) = pending.pop() {
            let variable =
                crate::representation::subject(uf, &self.tables.kinds, variable, allocated);
            if !seen.insert(uf.find(variable)) {
                continue;
            }
            match uf.get(variable).content.clone() {
                Content::FlexVar(_) | Content::RigidVar(_) | Content::PartialAlias { .. } => {
                    return false;
                }
                Content::Structure(FlatType::Fun1(argument, result)) => {
                    if !allow_functions {
                        return false;
                    }
                    pending.extend([argument, result]);
                }
                Content::Structure(FlatType::Function1(arguments, result)) => {
                    if !allow_functions {
                        return false;
                    }
                    pending.extend(arguments);
                    pending.push(result);
                }
                Content::Structure(FlatType::App1(_, _, args)) => pending.extend(args),
                Content::Structure(FlatType::AppV1(head, args)) => {
                    pending.push(head);
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
                    pending.extend(args.into_iter().map(|(_, argument)| argument));
                    pending.push(real);
                }
                Content::Error => {}
            }
        }
        true
    }

    fn runnable_error(
        &self,
        uf: &mut UnionFind<'a>,
        state: State<'a>,
        region: Region,
        variable: Variable,
        message: &'static str,
    ) -> State<'a> {
        add_error(
            state,
            Error::InvalidRunnable {
                region,
                message,
                typ: to_error_type(self.bump, uf, variable),
            },
        )
    }
}
