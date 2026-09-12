//! Synthetic fixtures exercise every module, declaration, pattern and type error variant.
//! The source illustrates each error, including variants the parser cannot currently emit.
use super::*;
use crate::render_plain;
use nash_parse::error::*;

#[test]
fn variant_excessive_nesting() {
    let input = "f = (((1)))";
    let source = Source::new(input);
    let error = Module::Space(Space::TooDeep, 1, 7);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_space() {
    let input = "f =\t1";
    let source = Source::new(input);
    let error = Module::Space(Space::HasTab, 1, 4);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_bad_end() {
    let input = "f = 1\n)";
    let source = Source::new(input);
    let error = Module::BadEnd(2, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_problem() {
    let input = "module";
    let source = Source::new(input);
    let error = Module::Problem(1, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_validator() {
    let input = "validator lower";
    let source = Source::new(input);
    let error = Module::Validator(1, 11);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_name() {
    let input = "module lower exposing (..)";
    let source = Source::new(input);
    let error = Module::Name(1, 8);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_exposing() {
    let input = "module Main exposing ..";
    let source = Source::new(input);
    let error = Module::Exposing(&Exposing::Start(1, 22), 1, 22);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_fresh_line() {
    let input = "f = 1 g = 2";
    let source = Source::new(input);
    let error = Module::FreshLine(1, 7);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_start() {
    let input = "import ";
    let source = Source::new(input);
    let error = Module::ImportStart(1, 8);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_name() {
    let input = "import lower";
    let source = Source::new(input);
    let error = Module::ImportName(1, 8);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_as() {
    let input = "import Main alias";
    let source = Source::new(input);
    let error = Module::ImportAs(1, 13);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_alias() {
    let input = "import Main as lower";
    let source = Source::new(input);
    let error = Module::ImportAlias(1, 16);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_exposing() {
    let input = "import Main expose";
    let source = Source::new(input);
    let error = Module::ImportExposing(1, 13);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_exposing_list() {
    let input = "import Main exposing x";
    let source = Source::new(input);
    let error = Module::ImportExposingList(&Exposing::Start(1, 22), 1, 22);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_end() {
    let input = "import Main junk";
    let source = Source::new(input);
    let error = Module::ImportEnd(1, 13);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_indent_name() {
    let input = "import\nMain";
    let source = Source::new(input);
    let error = Module::ImportIndentName(2, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_indent_alias() {
    let input = "import Main as\nAlias";
    let source = Source::new(input);
    let error = Module::ImportIndentAlias(2, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_import_indent_exposing_list() {
    let input = "import Main exposing\n(x)";
    let source = Source::new(input);
    let error = Module::ImportIndentExposingList(2, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_infix() {
    let input = "infix ";
    let source = Source::new(input);
    let error = Module::Infix(1, 7);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_declarations() {
    let input = "=";
    let source = Source::new(input);
    let error = Module::Declarations(&Decl::Start(1, 1), 1, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_module_tests() {
    let input = "tests\nbad";
    let source = Source::new(input);
    let error = Module::Tests(&Tests::Start(2, 1), 2, 1);
    let report = module::to_parse_error_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_space() {
    let input = "module Main exposing (\tx)";
    let source = Source::new(input);
    let error = Exposing::Space(Space::HasTab, 1, 23);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_start() {
    let input = "module Main exposing x";
    let source = Source::new(input);
    let error = Exposing::Start(1, 22);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_value() {
    let input = "module Main exposing (1)";
    let source = Source::new(input);
    let error = Exposing::Value(1, 23);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_operator() {
    let input = "module Main exposing (())";
    let source = Source::new(input);
    let error = Exposing::Operator(1, 24);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_operator_reserved() {
    let input = "module Main exposing ((=))";
    let source = Source::new(input);
    let error = Exposing::OperatorReserved(BadOperator::Equals, 1, 24);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_operator_right_paren() {
    let input = "module Main exposing ((+ x))";
    let source = Source::new(input);
    let error = Exposing::OperatorRightParen(nash_region::Position::new(1, 23), 1, 26);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_type_privacy() {
    let input = "module Main exposing (Box(x))";
    let source = Source::new(input);
    let error = Exposing::TypePrivacy(1, 27);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_type_name() {
    let input = "module Main exposing (1)";
    let source = Source::new(input);
    let error = Exposing::TypeName(1, 23);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_end() {
    let input = "module Main exposing (x ";
    let source = Source::new(input);
    let error = Exposing::End(1, 25);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_indent_end() {
    let input = "module Main exposing (x\n)";
    let source = Source::new(input);
    let error = Exposing::IndentEnd(2, 1);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_exposing_indent_value() {
    let input = "module Main exposing (\nx)";
    let source = Source::new(input);
    let error = Exposing::IndentValue(2, 1);
    let report = module::to_exposing_report(&source, &error, 1, 22);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_start() {
    let input = "=";
    let source = Source::new(input);
    let error = Decl::Start(1, 1);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_space() {
    let input = "f\t= 1";
    let source = Source::new(input);
    let error = Decl::Space(Space::HasTab, 1, 2);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type() {
    let input = "type 1";
    let source = Source::new(input);
    let error = Decl::Type(&DeclType::Name(1, 6), 1, 6);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def() {
    let input = "f =\n1";
    let source = Source::new(input);
    let error = Decl::Def("f", &DeclDef::IndentBody(2, 1), 2, 1);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_fresh_line_after_doc_comment() {
    let input = "{-| docs -} f = 1";
    let source = Source::new(input);
    let error = Decl::FreshLineAfterDocComment(1, 13);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_attribute() {
    let input = "@1";
    let source = Source::new(input);
    let error = Decl::Attribute(&Attribute::Name(1, 2), 1, 2);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_trait() {
    let input = "trait 1";
    let source = Source::new(input);
    let error = Decl::Trait(&Trait::Name(1, 7), 1, 7);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_impl() {
    let input = "impl 1";
    let source = Source::new(input);
    let error = Decl::Impl(&Impl::BadHead(1, 6), 1, 6);
    let report = decl::to_declarations_report(&source, &error);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_space() {
    let input = "f\t= 1";
    let source = Source::new(input);
    let error = DeclDef::Space(Space::HasTab, 1, 2);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_equals() {
    let input = "f x ";
    let source = Source::new(input);
    let error = DeclDef::Equals(1, 5);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_type() {
    let input = "f : 42";
    let source = Source::new(input);
    let error = DeclDef::Type(&Type::Start(1, 5), 1, 5);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_arg() {
    let input = "f )";
    let source = Source::new(input);
    let error = DeclDef::Arg(&Pattern::Start(1, 3), 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_body() {
    let input = "f = )";
    let source = Source::new(input);
    let error = DeclDef::Body(&Expr::Start(1, 5), 1, 5);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_name_repeat() {
    let input = "f : int\n";
    let source = Source::new(input);
    let error = DeclDef::NameRepeat(2, 1);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_name_match() {
    let input = "f : int\ng = 1";
    let source = Source::new(input);
    let error = DeclDef::NameMatch("g", 2, 1);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_indent_type() {
    let input = "f :\nint";
    let source = Source::new(input);
    let error = DeclDef::IndentType(2, 1);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_indent_equals() {
    let input = "f x\n= 1";
    let source = Source::new(input);
    let error = DeclDef::IndentEquals(2, 1);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_def_indent_body() {
    let input = "f =\n1";
    let source = Source::new(input);
    let error = DeclDef::IndentBody(2, 1);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type_space() {
    let input = "type\tbox = Box";
    let source = Source::new(input);
    let error = DeclType::Space(Space::HasTab, 1, 5);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type_name() {
    let input = "type 1";
    let source = Source::new(input);
    let error = DeclType::Name(1, 6);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type_alias() {
    let input = "type alias 1";
    let source = Source::new(input);
    let error = DeclType::Alias(&TypeAlias::Name(1, 12), 1, 12);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type_union() {
    let input = "type 1 = Box";
    let source = Source::new(input);
    let error = DeclType::Union(&CustomType::Name(1, 6), 1, 6);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_decl_type_indent_name() {
    let input = "type\nbox";
    let source = Source::new(input);
    let error = DeclType::IndentName(2, 1);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_space() {
    let input = "type alias\tbox = int";
    let source = Source::new(input);
    let error = TypeAlias::Space(Space::HasTab, 1, 11);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_name() {
    let input = "type alias 1";
    let source = Source::new(input);
    let error = TypeAlias::Name(1, 12);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_param() {
    let input = "type alias box 1 = int";
    let source = Source::new(input);
    let error = TypeAlias::Param(&TypeParam::Start(1, 16), 1, 16);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_equals() {
    let input = "type alias box ";
    let source = Source::new(input);
    let error = TypeAlias::Equals(1, 16);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_body() {
    let input = "type alias box = 42";
    let source = Source::new(input);
    let error = TypeAlias::Body(&Type::Start(1, 18), 1, 18);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_indent_equals() {
    let input = "type alias box\n= int";
    let source = Source::new(input);
    let error = TypeAlias::IndentEquals(2, 1);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_alias_indent_body() {
    let input = "type alias box =\nint";
    let source = Source::new(input);
    let error = TypeAlias::IndentBody(2, 1);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_space() {
    let input = "type\tbox = Box";
    let source = Source::new(input);
    let error = CustomType::Space(Space::HasTab, 1, 5);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_name() {
    let input = "type 1";
    let source = Source::new(input);
    let error = CustomType::Name(1, 6);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_param() {
    let input = "type box 1 = Box";
    let source = Source::new(input);
    let error = CustomType::Param(&TypeParam::Start(1, 10), 1, 10);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_equals() {
    let input = "type box ";
    let source = Source::new(input);
    let error = CustomType::Equals(1, 10);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_bar() {
    let input = "type box = Box Other";
    let source = Source::new(input);
    let error = CustomType::Bar(1, 16);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_variant() {
    let input = "type box = lower";
    let source = Source::new(input);
    let error = CustomType::Variant(1, 12);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_variant_arg() {
    let input = "type box = Box 42";
    let source = Source::new(input);
    let error = CustomType::VariantArg(&Type::Start(1, 16), 1, 16);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_equals() {
    let input = "type box\n= Box";
    let source = Source::new(input);
    let error = CustomType::IndentEquals(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_bar() {
    let input = "type box = Box\n| Other";
    let source = Source::new(input);
    let error = CustomType::IndentBar(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_after_bar() {
    let input = "type box = Box |\nOther";
    let source = Source::new(input);
    let error = CustomType::IndentAfterBar(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_after_equals() {
    let input = "type box =\nBox";
    let source = Source::new(input);
    let error = CustomType::IndentAfterEquals(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_field() {
    let input = "type box = Box { 1 }";
    let source = Source::new(input);
    let error = CustomType::Field(1, 18);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_field_colon() {
    let input = "type box = Box { x int }";
    let source = Source::new(input);
    let error = CustomType::FieldColon(1, 20);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_field_type() {
    let input = "type box = Box { x : 42 }";
    let source = Source::new(input);
    let error = CustomType::FieldType(&Type::Start(1, 22), 1, 22);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_field_end() {
    let input = "type box = Box { x : int ";
    let source = Source::new(input);
    let error = CustomType::FieldEnd(nash_region::Position::new(1, 16), 1, 26);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_field() {
    let input = "type box = Box {\nx : int }";
    let source = Source::new(input);
    let error = CustomType::IndentField(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_custom_type_indent_field_type() {
    let input = "type box = Box { x :\nint }";
    let source = Source::new(input);
    let error = CustomType::IndentFieldType(2, 1);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_name() {
    let input = "@1";
    let source = Source::new(input);
    let error = Attribute::Name(1, 2);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_arg() {
    let input = "@name(=)";
    let source = Source::new(input);
    let error = Attribute::Arg(&Expr::Start(1, 7), 1, 7);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_end() {
    let input = "@name(1 ";
    let source = Source::new(input);
    let error = Attribute::End(nash_region::Position::new(1, 6), 1, 9);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_space() {
    let input = "@name(\t1)";
    let source = Source::new(input);
    let error = Attribute::Space(Space::HasTab, 1, 7);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_fresh_line() {
    let input = "@name f = 1";
    let source = Source::new(input);
    let error = Attribute::FreshLine(1, 7);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_indent_arg() {
    let input = "@name(\n1)";
    let source = Source::new(input);
    let error = Attribute::IndentArg(2, 1);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_attribute_indent_end() {
    let input = "@name(1\n)";
    let source = Source::new(input);
    let error = Attribute::IndentEnd(nash_region::Position::new(1, 6), 2, 1);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_space() {
    let input = "trait\tEq 'a where";
    let source = Source::new(input);
    let error = Trait::Space(Space::HasTab, 1, 6);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_name() {
    let input = "trait 1";
    let source = Source::new(input);
    let error = Trait::Name(1, 7);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_param() {
    let input = "trait Eq 1 where";
    let source = Source::new(input);
    let error = Trait::Param(&TypeParam::Start(1, 10), 1, 10);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_super() {
    let input = "trait 1 => Ord 'a where";
    let source = Source::new(input);
    let error = Trait::Super(&Type::Start(1, 7), 1, 7);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_super_arg() {
    let input = "trait Eq 1 => Ord 'a where";
    let source = Source::new(input);
    let error = Trait::SuperArg(1, 10);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_where() {
    let input = "trait Eq 'a ";
    let source = Source::new(input);
    let error = Trait::Where(1, 13);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_method_name() {
    let input = "trait Eq 'a where\n    1 : int";
    let source = Source::new(input);
    let error = Trait::MethodName(2, 5);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_colon() {
    let input = "trait Eq 'a where\n    equal int";
    let source = Source::new(input);
    let error = Trait::Colon(2, 11);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_type() {
    let input = "trait Eq 'a where\n    equal : 42";
    let source = Source::new(input);
    let error = Trait::Type(&Type::Start(2, 13), 2, 13);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_default() {
    let input = "trait Eq 'a where\n    equal : int\n    equal = )";
    let source = Source::new(input);
    let error = Trait::Default("f", &Def::IndentBody(3, 13), 3, 13);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_name() {
    let input = "trait\nEq";
    let source = Source::new(input);
    let error = Trait::IndentName(2, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_param() {
    let input = "trait Eq\n'a";
    let source = Source::new(input);
    let error = Trait::IndentParam(2, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_where() {
    let input = "trait Eq 'a\nwhere";
    let source = Source::new(input);
    let error = Trait::IndentWhere(2, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_method() {
    let input = "trait Eq 'a where\nequal : int";
    let source = Source::new(input);
    let error = Trait::IndentMethod(2, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_colon() {
    let input = "trait Eq 'a where\n    equal\n: int";
    let source = Source::new(input);
    let error = Trait::IndentColon(3, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_indent_type() {
    let input = "trait Eq 'a where\n    equal :\nint";
    let source = Source::new(input);
    let error = Trait::IndentType(3, 1);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_trait_alignment() {
    let input = "trait Eq 'a where\n    first : int\n  second : int";
    let source = Source::new(input);
    let error = Trait::Alignment(5, 3, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_space() {
    let input = "impl\tEq int where";
    let source = Source::new(input);
    let error = Impl::Space(Space::HasTab, 1, 5);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_head() {
    let input = "impl ";
    let source = Source::new(input);
    let error = Impl::Head(&Type::Start(1, 6), 1, 6);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_bad_head() {
    let input = "impl 1";
    let source = Source::new(input);
    let error = Impl::BadHead(1, 6);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_where() {
    let input = "impl Eq int ";
    let source = Source::new(input);
    let error = Impl::Where(1, 13);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_method() {
    let input = "impl Eq int where\n    f ";
    let source = Source::new(input);
    let error = Impl::Method("f", &Def::IndentBody(2, 7), 2, 7);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_method_name() {
    let input = "impl Eq int where\n    1 = 1";
    let source = Source::new(input);
    let error = Impl::MethodName(2, 5);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_indent_head() {
    let input = "impl\nEq int";
    let source = Source::new(input);
    let error = Impl::IndentHead(2, 1);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_indent_where() {
    let input = "impl Eq int\nwhere";
    let source = Source::new(input);
    let error = Impl::IndentWhere(2, 1);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_indent_method() {
    let input = "impl Eq int where\nf = 1";
    let source = Source::new(input);
    let error = Impl::IndentMethod(2, 1);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_impl_alignment() {
    let input = "impl Eq int where\n    f = 1\n  g = 2";
    let source = Source::new(input);
    let error = Impl::Alignment(5, 3, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_space() {
    let input = "tests\t";
    let source = Source::new(input);
    let error = Tests::Space(Space::HasTab, 1, 6);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_import() {
    let input = "tests\n    import lower";
    let source = Source::new(input);
    let error = Tests::Import(&Module::Problem(2, 12), 2, 12);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_test() {
    let input = "tests\n    test 1";
    let source = Source::new(input);
    let error = Tests::Test(&Test::NameStart(2, 10), 2, 10);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_start() {
    let input = "tests 1";
    let source = Source::new(input);
    let error = Tests::Start(1, 7);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_indent_start() {
    let input = "tests\ntest \"name\" = 1";
    let source = Source::new(input);
    let error = Tests::IndentStart(2, 1);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_tests_alignment() {
    let input = "tests\n    test \"a\" = 1\n  test \"b\" = 2";
    let source = Source::new(input);
    let error = Tests::Alignment(5, 3, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_space() {
    let input = "test\t\"name\" = 1";
    let source = Source::new(input);
    let error = Test::Space(Space::HasTab, 1, 5);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_name() {
    let input = "test \"unfinished";
    let source = Source::new(input);
    let error = Test::Name(
        StringError::EndlessSingle(nash_region::Position::new(1, 6)),
        1,
        7,
    );
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_name_start() {
    let input = "test 1";
    let source = Source::new(input);
    let error = Test::NameStart(1, 6);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_once_on_unit_test() {
    let input = "test \"name\" once = 1";
    let source = Source::new(input);
    let error = Test::OnceOnUnitTest(1, 13);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_within_open() {
    let input = "test \"name\" within 1";
    let source = Source::new(input);
    let error = Test::WithinOpen(1, 20);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_within_kind() {
    let input = "test \"name\" within (unknown 1)";
    let source = Source::new(input);
    let error = Test::WithinKind(1, 21);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_within_number() {
    let input = "test \"name\" within (cpu 12x)";
    let source = Source::new(input);
    let error = Test::WithinNumber(Number::End, 1, 25);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_within_duplicate() {
    let input = "test \"name\" within (cpu 1, cpu 2)";
    let source = Source::new(input);
    let error = Test::WithinDuplicate(1, 28);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_within_end() {
    let input = "test \"name\" within (cpu 1 ";
    let source = Source::new(input);
    let error = Test::WithinEnd(nash_region::Position::new(1, 20), 1, 27);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_equals() {
    let input = "test \"name\" ";
    let source = Source::new(input);
    let error = Test::Equals(1, 13);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_do() {
    let input = "test \"name\" = 1";
    let source = Source::new(input);
    let error = Test::Do(1, 15);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_body() {
    let input = "test \"name\" = do\n    x <- action";
    let source = Source::new(input);
    let error = Test::Body(&Do::LastNotExpr(2, 5), 2, 5);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_let() {
    let input = "prop \"name\" = do";
    let source = Source::new(input);
    let error = Test::Let(1, 15);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_pattern() {
    let input = "prop \"name\" = let ) via fuzz in do";
    let source = Source::new(input);
    let error = Test::Pattern(&Pattern::Start(1, 19), 1, 19);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_via() {
    let input = "prop \"name\" = let x fuzz in do";
    let source = Source::new(input);
    let error = Test::Via(1, 21);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_fuzzer() {
    let input = "prop \"name\" = let x via ) in do";
    let source = Source::new(input);
    let error = Test::Fuzzer(&Expr::Start(1, 25), 1, 25);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_in() {
    let input = "prop \"name\" = let x via fuzz do";
    let source = Source::new(input);
    let error = Test::In(1, 30);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_indent_name() {
    let input = "test\n\"name\"";
    let source = Source::new(input);
    let error = Test::IndentName(2, 1);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_indent_equals() {
    let input = "test \"name\"\n= 1";
    let source = Source::new(input);
    let error = Test::IndentEquals(2, 1);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_indent_body() {
    let input = "test \"name\" =\n1";
    let source = Source::new(input);
    let error = Test::IndentBody(2, 1);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_indent_binder() {
    let input = "prop \"name\" = let\nx via fuzz in do";
    let source = Source::new(input);
    let error = Test::IndentBinder(2, 1);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_indent_in() {
    let input = "prop \"name\" = let x via fuzz\nin do";
    let source = Source::new(input);
    let error = Test::IndentIn(2, 1);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_test_binder_alignment() {
    let input = "prop \"name\" = let\n    x via fuzz\n  y via fuzz\n    in do";
    let source = Source::new(input);
    let error = Test::BinderAlignment(5, 3, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_record() {
    let input = "f {= = 1";
    let source = Source::new(input);
    let error = Pattern::Record(&PRecord::Open(1, 4), 1, 4);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_tuple() {
    let input = "f (= = 1";
    let source = Source::new(input);
    let error = Pattern::Tuple(&PTuple::Open(1, 4), 1, 4);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_list() {
    let input = "f [= = 1";
    let source = Source::new(input);
    let error = Pattern::List(&PList::Open(1, 4), 1, 4);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_start() {
    let input = "f = 1";
    let source = Source::new(input);
    let error = Pattern::Start(1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_string() {
    let input = "f \"unfinished";
    let source = Source::new(input);
    let error = Pattern::String(
        StringError::EndlessSingle(nash_region::Position::new(1, 3)),
        1,
        3,
    );
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_bytes() {
    let input = "f #\"aa";
    let source = Source::new(input);
    let error = Pattern::Bytes(Bytes::Endless(nash_region::Position::new(1, 3)), 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_number() {
    let input = "f 12x = 1";
    let source = Source::new(input);
    let error = Pattern::Number(Number::End, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_alias() {
    let input = "f (x as ) = x";
    let source = Source::new(input);
    let error = Pattern::Alias(1, 9);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_wildcard_not_var() {
    let input = "f _name = 1";
    let source = Source::new(input);
    let error = Pattern::WildcardNotVar("_foo", 4, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_space() {
    let input = "f \tx = 1";
    let source = Source::new(input);
    let error = Pattern::Space(Space::HasTab, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_indent_start() {
    let input = "f\nx = 1";
    let source = Source::new(input);
    let error = Pattern::IndentStart(2, 1);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_pattern_indent_alias() {
    let input = "f (x as\nname) = x";
    let source = Source::new(input);
    let error = Pattern::IndentAlias(2, 1);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_open() {
    let input = "f {= = 1";
    let source = Source::new(input);
    let error = PRecord::Open(1, 4);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_end() {
    let input = "f {x = 1";
    let source = Source::new(input);
    let error = PRecord::End(1, 6);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_field() {
    let input = "f {x, 1} = 1";
    let source = Source::new(input);
    let error = PRecord::Field(1, 7);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_space() {
    let input = "f {\tx} = 1";
    let source = Source::new(input);
    let error = PRecord::Space(Space::HasTab, 1, 4);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_indent_open() {
    let input = "f {\nx} = 1";
    let source = Source::new(input);
    let error = PRecord::IndentOpen(2, 1);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_indent_end() {
    let input = "f {x\n} = 1";
    let source = Source::new(input);
    let error = PRecord::IndentEnd(2, 1);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_record_indent_field() {
    let input = "f {x,\ny} = 1";
    let source = Source::new(input);
    let error = PRecord::IndentField(2, 1);
    let report = pattern::to_p_record_report(&source, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_open() {
    let input = "f (= = 1";
    let source = Source::new(input);
    let error = PTuple::Open(1, 4);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_end() {
    let input = "f (x = 1";
    let source = Source::new(input);
    let error = PTuple::End(1, 6);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_expr() {
    let input = "f (x, =) = 1";
    let source = Source::new(input);
    let error = PTuple::Expr(&Pattern::Start(1, 7), 1, 7);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_space() {
    let input = "f (\tx) = 1";
    let source = Source::new(input);
    let error = PTuple::Space(Space::HasTab, 1, 4);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_indent_end() {
    let input = "f (x\n) = 1";
    let source = Source::new(input);
    let error = PTuple::IndentEnd(2, 1);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_indent_expr1() {
    let input = "f (\nx) = 1";
    let source = Source::new(input);
    let error = PTuple::IndentExpr1(2, 1);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_tuple_indent_expr_n() {
    let input = "f (x,\ny) = 1";
    let source = Source::new(input);
    let error = PTuple::IndentExprN(2, 1);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_open() {
    let input = "f [= = 1";
    let source = Source::new(input);
    let error = PList::Open(1, 4);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_end() {
    let input = "f [x = 1";
    let source = Source::new(input);
    let error = PList::End(1, 6);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_expr() {
    let input = "f [x, =] = 1";
    let source = Source::new(input);
    let error = PList::Expr(&Pattern::Start(1, 7), 1, 7);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_space() {
    let input = "f [\tx] = 1";
    let source = Source::new(input);
    let error = PList::Space(Space::HasTab, 1, 4);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_indent_open() {
    let input = "f [\nx] = 1";
    let source = Source::new(input);
    let error = PList::IndentOpen(2, 1);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_indent_end() {
    let input = "f [x\n] = 1";
    let source = Source::new(input);
    let error = PList::IndentEnd(2, 1);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_p_list_indent_expr() {
    let input = "f [x,\ny] = 1";
    let source = Source::new(input);
    let error = PList::IndentExpr(2, 1);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 3);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_record() {
    let input = "f : {=";
    let source = Source::new(input);
    let error = Type::Record(&TRecord::Open(1, 6), 1, 6);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_tuple() {
    let input = "f : (=";
    let source = Source::new(input);
    let error = Type::Tuple(&TTuple::Open(1, 6), 1, 6);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_start() {
    let input = "f : 42";
    let source = Source::new(input);
    let error = Type::Start(1, 5);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_var_start() {
    let input = "f : '1";
    let source = Source::new(input);
    let error = Type::VarStart(1, 6);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_context() {
    let input = "f : Eq 'a => ";
    let source = Source::new(input);
    let error = Type::Context(1, 14);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_indent_after_context() {
    let input = "f : Eq 'a =>\nint";
    let source = Source::new(input);
    let error = Type::IndentAfterContext(2, 1);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_space() {
    let input = "f : \tint";
    let source = Source::new(input);
    let error = Type::Space(Space::HasTab, 1, 5);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_indent_start() {
    let input = "f :\nint";
    let source = Source::new(input);
    let error = Type::IndentStart(2, 1);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_open() {
    let input = "f : {=";
    let source = Source::new(input);
    let error = TRecord::Open(1, 6);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_end() {
    let input = "f : { x : int ";
    let source = Source::new(input);
    let error = TRecord::End(1, 15);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_field() {
    let input = "f : { 1 : int }";
    let source = Source::new(input);
    let error = TRecord::Field(1, 7);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_colon() {
    let input = "f : { x int }";
    let source = Source::new(input);
    let error = TRecord::Colon(1, 9);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_type() {
    let input = "f : { x : 42 }";
    let source = Source::new(input);
    let error = TRecord::Type(&Type::Start(1, 11), 1, 11);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_space() {
    let input = "f : {\tx : int }";
    let source = Source::new(input);
    let error = TRecord::Space(Space::HasTab, 1, 6);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_indent_open() {
    let input = "f : {\nx : int }";
    let source = Source::new(input);
    let error = TRecord::IndentOpen(2, 1);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_indent_field() {
    let input = "f : { x : int,\ny : int }";
    let source = Source::new(input);
    let error = TRecord::IndentField(2, 1);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_indent_colon() {
    let input = "f : { x\n: int }";
    let source = Source::new(input);
    let error = TRecord::IndentColon(2, 1);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_indent_type() {
    let input = "f : { x :\nint }";
    let source = Source::new(input);
    let error = TRecord::IndentType(2, 1);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_record_indent_end() {
    let input = "f : { x : int\n}";
    let source = Source::new(input);
    let error = TRecord::IndentEnd(2, 1);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_repr() {
    let input = "f : ('a : 1)";
    let source = Source::new(input);
    let error = TTuple::Repr(&Repr::Start(1, 11), 1, 11);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_indent_repr() {
    let input = "f : ('a :\nTerm)";
    let source = Source::new(input);
    let error = TTuple::IndentRepr(2, 1);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_open() {
    let input = "f : (=";
    let source = Source::new(input);
    let error = TTuple::Open(1, 6);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_end() {
    let input = "f : (int, int ";
    let source = Source::new(input);
    let error = TTuple::End(1, 15);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_type() {
    let input = "f : (int, 42)";
    let source = Source::new(input);
    let error = TTuple::Type(&Type::Start(1, 11), 1, 11);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_space() {
    let input = "f : (\tint, int)";
    let source = Source::new(input);
    let error = TTuple::Space(Space::HasTab, 1, 6);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_indent_type1() {
    let input = "f : (\nint, int)";
    let source = Source::new(input);
    let error = TTuple::IndentType1(2, 1);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_indent_type_n() {
    let input = "f : (int,\nint)";
    let source = Source::new(input);
    let error = TTuple::IndentTypeN(2, 1);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_t_tuple_indent_end() {
    let input = "f : (int, int\n)";
    let source = Source::new(input);
    let error = TTuple::IndentEnd(2, 1);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 5);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_start() {
    let input = "type box (1) = Box";
    let source = Source::new(input);
    let error = TypeParam::Start(1, 11);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_colon() {
    let input = "type box ('a Term) = Box";
    let source = Source::new(input);
    let error = TypeParam::Colon(1, 14);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_repr() {
    let input = "type box ('a : 1) = Box";
    let source = Source::new(input);
    let error = TypeParam::Repr(&Repr::Start(1, 16), 1, 16);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_end() {
    let input = "type box ('a : Term = Box";
    let source = Source::new(input);
    let error = TypeParam::End(1, 21);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_space() {
    let input = "type box ('a\t: Term) = Box";
    let source = Source::new(input);
    let error = TypeParam::Space(Space::HasTab, 1, 13);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_indent_colon() {
    let input = "type box ('a\n: Term) = Box";
    let source = Source::new(input);
    let error = TypeParam::IndentColon(2, 1);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_indent_repr() {
    let input = "type box ('a :\nTerm) = Box";
    let source = Source::new(input);
    let error = TypeParam::IndentRepr(2, 1);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_type_param_indent_end() {
    let input = "type box ('a : Term\n) = Box";
    let source = Source::new(input);
    let error = TypeParam::IndentEnd(2, 1);
    let report = type_::to_type_param_report(&source, &error, 1, 10);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_repr_arrow() {
    let input = "type box ('a : Term Term) = Box";
    let source = Source::new(input);
    let error = Repr::Arrow(1, 21);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_repr_start() {
    let input = "type box ('a : ) = Box";
    let source = Source::new(input);
    let error = Repr::Start(1, 16);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_repr_name() {
    let input = "type box ('a : Unknown) = Box";
    let source = Source::new(input);
    let error = Repr::Name("Unknown", 1, 16);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}

#[test]
fn variant_repr_space() {
    let input = "type box ('a :\tTerm) = Box";
    let source = Source::new(input);
    let error = Repr::Space(Space::HasTab, 1, 15);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
    });
}
