//! Canonicalization reports, adapted from Elm's Reporting/Error/Canonicalize.hs.
//! Nash adds trait, kind, representation, and nominal-record diagnostics.
use crate::{Doc, Label, Report, Source, suggest};
use nash_ast::{Kind, ModuleName, QualifiedName};
use nash_can::{
    BadArityContext, DuplicatePatternContext, Error, KindContext, PossibleNames, VarKind,
};
use nash_region::Region;

#[cfg(test)]
fn marked_source(input: &str) -> (String, Vec<Region>) {
    let mut text = String::new();
    let mut regions = Vec::new();
    let mut start = None;
    let mut row = 1;
    let mut column = 1;
    for ch in input.chars() {
        match ch {
            '«' => {
                assert!(start.is_none());
                start = Some(nash_region::Position::new(row, column));
            }
            '»' => regions.push(Region::new(
                start.take().expect("opening marker"),
                nash_region::Position::new(row, column),
            )),
            _ => {
                text.push(ch);
                if ch == '\n' {
                    row += 1;
                    column = 1;
                } else {
                    column += ch.len_utf8();
                }
            }
        }
    }
    assert!(start.is_none());
    (text, regions)
}

// Render-only fixtures isolate diagnostic branches, including errors that require
// foreign interfaces or exhausted solver budgets. Markers give their source spans.
// Pipeline fixtures additionally require the real canonicalizer to emit the variant.
#[cfg(test)]
macro_rules! source_snapshot {
    (@pipeline $name:expr, $input:expr, $r:ident, $r2:ident, $error:expr $(,)?) => {
        source_snapshot!(@render true, $name, $input, $r, $r2, $error);
    };
    ($name:expr, $input:expr, $r:ident, $r2:ident, $error:expr $(,)?) => {
        source_snapshot!(@render false, $name, $input, $r, $r2, $error);
    };
    (@render $pipeline:expr, $name:expr, $input:expr, $r:ident, $r2:ident, $error:expr $(,)?) => {{
        let (text, regions) = marked_source($input);
        #[allow(unused_variables)]
        let $r = || regions.first().copied().unwrap_or_else(Region::one);
        #[allow(unused_variables)]
        let $r2 = || regions.get(1).copied().unwrap_or_else($r);
        let error = $error;
        let text = if $pipeline { format!("module Main exposing (..)\n{text}\n") } else { text };
        let source = Source::new(&text);
        let rendered = if $pipeline {
            let bump = bumpalo::Bump::new();
            let mut parser = nash_parse::Parser::new(&bump, bump.alloc_str(&text));
            let module = parser.module().expect("fixture parses");
            let errors = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
                .expect_err("fixture has canonical errors");
            let actual = errors.iter().find(|actual| std::mem::discriminant(*actual) == std::mem::discriminant(&error))
                .unwrap_or_else(|| panic!("expected {error:?}, got {errors:?}"));
            crate::render_plain(&to_report(&source, actual), &source, "Main.nash")
        } else {
            crate::render_plain(&to_report(&source, &error), &source, "Main.nash")
        };
        insta::with_settings!({ description => text.as_str(), omit_expression => true }, {
            insta::assert_snapshot!($name, rendered);
        });
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn not_found_var() {
        source_snapshot!(@pipeline
            "not_found_var",
            "result = «name»", r, r2,
            Error::NotFoundVar {
                region: r(),
                prefix: None,
                name: "name",
                suggestions: PossibleNames {
                    locals: &[],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn not_found_var_with_suggestion() {
        source_snapshot!(@pipeline
            "not_found_var_with_suggestion",
            "name = 1\nother = 2\nresult = «naem»", r, r2,
            Error::NotFoundVar {
                region: r(),
                prefix: None,
                name: "naem",
                suggestions: PossibleNames {
                    locals: &["name", "other"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn validator_main_errors() {
        source_snapshot!(
            "validator_missing_main",
            "«validator module Main exposing (..)»\nvalue = 1",
            r,
            r2,
            Error::ValidatorMissingMain {
                region: Region::one(),
                module: "Main",
            },
        );
        source_snapshot!(
            "validator_main_not_exposed",
            "«validator module Main exposing (value)»\nvalue = 1\nmain context = True",
            r,
            r2,
            Error::ValidatorMainNotExposed {
                region: Region::one(),
                module: "Main",
            },
        );
    }

    #[test]
    fn missing_module_header() {
        source_snapshot!(
            "missing_module_header",
            "value = 1",
            r,
            r2,
            Error::MissingModuleHeader
        );
    }
    #[test]
    fn kind_mismatch() {
        source_snapshot!(
            "kind_mismatch",
            "value : «List»\nvalue = []",
            r,
            r2,
            Error::KindMismatch {
                region: r(),
                context: &KindContext::TypeAnnotation,
                expected: &Kind::Type,
                actual: &Kind::Arrow(&Kind::Type, &Kind::Type),
            },
        );
    }
}

pub fn to_report(source: &Source<'_>, error: &Error<'_>) -> Report {
    to_report_with_name(source, error, "Main")
}

pub fn to_report_with_name(source: &Source<'_>, error: &Error<'_>, expected_name: &str) -> Report {
    let report = match error {
        Error::ValidatorMissingMain { region, .. } => simple(
            "NO MAIN",
            *region,
            "This validator module requires a main value to compile into a script.",
            "Add a main value, or remove the validator keyword from the module header.",
        ),
        Error::ValidatorMainNotExposed { region, .. } => simple(
            "MAIN NOT EXPOSED",
            *region,
            "This validator module does not expose main.",
            "Add main to the exposing list.",
        ),
        Error::MissingModuleHeader => crate::syntax::to_report(
            source,
            &nash_parse::error::Error::ModuleNameUnspecified(expected_name),
        ),
        Error::NotFoundVar {
            region,
            prefix,
            name,
            suggestions,
        } => not_found(*region, *prefix, name, "variable", *suggestions),
        Error::NotFoundType {
            region,
            prefix,
            name,
            suggestions,
        } => not_found(*region, *prefix, name, "type", *suggestions),
        Error::NotFoundCtor {
            region,
            prefix,
            name,
            suggestions,
        } => not_found(*region, *prefix, name, "variant", *suggestions),
        Error::NotFoundTrait {
            region,
            prefix,
            name,
        } => not_found(
            *region,
            *prefix,
            name,
            "trait",
            PossibleNames {
                locals: &[],
                qualified: &[],
            },
        ),
        Error::AmbiguousVar {
            region,
            prefix,
            name,
            first_module,
            other_modules,
        } => ambiguous_name(
            *region,
            *prefix,
            name,
            *first_module,
            other_modules,
            "variable",
        ),
        Error::AmbiguousType {
            region,
            prefix,
            name,
            first_module,
            other_modules,
        } => ambiguous_name(*region, *prefix, name, *first_module, other_modules, "type"),
        Error::AmbiguousCtor {
            region,
            prefix,
            name,
            first_module,
            other_modules,
        } => ambiguous_name(
            *region,
            *prefix,
            name,
            *first_module,
            other_modules,
            "variant",
        ),
        Error::AmbiguousTrait {
            region,
            prefix,
            name,
            first_module,
            other_modules,
        } => ambiguous_name(
            *region,
            *prefix,
            name,
            *first_module,
            other_modules,
            "trait",
        ),
        Error::AmbiguousBinop {
            region,
            name,
            first_module,
            other_modules,
        } => ambiguous_name(
            *region,
            None,
            name,
            *first_module,
            other_modules,
            "operator",
        ),
        Error::BadArity {
            region,
            context,
            name,
            expected,
            actual,
        } => arity(
            *region,
            name,
            match context {
                BadArityContext::TypeArity => "type",
                BadArityContext::PatternArity => "variant",
            },
            *expected,
            *actual,
        ),
        Error::TraitArity {
            region,
            name,
            expected,
            actual,
        } => arity(*region, name, "trait", *expected, *actual),
        Error::DuplicateDecl {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This file has multiple `{name}` declarations."),
        ),
        Error::DuplicateType {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This file defines multiple `{name}` types."),
        ),
        Error::DuplicateCtor {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This file defines multiple `{name}` type constructors."),
        ),
        Error::DuplicateBinop {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This file defines multiple ({name}) operators."),
        ),
        Error::DuplicateField {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This record has multiple `{name}` fields."),
        ),
        Error::DuplicateTrait {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This file defines multiple `{name}` traits."),
        ),
        Error::DuplicateMethod {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This trait has multiple `{name}` methods."),
        ),
        Error::DuplicateTraitParameter {
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("This trait has multiple `{name}` type parameters."),
        ),
        Error::DuplicateAliasArg {
            type_name,
            arg_name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("The `{type_name}` type alias has multiple `{arg_name}` type variables."),
        ),
        Error::DuplicateUnionArg {
            type_name,
            arg_name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &format!("The `{type_name}` type has multiple `{arg_name}` type variables."),
        ),
        Error::DuplicatePattern {
            context,
            name,
            first,
            second,
        } => name_clash(
            *first,
            *second,
            &match context {
                DuplicatePatternContext::LambdaArgs => {
                    format!("This anonymous function has multiple `{name}` arguments.")
                }
                DuplicatePatternContext::FuncArgs(function) => {
                    format!("The `{function}` function has multiple `{name}` arguments.")
                }
                DuplicatePatternContext::CaseBranch => {
                    format!("This `case` pattern has multiple `{name}` variables.")
                }
                DuplicatePatternContext::LetBinding => {
                    format!("This `let` expression defines `{name}` more than once!")
                }
                DuplicatePatternContext::Destruct => {
                    format!("This pattern contains multiple `{name}` variables.")
                }
            },
        ),
        Error::ExportDuplicate {
            name,
            first,
            second,
        } => Report::pair(
            "REDUNDANT EXPORT",
            label(*first, "first export"),
            label(*second, "duplicate export"),
            Doc::reflow(&format!("Duplicate export `{name}`.")),
            Doc::text("Remove the duplicate export."),
        ),
        Error::ExportNotFound {
            region,
            kind,
            name,
            suggestions,
        } => {
            let (_article, thing, display) = to_kind_info(*kind, name);
            let nearby = nearby(name, suggestions, 4);
            let mut report = simple(
                "UNKNOWN EXPORT",
                *region,
                &format!("Unknown exported {thing} {display}."),
                "",
            );
            report.after = suggestion_details(
                &nearby,
                "Define the name or remove it from the exposing list.",
            );
            report.with_suggestions(nearby)
        }
        Error::ExportOpenAlias { region, name } => simple(
            "BAD EXPORT",
            *region,
            &format!("Type alias `{name}` has no variants to expose."),
            "Remove `(..)`.",
        ),
        Error::ImportOpenAlias { region, name } => simple(
            "BAD IMPORT",
            *region,
            &format!("Type alias `{name}` has no variants to import."),
            "Remove `(..)`.",
        ),
        Error::ImportCtorByName {
            region,
            name,
            type_name,
        } => simple(
            "BAD IMPORT",
            *region,
            &format!("Cannot import variant `{name}` directly."),
            &format!("Import `{type_name}(..)` to make its variants available."),
        ),
        Error::ImportNotFound { region, module } => simple(
            "UNKNOWN IMPORT",
            *region,
            &format!("Unknown module `{module}`."),
            "",
        ),
        Error::ImportExposingNotFound {
            region,
            module,
            name,
            available,
        } => {
            // Rank against the missing value, correcting Elm's module-name ranking typo.
            let nearby = nearby(name, available, 4);
            let mut report = simple(
                "BAD IMPORT",
                *region,
                &format!("The `{}` module does not expose `{name}`:", module.name),
                "",
            );
            report.after = suggestion_details(&nearby, "Check that the module exposes this name.");
            report.with_suggestions(nearby)
        }
        Error::BinopFunctionNotFound {
            region,
            op,
            function,
        } => simple(
            "INFIX PROBLEM",
            *region,
            &format!("Operator `({op})` refers to undefined function `{function}`."),
            "Define it, or point the `infix` declaration at an existing top-level value.",
        ),
        Error::BinopConflict { region, op1, op2 } => simple(
            "INFIX PROBLEM",
            *region,
            &format!("You cannot mix ({op1}) and ({op2}) without parentheses."),
            "Add parentheses to specify the grouping.",
        ),
        Error::NotFoundBinop {
            region,
            name,
            available,
        } => not_found_binop(*region, name, available),
        Error::PatternHasRecordCtor { region, name } => simple(
            "BAD PATTERN",
            *region,
            &format!("Record constructor `{name}` cannot be used in a pattern."),
            "Bind the record to a variable and access its fields.",
        ),
        Error::Shadowing {
            name,
            original,
            new,
        } => Report::pair(
            "SHADOWING",
            label(*original, "first defined here"),
            label(*new, "shadows this name"),
            Doc::reflow(&format!("Name `{name}` is already defined.")),
            Doc::text("Rename one of these bindings."),
        ),
        Error::RecursiveDecl { name, others } => {
            recursive_value(name.region, name.value, others, false)
        }
        Error::RecursiveLet { name, others } => {
            recursive_value(name.region, name.value, others, true)
        }
        Error::AnnotationTooShort {
            region,
            name,
            index,
            leftovers,
        } => simple(
            "BAD TYPE ANNOTATION",
            *region,
            &format!(
                "Annotation for `{name}` expects {}; definition has {}.",
                args(*index),
                args(index + leftovers)
            ),
            &format!(
                "Match the annotation to the definition's argument{}.",
                if *leftovers == 1 { "" } else { "s" }
            ),
        ),
        Error::RecursiveAlias {
            region,
            name,
            args,
            typ,
            others,
        } => alias_recursion_report(*region, name, args, typ, others),
        Error::TypeVarsUnboundInUnion {
            region,
            name,
            args,
            unbound,
            more_unbound,
        } => unbound_type_vars(*region, "type", name, args, *unbound, more_unbound),
        Error::TypeVarsMessedUpInAlias {
            region,
            name,
            args,
            unused,
            unbound,
        } => alias_vars(*region, name, args, unused, unbound),
        Error::KindMismatch {
            region,
            context,
            expected,
            actual,
        } => simple(
            "KIND MISMATCH",
            *region,
            &format!("Kind mismatch in {}.", kind_context(context)),
            &format!(
                "Expected kind `{}`, found `{}`.",
                kind(expected),
                kind(actual)
            ),
        ),
        Error::KindInfinite { region, context } => simple(
            "INFINITE KIND",
            *region,
            &format!(
                "This application in {} would require an infinite kind:",
                kind_context(context)
            ),
            "Check the type application and the kinds of its arguments.",
        ),
        Error::RepresentationMismatch {
            region,
            context,
            required,
            actual,
        } => simple(
            "REPRESENTATION MISMATCH",
            *region,
            &format!(
                "This position requires `{}` in {}:",
                required.name(),
                kind_context(context)
            ),
            &format!(
                "The type has `{}` representation, but `{}` admits {}. Change the type or the representation requirement.",
                repr(*actual),
                required.name(),
                admitted(*required)
            ),
        ),
        Error::ContradictoryRepresentation { region, variable } => simple(
            "CONTRADICTORY REPRESENTATION",
            *region,
            &format!("The representation requirements on '{variable} are incompatible:"),
            "Change the incompatible constraints or uses of this variable.",
        ),
        Error::IrregularRecursion {
            region,
            constructor,
            parameter,
        } => simple(
            "IRREGULAR RECURSION",
            *region,
            &format!(
                "This recursive use of `{}` constructs the parameter '{parameter}:",
                qualified(*constructor)
            ),
            "Pass a type variable for this parameter so context inference can terminate.",
        ),
        Error::ImplOfBuiltinTrait { region, trait_ } => simple(
            "BUILTIN TRAIT",
            *region,
            &format!(
                "The `{}` trait is owned by the compiler:",
                qualified(*trait_)
            ),
            "Its representation rules cannot be replaced by an impl. Remove this impl and use a type with an admitted representation.",
        ),
        Error::MissingMethod {
            region,
            trait_,
            name,
        } => simple(
            "MISSING METHOD",
            *region,
            &format!("This `{trait_}` impl does not define `{name}`:"),
            &format!("Add a `{name}` definition to this impl."),
        ),
        Error::UnknownMethod {
            region,
            trait_,
            name,
        } => simple(
            "UNKNOWN METHOD",
            *region,
            &format!("The `{trait_}` trait has no `{name}` method:"),
            "Remove or rename this method to match the trait declaration.",
        ),
        Error::BadInstanceHead { region, reason } => {
            use nash_can::BadHead;
            let reason = match reason {
                BadHead::BareVariable => "a bare type variable",
                BadHead::Function => "a function type",
                BadHead::Record => "an anonymous record",
                BadHead::VariableApplication => "an application of a type variable",
            };
            simple(
                "BAD IMPL HEAD",
                *region,
                &format!("This impl head is {reason}:"),
                "Use a named type constructor or a tuple as the outermost type of an impl head.",
            )
        }
        Error::OrphanImpl {
            region,
            trait_,
            heads,
        } => simple(
            "ORPHAN IMPL",
            *region,
            &format!(
                "This module cannot define an impl of `{}` for {}:",
                qualified(*trait_),
                heads.iter().map(head_con).collect::<Vec<_>>().join(", ")
            ),
            "An impl must be defined in the module that defines its trait or one of its head types. Move this impl to one of those modules.",
        ),
        Error::OverlappingImpls {
            key,
            first,
            second,
            first_home,
            second_home,
        } => Report::pair(
            "OVERLAPPING IMPL",
            label(*first, &format!("first impl in `{}`", first_home.name)),
            label(
                *second,
                &format!("overlapping impl in `{}`", second_home.name),
            ),
            Doc::reflow(&format!(
                "These `{}` impls can both match the same trait arguments. The overlapping head is {}:",
                qualified(key.trait_),
                key.heads.iter().map(head).collect::<Vec<_>>().join(" ")
            )),
            Doc::reflow(
                "Remove one impl or make their heads disjoint; context constraints do not disambiguate heads.",
            ),
        ),
        Error::MissingSuperclass {
            region,
            trait_,
            heads,
            superclass,
            index,
            reason,
        } => {
            let reason = match reason {
                nash_can::EntailmentFailure::Missing => {
                    "I cannot find an impl or context constraint that provides it."
                }
                nash_can::EntailmentFailure::Cycle => {
                    "Resolving it leads back to the same requirement, forming a cycle."
                }
                nash_can::EntailmentFailure::Limit => {
                    "Resolving it exceeded the search limit because the requirements keep expanding."
                }
            };
            simple(
                "MISSING SUPERCLASS",
                *region,
                &format!(
                    "The `{}` impl for {} does not establish superclass {}:",
                    qualified(*trait_),
                    heads
                        .iter()
                        .map(|h| head(&h.value))
                        .collect::<Vec<_>>()
                        .join(" "),
                    usize::from(*index) + 1
                ),
                &format!(
                    "It must also satisfy `{}`. {reason} Add the required impl or include the necessary constraint in this impl's context.",
                    predicate(superclass)
                ),
            )
        }
        Error::ImplContextVarNotInHead { region, name } => simple(
            "TRAIT PROBLEM",
            *region,
            &format!(
                "The impl context mentions '{name}, but this variable does not occur in the impl head:"
            ),
            "Each variable in an impl context must occur in its head. Check for a misspelled type variable.",
        ),
        Error::ContextVarNotInType { region, name } => simple(
            "TRAIT PROBLEM",
            *region,
            &format!(
                "The context mentions '{name}, but this variable does not occur in the annotated type:"
            ),
            "Use the variable in the type, or remove the constraint that mentions it.",
        ),
        Error::MethodMissingParameter {
            region,
            method,
            parameter,
        } => simple(
            "TRAIT PROBLEM",
            *region,
            &format!("The `{method}` method does not mention trait parameter '{parameter}:"),
            "Every trait parameter must occur in the method's type so a call can determine which impl to use.",
        ),
        Error::SuperclassBadArg { region, trait_ } => simple(
            "TRAIT PROBLEM",
            *region,
            &format!("The `{trait_}` superclass has an invalid argument:"),
            "Superclass arguments must be parameters declared by this trait. Replace this argument with the intended trait parameter.",
        ),
        Error::RecursiveSuperclass { names } => {
            let first = names.first();
            let region = first.map_or(
                Region::new(
                    nash_region::Position::new(1, 1),
                    nash_region::Position::new(1, 1),
                ),
                |n| n.region,
            );
            let mut report = simple(
                "TRAIT PROBLEM",
                region,
                "These superclass declarations form a cycle:",
                "",
            );
            report.after = Doc::stack([
                Doc::cycle(
                    4,
                    first.map_or("trait", |n| n.value),
                    &names.iter().skip(1).map(|n| n.value).collect::<Vec<_>>(),
                ),
                Doc::reflow("Remove a superclass dependency to break the cycle."),
            ]);
            report
        }
        Error::ExportOpenTrait { region, name } | Error::ImportOpenTrait { region, name } => {
            simple(
                if matches!(error, Error::ExportOpenTrait { .. }) {
                    "BAD EXPORT"
                } else {
                    "BAD IMPORT"
                },
                *region,
                &format!("The `{name}` trait cannot be followed by (..) like this:"),
                "The (..) syntax exposes variants of a custom type. Remove the dots and name the trait methods explicitly when you need them.",
            )
        }
        Error::RecordTypeOutsideAlias { region } => simple(
            "RECORD TYPE",
            *region,
            "This record type needs a name:",
            "A record type is only allowed as the direct body of a type alias. Give this record a named alias.",
        ),
        Error::RecordLiteralNoAlias { region, fields } => simple(
            "UNKNOWN RECORD",
            *region,
            &format!(
                "No visible record alias has exactly these fields: {}.",
                fields.join(", ")
            ),
            "Declare or import an alias for this record.",
        ),
        Error::RecordLiteralAmbiguous { region, candidates } => simple(
            "AMBIGUOUS RECORD",
            *region,
            "Several visible record aliases have exactly these fields:",
            &format!(
                "The candidates are {}. Use the intended alias constructor to make the record type clear.",
                candidates
                    .iter()
                    .map(|q| qualified(*q))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        Error::LabeledCtorMissingField {
            region,
            ctor,
            field,
        } => simple(
            "MISSING FIELD",
            *region,
            &format!("The `{ctor}` constructor needs a `{field}` field:"),
            "Add the missing field to this constructor application.",
        ),
        Error::LabeledCtorExtraField {
            region,
            ctor,
            field,
        } => simple(
            "UNKNOWN FIELD",
            *region,
            &format!("The `{ctor}` constructor has no `{field}` field:"),
            "Remove this extra field or check its spelling against the constructor declaration.",
        ),
        Error::LabeledCtorUnknownField {
            region,
            ctor,
            field,
        } => simple(
            "UNKNOWN FIELD",
            *region,
            &format!("The `{ctor}` constructor has no `{field}` field:"),
            "Check the field name against the constructor declaration.",
        ),
        Error::ImplPatternLimit { region } => simple(
            "IMPL PATTERN LIMIT",
            *region,
            "This impl pattern is too large or deeply nested:",
            "Simplify the impl head.",
        ),
        Error::NegateWithoutNum { region } => simple(
            "NAMING ERROR",
            *region,
            "Numeric negation requires `Num`.",
            "Import the module that defines `Num`.",
        ),
        Error::DoWithoutMonad { region } => simple(
            "NAMING ERROR",
            *region,
            "A `do` expression requires `Monad`.",
            "Import the module that defines `Monad`.",
        ),
        Error::RefutableBindPattern { region } => simple(
            "UNSAFE PATTERN",
            *region,
            "This `do` binding has a pattern that can fail to match:",
            "Bind a variable here, then use `case` to handle every variant.",
        ),
        Error::StructuralEqOverride { head } => simple(
            "STRUCTURAL EQUALITY",
            head.region,
            "This impl would replace structural equality:",
            "Equality for this type is supplied by the compiler. Remove the explicit `Eq` impl.",
        ),
        Error::ReflexiveLiftOverlap { heads } => simple(
            "OVERLAPPING IMPL",
            heads.first().map_or(
                Region::new(
                    nash_region::Position::new(1, 1),
                    nash_region::Position::new(1, 1),
                ),
                |h| h.region,
            ),
            "This impl overlaps the reflexive `Lift` rule:",
            "Every Big type can lift to itself. Remove this impl or choose heads that do not overlap that built-in rule.",
        ),
        Error::Unsupported { feature, region } => simple(
            "NOT SUPPORTED",
            *region,
            &format!("Unsupported feature: {feature}."),
            "This syntax is recognized, but its compiler implementation is not available yet.",
        ),
    };
    report.with_code(match error {
        Error::ValidatorMissingMain { .. } => "nash::validator::missing_main",
        Error::ValidatorMainNotExposed { .. } => "nash::validator::main_not_exposed",
        Error::RecordLiteralNoAlias { .. } => "nash::names::record_literal_no_alias",
        Error::RecordLiteralAmbiguous { .. } => "nash::names::record_literal_ambiguous",
        Error::RecordTypeOutsideAlias { .. } => "nash::names::record_type_outside_alias",
        Error::ImplPatternLimit { .. } => "nash::names::impl_pattern_limit",
        Error::NegateWithoutNum { .. } => "nash::names::negate_without_num",
        Error::DoWithoutMonad { .. } => "nash::names::do_without_monad",
        Error::RefutableBindPattern { .. } => "nash::names::refutable_bind_pattern",
        Error::StructuralEqOverride { .. } => "nash::names::structural_eq_override",
        Error::ReflexiveLiftOverlap { .. } => "nash::names::reflexive_lift_overlap",
        Error::MissingSuperclass { .. } => "nash::names::missing_superclass",
        Error::BadInstanceHead { .. } => "nash::names::bad_instance_head",
        Error::ImplContextVarNotInHead { .. } => "nash::names::impl_context_var_not_in_head",
        Error::MissingMethod { .. } => "nash::names::missing_method",
        Error::UnknownMethod { .. } => "nash::names::unknown_method",
        Error::OrphanImpl { .. } => "nash::names::orphan_impl",
        Error::OverlappingImpls { .. } => "nash::names::overlapping_impls",
        Error::ImportOpenTrait { .. } => "nash::names::import_open_trait",
        Error::DuplicateTrait { .. } => "nash::names::duplicate_trait",
        Error::DuplicateMethod { .. } => "nash::names::duplicate_method",
        Error::DuplicateTraitParameter { .. } => "nash::names::duplicate_trait_parameter",
        Error::SuperclassBadArg { .. } => "nash::names::superclass_bad_arg",
        Error::MethodMissingParameter { .. } => "nash::names::method_missing_parameter",
        Error::RecursiveSuperclass { .. } => "nash::names::recursive_superclass",
        Error::ExportOpenTrait { .. } => "nash::names::export_open_trait",
        Error::NotFoundTrait { .. } => "nash::names::not_found_trait",
        Error::AmbiguousTrait { .. } => "nash::names::ambiguous_trait",
        Error::TraitArity { .. } => "nash::names::trait_arity",
        Error::ContextVarNotInType { .. } => "nash::names::context_var_not_in_type",
        Error::KindMismatch { .. } => "nash::names::kind_mismatch",
        Error::KindInfinite { .. } => "nash::names::kind_infinite",
        Error::RepresentationMismatch { .. } => "nash::names::representation_mismatch",
        Error::ContradictoryRepresentation { .. } => "nash::names::contradictory_representation",
        Error::ImplOfBuiltinTrait { .. } => "nash::names::impl_of_builtin_trait",
        Error::IrregularRecursion { .. } => "nash::names::irregular_recursion",
        Error::Unsupported { .. } => "nash::names::unsupported",
        Error::MissingModuleHeader => "nash::names::missing_module_header",
        Error::NotFoundType { .. } => "nash::names::not_found_type",
        Error::ImportNotFound { .. } => "nash::names::import_not_found",
        Error::AmbiguousType { .. } => "nash::names::ambiguous_type",
        Error::BadArity { .. } => "nash::names::bad_arity",
        Error::ExportNotFound { .. } => "nash::names::export_not_found",
        Error::ExportOpenAlias { .. } => "nash::names::export_open_alias",
        Error::DuplicateDecl { .. } => "nash::names::duplicate_decl",
        Error::DuplicateType { .. } => "nash::names::duplicate_type",
        Error::DuplicateCtor { .. } => "nash::names::duplicate_ctor",
        Error::DuplicateBinop { .. } => "nash::names::duplicate_binop",
        Error::BinopFunctionNotFound { .. } => "nash::names::binop_function_not_found",
        Error::DuplicateUnionArg { .. } => "nash::names::duplicate_union_arg",
        Error::DuplicateAliasArg { .. } => "nash::names::duplicate_alias_arg",
        Error::RecursiveAlias { .. } => "nash::names::recursive_alias",
        Error::TypeVarsUnboundInUnion { .. } => "nash::names::type_vars_unbound_in_union",
        Error::TypeVarsMessedUpInAlias { .. } => "nash::names::type_vars_messed_up_in_alias",
        Error::LabeledCtorMissingField { .. } => "nash::names::labeled_ctor_missing_field",
        Error::LabeledCtorExtraField { .. } => "nash::names::labeled_ctor_extra_field",
        Error::LabeledCtorUnknownField { .. } => "nash::names::labeled_ctor_unknown_field",
        Error::DuplicateField { .. } => "nash::names::duplicate_field",
        Error::ExportDuplicate { .. } => "nash::names::export_duplicate",
        Error::NotFoundCtor { .. } => "nash::names::not_found_ctor",
        Error::AmbiguousCtor { .. } => "nash::names::ambiguous_ctor",
        Error::PatternHasRecordCtor { .. } => "nash::names::pattern_has_record_ctor",
        Error::DuplicatePattern { .. } => "nash::names::duplicate_pattern",
        Error::NotFoundVar { .. } => "nash::names::not_found_var",
        Error::AmbiguousVar { .. } => "nash::names::ambiguous_var",
        Error::NotFoundBinop { .. } => "nash::names::not_found_binop",
        Error::AmbiguousBinop { .. } => "nash::names::ambiguous_binop",
        Error::BinopConflict { .. } => "nash::names::binop_conflict",
        Error::Shadowing { .. } => "nash::names::shadowing",
        Error::RecursiveLet { .. } => "nash::names::recursive_let",
        Error::RecursiveDecl { .. } => "nash::names::recursive_decl",
        Error::AnnotationTooShort { .. } => "nash::names::annotation_too_short",
        Error::ImportExposingNotFound { .. } => "nash::names::import_exposing_not_found",
        Error::ImportCtorByName { .. } => "nash::names::import_ctor_by_name",
        Error::ImportOpenAlias { .. } => "nash::names::import_open_alias",
    })
}

fn simple(title: &str, region: Region, before: &str, after: &str) -> Report {
    Report::snippet(title, region, None, Doc::reflow(before), Doc::reflow(after))
}
fn label(region: Region, text: &str) -> Label {
    Label {
        region,
        text: text.into(),
    }
}
fn name_clash(first: Region, second: Region, message: &str) -> Report {
    Report::pair(
        "NAME CLASH",
        label(first, "first definition"),
        label(second, "and another first definition"),
        Doc::reflow(message),
        Doc::text("Rename one of the definitions."),
    )
}
fn qualified(name: QualifiedName<'_>) -> String {
    to_qual_string(name.home.name, name.name)
}
fn to_qual_string(prefix: &str, name: &str) -> String {
    format!("{prefix}.{name}")
}
fn nearby(name: &str, possible: &[&str], limit: usize) -> Vec<String> {
    suggest::sort(
        name,
        Clone::clone,
        possible.iter().map(|s| s.to_string()).collect(),
    )
    .into_iter()
    .take(limit)
    .collect()
}
fn suggestion_details(nearby: &[String], empty: &str) -> Doc {
    match nearby {
        [] => Doc::reflow(empty),
        [one] => Doc::hsep([
            Doc::text("Try"),
            Doc::text(one).dullyellow(),
            Doc::text("instead."),
        ]),
        _ => Doc::stack([
            Doc::text("Similar names:"),
            Doc::indent(
                4,
                Doc::vcat(nearby.iter().map(|n| Doc::text(n).dullyellow())),
            ),
        ]),
    }
}
fn to_kind_info(kind: VarKind, name: &str) -> (&'static str, &'static str, String) {
    match kind {
        VarKind::BadOp => ("an", "operator", format!("({name})")),
        VarKind::BadVar => ("a", "value", format!("`{name}`")),
        VarKind::BadPattern => ("a", "pattern", format!("`{name}`")),
        VarKind::BadType => ("a", "type", format!("`{name}`")),
    }
}
fn not_found(
    region: Region,
    prefix: Option<&str>,
    name: &str,
    thing: &str,
    possible: PossibleNames<'_>,
) -> Report {
    let given = prefix.map_or_else(|| name.into(), |p| to_qual_string(p, name));
    let mut names: Vec<String> = possible.locals.iter().map(|s| s.to_string()).collect();
    for (module, values) in possible.qualified {
        names.extend(values.iter().map(|n| to_qual_string(module, n)));
    }
    let nearby: Vec<_> = suggest::sort(&given, Clone::clone, names)
        .into_iter()
        .take(4)
        .collect();
    let hint = match prefix {
        Some(p) if !possible.qualified.iter().any(|(m, _)| *m == p) => {
            format!("Import `{p}` or check its alias.")
        }
        Some(p) => format!("Check that `{p}` exposes `{name}`."),
        None => "Define or import this name.".into(),
    };
    Report::snippet(
        "NAMING ERROR",
        region,
        None,
        Doc::text(format!("Unknown {thing} `{given}`.")),
        suggestion_details(&nearby, &hint),
    )
    .with_suggestions(nearby)
}
fn ambiguous_name(
    region: Region,
    prefix: Option<&str>,
    name: &str,
    first: ModuleName<'_>,
    others: &[ModuleName<'_>],
    thing: &str,
) -> Report {
    let mut homes = vec![first];
    homes.extend_from_slice(others);
    homes.sort();
    let given = prefix.map_or_else(|| name.to_string(), |p| to_qual_string(p, name));
    Report::snippet(
        "AMBIGUOUS NAME",
        region,
        None,
        Doc::text(format!("Ambiguous {thing} `{given}`.")),
        Doc::stack([
            Doc::indent(
                4,
                Doc::vcat(
                    homes
                        .iter()
                        .map(|h| Doc::text(to_qual_string(h.name, name))),
                ),
            ),
            Doc::text(if prefix.is_some() {
                "Give these imports distinct aliases."
            } else {
                "Use a qualified name."
            }),
        ]),
    )
}

fn args(n: usize) -> String {
    format!("{n} argument{}", if n == 1 { "" } else { "s" })
}
fn arity(region: Region, name: &str, thing: &str, expected: usize, actual: usize) -> Report {
    simple(
        if actual < expected {
            "TOO FEW ARGS"
        } else if thing == "type" {
            "TOO MANY TYPE ARGS"
        } else {
            "TOO MANY ARGS"
        },
        region,
        &format!(
            "The `{name}` {thing} needs {}, but I see {actual} instead:",
            args(expected)
        ),
        if actual < expected {
            "Supply the missing arguments."
        } else {
            "Remove the extra arguments or check the grouping."
        },
    )
}
fn not_found_binop(region: Region, name: &str, available: &[&str]) -> Report {
    let suggestions = match name {
        "===" => vec!["==".into()],
        "!=" | "!==" => vec!["/=".into()],
        "**" => vec!["^".into(), "*".into()],
        "%" => vec![],
        _ => nearby(name, available, 2),
    };
    let mut report = simple(
        "UNKNOWN OPERATOR",
        region,
        &format!("Unknown operator `({name})`."),
        "",
    );
    report.after = suggestion_details(
        &suggestions,
        if name == "%" {
            "Use an integer remainder or modulus function."
        } else {
            "Import and expose the operator."
        },
    );
    report.with_suggestions(suggestions)
}
fn recursive_value(region: Region, name: &str, others: &[&str], is_let: bool) -> Report {
    Report::snippet(
        if is_let {
            "CYCLIC VALUE"
        } else {
            "CYCLIC DEFINITION"
        },
        region,
        None,
        Doc::text(format!("Value `{name}` depends on itself.")),
        Doc::stack([
            if others.is_empty() {
                Doc::Empty
            } else {
                Doc::cycle(4, name, others)
            },
            Doc::text("Break the cycle between these value definitions."),
        ]),
    )
}
fn alias_recursion_report(
    region: Region,
    name: &str,
    args: &[&str],
    typ: &nash_region::Located<nash_source::Type<'_>>,
    others: &[&str],
) -> Report {
    Report::snippet(
        "ALIAS PROBLEM",
        region,
        None,
        Doc::text(format!("Type alias `{name}` expands recursively.")),
        Doc::stack(if others.is_empty() {
            vec![
                Doc::text("Use a custom type:"),
                Doc::indent(4, alias_to_union_doc(name, args, typ)),
            ]
        } else {
            vec![
                Doc::cycle(4, name, others),
                Doc::text("Convert at least one alias in this cycle to a custom type."),
            ]
        }),
    )
}
fn alias_to_union_doc(
    name: &str,
    args: &[&str],
    typ: &nash_region::Located<nash_source::Type<'_>>,
) -> Doc {
    Doc::vcat([
        Doc::hsep(
            [Doc::text("type"), Doc::text(name)]
                .into_iter()
                .chain(args.iter().map(|a| Doc::text(format!("'{a}"))))
                .chain([Doc::text("=")]),
        )
        .dullyellow(),
        Doc::indent(4, Doc::text(name)).green(),
        Doc::indent(
            8,
            crate::render_type::src_to_doc(crate::render_type::Ctx::App, typ),
        )
        .dullyellow(),
    ])
}
fn unbound_type_vars(
    region: Region,
    decl: &str,
    name: &str,
    args: &[&str],
    first: (&str, Region),
    others: &[(&str, Region)],
) -> Report {
    let names: Vec<_> = std::iter::once(first.0)
        .chain(others.iter().map(|(n, _)| *n))
        .collect();
    let before = if others.is_empty() {
        format!(
            "The `{name}` {decl} uses an unbound type variable `{}` in its definition:",
            first.0
        )
    } else {
        format!(
            "Type variables {} are unbound in the `{name}` {decl} definition:",
            names.join(" and ")
        )
    };
    Report::snippet(
        if others.is_empty() {
            "UNBOUND TYPE VARIABLE"
        } else {
            "UNBOUND TYPE VARIABLES"
        },
        region,
        others.is_empty().then_some(first.1),
        Doc::reflow(&before),
        Doc::stack([
            Doc::text("Declare the type variables:"),
            declaration(decl, name, args, &names),
        ]),
    )
}
fn declaration(decl: &str, name: &str, args: &[&str], added: &[&str]) -> Doc {
    Doc::indent(
        4,
        Doc::hsep(
            [Doc::text(decl), Doc::text(name)]
                .into_iter()
                .chain(args.iter().map(|a| Doc::text(format!("'{a}"))))
                .chain(added.iter().map(|a| Doc::text(format!("'{a}")).green()))
                .chain([Doc::text("= ...")]),
        ),
    )
}
fn alias_vars(
    region: Region,
    name: &str,
    args: &[&str],
    unused: &[(&str, Region)],
    unbound: &[(&str, Region)],
) -> Report {
    if unused.is_empty()
        && let Some((first, rest)) = unbound.split_first()
    {
        return unbound_type_vars(region, "type alias", name, args, *first, rest);
    }
    let kept: Vec<_> = args
        .iter()
        .copied()
        .filter(|a| !unused.iter().any(|(u, _)| u == a))
        .collect();
    let unused_names = unused.iter().map(|(n, _)| *n).collect::<Vec<_>>();
    if unbound.is_empty() {
        Report::snippet(
            if unused.len() == 1 {
                "UNUSED TYPE VARIABLE"
            } else {
                "UNUSED TYPE VARIABLES"
            },
            region,
            if unused.len() == 1 {
                Some(unused[0].1)
            } else {
                None
            },
            Doc::reflow(&if unused.len() == 1 {
                format!(
                    "Type alias `{name}` does not use the `{}` type variable.",
                    unused[0].0
                )
            } else {
                format!(
                    "Type variables {} are unused in the `{name}` definition.",
                    unused_names.join(" and ")
                )
            }),
            Doc::stack([
                Doc::reflow(&format!(
                    "Remove {} from the declaration:",
                    unused_names.join(" and ")
                )),
                declaration("type alias", name, &kept, &[]),
            ]),
        )
    } else {
        let unbound_names = unbound.iter().map(|(n, _)| *n).collect::<Vec<_>>();
        Report::snippet(
            "TYPE VARIABLE PROBLEMS",
            region,
            None,
            Doc::reflow(&format!(
                "Type alias `{name}` has some type variable problems."
            )),
            Doc::stack([
                Doc::reflow(&format!(
                    "{} {}",
                    if let [one] = unbound_names.as_slice() {
                        format!(
                            "Type variable `{one}` appears in the definition, but I do not see it declared."
                        )
                    } else {
                        format!(
                            "Type variables {} are used in the definition, but I do not see them declared.",
                            unbound_names.join(" and ")
                        )
                    },
                    if let [one] = unused_names.as_slice() {
                        format!("Likewise, type variable `{one}` is declared, but not used.")
                    } else {
                        format!(
                            "Likewise, type variables {} are declared, but not used.",
                            unused_names.join(" and ")
                        )
                    }
                )),
                Doc::reflow("Match the declaration to the variables used:"),
                declaration("type alias", name, &kept, &unbound_names),
            ]),
        )
    }
}
fn kind(value: &Kind<'_>) -> String {
    match value {
        Kind::Type => "Type".into(),
        Kind::Arrow(a, b) => format!(
            "{} -> {}",
            if matches!(a, Kind::Arrow(..)) {
                format!("({})", kind(a))
            } else {
                kind(a)
            },
            kind(b)
        ),
    }
}
fn kind_context(value: &KindContext<'_>) -> String {
    match value {
        KindContext::TypeAnnotation => "the type annotation".into(),
        KindContext::Annotation { name } => format!("the annotation for `{name}`"),
        KindContext::BigField { union, ctor, index } => format!(
            "field {} of Big constructor `{ctor}` in `{union}`",
            usize::from(*index) + 1
        ),
        KindContext::LittleField { union, ctor, index } => format!(
            "field {} of little constructor `{ctor}` in `{union}`",
            usize::from(*index) + 1
        ),
        KindContext::RecordField { alias, field, big } => format!(
            "field `{field}` of {} record alias `{alias}`",
            if *big { "Big" } else { "little" }
        ),
        KindContext::AliasCasing { alias, big } => format!(
            "the {}case name of alias `{alias}`",
            if *big { "upper" } else { "lower" }
        ),
        KindContext::ImplHead { trait_, index } => format!(
            "head {} of impl `{}`",
            usize::from(*index) + 1,
            qualified(*trait_)
        ),
    }
}
fn repr(value: nash_ast::primitives::Repr) -> &'static str {
    use nash_ast::primitives::Repr;
    match value {
        Repr::Big => "Big",
        Repr::Const => "Const",
        Repr::Term => "Term",
    }
}
fn admitted(value: nash_ast::primitives::ReprTrait) -> &'static str {
    use nash_ast::primitives::ReprTrait;
    match value {
        ReprTrait::Big => "only Big types",
        ReprTrait::Const => "only Const types",
        ReprTrait::Term => "only Term types",
        ReprTrait::Storable => "Big or Const types",
        ReprTrait::Little => "Const or Term types",
    }
}
fn head_con(value: &nash_ast::HeadCon<'_>) -> String {
    match value {
        nash_ast::HeadCon::Named(n) => qualified(*n),
        nash_ast::HeadCon::Tuple(n) => format!("a {n}-item tuple"),
        nash_ast::HeadCon::Fun => "a function".into(),
    }
}
fn head(value: &nash_ast::Head<'_>) -> String {
    use nash_ast::Head;
    match value {
        Head::Var(n) => format!("'a{n}"),
        Head::Named { reference, args } => {
            if args.is_empty() {
                qualified(*reference)
            } else {
                format!(
                    "({} {})",
                    qualified(*reference),
                    args.iter().map(head).collect::<Vec<_>>().join(" ")
                )
            }
        }
        Head::Tuple(items) => format!(
            "({})",
            items.iter().map(head).collect::<Vec<_>>().join(", ")
        ),
        Head::Function(a, b) => format!("({} -> {})", head(a), head(b)),
    }
}

fn predicate(value: &nash_ast::Pred<'_>) -> String {
    let localizer = crate::localizer::Localizer::default();
    let render = |t: &nash_region::Located<nash_ast::Type<'_>>| {
        crate::render_type::can_to_doc(&localizer, crate::render_type::Ctx::App, &t.value)
            .render(80, false)
    };
    let (name, args) = match value {
        nash_ast::Pred::Trait { trait_, args } | nash_ast::Pred::Implied { trait_, args } => {
            (qualified(*trait_), *args)
        }
        nash_ast::Pred::Apply { head, args } => (format!("formation of {}", render(head)), *args),
    };
    std::iter::once(name)
        .chain(args.iter().map(|t| render(t)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod coverage {
    use super::*;
    use nash_region::Located;
    fn home() -> ModuleName<'static> {
        ModuleName {
            package: None,
            name: "First",
        }
    }
    fn other() -> ModuleName<'static> {
        ModuleName {
            package: None,
            name: "Second",
        }
    }
    fn q() -> QualifiedName<'static> {
        QualifiedName {
            home: home(),
            name: "Equal",
        }
    }
    #[test]
    fn record_literal_no_alias() {
        source_snapshot!(
            "variant_record_literal_no_alias",
            "value = «{ x = 1, y = 2 }»",
            r,
            r2,
            Error::RecordLiteralNoAlias {
                region: r(),
                fields: &["x", "y"],
            },
        );
    }
    #[test]
    fn record_literal_ambiguous() {
        source_snapshot!(
            "variant_record_literal_ambiguous",
            "import First exposing (Equal)\nimport Second exposing (Other)\nvalue = «{ x = 1, y = 2 }»",
            r,
            r2,
            Error::RecordLiteralAmbiguous {
                region: r(),
                candidates: &[
                    q(),
                    QualifiedName {
                        home: other(),
                        name: "Other",
                    },
                ],
            },
        );
    }
    #[test]
    fn record_type_outside_alias() {
        source_snapshot!(
            "variant_record_type_outside_alias",
            "value : «{ x : Int }»\nvalue = { x = 1 }",
            r,
            r2,
            Error::RecordTypeOutsideAlias { region: r() },
        );
    }
    #[test]
    fn impl_pattern_limit() {
        source_snapshot!(
            "variant_impl_pattern_limit",
            "-- Reduced head from an impl-pattern budget exhaustion.\nimpl Equal «(List (List 'a))» where\n    equal x y = True",
            r,
            r2,
            Error::ImplPatternLimit { region: r() },
        );
    }
    #[test]
    fn negate_without_num() {
        source_snapshot!(
            "variant_negate_without_num",
            "value = «-1»",
            r,
            r2,
            Error::NegateWithoutNum { region: r() },
        );
    }
    #[test]
    fn do_without_monad() {
        source_snapshot!(
            "variant_do_without_monad",
            "value = «do\n    pure ()»",
            r,
            r2,
            Error::DoWithoutMonad { region: r() },
        );
    }
    #[test]
    fn refutable_bind_pattern() {
        source_snapshot!(
            "variant_refutable_bind_pattern",
            "value = do\n    «Just x» <- action\n    pure x",
            r,
            r2,
            Error::RefutableBindPattern { region: r() },
        );
    }
    #[test]
    fn structural_eq_override() {
        source_snapshot!(
            "variant_structural_eq_override",
            "impl (Big 'a) => Eq «'a» where\n    eq x y = True",
            r,
            r2,
            Error::StructuralEqOverride {
                head: &Located::at(r(), nash_ast::Type::Var("a")),
            },
        );
    }
    #[test]
    fn reflexive_lift_overlap() {
        source_snapshot!(
            "variant_reflexive_lift_overlap",
            "impl (Big 'a) => Lift «'a 'a» where\n    lift x = x",
            r,
            r2,
            Error::ReflexiveLiftOverlap {
                heads: &[&Located::at(r(), nash_ast::Type::Var("a"))],
            },
        );
    }
    #[test]
    fn missing_superclass() {
        source_snapshot!(
            "variant_missing_superclass",
            "impl «First.Equal 'a» where\n    equal x y = True",
            r,
            r2,
            Error::MissingSuperclass {
                region: r(),
                trait_: q(),
                heads: &[Located::at(r(), nash_ast::Head::Var(0))],
                superclass: &nash_ast::Pred::Trait {
                    trait_: q(),
                    args: &[],
                },
                index: 0,
                reason: nash_can::EntailmentFailure::Missing,
            },
        );
    }
    #[test]
    fn bad_instance_head() {
        source_snapshot!(
            "variant_bad_instance_head",
            "impl Equal «'a» where\n    equal x y = True",
            r,
            r2,
            Error::BadInstanceHead {
                region: r(),
                reason: nash_can::BadHead::BareVariable,
            },
        );
    }
    #[test]
    fn impl_context_var_not_in_head() {
        source_snapshot!(
            "variant_impl_context_var_not_in_head",
            "impl (Equal «'name») => Equal (List 'a) where\n    equal x y = True",
            r,
            r2,
            Error::ImplContextVarNotInHead {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn missing_method() {
        source_snapshot!(
            "variant_missing_method",
            "trait Equal 'a where\n    name : 'a -> Bool\n«impl Equal () where»\n    equal x = True",
            r,
            r2,
            Error::MissingMethod {
                region: r(),
                trait_: "Equal",
                name: "name",
            },
        );
    }
    #[test]
    fn unknown_method() {
        source_snapshot!(
            "variant_unknown_method",
            "trait Equal 'a where\n    equal : 'a -> Bool\nimpl Equal () where\n    «name» x = True",
            r,
            r2,
            Error::UnknownMethod {
                region: r(),
                trait_: "Equal",
                name: "name",
            },
        );
    }
    #[test]
    fn orphan_impl() {
        source_snapshot!(
            "variant_orphan_impl",
            "import First exposing (Equal)\n«impl First.Equal First.Equal where»\n    equal x y = True",
            r,
            r2,
            Error::OrphanImpl {
                region: r(),
                trait_: q(),
                heads: &[nash_ast::HeadCon::Named(q())],
            },
        );
    }
    #[test]
    fn overlapping_impls() {
        source_snapshot!(
            "variant_overlapping_impls",
            "«impl First.Equal 'a where»\n    equal x y = True\n«impl First.Equal 'a where»\n    equal x y = False",
            r,
            r2,
            Error::OverlappingImpls {
                key: &nash_ast::ImplKey {
                    trait_: q(),
                    heads: &[nash_ast::Head::Var(0)],
                },
                first: r(),
                second: r2(),
                first_home: home(),
                second_home: other(),
            },
        );
    }
    #[test]
    fn import_open_trait() {
        source_snapshot!(
            "variant_import_open_trait",
            "import First exposing («Name(..)»)",
            r,
            r2,
            Error::ImportOpenTrait {
                region: r(),
                name: "Name",
            },
        );
    }
    #[test]
    fn duplicate_trait() {
        source_snapshot!(@pipeline
            "variant_duplicate_trait",
            "trait «Name» 'a where\n    first : 'a -> 'a\ntrait «Name» 'a where\n    second : 'a -> 'a", r, r2,
            Error::DuplicateTrait {
                name: "Name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_method() {
        source_snapshot!(@pipeline
            "variant_duplicate_method",
            "trait Equal 'a where\n    «name» : 'a -> 'a\n    name x = x\n    «name» : 'a -> Bool\n    name x = True", r, r2,
            Error::DuplicateMethod {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_trait_parameter() {
        source_snapshot!(@pipeline
            "variant_duplicate_trait_parameter",
            "trait Equal '«name» '«name» where\n    equal : 'name -> Bool", r, r2,
            Error::DuplicateTraitParameter {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn superclass_bad_arg() {
        source_snapshot!(
            "variant_superclass_bad_arg",
            "trait (Equal «(List 'a)») => Ordered 'a where\n    compare : 'a -> 'a -> Int",
            r,
            r2,
            Error::SuperclassBadArg {
                region: r(),
                trait_: "Equal",
            },
        );
    }
    #[test]
    fn method_missing_parameter() {
        source_snapshot!(
            "variant_method_missing_parameter",
            "trait Equal 'a where\n    «a : Int -> Bool»",
            r,
            r2,
            Error::MethodMissingParameter {
                region: r(),
                method: "a",
                parameter: "a",
            },
        );
    }
    #[test]
    fn recursive_superclass() {
        source_snapshot!(
            "variant_recursive_superclass",
            "trait (Second 'a) => «First» 'a where\n    first : 'a -> 'a\ntrait (First 'a) => «Second» 'a where\n    second : 'a -> 'a",
            r,
            r2,
            Error::RecursiveSuperclass {
                names: &[&Located::at(r(), "First"), &Located::at(r2(), "Second")],
            },
        );
    }
    #[test]
    fn export_open_trait() {
        source_snapshot!(
            "variant_export_open_trait",
            "module Main exposing («Name(..)»)\ntrait Name 'a where\n    name : 'a -> 'a",
            r,
            r2,
            Error::ExportOpenTrait {
                region: r(),
                name: "Name",
            },
        );
    }
    #[test]
    fn not_found_trait() {
        source_snapshot!(@pipeline
            "variant_not_found_trait",
            "value : («Name» 'a) => 'a -> 'a\nvalue x = x", r, r2,
            Error::NotFoundTrait {
                region: r(),
                prefix: None,
                name: "Name",
            },
        );
    }
    #[test]
    fn ambiguous_trait() {
        source_snapshot!(
            "variant_ambiguous_trait",
            "import First exposing (Name)\nimport Second exposing (Name)\nvalue : («Name» 'a) => 'a -> 'a\nvalue x = x",
            r,
            r2,
            Error::AmbiguousTrait {
                region: r(),
                prefix: None,
                name: "Name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn trait_arity() {
        source_snapshot!(
            "variant_trait_arity",
            "value : («Name 'a 'b») => 'a -> 'b\nvalue x = x",
            r,
            r2,
            Error::TraitArity {
                region: r(),
                name: "Name",
                expected: 1,
                actual: 2,
            },
        );
    }
    #[test]
    fn context_var_not_in_type() {
        source_snapshot!(
            "variant_context_var_not_in_type",
            "value : (Equal '«name») => Int\nvalue = 1",
            r,
            r2,
            Error::ContextVarNotInType {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn kind_mismatch() {
        source_snapshot!(
            "variant_kind_mismatch",
            "value : «List»\nvalue = []",
            r,
            r2,
            Error::KindMismatch {
                region: r(),
                context: &KindContext::TypeAnnotation,
                expected: &Kind::Type,
                actual: &Kind::Arrow(&Kind::Type, &Kind::Type),
            },
        );
    }
    #[test]
    fn kind_infinite() {
        source_snapshot!(@pipeline
            "variant_kind_infinite",
            "name : «'a 'a»\nname = 1", r, r2,
            Error::KindInfinite {
                region: r(),
                context: &KindContext::Annotation { name: "name" },
            },
        );
    }
    #[test]
    fn representation_mismatch() {
        source_snapshot!(
            "variant_representation_mismatch",
            "value : «(Int -> Int : Storable)»\nvalue x = x",
            r,
            r2,
            Error::RepresentationMismatch {
                region: r(),
                context: &KindContext::TypeAnnotation,
                required: nash_ast::primitives::ReprTrait::Storable,
                actual: nash_ast::primitives::Repr::Term,
            },
        );
    }
    #[test]
    fn contradictory_representation() {
        source_snapshot!(
            "variant_contradictory_representation",
            "value : (Storable '«a», Term 'a) => 'a -> 'a\nvalue x = x",
            r,
            r2,
            Error::ContradictoryRepresentation {
                region: r(),
                variable: "a",
            },
        );
    }
    #[test]
    fn impl_of_builtin_trait() {
        source_snapshot!(
            "variant_impl_of_builtin_trait",
            "«impl Storable Int where»",
            r,
            r2,
            Error::ImplOfBuiltinTrait {
                region: r(),
                trait_: nash_ast::primitives::ReprTrait::Storable.qualified(),
            },
        );
    }
    #[test]
    fn irregular_recursion() {
        source_snapshot!(
            "variant_irregular_recursion",
            "type Equal 'a = More «(Equal (List 'a))»",
            r,
            r2,
            Error::IrregularRecursion {
                region: r(),
                constructor: q(),
                parameter: "a",
            },
        );
    }
    #[test]
    fn unsupported() {
        source_snapshot!(
            "variant_unsupported",
            "value = «[1, 2]»",
            r,
            r2,
            Error::Unsupported {
                feature: "list literals",
                region: r(),
            },
        );
    }
    #[test]
    fn missing_module_header() {
        source_snapshot!(
            "variant_missing_module_header",
            "value = 1",
            r,
            r2,
            Error::MissingModuleHeader
        );
    }
    #[test]
    fn not_found_type() {
        source_snapshot!(@pipeline
            "variant_not_found_type",
            "value : «Name»\nvalue = 1", r, r2,
            Error::NotFoundType {
                region: r(),
                prefix: None,
                name: "Name",
                suggestions: PossibleNames {
                    locals: &["Named"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn import_not_found() {
        source_snapshot!(
            "variant_import_not_found",
            "import «Missing»",
            r,
            r2,
            Error::ImportNotFound {
                region: r(),
                module: "Missing",
            },
        );
    }
    #[test]
    fn ambiguous_type() {
        source_snapshot!(
            "variant_ambiguous_type",
            "import First exposing (Name)\nimport Second exposing (Name)\nvalue : «Name»\nvalue = 1",
            r,
            r2,
            Error::AmbiguousType {
                region: r(),
                prefix: None,
                name: "Name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn bad_arity() {
        source_snapshot!(
            "variant_bad_arity",
            "type alias Box 'a = List 'a\nvalue : «Box Int Int»\nvalue = []",
            r,
            r2,
            Error::BadArity {
                region: r(),
                context: BadArityContext::TypeArity,
                name: "Box",
                expected: 1,
                actual: 2,
            },
        );
    }
    #[test]
    fn export_not_found() {
        source_snapshot!(
            "variant_export_not_found",
            "module Main exposing («naem»)\nname = 1",
            r,
            r2,
            Error::ExportNotFound {
                region: r(),
                kind: VarKind::BadVar,
                name: "naem",
                suggestions: &["name"],
            },
        );
    }
    #[test]
    fn export_open_alias() {
        source_snapshot!(
            "variant_export_open_alias",
            "module Main exposing («Name(..)»)\ntype alias Name = Int",
            r,
            r2,
            Error::ExportOpenAlias {
                region: r(),
                name: "Name",
            },
        );
    }
    #[test]
    fn duplicate_decl() {
        source_snapshot!(@pipeline
            "variant_duplicate_decl",
            "«name» = 1\n«name» = 2", r, r2,
            Error::DuplicateDecl {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_type() {
        source_snapshot!(@pipeline
            "variant_duplicate_type",
            "type «Name» = One\ntype «Name» = Two", r, r2,
            Error::DuplicateType {
                name: "Name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_ctor() {
        source_snapshot!(@pipeline
            "variant_duplicate_ctor",
            "type First = «Name»\ntype Second = «Name»", r, r2,
            Error::DuplicateCtor {
                name: "Name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_binop() {
        source_snapshot!(
            "variant_duplicate_binop",
            "infix left 5 («++») = append\ninfix right 5 («++») = append",
            r,
            r2,
            Error::DuplicateBinop {
                name: "++",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn binop_function_not_found() {
        source_snapshot!(
            "variant_binop_function_not_found",
            "infix left 5 (+) = «add»",
            r,
            r2,
            Error::BinopFunctionNotFound {
                region: r(),
                op: "+",
                function: "add",
            },
        );
    }
    #[test]
    fn duplicate_union_arg() {
        source_snapshot!(@pipeline
            "variant_duplicate_union_arg",
            "type Box '«a» '«a» = Box 'a", r, r2,
            Error::DuplicateUnionArg {
                type_name: "Box",
                arg_name: "a",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_alias_arg() {
        source_snapshot!(@pipeline
            "variant_duplicate_alias_arg",
            "type alias Box '«a» '«a» = List 'a", r, r2,
            Error::DuplicateAliasArg {
                type_name: "Box",
                arg_name: "a",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn recursive_alias() {
        source_snapshot!(
            "variant_recursive_alias",
            "type alias «Loop» 'a = Loop 'a",
            r,
            r2,
            Error::RecursiveAlias {
                region: r(),
                name: "Loop",
                args: &["a"],
                typ: &Located::at(r(), nash_source::Type::Var("a")),
                others: &[],
            },
        );
    }
    #[test]
    fn type_vars_unbound_in_union() {
        source_snapshot!(
            "variant_type_vars_unbound_in_union",
            "type Box = Box '«a»",
            r,
            r2,
            Error::TypeVarsUnboundInUnion {
                region: r(),
                name: "Box",
                args: &[],
                unbound: ("a", r()),
                more_unbound: &[],
            },
        );
    }
    #[test]
    fn type_vars_messed_up_in_alias() {
        source_snapshot!(
            "variant_type_vars_messed_up_in_alias",
            "type alias Box '«a» = List '«b»",
            r,
            r2,
            Error::TypeVarsMessedUpInAlias {
                region: r(),
                name: "Box",
                args: &["a"],
                unused: &[("a", r())],
                unbound: &[("b", r2())],
            },
        );
    }
    #[test]
    fn labeled_ctor_missing_field() {
        source_snapshot!(
            "variant_labeled_ctor_missing_field",
            "type Point = Point { x : Int, y : Int }\nvalue = «Point { x = 1 }»",
            r,
            r2,
            Error::LabeledCtorMissingField {
                region: r(),
                ctor: "Point",
                field: "y",
            },
        );
    }
    #[test]
    fn labeled_ctor_extra_field() {
        source_snapshot!(
            "variant_labeled_ctor_extra_field",
            "type Point = Point Int\nvalue = Point { «x = 1» }",
            r,
            r2,
            Error::LabeledCtorExtraField {
                region: r(),
                ctor: "Point",
                field: "x",
            },
        );
    }
    #[test]
    fn labeled_ctor_unknown_field() {
        source_snapshot!(
            "variant_labeled_ctor_unknown_field",
            "type Point = Point { x : Int }\nvalue = Point { «y = 1» }",
            r,
            r2,
            Error::LabeledCtorUnknownField {
                region: r(),
                ctor: "Point",
                field: "y",
            },
        );
    }
    #[test]
    fn duplicate_field() {
        source_snapshot!(
            "variant_duplicate_field",
            "value = { «name» = 1, «name» = 2 }",
            r,
            r2,
            Error::DuplicateField {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn export_duplicate() {
        source_snapshot!(
            "variant_export_duplicate",
            "module Main exposing («name», «name»)\nname = 1",
            r,
            r2,
            Error::ExportDuplicate {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn not_found_ctor() {
        source_snapshot!(@pipeline
            "variant_not_found_ctor",
            "value = «Name»", r, r2,
            Error::NotFoundCtor {
                region: r(),
                prefix: None,
                name: "Name",
                suggestions: PossibleNames {
                    locals: &["Named"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn ambiguous_ctor() {
        source_snapshot!(
            "variant_ambiguous_ctor",
            "import First exposing (First(..))\nimport Second exposing (Second(..))\nvalue = «Name»",
            r,
            r2,
            Error::AmbiguousCtor {
                region: r(),
                prefix: None,
                name: "Name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn pattern_has_record_ctor() {
        source_snapshot!(
            "variant_pattern_has_record_ctor",
            "type alias Name = { x : Int }\nvalue («Name» x) = x",
            r,
            r2,
            Error::PatternHasRecordCtor {
                region: r(),
                name: "Name",
            },
        );
    }
    #[test]
    fn duplicate_pattern() {
        source_snapshot!(
            "variant_duplicate_pattern",
            "value pair =\n    case pair of\n        («x», «x») -> x",
            r,
            r2,
            Error::DuplicatePattern {
                context: DuplicatePatternContext::CaseBranch,
                name: "x",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn not_found_var() {
        source_snapshot!(
            "variant_not_found_var",
            "result = «name»",
            r,
            r2,
            Error::NotFoundVar {
                region: r(),
                prefix: None,
                name: "name",
                suggestions: PossibleNames {
                    locals: &["named"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn ambiguous_var() {
        source_snapshot!(
            "variant_ambiguous_var",
            "import First exposing (name)\nimport Second exposing (name)\nvalue = «name»",
            r,
            r2,
            Error::AmbiguousVar {
                region: r(),
                prefix: None,
                name: "name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn not_found_binop() {
        source_snapshot!(
            "variant_not_found_binop",
            "value = 1 «<+>» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "<+>",
                available: &["+"],
            },
        );
    }
    #[test]
    fn ambiguous_binop() {
        source_snapshot!(
            "variant_ambiguous_binop",
            "import First exposing ((++))\nimport Second exposing ((++))\nvalue = [] «++» []",
            r,
            r2,
            Error::AmbiguousBinop {
                region: r(),
                name: "++",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn binop_conflict() {
        source_snapshot!(
            "variant_binop_conflict",
            "value = «1 < 2 > 3»",
            r,
            r2,
            Error::BinopConflict {
                region: r(),
                op1: "<",
                op2: ">",
            },
        );
    }
    #[test]
    fn shadowing() {
        source_snapshot!(
            "variant_shadowing",
            "«name» = 1\nvalue «name» = name",
            r,
            r2,
            Error::Shadowing {
                name: "name",
                original: r(),
                new: r2(),
            },
        );
    }
    #[test]
    fn recursive_let() {
        source_snapshot!(@pipeline
            "variant_recursive_let",
            "value =\n    let\n        «name» = other\n        other = name\n    in\n    name", r, r2,
            Error::RecursiveLet {
                name: &Located::at(r(), "name"),
                others: &["other"],
            },
        );
    }
    #[test]
    fn recursive_decl() {
        source_snapshot!(@pipeline
            "variant_recursive_decl",
            "«name» = name", r, r2,
            Error::RecursiveDecl {
                name: &Located::at(r(), "name"),
                others: &[],
            },
        );
    }
    #[test]
    fn annotation_too_short() {
        source_snapshot!(
            "variant_annotation_too_short",
            "name : Int -> Int\n«name x y z» = x",
            r,
            r2,
            Error::AnnotationTooShort {
                region: r(),
                name: "name",
                index: 1,
                leftovers: 2,
            },
        );
    }
    #[test]
    fn import_exposing_not_found() {
        source_snapshot!(
            "variant_import_exposing_not_found",
            "import First exposing («name»)",
            r,
            r2,
            Error::ImportExposingNotFound {
                region: r(),
                module: home(),
                name: "name",
                available: &["other"],
            },
        );
    }
    #[test]
    fn import_ctor_by_name() {
        source_snapshot!(
            "variant_import_ctor_by_name",
            "import First exposing («Just»)",
            r,
            r2,
            Error::ImportCtorByName {
                region: r(),
                name: "Just",
                type_name: "Maybe",
            },
        );
    }
    #[test]
    fn import_open_alias() {
        source_snapshot!(
            "variant_import_open_alias",
            "import First exposing («Name(..)»)",
            r,
            r2,
            Error::ImportOpenAlias {
                region: r(),
                name: "Name",
            },
        );
    }
}

#[cfg(test)]
mod branches {
    use super::*;
    use nash_region::Located;
    #[test]
    fn qualified_missing_import() {
        source_snapshot!(
            "qualified_missing_import",
            "value = «Missing.value»",
            r,
            r2,
            Error::NotFoundVar {
                region: r(),
                prefix: Some("Missing"),
                name: "value",
                suggestions: PossibleNames {
                    locals: &[],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn qualified_not_exposed() {
        source_snapshot!(
            "qualified_not_exposed",
            "import Known\nvalue : «Known.Box»\nvalue = 1",
            r,
            r2,
            Error::NotFoundType {
                region: r(),
                prefix: Some("Known"),
                name: "Box",
                suggestions: PossibleNames {
                    locals: &[],
                    qualified: &[("Known", &["Bag"])],
                },
            },
        );
    }
    #[test]
    fn qualified_ambiguity() {
        source_snapshot!(
            "qualified_ambiguity",
            "import First as A\nimport Second as A\nvalue : «A.Box»\nvalue = 1",
            r,
            r2,
            Error::AmbiguousType {
                region: r(),
                prefix: Some("A"),
                name: "Box",
                first_module: ModuleName {
                    package: None,
                    name: "First",
                },
                other_modules: &[ModuleName {
                    package: None,
                    name: "Second",
                }],
            },
        );
    }
    #[test]
    fn too_few_args() {
        source_snapshot!(
            "too_few_args",
            "type Pair 'a 'b = Pair 'a 'b\nvalue («Pair x») = x",
            r,
            r2,
            Error::BadArity {
                region: r(),
                context: BadArityContext::PatternArity,
                name: "Pair",
                expected: 2,
                actual: 1,
            },
        );
    }
    #[test]
    fn too_many_args_plural() {
        source_snapshot!(
            "too_many_args_plural",
            "type One 'a = One 'a\nvalue («One x y z») = x",
            r,
            r2,
            Error::BadArity {
                region: r(),
                context: BadArityContext::PatternArity,
                name: "One",
                expected: 1,
                actual: 3,
            },
        );
    }
    #[test]
    fn recursive_alias_cycle() {
        source_snapshot!(
            "recursive_alias_cycle",
            "type alias «First» = Second\ntype alias Second = Third\ntype alias Third = First",
            r,
            r2,
            Error::RecursiveAlias {
                region: r(),
                name: "First",
                args: &[],
                typ: &Located::at(r(), nash_source::Type::Var("a")),
                others: &["Second", "Third"],
            },
        );
    }
    #[test]
    fn recursive_decl_cycle() {
        source_snapshot!(@pipeline
            "recursive_decl_cycle",
            "«name» = other\nother = name", r, r2,
            Error::RecursiveDecl {
                name: &Located::at(r(), "name"),
                others: &["other"],
            },
        );
    }
    #[test]
    fn recursive_let_self() {
        source_snapshot!(@pipeline
            "recursive_let_self",
            "value =\n    let\n        «name» = name\n    in\n    name", r, r2,
            Error::RecursiveLet {
                name: &Located::at(r(), "name"),
                others: &[],
            },
        );
    }
    #[test]
    fn unused_alias_variable() {
        source_snapshot!(
            "unused_alias_variable",
            "type alias Box '«a» = Int",
            r,
            r2,
            Error::TypeVarsMessedUpInAlias {
                region: r(),
                name: "Box",
                args: &["a"],
                unused: &[("a", r())],
                unbound: &[],
            },
        );
    }
    #[test]
    fn unused_alias_variables() {
        source_snapshot!(
            "unused_alias_variables",
            "type alias Box '«a» '«b» = Int",
            r,
            r2,
            Error::TypeVarsMessedUpInAlias {
                region: r(),
                name: "Box",
                args: &["a", "b"],
                unused: &[("a", r()), ("b", r2())],
                unbound: &[],
            },
        );
    }
    #[test]
    fn unbound_alias_variable() {
        source_snapshot!(
            "unbound_alias_variable",
            "type alias Box = List '«a»",
            r,
            r2,
            Error::TypeVarsMessedUpInAlias {
                region: r(),
                name: "Box",
                args: &[],
                unused: &[],
                unbound: &[("a", r())],
            },
        );
    }
    #[test]
    fn unbound_union_variables() {
        source_snapshot!(
            "unbound_union_variables",
            "type Box = Box '«a» '«b»",
            r,
            r2,
            Error::TypeVarsUnboundInUnion {
                region: r(),
                name: "Box",
                args: &[],
                unbound: ("a", r()),
                more_unbound: &[("b", r2())],
            },
        );
    }
    #[test]
    fn operator_javascript_equal() {
        source_snapshot!(
            "operator_javascript_equal",
            "value = 1 «===» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "===",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_javascript_not_equal() {
        source_snapshot!(
            "operator_javascript_not_equal",
            "value = 1 «!=» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "!=",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_javascript_strict_not_equal() {
        source_snapshot!(
            "operator_javascript_strict_not_equal",
            "value = 1 «!==» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "!==",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_power() {
        source_snapshot!(
            "operator_power",
            "value = 1 «**» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "**",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_percent() {
        source_snapshot!(
            "operator_percent",
            "value = 1 «%» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "%",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_missing() {
        source_snapshot!(
            "operator_missing",
            "value = 1 «<+>» 2",
            r,
            r2,
            Error::NotFoundBinop {
                region: r(),
                name: "<+>",
                available: &[],
            },
        );
    }
    #[test]
    fn module_name_is_preserved() {
        let source = Source::new("");
        let report = to_report_with_name(&source, &Error::MissingModuleHeader, "App.Main");
        insta::with_settings!({ description => "", omit_expression => true }, {
            insta::assert_snapshot!(crate::render_plain(&report, &source, "App/Main.nash"));
        });
    }
    #[test]
    fn closed_kind_precedence() {
        let arrow = Kind::Arrow(&Kind::Type, &Kind::Type);
        assert_eq!(
            kind(&Kind::Arrow(&arrow, &arrow)),
            "(Type -> Type) -> Type -> Type"
        );
    }
    #[test]
    fn formation_contexts() {
        let trait_ = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Core",
            },
            name: "Functor",
        };
        let contexts = [
            KindContext::TypeAnnotation,
            KindContext::BigField {
                union: "Tree",
                ctor: "Node",
                index: 1,
            },
            KindContext::LittleField {
                union: "tree",
                ctor: "Node",
                index: 0,
            },
            KindContext::RecordField {
                alias: "State",
                field: "x",
                big: true,
            },
            KindContext::RecordField {
                alias: "state",
                field: "x",
                big: false,
            },
            KindContext::AliasCasing {
                alias: "Box",
                big: true,
            },
            KindContext::AliasCasing {
                alias: "box",
                big: false,
            },
            KindContext::Annotation { name: "map" },
            KindContext::ImplHead { trait_, index: 1 },
        ];
        let inputs = [
            "value : «List»\nvalue = []",
            "type Tree = Node Int «List»",
            "type tree = Node «List»",
            "type alias State = { x : «List» }",
            "type alias state = { x : «List» }",
            "type alias Box = «List»",
            "type alias box = «List»",
            "map : «List»\nmap = []",
            "impl Core.Functor Int «List» where\n    map f x = x",
        ];
        for (index, (context, input)) in contexts.iter().zip(inputs).enumerate() {
            source_snapshot!(
                format!("formation_context_{index}"),
                input,
                r,
                r2,
                Error::KindMismatch {
                    region: r(),
                    context,
                    expected: &Kind::Type,
                    actual: &Kind::Arrow(&Kind::Type, &Kind::Type),
                }
            );
        }
    }
    #[test]
    fn source_pipeline_reports_all_missing_names() {
        let input = "module Main exposing (..)\nfirst = missing\nsecond = absent\n";
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, src);
        let module = parser.module().expect("parse");
        let errors = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
            .expect_err("canonical errors");
        assert_eq!(errors.len(), 2);
        let source = Source::new(input);
        insta::with_settings!({ description => input, omit_expression => true }, {
        insta::assert_snapshot!(
            errors
                .iter()
                .map(|e| crate::render_plain(&to_report(&source, e), &source, "Main.nash"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        });
    }
    #[test]
    fn bad_impl_head_reasons() {
        for (name, input, reason) in [
            (
                "bad_head_function",
                "impl Equal «(Int -> Int)» where\n    equal x y = True",
                nash_can::BadHead::Function,
            ),
            (
                "bad_head_record",
                "impl Equal «{ x : Int }» where\n    equal x y = True",
                nash_can::BadHead::Record,
            ),
            (
                "bad_head_variable_application",
                "impl Equal «('f 'a)» where\n    equal x y = True",
                nash_can::BadHead::VariableApplication,
            ),
        ] {
            source_snapshot!(
                name,
                input,
                r,
                r2,
                Error::BadInstanceHead {
                    region: r(),
                    reason,
                },
            );
        }
    }
    #[test]
    fn superclass_failure_reasons() {
        let trait_ = QualifiedName {
            home: ModuleName {
                package: None,
                name: "Core",
            },
            name: "Equal",
        };

        for (name, reason) in [
            ("superclass_cycle", nash_can::EntailmentFailure::Cycle),
            ("superclass_limit", nash_can::EntailmentFailure::Limit),
        ] {
            let (_, regions) = marked_source("impl «Core.Equal 'a» where\n    equal x y = True");
            let r = || regions[0];
            let arg = Located::at(r(), nash_ast::Type::Var("a"));
            source_snapshot!(
                name,
                "impl «Core.Equal 'a» where\n    equal x y = True",
                r,
                r2,
                Error::MissingSuperclass {
                    region: r(),
                    trait_,
                    heads: &[Located::at(r(), nash_ast::Head::Var(0))],
                    superclass: &nash_ast::Pred::Trait {
                        trait_,
                        args: &[&arg],
                    },
                    index: 0,
                    reason,
                },
            );
        }
    }
    #[test]
    fn duplicate_pattern_contexts() {
        for (name, input, context) in [
            (
                "duplicate_lambda",
                "value = \\«x» «x» -> x",
                DuplicatePatternContext::LambdaArgs,
            ),
            (
                "duplicate_function",
                "map «x» «x» = x",
                DuplicatePatternContext::FuncArgs("map"),
            ),
            (
                "duplicate_let",
                "value =\n    let\n        «x» = 1\n        «x» = 2\n    in\n    x",
                DuplicatePatternContext::LetBinding,
            ),
            (
                "duplicate_destruct",
                "value =\n    let\n        («x», «x») = (1, 2)\n    in\n    x",
                DuplicatePatternContext::Destruct,
            ),
        ] {
            source_snapshot!(
                name,
                input,
                r,
                r2,
                Error::DuplicatePattern {
                    context,
                    name: "x",
                    first: r(),
                    second: r2(),
                },
            );
        }
    }
    #[test]
    fn export_suggestion_counts() {
        source_snapshot!(
            "export_no_suggestions",
            "module Main exposing («Box»)\nvalue = 1",
            r,
            r2,
            Error::ExportNotFound {
                region: r(),
                kind: VarKind::BadType,
                name: "Box",
                suggestions: &[],
            },
        );
        source_snapshot!(
            "export_multiple_suggestions",
            "module Main exposing ((«<+>»))\ninfix left 5 (+) = add\ninfix right 5 (++) = append",
            r,
            r2,
            Error::ExportNotFound {
                region: r(),
                kind: VarKind::BadOp,
                name: "<+>",
                suggestions: &["+", "++"],
            },
        );
    }
    #[test]
    fn source_pipeline_overlapping_impls() {
        let input = "module Bad exposing (..)\ntrait Keep 'a where\n    keep : 'a -> 'a\nimpl Keep () where\n    keep x = x\nimpl Keep () where\n    keep x = x\n";
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, src);
        let module = parser.module().expect("parse");
        let errors = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
            .expect_err("overlapping impls");
        let [error @ Error::OverlappingImpls { first, second, .. }] = errors.as_slice() else {
            panic!("expected one overlap error");
        };
        let source = Source::new(input);
        let report = to_report(&source, error);
        assert_eq!(report.title, "OVERLAPPING IMPL");
        assert_eq!(report.region, *second);
        assert!(report.labels[0].region == *first && report.region == *second);
        let rendered = crate::render_plain(&report, &source, "Bad.nash");
        assert!(!rendered.contains("Rename"));
        assert!(rendered.contains("context constraints"));
        insta::with_settings!({ description => input, omit_expression => true }, {
            insta::assert_snapshot!(rendered);
        });
    }
}
