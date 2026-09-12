use super::*;

use nash_region::Position;

fn region() -> Region {
    Region::new(Position::new(1, 1), Position::new(1, 6))
}
fn source_region(source: &str, text: &str) -> Region {
    let start = source.rfind(text).expect("highlight must occur in fixture");
    let position = |offset: usize| {
        let prefix = &source[..offset];
        Position::new(
            prefix.bytes().filter(|b| *b == b'\n').count() + 1,
            prefix.rsplit('\n').next().unwrap().len() + 1,
        )
    };
    Region::new(position(start), position(start + text.len()))
}

fn source_settings(source: &str) -> insta::Settings {
    let bump = bumpalo::Bump::new();
    nash_parse::Parser::new(&bump, source)
        .module()
        .expect("renderer fixture must be valid Nash syntax");
    let mut settings = insta::Settings::clone_current();
    settings.set_description(source);
    settings.set_omit_expression(true);
    settings
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
// Synthetic fixtures below exercise renderer branches; source_pipeline tests also verify producers.
fn show(source: &str, error: &Error<'_>) -> String {
    crate::render_plain(
        &to_report(
            &Localizer::from_names(["Builtin", "Main", "Eq", "Num"]),
            error,
        ),
        &crate::Source::new(source),
        "Main.nash",
    )
}

#[test]
fn mismatch_annotation_body() {
    let source = "module Main exposing (..)\nvalue : Builtin.int\nvalue = \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromAnnotation(
                "value",
                source_region(source, "Builtin.int"),
                0,
                SubContext::TypedBody,
                &int()
            )
        )
    ));
}

#[test]
fn mismatch_if_branches() {
    let source = "module Main exposing (..)\nvalue flag = if flag then 1 else \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::IfBranch(1, None), &int())
        )
    ));
}
#[test]
fn mismatch_case_branches() {
    let source = "module Main exposing (..)\nvalue flag =\n    case flag of\n        True -> 1\n        False -> \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::CaseBranch(1, None), &int())
        )
    ));
}
#[test]
fn mismatch_list_entries() {
    let source = "module Main exposing (..)\nvalue = [1, \"hello\"]\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::ListEntry(1, None), &int())
        )
    ));
}
#[test]
fn mismatch_if_condition_not_bool() {
    let source = "module Main exposing (..)\nvalue = if \"hello\" then 1 else 2\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let boolean = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "bool",
        args: &[],
    };
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::IfCondition, &boolean)
        )
    ));
}
#[test]
fn mismatch_call_arg_first() {
    let source =
        "module Main exposing (..)\nf : Builtin.int -> Builtin.int\nf x = x\nvalue = f \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::CallArg(MaybeName::FuncName("f"), 0),
                &int()
            )
        )
    ));
}
#[test]
fn mismatch_call_arg_second_has_hint() {
    let source = "module Main exposing (..)\nf : Builtin.int -> Builtin.int -> Builtin.int\nf x y = x\nvalue = f 1 \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::CallArg(MaybeName::FuncName("f"), 1),
                &int()
            )
        )
    ));
}
#[test]
fn too_many_args_on_value() {
    let source = "module Main exposing (..)\nvalue = \"hello\"\nresult = value 1 2\n";
    let region = || source_region(source, "value 1 2");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::CallArity(MaybeName::FuncName("value"), 2),
                &int()
            )
        )
    ));
}
#[test]
fn record_access_on_non_record() {
    let source = "module Main exposing (..)\nvalue = \"hello\"\nresult = value.name\n";
    let region = || source_region(source, "value");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::RecordAccess {
                    record_region: source_region(source, "value"),
                    maybe_name: Some("value"),
                    field_region: source_region(source, "name"),
                    field: "name"
                },
                &int()
            )
        )
    ));
}
#[test]
fn record_update_change_type() {
    let source =
        "module Main exposing (..)\nperson = { name = 1 }\nvalue = { person | name = \"hello\" }\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::RecordUpdateValue("name"), &int())
        )
    ));
}
#[test]
fn op_plus_left_string() {
    let source = "module Main exposing (..)\nvalue = \"hello\" + 1\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::OpLeft("+"), &int())
        )
    ));
}
#[test]
fn op_cons_right_not_list() {
    let source = "module Main exposing (..)\nvalue = 1 :: \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::OpRight("::"),
                &ErrorType::Type {
                    home: nash_ast::primitives::builtin_home(),
                    name: "list",
                    args: &[&int()]
                }
            )
        )
    ));
}
#[test]
fn op_compare_mismatch() {
    let source = "module Main exposing (..)\nvalue = 1 < \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::OpRight("<"), &int())
        )
    ));
}
#[test]
fn op_equality_mismatch() {
    let source = "module Main exposing (..)\nvalue = 1 == \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::OpRight("=="), &int())
        )
    ));
}
#[test]
fn op_pipe_right_not_function() {
    let source = "module Main exposing (..)\nvalue = 1 |> \"hello\"\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::OpRight("|>"),
                &ErrorType::Lambda(&int(), &int(), &[])
            )
        )
    ));
}
#[test]
fn destructure_mismatch() {
    let source = "module Main exposing (..)\nvalue =\n    let\n        (first, second) = \"hello\"\n    in\n    first\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(
                region(),
                Context::Destructure,
                &ErrorType::Tuple(&int(), &int(), &[])
            )
        )
    ));
}
#[test]
fn record_field_mismatch() {
    let source = "module Main exposing (..)\ntype alias Person = { name : Builtin.int }\nvalue : Person\nvalue = { name = \"hello\" }\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::String,
            &string(),
            Expected::FromContext(region(), Context::RecordField("value", "name"), &int())
        )
    ));
}

#[test]
fn pattern_case_first_mismatch() {
    let source = "module Main exposing (..)\nvalue number =\n    case number of\n        \"hello\" -> 1\n        0 -> 2\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(region(), PContext::CaseMatch(0), &int())
        )
    ));
}
#[test]
fn pattern_case_later_mismatch() {
    let source = "module Main exposing (..)\nvalue number =\n    case number of\n        0 -> 1\n        \"hello\" -> 2\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(region(), PContext::CaseMatch(1), &int())
        )
    ));
}
#[test]
fn pattern_ctor_arg_mismatch() {
    let source =
        "module Main exposing (..)\ntype Option = Some Builtin.int\nvalue (Some \"hello\") = 1\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(region(), PContext::CtorArg("Some", 0), &int())
        )
    ));
}
#[test]
fn pattern_typed_arg_mismatch() {
    let source = "module Main exposing (..)\nf : Builtin.int -> Builtin.int\nf \"hello\" = 1\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(
                region(),
                PContext::TypedArg("f", 0, source_region(source, "Builtin.int -> Builtin.int")),
                &int()
            )
        )
    ));
}
#[test]
fn pattern_list_entry() {
    let source = "module Main exposing (..)\nvalue [0, \"hello\"] = 1\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(region(), PContext::ListEntry(1), &int())
        )
    ));
}
#[test]
fn pattern_list_tail() {
    let source = "module Main exposing (..)\nvalue (first :: \"hello\") = first\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let list = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&int()],
    };
    insta::assert_snapshot!(show(
        source,
        &Error::BadPattern(
            region(),
            PCategory::Str,
            &string(),
            PExpected::FromContext(region(), PContext::Tail, &list)
        )
    ));
}

#[test]
fn too_many_args_on_function() {
    let source =
        "module Main exposing (..)\nf : Builtin.int -> Builtin.int\nf x = x\nvalue = f 1 2 3\n";
    let region = || source_region(source, "f 1 2 3");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let i = int();
    let function = ErrorType::Lambda(&i, &i, &[]);
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::Lambda,
            &function,
            Expected::FromContext(
                region(),
                Context::CallArity(MaybeName::FuncName("f"), 3),
                &i
            )
        )
    ));
}
#[test]
fn infinite_type() {
    let source = "module Main exposing (..)\nf x = x x\n";
    let region = || source_region(source, "x x");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let t = ErrorType::Lambda(&ErrorType::Infinite, &ErrorType::FlexVar("a"), &[]);
    insta::assert_snapshot!(show(
        source,
        &Error::InfiniteType {
            region: region(),
            name: "f",
            overall_type: &t
        }
    ));
}
#[test]
fn rigid_var_mismatch() {
    let source = "module Main exposing (..)\nf : 'a\nf = 1\n";
    let region = || source_region(source, "1");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::CallResult(MaybeName::NoName),
            &int(),
            Expected::FromAnnotation(
                "f",
                source_region(source, "'a"),
                0,
                SubContext::TypedBody,
                &ErrorType::RigidVar("a")
            )
        )
    ));
}
#[test]
fn record_access_missing_field_typo() {
    let source =
        "module Main exposing (..)\nperson = { name = \"hello\", age = 1 }\nvalue = person.naem\n";
    let region = || source_region(source, "naem");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
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
                record_region: source_region(source, "person"),
                maybe_name: Some("person"),
                field_region: region(),
                field: "naem",
            },
            &int(),
        ),
    );
    let report = to_report(&Localizer::from_names(["Builtin"]), &error);
    assert_eq!(report.suggestions, ["name", "age"]);
    insta::assert_snapshot!(show(source, &error));
}
#[test]
fn record_update_unknown_field() {
    let source = "module Main exposing (..)\nperson = { name = \"hello\" }\nvalue = { person | naem = () }\n";
    let region = || source_region(source, "naem");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let record = ErrorType::Record {
        fields: &[("name", &string())],
    };
    let field = nash_region::Located::at(region(), "naem");
    let value = nash_region::Located::at(region(), nash_ast::Expr::Unit);
    let updates = [nash_ast::FieldUpdate {
        field: &field,
        value: &value,
    }];
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::Record,
            &record,
            Expected::FromContext(
                region(),
                Context::RecordUpdateKeys("person", &updates),
                &int()
            )
        )
    ));
}
#[test]
fn missing_field_alias() {
    let source = "module Main exposing (..)\ntype alias Person = { age : Builtin.int }\nvalue : Person -> Builtin.int\nvalue person = person.aeg\n";
    let region = || source_region(source, "aeg");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
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
    insta::assert_snapshot!(show(
        source,
        &Error::MissingField {
            region: region(),
            context: FieldContext::Access {
                record_region: source_region(source, "person"),
                maybe_name: Some("person")
            },
            field: "aeg",
            record: &actual,
            available: &["age"]
        }
    ));
}
#[test]
fn op_append_string_list() {
    let source = "module Main exposing (..)\nvalue = \"hello\" ++ [1]\n";
    let region = || source_region(source, "[1]");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let list = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "list",
        args: &[&int()],
    };
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::List,
            &list,
            Expected::FromContext(region(), Context::OpRight("++"), &string())
        )
    ));
}
#[test]
fn op_cons_element_mismatch() {
    let source = "module Main exposing (..)\nvalue = 1 :: [\"hello\"]\n";
    let region = || source_region(source, "[\"hello\"]");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
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
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::List,
            &actual,
            Expected::FromContext(region(), Context::OpRight("::"), &expected)
        )
    ));
}
#[test]
fn op_pipe_argument_mismatch() {
    let source = "module Main exposing (..)\nf : Builtin.string -> Builtin.string\nf x = x\nvalue = 1 |> f\n";
    let region = || source_region(source, "f");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let actual = ErrorType::Lambda(&string(), &string(), &[]);
    let expected = ErrorType::Lambda(&int(), &string(), &[]);
    insta::assert_snapshot!(show(
        source,
        &Error::BadExpr(
            region(),
            Category::Lambda,
            &actual,
            Expected::FromContext(region(), Context::OpRight("|>"), &expected)
        )
    ));
}

#[test]
fn ambiguous_record_access() {
    let source = "module Main exposing (..)\nvalue = .name\n";
    let region = || source_region(source, ".name");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::AmbiguousRecordAccess {
            region: region(),
            context: nash_constrain::type_::FieldContext::Accessor,
            field: Some("name"),
            record: &ErrorType::FlexVar("a")
        }
    ));
}
#[test]
fn not_a_record_pattern() {
    let source =
        "module Main exposing (..)\nvalue : Builtin.int -> Builtin.int\nvalue { name } = name\n";
    let region = || source_region(source, "{ name }");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::NotARecord {
            region: region(),
            context: nash_constrain::type_::FieldContext::Pattern,
            field: Some("name"),
            record: &int()
        }
    ));
}
#[test]
fn update_not_record() {
    let source = "module Main exposing (..)\nperson = 1\nvalue = { person | age = 2 }\n";
    let region = || source_region(source, "person | age = 2");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::UpdateNotRecord {
            region: region(),
            record: &int()
        }
    ));
}
#[test]
fn field_mismatch_update() {
    let source =
        "module Main exposing (..)\nperson = { age = 1 }\nvalue = { person | age = \"hello\" }\n";
    let region = || source_region(source, "\"hello\"");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::FieldMismatch {
            region: region(),
            context: nash_constrain::type_::FieldContext::Update { record: "person" },
            field: "age",
            actual: &string(),
            expected: &int()
        }
    ));
}
#[test]
fn kind_mismatch() {
    let source = "module Main exposing (..)\nf : 'f Builtin.int -> 'f\nf x = x\n";
    let region = || source_region(source, "'f Builtin.int");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadKind {
            region: region(),
            name: "f",
            args: &[&int()],
            reason: nash_constrain::error::KindProblem::Mismatch {
                expected: &nash_ast::Kind::Arrow(&nash_ast::Kind::Type, &nash_ast::Kind::Type),
                actual: &nash_ast::Kind::Type
            }
        }
    ));
}
#[test]
fn infinite_kind() {
    let source = "module Main exposing (..)\nf : 'a 'a\nf = f\n";
    let region = || source_region(source, "'a 'a");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::BadKind {
            region: region(),
            name: "f",
            args: &[&ErrorType::FlexVar("a")],
            reason: nash_constrain::error::KindProblem::Infinite
        }
    ));
}
#[test]
fn ambiguous_type() {
    let source = "module Main exposing (..)\nvalue = let unused = 1 in ()\n";
    let region = || source_region(source, "value");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::AmbiguousType {
            region: region(),
            name: "value",
            variable: &ErrorType::FlexVar("a"),
            predicates: &[nash_constrain::error::AmbiguousPredicate {
                trait_: nash_ast::primitives::num_trait(),
                args: &[&ErrorType::FlexVar("a")]
            }]
        }
    ));
}
#[test]
fn contradictory_representation() {
    let source = "module Main exposing (..)\nf : (Big 'a, Little 'a) => 'a -> 'a\nf x = x\n";
    let region = || source_region(source, "'a -> 'a");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::ContradictoryRepresentation {
            region: region(),
            name: "f",
            typ: &ErrorType::RigidVar("a"),
            requirements: &[
                nash_ast::primitives::ReprTrait::Big,
                nash_ast::primitives::ReprTrait::Little
            ]
        }
    ));
}
#[test]
fn polymorphic_recursion() {
    let source = "module Main exposing (..)\nf x = if x == x then f [x] else f x\n";
    let region = || source_region(source, "f [x]");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::PolymorphicRecursion {
            region: region(),
            name: "f",
            trait_: nash_ast::primitives::eq_trait(),
            args: &[&ErrorType::FlexVar("a")]
        }
    ));
}
#[test]
fn unresolved_constraint() {
    let source = "module Main exposing (..)\nf x = x == x\n";
    let region = || source_region(source, "x == x");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::UnresolvedConstraint {
            region: region(),
            name: "f",
            trait_: nash_ast::primitives::eq_trait(),
            args: &[&ErrorType::FlexVar("a")]
        }
    ));
}
#[test]
fn unresolved_application() {
    let source = "module Main exposing (..)\nf : 'f Builtin.int -> Builtin.int\nf x = 1\n";
    let region = || source_region(source, "'f Builtin.int");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::UnresolvedApplication {
            region: region(),
            name: "f",
            head: &ErrorType::FlexVar("f"),
            args: &[&int()]
        }
    ));
}
#[test]
fn impl_resolution_limit() {
    let source = "module Main exposing (..)\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nimpl Eq (Builtin.list 'a) => Eq 'a where\n    eq xs ys = True\nf xs = eq xs xs\n";
    let region = || source_region(source, "eq xs xs");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::ImplResolutionLimit {
            region: region(),
            name: "f",
            trait_: nash_ast::primitives::eq_trait()
        }
    ));
}
#[test]
fn missing_constraint() {
    let source = "module Main exposing (..)\nf : 'a -> Builtin.bool\nf x = x == x\n";
    let region = || source_region(source, "x == x");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::MissingConstraint {
            region: region(),
            name: "==",
            trait_: nash_ast::primitives::eq_trait(),
            args: &[&ErrorType::RigidVar("a")],
            binder: &nash_region::Located::at(source_region(source, "f"), "f")
        }
    ));
}
#[test]
fn annotation_variable_escapes() {
    let source = "module Main exposing (..)\nf x =\n    let\n        inner : 'a\n        inner = x\n    in\n    inner\n";
    let region = || source_region(source, "inner = x");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::AnnotationVariableEscapes {
            region: region(),
            name: Some("f"),
            variable: &ErrorType::RigidVar("a")
        }
    ));
}
#[test]
fn missing_impl() {
    let source =
        "module Main exposing (..)\ntype step = Done | Next Builtin.int\nf = Done == Done\n";
    let region = || source_region(source, "Done == Done");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::MissingImpl {
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
    ));
}
#[test]
fn missing_storable_constraint_for_list_element() {
    let source = "module Main exposing (..)\nvalues = [\\x -> x + 1]\n";
    let region = || source_region(source, "[\\x -> x + 1]");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(show(
        source,
        &Error::MissingImpl {
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
    ));
}

#[test]
fn every_category() {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
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
            .map(|category| category_label(category))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
#[test]
fn every_pattern_category() {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
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
            .map(|category| pattern_label(category))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
#[test]
fn every_subcontext() {
    for (name, context, source) in [
        (
            "typed_if",
            SubContext::TypedIfBranch(1),
            "module Main exposing (..)\nf : Builtin.bool -> Builtin.int\nf flag = if flag then 1 else \"hello\"\n",
        ),
        (
            "typed_case",
            SubContext::TypedCaseBranch(1),
            "module Main exposing (..)\nf : Builtin.bool -> Builtin.int\nf flag =\n    case flag of\n        True -> 1\n        False -> \"hello\"\n",
        ),
    ] {
        let settings = source_settings(source);
        let _guard = settings.bind_to_scope();
        insta::assert_snapshot!(
            name,
            show(
                source,
                &Error::BadExpr(
                    source_region(source, "\"hello\""),
                    Category::String,
                    &string(),
                    Expected::FromAnnotation(
                        "f",
                        source_region(source, "Builtin.bool -> Builtin.int"),
                        1,
                        context,
                        &int()
                    )
                )
            )
        );
    }
}

#[test]
fn expression_and_pattern_without_expectation() {
    let source = "module Main exposing (..)\nvalue = [1, \"hello\"]\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(
        "expression_without_expectation",
        show(
            source,
            &Error::BadExpr(
                source_region(source, "\"hello\""),
                Category::String,
                &string(),
                Expected::NoExpectation(&int())
            )
        )
    );
    let source = "module Main exposing (..)\nvalue [0, \"hello\"] = 1\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(
        "pattern_without_expectation",
        show(
            source,
            &Error::BadPattern(
                source_region(source, "\"hello\""),
                PCategory::Str,
                &string(),
                PExpected::NoExpectation(&int())
            )
        )
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
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : A\nf = B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_call_argument() {
    let source =
        "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A\nf x = x\ng = f B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_if_branches() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf condition = if condition then A else B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_case_branches() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf a =\n    case a of\n        A -> A\n        _ -> B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_list_entries() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf = [A, B]\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}

#[test]
fn source_pipeline_if_condition() {
    let source = "module Main exposing (..)\ntype A = A\nf = if A then A else A\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_call_second_argument() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : A -> A -> A\nf x y = x\ng = f A B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_pattern_typed_arg() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\nf : B -> B\nf A = B\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_pattern_ctor_arg() {
    let source =
        "module Main exposing (..)\ntype A = A\ntype B = B\ntype Box = Box A\nf (Box B) = A\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_record_update_type() {
    let source = "module Main exposing (..)\ntype A = A\ntype B = B\ntype alias Person = { age : A }\nf : Person -> Person\nf p = { p | age = B }\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_record_access() {
    let source = "module Main exposing (..)\ntype A = A\ntype alias Person = { age : A }\nf : Person -> A\nf p = p.aeg\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_infinite_type() {
    let source = "module Main exposing (..)\nf x = x x\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_missing_impl() {
    let source = "module Main exposing (..)\ntype A = A\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : A -> Builtin.bool\nf a = eq a a\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}
#[test]
fn source_pipeline_missing_constraint() {
    let source = "module Main exposing (..)\ntrait Eq 'a where\n    eq : 'a -> 'a -> Builtin.bool\nf : 'a -> Builtin.bool\nf a = eq a a\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!(type_error_reports(source));
}

#[test]
fn example_one_big_little_annotation() {
    let source = "module Ledger exposing (settle)\n\ntype alias Account = { owner : Bytes, balance : Int }\n\nbalanceOf : Account -> Int\nbalanceOf account = account.balance\n\nsettle : list Account -> list int\nsettle accounts =\n    List.map balanceOf accounts\n";
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
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
        Expected::FromAnnotation(
            "settle",
            source_region(source, "list Account -> list int"),
            1,
            SubContext::TypedBody,
            &expected,
        ),
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
        let operand = match op {
            "&&" | "||" => "True",
            "++" => "[1]",
            "<|" if !left => "(\\x -> x + 1)",
            _ => "1",
        };
        let source = if left {
            format!("module Main exposing (..)\nvalue = \"hello\" {op} {operand}\n")
        } else {
            format!("module Main exposing (..)\nvalue = {operand} {op} \"hello\"\n")
        };
        let region = || source_region(&source, "\"hello\"");
        let settings = source_settings(&source);
        let _guard = settings.bind_to_scope();
        let actual = string();
        let expected_element = int();
        let expected = if matches!(op, "&&" | "||") {
            ErrorType::Type {
                home: nash_ast::primitives::builtin_home(),
                name: "bool",
                args: &[],
            }
        } else if op == "++" {
            ErrorType::Type {
                home: nash_ast::primitives::builtin_home(),
                name: "list",
                args: &[&expected_element],
            }
        } else if op == "<|" && left {
            ErrorType::Lambda(&expected_element, &expected_element, &[])
        } else {
            int()
        };
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
            crate::render_plain(&report, &crate::Source::new(&source), "Main.nash")
        );
    }
}

#[test]
fn problem_hints() {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    let _guard = settings.bind_to_scope();
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
fn missing_impl_local_union_suggests_a_supported_impl() {
    let source =
        "module Main exposing (..)\ntype step = Done | Next Builtin.int\nf = Done == Done\n";
    let region = || source_region(source, "Done == Done");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
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
    assert!(text.contains("Import or define an impl for"), "{text}");
    assert!(text.contains("step"), "{text}");
    assert!(!text.contains("@derive"));
    insta::assert_snapshot!(crate::render_plain(
        &report,
        &crate::Source::new(source),
        "Main.nash"
    ));
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
        let source = if name == "append_int_left" {
            "module Main exposing (..)\nvalue = 1 ++ \"hello\"\n"
        } else {
            "module Main exposing (..)\nvalue = \"hello\" ++ 1\n"
        };
        let region = || source_region(source, "1");
        let settings = source_settings(source);
        let _guard = settings.bind_to_scope();
        let error = Error::BadExpr(
            region(),
            Category::CallResult(MaybeName::NoName),
            &int(),
            Expected::FromContext(region(), context, &expected),
        );
        insta::assert_snapshot!(name, show(source, &error));
    }
}

#[test]
fn pipe_argument_mismatch_does_not_blame_the_function_operand() {
    let unit = ErrorType::Type {
        home: nash_ast::primitives::builtin_home(),
        name: "unit",
        args: &[],
    };
    let function = ErrorType::Lambda(&unit, &unit, &[]);
    let error = Error::BadExpr(
        region(),
        Category::Unit,
        &unit,
        Expected::FromContext(region(), Context::OpRight("<|"), &function),
    );
    let report = to_report(&Localizer::from_names([]), &error);
    assert!(!report.after.render(80, false).contains("left operand"));
    assert_eq!(
        report.primary_label.as_deref(),
        Some("right operand of (<|)")
    );
}

#[test]
fn validator_term_parameter() {
    let source = "module Main exposing (..)\nmain : Builtin.Data -> ((), ()) -> Builtin.bool\nmain datum pair = True\n";
    let region = || source_region(source, "((), ())");
    let settings = source_settings(source);
    let _guard = settings.bind_to_scope();
    let unit = nash_region::Located::at_zero(nash_ast::Type::unit());
    let typ = nash_region::Located::at(
        region(),
        nash_ast::Type::Tuple {
            first: &unit,
            second: &unit,
            rest: &[],
        },
    );
    insta::assert_snapshot!(show(
        source,
        &Error::MainParameterIsTerm {
            region: region(),
            index: 1,
            typ: &typ
        }
    ));
}
