use super::*;

use nash_region::Position;

fn region() -> Region {
    Region::new(Position::new(1, 1), Position::new(1, 6))
}
fn int() -> ErrorType<'static> {
    ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "int",
        args: &[],
    }
}
fn string() -> ErrorType<'static> {
    ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "string",
        args: &[],
    }
}
fn show(error: &Error<'_>) -> String {
    crate::render_plain(
        &to_report(
            &Localizer::from_names(["Builtin", "Main", "Eq", "Num"]),
            error,
        ),
        &crate::Source::new("value\n"),
        "Main.nash",
    )
}

#[test]
fn mismatch_annotation_body() {
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::String,
        &string(),
        Expected::FromAnnotation("value", 0, SubContext::TypedBody, &int())
    )));
}

macro_rules! context_snapshot {
    ($test:ident, $context:expr) => {
        #[test]
        fn $test() {
            insta::assert_snapshot!(show(&Error::BadExpr(
                region(),
                Category::String,
                &string(),
                Expected::FromContext(region(), $context, &int())
            )));
        }
    };
}
context_snapshot!(mismatch_if_branches, Context::IfBranch(1));
context_snapshot!(mismatch_case_branches, Context::CaseBranch(1));
context_snapshot!(mismatch_list_entries, Context::ListEntry(1));
#[test]
fn mismatch_if_condition_not_bool() {
    let boolean = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "bool",
        args: &[],
    };
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::String,
        &string(),
        Expected::FromContext(region(), Context::IfCondition, &boolean)
    )));
}
context_snapshot!(
    mismatch_call_arg_first,
    Context::CallArg(MaybeName::FuncName("f"), 0)
);
context_snapshot!(
    mismatch_call_arg_second_has_hint,
    Context::CallArg(MaybeName::FuncName("f"), 1)
);
context_snapshot!(
    too_many_args_on_value,
    Context::CallArity(MaybeName::FuncName("value"), 2)
);
context_snapshot!(
    record_access_on_non_record,
    Context::RecordAccess {
        record_region: region(),
        maybe_name: Some("value"),
        field_region: region(),
        field: "name"
    }
);
context_snapshot!(
    record_update_change_type,
    Context::RecordUpdateValue("name")
);
context_snapshot!(op_plus_left_string, Context::OpLeft("+"));
context_snapshot!(op_cons_right_not_list, Context::OpRight("::"));
context_snapshot!(op_compare_mismatch, Context::OpRight("<"));
context_snapshot!(op_equality_mismatch, Context::OpRight("=="));
context_snapshot!(op_pipe_right_not_function, Context::OpRight("|>"));
context_snapshot!(destructure_mismatch, Context::Destructure);
context_snapshot!(record_field_mismatch, Context::RecordField("value", "name"));

macro_rules! pattern_snapshot {
    ($test:ident, $context:expr) => {
        #[test]
        fn $test() {
            insta::assert_snapshot!(show(&Error::BadPattern(
                region(),
                PCategory::Str,
                &string(),
                PExpected::FromContext(region(), $context, &int())
            )));
        }
    };
}
pattern_snapshot!(pattern_case_first_mismatch, PContext::CaseMatch(0));
pattern_snapshot!(pattern_case_later_mismatch, PContext::CaseMatch(1));
pattern_snapshot!(pattern_ctor_arg_mismatch, PContext::CtorArg("Some", 0));
pattern_snapshot!(pattern_typed_arg_mismatch, PContext::TypedArg("f", 0));
pattern_snapshot!(pattern_list_entry, PContext::ListEntry(1));
#[test]
fn pattern_list_tail() {
    let list = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&int()],
    };
    insta::assert_snapshot!(show(&Error::BadPattern(
        region(),
        PCategory::Str,
        &string(),
        PExpected::FromContext(region(), PContext::Tail, &list)
    )));
}

#[test]
fn too_many_args_on_function() {
    let i = int();
    let function = ErrorType::Lambda(&i, &i, &[]);
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::Lambda,
        &function,
        Expected::FromContext(
            region(),
            Context::CallArity(MaybeName::FuncName("f"), 3),
            &i
        )
    )));
}
#[test]
fn infinite_type() {
    let t = ErrorType::Lambda(&ErrorType::Infinite, &ErrorType::FlexVar("a"), &[]);
    insta::assert_snapshot!(show(&Error::InfiniteType {
        region: region(),
        name: "f",
        overall_type: &t
    }));
}
#[test]
fn rigid_var_mismatch() {
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::CallResult(MaybeName::NoName),
        &int(),
        Expected::FromAnnotation("f", 0, SubContext::TypedBody, &ErrorType::RigidVar("a"))
    )));
}
#[test]
fn record_access_missing_field_typo() {
    let record = ErrorType::Record {
        fields: &[("name", &string()), ("age", &int())],
    };
    let error = Error::BadExpr(
        region(),
        Category::Record,
        &record,
        Expected::FromContext(
            region(),
            Context::RecordAccess {
                record_region: region(),
                maybe_name: Some("person"),
                field_region: region(),
                field: "naem",
            },
            &int(),
        ),
    );
    let report = to_report(&Localizer::from_names(["Builtin"]), &error);
    assert_eq!(report.suggestions, ["name", "age"]);
    insta::assert_snapshot!(show(&error));
}
#[test]
fn record_update_unknown_field() {
    let record = ErrorType::Record {
        fields: &[("name", &string())],
    };
    let field = nash_region::Located::at(region(), "naem");
    let value = nash_region::Located::at(region(), nash_ast::Expr::Unit);
    let updates = [nash_ast::FieldUpdate {
        field: &field,
        value: &value,
    }];
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::Record,
        &record,
        Expected::FromContext(
            region(),
            Context::RecordUpdateKeys("person", &updates),
            &int()
        )
    )));
}
#[test]
fn missing_field_alias() {
    use nash_constrain::type_::FieldContext;
    let i = int();
    let actual = ErrorType::Alias {
        home: nash_ast::ModuleName {
            package: None,
            name: "Main",
        },
        name: "Person",
        args: &[],
        real: &ErrorType::Record {
            fields: &[("age", &i)],
        },
    };
    insta::assert_snapshot!(show(&Error::MissingField {
        region: region(),
        context: FieldContext::Access {
            record_region: region(),
            maybe_name: Some("person")
        },
        field: "aeg",
        record: &actual,
        available: &["age"]
    }));
}
#[test]
fn op_append_string_list() {
    let list = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&int()],
    };
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::List,
        &list,
        Expected::FromContext(region(), Context::OpRight("++"), &string())
    )));
}
#[test]
fn op_cons_element_mismatch() {
    let actual = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&string()],
    };
    let expected = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&int()],
    };
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::List,
        &actual,
        Expected::FromContext(region(), Context::OpRight("::"), &expected)
    )));
}
#[test]
fn op_pipe_argument_mismatch() {
    let actual = ErrorType::Lambda(&string(), &string(), &[]);
    let expected = ErrorType::Lambda(&int(), &string(), &[]);
    insta::assert_snapshot!(show(&Error::BadExpr(
        region(),
        Category::Lambda,
        &actual,
        Expected::FromContext(region(), Context::OpRight("|>"), &expected)
    )));
}

macro_rules! error_snapshot {
    ($name:ident, $error:expr) => {
        #[test]
        fn $name() {
            insta::assert_snapshot!(show(&$error));
        }
    };
}
error_snapshot!(
    ambiguous_record_access,
    Error::AmbiguousRecordAccess {
        region: region(),
        context: nash_constrain::type_::FieldContext::Accessor,
        field: Some("name"),
        record: &ErrorType::FlexVar("a")
    }
);
error_snapshot!(
    not_a_record_pattern,
    Error::NotARecord {
        region: region(),
        context: nash_constrain::type_::FieldContext::Pattern,
        field: Some("name"),
        record: &int()
    }
);
error_snapshot!(
    update_not_record,
    Error::UpdateNotRecord {
        region: region(),
        record: &int()
    }
);
error_snapshot!(
    field_mismatch_update,
    Error::FieldMismatch {
        region: region(),
        context: nash_constrain::type_::FieldContext::Update { record: "person" },
        field: "age",
        actual: &string(),
        expected: &int()
    }
);
error_snapshot!(
    kind_mismatch,
    Error::BadKind {
        region: region(),
        name: "f",
        args: &[&int()],
        reason: nash_constrain::error::KindProblem::Mismatch {
            expected: &nash_ast::Kind::Arrow(&nash_ast::Kind::Type, &nash_ast::Kind::Type),
            actual: &nash_ast::Kind::Type
        }
    }
);
error_snapshot!(
    infinite_kind,
    Error::BadKind {
        region: region(),
        name: "f",
        args: &[&ErrorType::FlexVar("a")],
        reason: nash_constrain::error::KindProblem::Infinite
    }
);
error_snapshot!(
    ambiguous_type,
    Error::AmbiguousType {
        region: region(),
        name: "value",
        variable: &ErrorType::FlexVar("a"),
        predicates: &[nash_constrain::error::AmbiguousPredicate {
            trait_: nash_ast::primitives::num_trait(),
            args: &[&ErrorType::FlexVar("a")]
        }]
    }
);
error_snapshot!(
    contradictory_representation,
    Error::ContradictoryRepresentation {
        region: region(),
        name: "f",
        typ: &ErrorType::RigidVar("a"),
        requirements: &[
            nash_ast::primitives::ReprTrait::Big,
            nash_ast::primitives::ReprTrait::Little
        ]
    }
);
error_snapshot!(
    polymorphic_recursion,
    Error::PolymorphicRecursion {
        region: region(),
        name: "f",
        trait_: nash_ast::primitives::eq_trait(),
        args: &[&ErrorType::FlexVar("a")]
    }
);
error_snapshot!(
    unresolved_constraint,
    Error::UnresolvedConstraint {
        region: region(),
        name: "f",
        trait_: nash_ast::primitives::eq_trait(),
        args: &[&ErrorType::FlexVar("a")]
    }
);
error_snapshot!(
    unresolved_application,
    Error::UnresolvedApplication {
        region: region(),
        name: "f",
        head: &ErrorType::FlexVar("f"),
        args: &[&int()]
    }
);
error_snapshot!(
    impl_resolution_limit,
    Error::ImplResolutionLimit {
        region: region(),
        name: "f",
        trait_: nash_ast::primitives::eq_trait()
    }
);
error_snapshot!(
    missing_constraint,
    Error::MissingConstraint {
        region: region(),
        name: "==",
        trait_: nash_ast::primitives::eq_trait(),
        args: &[&ErrorType::RigidVar("a")],
        binder: &nash_region::Located::at(region(), "f")
    }
);
error_snapshot!(
    annotation_variable_escapes,
    Error::AnnotationVariableEscapes {
        region: region(),
        name: Some("f"),
        variable: &ErrorType::RigidVar("a")
    }
);
error_snapshot!(
    missing_impl,
    Error::MissingImpl {
        region: region(),
        name: "==",
        trait_: nash_ast::primitives::eq_trait(),
        args: &[&ErrorType::Type {
            home: nash_ast::ModuleName {
                package: None,
                name: "Main"
            },
            name: "step",
            args: &[]
        }],
        available: &[&[nash_ast::Head::Named {
            reference: nash_ast::QualifiedName {
                home: nash_ast::primitives::builtin_home(),
                name: "int"
            },
            args: &[]
        }]],
        because: &[]
    }
);
error_snapshot!(
    missing_storable_constraint_for_list_element,
    Error::MissingImpl {
        region: region(),
        name: "values",
        trait_: nash_ast::primitives::ReprTrait::Storable.qualified(),
        args: &[&ErrorType::Lambda(&int(), &int(), &[])],
        available: &[],
        because: &[nash_constrain::error::Requirement::Formation(
            &ErrorType::Type {
                home: nash_ast::primitives::builtin_home(),
                name: "list",
                args: &[&ErrorType::Lambda(&int(), &int(), &[])]
            }
        )]
    }
);

#[test]
fn every_category() {
    let categories = [
        Category::List,
        Category::String,
        Category::If,
        Category::Case,
        Category::CallResult(MaybeName::FuncName("f")),
        Category::CallResult(MaybeName::CtorName("Box")),
        Category::CallResult(MaybeName::OpName("+")),
        Category::CallResult(MaybeName::NoName),
        Category::Lambda,
        Category::Accessor("field"),
        Category::Access("field"),
        Category::Record,
        Category::Tuple,
        Category::Unit,
        Category::Local("local"),
        Category::Foreign("foreign"),
    ];
    insta::assert_snapshot!(
        categories
            .into_iter()
            .map(|category| add_category("It is", category))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
#[test]
fn every_pattern_category() {
    let categories = [
        PCategory::Record,
        PCategory::Unit,
        PCategory::Tuple,
        PCategory::List,
        PCategory::Ctor("Box"),
        PCategory::Int,
        PCategory::Bytes,
        PCategory::Str,
        PCategory::Bool,
    ];
    insta::assert_snapshot!(
        categories
            .into_iter()
            .map(|category| add_pattern_category("It matches", category))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
#[test]
fn every_subcontext() {
    for (name, context) in [
        ("typed_if", SubContext::TypedIfBranch(1)),
        ("typed_case", SubContext::TypedCaseBranch(1)),
    ] {
        insta::assert_snapshot!(
            name,
            show(&Error::BadExpr(
                region(),
                Category::String,
                &string(),
                Expected::FromAnnotation("f", 0, context, &int())
            ))
        );
    }
}
#[test]
fn expression_and_pattern_without_expectation() {
    insta::assert_snapshot!(
        "expression_without_expectation",
        show(&Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::NoExpectation(&int())
        ))
    );
    insta::assert_snapshot!(
        "pattern_without_expectation",
        show(&Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::NoExpectation(&int())
        ))
    );
}

fn type_error_reports(input: &str) -> String {
    let bump = bumpalo::Bump::new();
    let source = bump.alloc_str(input);
    let module = nash_parse::Parser::new(&bump, source)
        .module()
        .expect("parse fixture");
    let localizer = Localizer::from_module(&module, &[]);
    let canonical = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
        .expect("canonicalize fixture");
    let mut uf = nash_constrain::UnionFind::new();
    let module = &canonical.module;
    let errors = nash_solve::run(&bump, &mut uf, module, &canonical.tables)
        .expect_err("fixture must type-fail");
    assert!(!errors.is_empty());
    errors
        .iter()
        .map(|error| {
            crate::render_plain(
                &to_report(&localizer, error),
                &crate::Source::new(source),
                "Main.nash",
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn source_pipeline_annotation_body() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : A\nf = B\n"
    ));
}
#[test]
fn source_pipeline_call_argument() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A\nf x = x\ng = f B\n"
    ));
}
#[test]
fn source_pipeline_if_branches() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf condition = if condition then A else B\n"
    ));
}
#[test]
fn source_pipeline_case_branches() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf a =\n    case a of\n        A -> A\n        _ -> B\n"
    ));
}
#[test]
fn source_pipeline_list_entries() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf = [A, B]\n"
    ));
}

#[test]
fn source_pipeline_if_condition() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\nf = if A then A else A\n"
    ));
}
#[test]
fn source_pipeline_call_second_argument() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A -> A\nf x y = x\ng = f A B\n"
    ));
}
#[test]
fn source_pipeline_pattern_typed_arg() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : B -> B\nf A = B\n"
    ));
}
#[test]
fn source_pipeline_pattern_ctor_arg() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\ntype Box = Box A\nf (Box B) = A\n"
    ));
}
#[test]
fn source_pipeline_record_update_type() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype B = B\ntype alias Person = { age : A }\nf : Person -> Person\nf p = { p | age = B }\n"
    ));
}
#[test]
fn source_pipeline_record_access() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntype alias Person = { age : A }\nf : Person -> A\nf p = p.aeg\n"
    ));
}
#[test]
fn source_pipeline_infinite_type() {
    insta::assert_snapshot!(type_error_reports("module Main exposing (..)\nf x = x x\n"));
}
#[test]
fn source_pipeline_missing_impl() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntype A = A\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : A -> Builtin.bool\nf a = eq a a\n"
    ));
}
#[test]
fn source_pipeline_missing_constraint() {
    insta::assert_snapshot!(type_error_reports(
        "module Main exposing (..)\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : 'a -> Builtin.bool\nf a = eq a a\n"
    ));
}

#[test]
fn example_one_big_little_annotation() {
    let source = "module Ledger exposing (settle)\n\ntype alias Account = { owner : Bytes, balance : Int }\n\nbalanceOf : Account -> Int\nbalanceOf account = account.balance\n\nsettle : list Account -> list int\nsettle accounts =\n    List.map balanceOf accounts\n";
    let home = nash_ast::primitives::builtin_home();
    let big = ErrorType::Type {
        home,
        name: "Int",
        args: &[],
    };
    let little = int();
    let actual = ErrorType::Type {
        home,
        name: "list",
        args: &[&big],
    };
    let expected = ErrorType::Type {
        home,
        name: "list",
        args: &[&little],
    };
    let error = Error::BadExpr(
        Region::new(Position::new(10, 5), Position::new(10, 32)),
        Category::CallResult(MaybeName::FuncName("List.map")),
        &actual,
        Expected::FromAnnotation("settle", 1, SubContext::TypedBody, &expected),
    );
    let report = to_report(&Localizer::from_names(["Builtin"]), &error);
    insta::assert_snapshot!(crate::render_plain(
        &report,
        &crate::Source::new(source),
        "src/Ledger.nash"
    ));
}

#[test]
fn operator_branches() {
    let l = Localizer::from_names(["Builtin"]);
    for (name, op, left) in [
        ("minus_left", "-", true),
        ("multiply_left", "*", true),
        ("power_left", "^", true),
        ("division_left", "/", true),
        ("boolean_left", "&&", true),
        ("compare_left", "<", true),
        ("append_left", "++", true),
        ("pipe_left_not_function", "<|", true),
        ("custom_left", "<?>", true),
        ("plus_right", "+", false),
        ("minus_right", "-", false),
        ("multiply_right", "*", false),
        ("power_right", "^", false),
        ("division_right", "/", false),
        ("boolean_right", "||", false),
        ("pipe_left_argument", "<|", false),
        ("custom_right", "<?>", false),
    ] {
        let actual = string();
        let expected = int();
        let context = if left {
            Context::OpLeft(op)
        } else {
            Context::OpRight(op)
        };
        let report = to_report(
            &l,
            &Error::BadExpr(
                region(),
                Category::String,
                &actual,
                Expected::FromContext(region(), context, &expected),
            ),
        );
        insta::assert_snapshot!(
            name,
            Doc::stack([report.before, report.after]).render(80, false)
        );
    }
}

#[test]
fn problem_hints() {
    for (name, problem) in [
        ("hint_arity_fewer", Problem::ArityMismatch(1, 3)),
        ("hint_arity_more", Problem::ArityMismatch(3, 1)),
        (
            "hint_missing_fields",
            Problem::FieldsMissing(vec!["name", "age"]),
        ),
        (
            "hint_field_typo",
            Problem::FieldTypo("naem", vec!["age", "name"]),
        ),
        (
            "hint_big_little_need",
            Problem::BigLittle {
                big: "Int",
                little: "int",
                direction: Direction::Need,
            },
        ),
        ("hint_option", Problem::AnythingFromOption),
        (
            "hint_double_rigid",
            Problem::BadRigidVar("a", &ErrorType::RigidVar("b")),
        ),
    ] {
        insta::assert_snapshot!(
            name,
            Doc::stack(problem_to_hint(&problem)).render(80, false)
        );
    }
}

#[test]
fn missing_impl_local_union_deriving_not_yet_available() {
    let source = "module Main exposing (..)\ntype step = Done | Next Builtin.int\n";
    let bump = bumpalo::Bump::new();
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let l = Localizer::from_module(&module, &[]);
    let typ = ErrorType::Type {
        home: nash_ast::ModuleName {
            package: None,
            name: "Main",
        },
        name: "step",
        args: &[],
    };
    let error = Error::MissingImpl {
        region: region(),
        name: "==",
        trait_: nash_ast::primitives::eq_trait(),
        args: &[&typ],
        available: &[],
        because: &[],
    };
    let report = to_report(&l, &error);
    let text = report.after.render(80, false);
    assert!(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("automatic deriving is not available yet")
    );
    assert!(text.contains("eq a b = ..."));
    insta::assert_snapshot!(text);
}

#[test]
fn missing_impl_imported_or_custom_trait_has_no_derive_hint() {
    let source = "module Main exposing (..)\ntype step = Done\n";
    let bump = bumpalo::Bump::new();
    let module = nash_parse::Parser::new(&bump, source).module().unwrap();
    let l = Localizer::from_module(&module, &[]);
    for (home, trait_) in [
        (
            nash_ast::ModuleName {
                package: None,
                name: "Imported",
            },
            nash_ast::primitives::eq_trait(),
        ),
        (
            nash_ast::ModuleName {
                package: None,
                name: "Main",
            },
            nash_ast::QualifiedName {
                home: nash_ast::ModuleName {
                    package: None,
                    name: "Main",
                },
                name: "Eq",
            },
        ),
    ] {
        let typ = ErrorType::Type {
            home,
            name: "step",
            args: &[],
        };
        let error = Error::MissingImpl {
            region: region(),
            name: "eq",
            trait_,
            args: &[&typ],
            available: &[],
            because: &[],
        };
        assert!(
            !to_report(&l, &error)
                .after
                .render(80, false)
                .contains("@derive")
        );
    }
}

#[test]
fn append_number_hints_wrap() {
    for (name, context, expected) in [
        ("append_int_left", Context::OpLeft("++"), string()),
        ("append_int_to_string", Context::OpRight("++"), string()),
    ] {
        let error = Error::BadExpr(
            region(),
            Category::CallResult(MaybeName::NoName),
            &int(),
            Expected::FromContext(region(), context, &expected),
        );
        insta::assert_snapshot!(name, show(&error));
    }
}
