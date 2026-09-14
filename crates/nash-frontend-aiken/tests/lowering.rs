use bumpalo::Bump;
use nash_frontend::{Frontend, ModuleName, ModuleRole, SourceInput};
use nash_frontend_aiken::AikenFrontend;
use nash_region::{Located, Position, Region};
use nash_source::{Constant, CtorArgs, Exposed, Exposing, Expr, Module, Pattern, Privacy, Type};
use url::Url;

fn parse<'a>(arena: &'a Bump, source: &'a str) -> &'a Module<'a> {
    let uri = Url::parse("file:///math.ak").unwrap();
    let name = ModuleName::new("math");
    AikenFrontend
        .parse(
            arena,
            SourceInput {
                source,
                uri: &uri,
                expected_module: &name,
                role: None,
            },
        )
        .unwrap()
        .module
}

fn failure(source: &str, role: Option<ModuleRole>) -> nash_frontend::FrontendFailure {
    let arena = Bump::new();
    let uri = Url::parse("file:///math.ak").unwrap();
    let name = ModuleName::new("math");
    AikenFrontend
        .parse(
            &arena,
            SourceInput {
                source,
                uri: &uri,
                expected_module: &name,
                role,
            },
        )
        .err()
        .expect("unsupported source must fail")
}

fn qualified_type<'a>(
    typ: &'a Located<Type<'a>>,
) -> (&'a str, &'a str, &'a [&'a Located<Type<'a>>]) {
    let Type::TypeQual {
        module, name, args, ..
    } = &typ.value
    else {
        panic!("expected a qualified type: {typ:?}")
    };
    (module, name, args)
}

#[test]
fn add_one_uses_real_integer_builtin_and_exact_type_region() {
    let arena = Bump::new();
    let module = parse(&arena, "pub fn add_one(value: Int) -> Int { value + 1 }");
    let value = &module.values[0].value;
    let Type::Lambda { from, to } = value.annotation.unwrap().typ.value else {
        panic!("function annotation")
    };
    assert_eq!(qualified_type(from).0, "Builtin");
    assert_eq!(qualified_type(from).1, "int");
    assert_eq!(qualified_type(to).1, "int");
    assert_eq!(
        from.region,
        Region::new(Position::new(1, 23), Position::new(1, 26))
    );
    let Expr::Call {
        function,
        arguments,
    } = value.body.value
    else {
        panic!("integer addition call")
    };
    assert!(matches!(
        function.value,
        Expr::VarQual {
            module: "Builtin",
            name: "addInteger",
            ..
        }
    ));
    assert!(matches!(
        arguments[1].value,
        Expr::Constant(Constant::Int(1))
    ));
    let Pattern::Var(parameter) = value.arguments[0].value else {
        panic!("parameter")
    };
    assert!(matches!(arguments[0].value, Expr::Var { name, .. } if name == parameter));
}

#[test]
fn primitive_types_use_constant_representations_not_big_names() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub type Inputs { Inputs(Int, ByteArray, Bool, String, Data, List<Int>, Pair<Int, ByteArray>) }",
    );
    let CtorArgs::Positional(args) = &module.unions[0].value.ctors[0].arguments else {
        panic!("positional fields")
    };
    let names = args
        .iter()
        .map(|typ| {
            let (module, name, _) = qualified_type(typ);
            assert_eq!(module, "Builtin");
            name
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["int", "bytes", "bool", "string", "Data", "list", "pair"]
    );
    assert_eq!(qualified_type(qualified_type(args[5]).2[0]).1, "int");
    assert_eq!(qualified_type(qualified_type(args[6]).2[1]).1, "bytes");
    assert_eq!(module.unions[0].value.name.value, "inputs");
}

#[test]
fn decoded_literals_are_fixed_constants_with_byte_columns() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "// π\r\npub const bytes = #\"00ff\"\r\npub const text = @\"é\"\r\npub const integer = 0xff",
    );
    assert!(matches!(
        module.values[0].value.body.value,
        Expr::Constant(Constant::Bytes(&[0, 255]))
    ));
    assert!(matches!(
        module.values[1].value.body.value,
        Expr::Constant(Constant::Str("é"))
    ));
    assert!(matches!(
        module.values[2].value.body.value,
        Expr::Constant(Constant::Int(255))
    ));
    assert_eq!(
        module.values[1].value.body.region,
        Region::new(Position::new(3, 18), Position::new(3, 23))
    );
}

#[test]
fn nullary_functions_and_calls_keep_the_native_unit_abi() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub fn thunk() -> Int { 1 }\npub fn force() -> Int { thunk() }",
    );
    let native = nash_parse::Parser::new(
        &arena,
        "module Math exposing (..)\nthunk () = 1\nforce () = thunk ()\n",
    )
    .module()
    .unwrap();
    for module in [module, &native] {
        assert!(
            matches!(module.values[0].value.arguments, [arg] if matches!(arg.value, Pattern::Unit))
        );
        let Expr::Call { arguments, .. } = module.values[1].value.body.value else {
            panic!("call")
        };
        assert!(matches!(arguments, [arg] if matches!(arg.value, Expr::Unit)));
    }
    let Type::Lambda { from, .. } = module.values[0].value.annotation.unwrap().typ.value else {
        panic!("nullary function annotation")
    };
    assert!(matches!(from.value, Type::Unit));
}

#[test]
fn sequential_shadowing_does_not_turn_into_recursive_let() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub fn increment(x: Int) -> Int {\n let x = x + 1\n x\n}",
    );
    let value = &module.values[0].value;
    let Pattern::Var(outer) = value.arguments[0].value else {
        panic!("outer binder")
    };
    let Expr::Call {
        function,
        arguments,
    } = value.body.value
    else {
        panic!("strict binding")
    };
    let Expr::Lambda { parameters, body } = function.value else {
        panic!("binding scope")
    };
    let Pattern::Var(inner) = parameters[0].value else {
        panic!("inner binder")
    };
    assert_ne!(outer, inner);
    assert!(matches!(body.value, Expr::Var { name, .. } if name == inner));
    let Expr::Call { arguments, .. } = arguments[0].value else {
        panic!("initializer")
    };
    assert!(matches!(arguments[0].value, Expr::Var { name, .. } if name == outer));
}

#[test]
fn imports_are_canonical_owned_and_renames_preserve_qualification() {
    let arena = Bump::new();
    let source =
        "use folder/math.{identity as id} as math\npub fn call(value: Int) -> Int { id(value) }";
    let inspected = {
        let uri = Url::parse("file:///caller.ak").unwrap();
        let name = ModuleName::new("caller");
        AikenFrontend
            .inspect(SourceInput {
                source,
                uri: &uri,
                expected_module: &name,
                role: None,
            })
            .unwrap()
    };
    assert_eq!(inspected.dependencies[0].module.as_str(), "folder.math");
    let module = parse(&arena, source);
    assert_eq!(
        module.imports[1].import.value,
        inspected.dependencies[0].module.as_str()
    );
    assert_eq!(
        module.imports[1].import.region,
        inspected.dependencies[0].region
    );
    let Expr::Call { function, .. } = module.values[0].value.body.value else {
        panic!("call")
    };
    assert!(matches!(
        function.value,
        Expr::VarQual {
            module: "math",
            name: "identity",
            ..
        }
    ));
    let shadow = parse(
        &arena,
        "use folder/math as math\npub fn read(math) { math.value }",
    );
    assert!(matches!(
        shadow.values[0].value.body.value,
        Expr::Access { .. }
    ));
}

#[test]
fn public_data_aliases_and_opaque_constructors_keep_visibility() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub type Choice { Some(Int) None }\npub opaque type Secret { Secret(ByteArray) }\npub type Alias = Choice\nfn private(value) { value }\npub fn choose(value: Choice) -> Int { when value is { Some(x) -> x None -> 0 } }",
    );
    let Exposing::Explicit(exports) = module.exports.value else {
        panic!("explicit visibility")
    };
    assert!(
        matches!(exports[0], Exposed::LowerType { name, privacy: Privacy::Public(_) } if name.value == "choice")
    );
    assert!(
        matches!(exports[1], Exposed::LowerType { name, privacy: Privacy::Private } if name.value == "secret")
    );
    assert!(
        !exports
            .iter()
            .any(|export| matches!(export, Exposed::Lower(name) if name.value == "private"))
    );
    assert!(matches!(
        module.aliases[0].value.typ.value,
        Type::Type { name: "choice", .. }
    ));
    let Expr::Case { arms, .. } = module.values[1].value.body.value else {
        panic!("pattern match")
    };
    assert!(matches!(
        arms[0].pattern.value,
        Pattern::Ctor {
            name: "Some",
            args: [_],
            ..
        }
    ));
    assert!(matches!(
        arms[1].pattern.value,
        Pattern::Ctor {
            name: "None",
            args: [],
            ..
        }
    ));
}

#[test]
fn short_circuit_and_comparison_preserve_left_to_right_evaluation() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub fn both(x: Bool) -> Bool { x && { fail } }\npub fn greater(x: Int, y: Int) -> Bool { x > y }",
    );
    let Expr::If {
        branches,
        final_else,
    } = module.values[0].value.body.value
    else {
        panic!("short circuit: {:#?}", module.values[0].value.body)
    };
    assert!(
        matches!(branches[0].then_branch.value, Expr::Call { function, .. } if matches!(function.value, Expr::VarQual { name: "error", .. }))
    );
    assert!(matches!(
        final_else.value,
        Expr::VarQual {
            module: "Builtin",
            name: "False",
            ..
        }
    ));
    let Expr::If { branches, .. } = module.values[1].value.body.value else {
        panic!("negated comparison")
    };
    let Expr::Call {
        function,
        arguments,
    } = branches[0].condition.value
    else {
        panic!("comparison")
    };
    assert!(matches!(
        function.value,
        Expr::VarQual {
            name: "lessThanEqualsInteger",
            ..
        }
    ));
    let parameters = module.values[1].value.arguments;
    for (arg, parameter) in arguments.iter().zip(parameters) {
        assert!(
            matches!((&arg.value, &parameter.value), (Expr::Var { name, .. }, Pattern::Var(parameter)) if name == parameter)
        );
    }
}

#[test]
fn signed_integer_bounds_include_the_minimum() {
    let arena = Bump::new();
    let module = parse(
        &arena,
        "pub const minimum = -170141183460469231731687303715884105728\npub const maximum = 170141183460469231731687303715884105727",
    );
    assert!(matches!(
        module.values[0].value.body.value,
        Expr::Constant(Constant::Int(i128::MIN))
    ));
    assert!(matches!(
        module.values[1].value.body.value,
        Expr::Constant(Constant::Int(i128::MAX))
    ));
    assert_eq!(
        failure(
            "pub const overflow = 170141183460469231731687303715884105728",
            None
        )
        .first
        .code,
        "NAF2101"
    );
}

#[test]
fn unsupported_semantics_have_stable_located_diagnostics() {
    for source in [
        "test addition() { 1 + 1 == 2 }",
        "@list pub type Encoded { Encoded(Int) }",
        "pub fn cast(value: Data) -> Int {\n expect x: Int = value\n x\n}",
        "pub fn equal(x, y) { x == y }",
        "pub fn partially_typed(x: a) { x }",
        "pub fn pair() { Pair(1, 2) }",
        "use aiken/builtin.{unsupported_builtin}\npub fn identity(x) { x }",
        "use aiken/builtin\npub fn unsupported(x) { builtin.unsupported_builtin(x) }",
    ] {
        let failure = failure(source, None);
        assert_eq!(failure.first.code, "NAF2201", "{source}: {failure:?}");
        assert!(
            failure
                .first
                .region
                .is_some_and(|region| region.start.line >= 1 && region.start.column >= 1)
        );
    }
    assert_eq!(
        failure("pub fn identity(x) { x }", Some(ModuleRole::Validator))
            .first
            .code,
        "NAF2301"
    );
    assert_eq!(failure("pub fn broken(", None).first.code, "NAF2001");
}

#[test]
fn validator_profile_rejects_unsupported_boundaries_during_inspection() {
    for source in [
        "validator v {\n spend(datum, redeemer, output, transaction) { True }\n}",
        "validator v { mint(r: Int, p: ByteArray, t: Data) { True } mint(r: Int, p: ByteArray, t: Data) { True } }",
        "validator v(limit: Int) { else(_) { True } }",
        "validator v { mint(r: List<Int>, p: ByteArray, t: Data) { True } }",
        "validator v { mint(r: Int, p: Data, t: Data) { True } }",
        "validator v { else(a, b) { True } }",
        "validator v { else(_) -> Int { 1 } }",
        "validator a { else(_) { True } } validator b { else(_) { True } }",
        "fn main() { True }\nvalidator v { else(_) { True } }",
        "use types.{Custom as Int}\nvalidator v { mint(r: Int, p: ByteArray, t: Data) { True } }",
    ] {
        let uri = Url::parse("file:///validator.ak").unwrap();
        let name = ModuleName::new("validator");
        let inspected = AikenFrontend
            .inspect(SourceInput {
                source,
                uri: &uri,
                expected_module: &name,
                role: None,
            })
            .unwrap_err();
        assert_eq!(inspected.first.code, "NAF2301", "{source}: {inspected:?}");
        let lowered = failure(source, None);
        assert_eq!(lowered.first.code, inspected.first.code);
        assert_eq!(lowered.first.region, inspected.first.region);
    }
    let located = failure(
        "validator v {\n spend(datum, redeemer, output, transaction) { True }\n}",
        None,
    );
    assert_eq!(located.first.region.unwrap().start, Position::new(2, 2));
    assert_eq!(
        failure(
            "validator v { else(_) { True } }",
            Some(ModuleRole::Library)
        )
        .first
        .code,
        "NAF2301"
    );
    assert_eq!(
        failure("validator v { else(context) { main(context) } }", None)
            .first
            .code,
        "NAF2201"
    );
    assert_eq!(
        failure("validator v { else(context) { v.else(context) } }", None)
            .first
            .code,
        "NAF2201"
    );
}

#[test]
fn official_builtin_imports_use_the_audited_nash_intrinsics() {
    let arena = Bump::new();
    let source = include_str!("fixtures/builtin_calls.ak");
    let uri = Url::parse("file:///builtin_calls.ak").unwrap();
    let name = ModuleName::new("builtin_calls");
    let inspected = AikenFrontend
        .inspect(SourceInput {
            source,
            uri: &uri,
            expected_module: &name,
            role: None,
        })
        .unwrap();
    assert_eq!(inspected.dependencies[0].module.as_str(), "Builtin");
    let module = parse(&arena, source);
    assert_eq!(
        module.imports[1].import.value,
        inspected.dependencies[0].module.as_str()
    );
    for (value, expected) in module.values.iter().zip(["addInteger", "equalsByteString"]) {
        let Expr::Call { function, .. } = value.value.body.value else {
            panic!("builtin application")
        };
        assert!(
            matches!(function.value, Expr::VarQual { module: "b", name, .. } if name == expected)
        );
    }
}
