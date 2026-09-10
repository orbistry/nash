//! One fixture for every module, declaration, pattern and type error variant.
use super::*;
use crate::render_plain;
use nash_parse::error::*;

#[test]
fn variant_excessive_nesting() {
    let source = Source::new("f = (((1)))");
    let error = Module::Space(Space::TooDeep, 1, 7);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_space() {
    let source = Source::new("f = value");
    let error = Module::Space(Space::HasTab, 1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_bad_end() {
    let source = Source::new("f = value");
    let error = Module::BadEnd(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_problem() {
    let source = Source::new("f = value");
    let error = Module::Problem(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_validator() {
    let source = Source::new("f = value");
    let error = Module::Validator(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_name() {
    let source = Source::new("f = value");
    let error = Module::Name(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_exposing() {
    let source = Source::new("f = value");
    let error = Module::Exposing(&Exposing::Start(1, 3), 1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_fresh_line() {
    let source = Source::new("f = value");
    let error = Module::FreshLine(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_start() {
    let source = Source::new("f = value");
    let error = Module::ImportStart(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_name() {
    let source = Source::new("f = value");
    let error = Module::ImportName(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_as() {
    let source = Source::new("f = value");
    let error = Module::ImportAs(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_alias() {
    let source = Source::new("f = value");
    let error = Module::ImportAlias(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_exposing() {
    let source = Source::new("f = value");
    let error = Module::ImportExposing(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_exposing_list() {
    let source = Source::new("f = value");
    let error = Module::ImportExposingList(&Exposing::Start(1, 3), 1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_end() {
    let source = Source::new("f = value");
    let error = Module::ImportEnd(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_indent_name() {
    let source = Source::new("f = value");
    let error = Module::ImportIndentName(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_indent_alias() {
    let source = Source::new("f = value");
    let error = Module::ImportIndentAlias(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_import_indent_exposing_list() {
    let source = Source::new("f = value");
    let error = Module::ImportIndentExposingList(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_infix() {
    let source = Source::new("f = value");
    let error = Module::Infix(1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_declarations() {
    let source = Source::new("f = value");
    let error = Module::Declarations(&Decl::Start(1, 3), 1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_module_tests() {
    let source = Source::new("f = value");
    let error = Module::Tests(&Tests::Start(1, 3), 1, 3);
    let report = module::to_parse_error_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_space() {
    let source = Source::new("f = value");
    let error = Exposing::Space(Space::HasTab, 1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_start() {
    let source = Source::new("f = value");
    let error = Exposing::Start(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_value() {
    let source = Source::new("f = value");
    let error = Exposing::Value(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_operator() {
    let source = Source::new("f = value");
    let error = Exposing::Operator(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_operator_reserved() {
    let source = Source::new("f = value");
    let error = Exposing::OperatorReserved(BadOperator::Equals, 1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_operator_right_paren() {
    let source = Source::new("f = value");
    let error = Exposing::OperatorRightParen(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_type_privacy() {
    let source = Source::new("f = value");
    let error = Exposing::TypePrivacy(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_type_name() {
    let source = Source::new("f = value");
    let error = Exposing::TypeName(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_end() {
    let source = Source::new("f = value");
    let error = Exposing::End(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_indent_end() {
    let source = Source::new("f = value");
    let error = Exposing::IndentEnd(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_exposing_indent_value() {
    let source = Source::new("f = value");
    let error = Exposing::IndentValue(1, 3);
    let report = module::to_exposing_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_start() {
    let source = Source::new("f = value");
    let error = Decl::Start(1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_space() {
    let source = Source::new("f = value");
    let error = Decl::Space(Space::HasTab, 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type() {
    let source = Source::new("f = value");
    let error = Decl::Type(&DeclType::Name(1, 3), 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def() {
    let source = Source::new("f = value");
    let error = Decl::Def("f", &DeclDef::IndentBody(1, 3), 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_fresh_line_after_doc_comment() {
    let source = Source::new("f = value");
    let error = Decl::FreshLineAfterDocComment(1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_attribute() {
    let source = Source::new("f = value");
    let error = Decl::Attribute(&Attribute::Name(1, 3), 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_trait() {
    let source = Source::new("f = value");
    let error = Decl::Trait(&Trait::Name(1, 3), 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_impl() {
    let source = Source::new("f = value");
    let error = Decl::Impl(&Impl::BadHead(1, 3), 1, 3);
    let report = decl::to_declarations_report(&source, &error);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_space() {
    let source = Source::new("f = value");
    let error = DeclDef::Space(Space::HasTab, 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_equals() {
    let source = Source::new("f = value");
    let error = DeclDef::Equals(1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_type() {
    let source = Source::new("f = value");
    let error = DeclDef::Type(&Type::Start(1, 3), 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_arg() {
    let source = Source::new("f = value");
    let error = DeclDef::Arg(&Pattern::Start(1, 3), 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_body() {
    let source = Source::new("f = value");
    let error = DeclDef::Body(&Expr::Start(1, 3), 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_name_repeat() {
    let source = Source::new("f = value");
    let error = DeclDef::NameRepeat(1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_name_match() {
    let source = Source::new("f = value");
    let error = DeclDef::NameMatch("g", 1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_indent_type() {
    let source = Source::new("f = value");
    let error = DeclDef::IndentType(1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_indent_equals() {
    let source = Source::new("f = value");
    let error = DeclDef::IndentEquals(1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_def_indent_body() {
    let source = Source::new("f = value");
    let error = DeclDef::IndentBody(1, 3);
    let report = decl::to_decl_def_report(&source, "f", &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type_space() {
    let source = Source::new("f = value");
    let error = DeclType::Space(Space::HasTab, 1, 3);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type_name() {
    let source = Source::new("f = value");
    let error = DeclType::Name(1, 3);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type_alias() {
    let source = Source::new("f = value");
    let error = DeclType::Alias(&TypeAlias::Name(1, 3), 1, 3);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type_union() {
    let source = Source::new("f = value");
    let error = DeclType::Union(&CustomType::Name(1, 3), 1, 3);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_decl_type_indent_name() {
    let source = Source::new("f = value");
    let error = DeclType::IndentName(1, 3);
    let report = decl::to_decl_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_space() {
    let source = Source::new("f = value");
    let error = TypeAlias::Space(Space::HasTab, 1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_name() {
    let source = Source::new("f = value");
    let error = TypeAlias::Name(1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_param() {
    let source = Source::new("f = value");
    let error = TypeAlias::Param(&TypeParam::Start(1, 3), 1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_equals() {
    let source = Source::new("f = value");
    let error = TypeAlias::Equals(1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_body() {
    let source = Source::new("f = value");
    let error = TypeAlias::Body(&Type::Start(1, 3), 1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_indent_equals() {
    let source = Source::new("f = value");
    let error = TypeAlias::IndentEquals(1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_alias_indent_body() {
    let source = Source::new("f = value");
    let error = TypeAlias::IndentBody(1, 3);
    let report = decl::to_type_alias_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_space() {
    let source = Source::new("f = value");
    let error = CustomType::Space(Space::HasTab, 1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_name() {
    let source = Source::new("f = value");
    let error = CustomType::Name(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_param() {
    let source = Source::new("f = value");
    let error = CustomType::Param(&TypeParam::Start(1, 3), 1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_equals() {
    let source = Source::new("f = value");
    let error = CustomType::Equals(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_bar() {
    let source = Source::new("f = value");
    let error = CustomType::Bar(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_variant() {
    let source = Source::new("f = value");
    let error = CustomType::Variant(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_variant_arg() {
    let source = Source::new("f = value");
    let error = CustomType::VariantArg(&Type::Start(1, 3), 1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_equals() {
    let source = Source::new("f = value");
    let error = CustomType::IndentEquals(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_bar() {
    let source = Source::new("f = value");
    let error = CustomType::IndentBar(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_after_bar() {
    let source = Source::new("f = value");
    let error = CustomType::IndentAfterBar(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_after_equals() {
    let source = Source::new("f = value");
    let error = CustomType::IndentAfterEquals(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_field() {
    let source = Source::new("f = value");
    let error = CustomType::Field(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_field_colon() {
    let source = Source::new("f = value");
    let error = CustomType::FieldColon(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_field_type() {
    let source = Source::new("f = value");
    let error = CustomType::FieldType(&Type::Start(1, 3), 1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_field_end() {
    let source = Source::new("f = value");
    let error = CustomType::FieldEnd(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_field() {
    let source = Source::new("f = value");
    let error = CustomType::IndentField(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_custom_type_indent_field_type() {
    let source = Source::new("f = value");
    let error = CustomType::IndentFieldType(1, 3);
    let report = decl::to_custom_type_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_name() {
    let source = Source::new("f = value");
    let error = Attribute::Name(1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_arg() {
    let source = Source::new("f = value");
    let error = Attribute::Arg(&Expr::Start(1, 3), 1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_end() {
    let source = Source::new("f = value");
    let error = Attribute::End(1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_space() {
    let source = Source::new("f = value");
    let error = Attribute::Space(Space::HasTab, 1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_fresh_line() {
    let source = Source::new("f = value");
    let error = Attribute::FreshLine(1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_indent_arg() {
    let source = Source::new("f = value");
    let error = Attribute::IndentArg(1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_attribute_indent_end() {
    let source = Source::new("f = value");
    let error = Attribute::IndentEnd(1, 3);
    let report = decl::to_attribute_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_space() {
    let source = Source::new("f = value");
    let error = Trait::Space(Space::HasTab, 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_name() {
    let source = Source::new("f = value");
    let error = Trait::Name(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_param() {
    let source = Source::new("f = value");
    let error = Trait::Param(&TypeParam::Start(1, 3), 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_super() {
    let source = Source::new("f = value");
    let error = Trait::Super(&Type::Start(1, 3), 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_super_arg() {
    let source = Source::new("f = value");
    let error = Trait::SuperArg(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_where() {
    let source = Source::new("f = value");
    let error = Trait::Where(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_method_name() {
    let source = Source::new("f = value");
    let error = Trait::MethodName(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_colon() {
    let source = Source::new("f = value");
    let error = Trait::Colon(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_type() {
    let source = Source::new("f = value");
    let error = Trait::Type(&Type::Start(1, 3), 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_default() {
    let source = Source::new("f = value");
    let error = Trait::Default("f", &Def::IndentBody(1, 3), 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_name() {
    let source = Source::new("f = value");
    let error = Trait::IndentName(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_param() {
    let source = Source::new("f = value");
    let error = Trait::IndentParam(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_where() {
    let source = Source::new("f = value");
    let error = Trait::IndentWhere(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_method() {
    let source = Source::new("f = value");
    let error = Trait::IndentMethod(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_colon() {
    let source = Source::new("f = value");
    let error = Trait::IndentColon(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_indent_type() {
    let source = Source::new("f = value");
    let error = Trait::IndentType(1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_trait_alignment() {
    let source = Source::new("f = value");
    let error = Trait::Alignment(3, 1, 3);
    let report = decl::to_trait_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_space() {
    let source = Source::new("f = value");
    let error = Impl::Space(Space::HasTab, 1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_head() {
    let source = Source::new("f = value");
    let error = Impl::Head(&Type::Start(1, 3), 1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_bad_head() {
    let source = Source::new("f = value");
    let error = Impl::BadHead(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_where() {
    let source = Source::new("f = value");
    let error = Impl::Where(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_method() {
    let source = Source::new("f = value");
    let error = Impl::Method("f", &Def::IndentBody(1, 3), 1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_method_name() {
    let source = Source::new("f = value");
    let error = Impl::MethodName(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_indent_head() {
    let source = Source::new("f = value");
    let error = Impl::IndentHead(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_indent_where() {
    let source = Source::new("f = value");
    let error = Impl::IndentWhere(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_indent_method() {
    let source = Source::new("f = value");
    let error = Impl::IndentMethod(1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_impl_alignment() {
    let source = Source::new("f = value");
    let error = Impl::Alignment(3, 1, 3);
    let report = decl::to_impl_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_space() {
    let source = Source::new("f = value");
    let error = Tests::Space(Space::HasTab, 1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_import() {
    let source = Source::new("f = value");
    let error = Tests::Import(&Module::Problem(1, 3), 1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_test() {
    let source = Source::new("f = value");
    let error = Tests::Test(&Test::NameStart(1, 3), 1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_start() {
    let source = Source::new("f = value");
    let error = Tests::Start(1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_indent_start() {
    let source = Source::new("f = value");
    let error = Tests::IndentStart(1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_tests_alignment() {
    let source = Source::new("f = value");
    let error = Tests::Alignment(3, 1, 3);
    let report = module::to_tests_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_space() {
    let source = Source::new("f = value");
    let error = Test::Space(Space::HasTab, 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_name() {
    let source = Source::new("f = value");
    let error = Test::Name(StringError::EndlessSingle, 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_name_start() {
    let source = Source::new("f = value");
    let error = Test::NameStart(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_once_on_unit_test() {
    let source = Source::new("f = value");
    let error = Test::OnceOnUnitTest(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_within_open() {
    let source = Source::new("f = value");
    let error = Test::WithinOpen(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_within_kind() {
    let source = Source::new("f = value");
    let error = Test::WithinKind(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_within_number() {
    let source = Source::new("f = value");
    let error = Test::WithinNumber(Number::End, 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_within_duplicate() {
    let source = Source::new("f = value");
    let error = Test::WithinDuplicate(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_within_end() {
    let source = Source::new("f = value");
    let error = Test::WithinEnd(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_equals() {
    let source = Source::new("f = value");
    let error = Test::Equals(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_do() {
    let source = Source::new("f = value");
    let error = Test::Do(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_body() {
    let source = Source::new("f = value");
    let error = Test::Body(&Do::LastNotExpr(1, 3), 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_let() {
    let source = Source::new("f = value");
    let error = Test::Let(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_pattern() {
    let source = Source::new("f = value");
    let error = Test::Pattern(&Pattern::Start(1, 3), 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_via() {
    let source = Source::new("f = value");
    let error = Test::Via(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_fuzzer() {
    let source = Source::new("f = value");
    let error = Test::Fuzzer(&Expr::Start(1, 3), 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_in() {
    let source = Source::new("f = value");
    let error = Test::In(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_indent_name() {
    let source = Source::new("f = value");
    let error = Test::IndentName(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_indent_equals() {
    let source = Source::new("f = value");
    let error = Test::IndentEquals(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_indent_body() {
    let source = Source::new("f = value");
    let error = Test::IndentBody(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_indent_binder() {
    let source = Source::new("f = value");
    let error = Test::IndentBinder(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_indent_in() {
    let source = Source::new("f = value");
    let error = Test::IndentIn(1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_test_binder_alignment() {
    let source = Source::new("f = value");
    let error = Test::BinderAlignment(3, 1, 3);
    let report = module::to_test_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_record() {
    let source = Source::new("f = value");
    let error = Pattern::Record(&PRecord::Open(1, 3), 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_tuple() {
    let source = Source::new("f = value");
    let error = Pattern::Tuple(&PTuple::Open(1, 3), 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_list() {
    let source = Source::new("f = value");
    let error = Pattern::List(&PList::Open(1, 3), 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_start() {
    let source = Source::new("f = value");
    let error = Pattern::Start(1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_string() {
    let source = Source::new("f = value");
    let error = Pattern::String(StringError::EndlessSingle, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_bytes() {
    let source = Source::new("f = value");
    let error = Pattern::Bytes(Bytes::Endless, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_number() {
    let source = Source::new("f = value");
    let error = Pattern::Number(Number::End, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_alias() {
    let source = Source::new("f = value");
    let error = Pattern::Alias(1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_wildcard_not_var() {
    let source = Source::new("f = value");
    let error = Pattern::WildcardNotVar("_foo", 4, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_space() {
    let source = Source::new("f = value");
    let error = Pattern::Space(Space::HasTab, 1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_indent_start() {
    let source = Source::new("f = value");
    let error = Pattern::IndentStart(1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_pattern_indent_alias() {
    let source = Source::new("f = value");
    let error = Pattern::IndentAlias(1, 3);
    let report = pattern::to_pattern_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_open() {
    let source = Source::new("f = value");
    let error = PRecord::Open(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_end() {
    let source = Source::new("f = value");
    let error = PRecord::End(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_field() {
    let source = Source::new("f = value");
    let error = PRecord::Field(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_space() {
    let source = Source::new("f = value");
    let error = PRecord::Space(Space::HasTab, 1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_indent_open() {
    let source = Source::new("f = value");
    let error = PRecord::IndentOpen(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_indent_end() {
    let source = Source::new("f = value");
    let error = PRecord::IndentEnd(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_record_indent_field() {
    let source = Source::new("f = value");
    let error = PRecord::IndentField(1, 3);
    let report = pattern::to_p_record_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_open() {
    let source = Source::new("f = value");
    let error = PTuple::Open(1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_end() {
    let source = Source::new("f = value");
    let error = PTuple::End(1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_expr() {
    let source = Source::new("f = value");
    let error = PTuple::Expr(&Pattern::Start(1, 3), 1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_space() {
    let source = Source::new("f = value");
    let error = PTuple::Space(Space::HasTab, 1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_indent_end() {
    let source = Source::new("f = value");
    let error = PTuple::IndentEnd(1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_indent_expr1() {
    let source = Source::new("f = value");
    let error = PTuple::IndentExpr1(1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_tuple_indent_expr_n() {
    let source = Source::new("f = value");
    let error = PTuple::IndentExprN(1, 3);
    let report = pattern::to_p_tuple_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_open() {
    let source = Source::new("f = value");
    let error = PList::Open(1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_end() {
    let source = Source::new("f = value");
    let error = PList::End(1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_expr() {
    let source = Source::new("f = value");
    let error = PList::Expr(&Pattern::Start(1, 3), 1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_space() {
    let source = Source::new("f = value");
    let error = PList::Space(Space::HasTab, 1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_indent_open() {
    let source = Source::new("f = value");
    let error = PList::IndentOpen(1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_indent_end() {
    let source = Source::new("f = value");
    let error = PList::IndentEnd(1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_p_list_indent_expr() {
    let source = Source::new("f = value");
    let error = PList::IndentExpr(1, 3);
    let report = pattern::to_p_list_report(&source, pattern::PContext::Arg, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_record() {
    let source = Source::new("f = value");
    let error = Type::Record(&TRecord::Open(1, 3), 1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_tuple() {
    let source = Source::new("f = value");
    let error = Type::Tuple(&TTuple::Open(1, 3), 1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_start() {
    let source = Source::new("f = value");
    let error = Type::Start(1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_var_start() {
    let source = Source::new("f = value");
    let error = Type::VarStart(1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_context() {
    let source = Source::new("f = value");
    let error = Type::Context(1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_indent_after_context() {
    let source = Source::new("f = value");
    let error = Type::IndentAfterContext(1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_space() {
    let source = Source::new("f = value");
    let error = Type::Space(Space::HasTab, 1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_indent_start() {
    let source = Source::new("f = value");
    let error = Type::IndentStart(1, 3);
    let report = type_::to_type_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_open() {
    let source = Source::new("f = value");
    let error = TRecord::Open(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_end() {
    let source = Source::new("f = value");
    let error = TRecord::End(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_field() {
    let source = Source::new("f = value");
    let error = TRecord::Field(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_colon() {
    let source = Source::new("f = value");
    let error = TRecord::Colon(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_type() {
    let source = Source::new("f = value");
    let error = TRecord::Type(&Type::Start(1, 3), 1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_space() {
    let source = Source::new("f = value");
    let error = TRecord::Space(Space::HasTab, 1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_indent_open() {
    let source = Source::new("f = value");
    let error = TRecord::IndentOpen(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_indent_field() {
    let source = Source::new("f = value");
    let error = TRecord::IndentField(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_indent_colon() {
    let source = Source::new("f = value");
    let error = TRecord::IndentColon(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_indent_type() {
    let source = Source::new("f = value");
    let error = TRecord::IndentType(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_record_indent_end() {
    let source = Source::new("f = value");
    let error = TRecord::IndentEnd(1, 3);
    let report = type_::to_t_record_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_repr() {
    let source = Source::new("f = value");
    let error = TTuple::Repr(&Repr::Start(1, 3), 1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_indent_repr() {
    let source = Source::new("f = value");
    let error = TTuple::IndentRepr(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_open() {
    let source = Source::new("f = value");
    let error = TTuple::Open(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_end() {
    let source = Source::new("f = value");
    let error = TTuple::End(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_type() {
    let source = Source::new("f = value");
    let error = TTuple::Type(&Type::Start(1, 3), 1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_space() {
    let source = Source::new("f = value");
    let error = TTuple::Space(Space::HasTab, 1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_indent_type1() {
    let source = Source::new("f = value");
    let error = TTuple::IndentType1(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_indent_type_n() {
    let source = Source::new("f = value");
    let error = TTuple::IndentTypeN(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_t_tuple_indent_end() {
    let source = Source::new("f = value");
    let error = TTuple::IndentEnd(1, 3);
    let report = type_::to_t_tuple_report(&source, type_::TContext::Annotation("f"), &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_start() {
    let source = Source::new("f = value");
    let error = TypeParam::Start(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_colon() {
    let source = Source::new("f = value");
    let error = TypeParam::Colon(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_repr() {
    let source = Source::new("f = value");
    let error = TypeParam::Repr(&Repr::Start(1, 3), 1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_end() {
    let source = Source::new("f = value");
    let error = TypeParam::End(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_space() {
    let source = Source::new("f = value");
    let error = TypeParam::Space(Space::HasTab, 1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_indent_colon() {
    let source = Source::new("f = value");
    let error = TypeParam::IndentColon(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_indent_repr() {
    let source = Source::new("f = value");
    let error = TypeParam::IndentRepr(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_type_param_indent_end() {
    let source = Source::new("f = value");
    let error = TypeParam::IndentEnd(1, 3);
    let report = type_::to_type_param_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_repr_arrow() {
    let source = Source::new("f = value");
    let error = Repr::Arrow(1, 3);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_repr_start() {
    let source = Source::new("f = value");
    let error = Repr::Start(1, 3);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_repr_name() {
    let source = Source::new("f = value");
    let error = Repr::Name("Unknown", 1, 3);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}

#[test]
fn variant_repr_space() {
    let source = Source::new("f = value");
    let error = Repr::Space(Space::HasTab, 1, 3);
    let report = type_::to_repr_report(&source, &error, 1, 1);
    insta::assert_snapshot!(render_plain(&report, &source, "src/Main.nash"));
}
