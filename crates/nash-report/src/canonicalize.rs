//! Canonicalization reports, adapted from Elm's Reporting/Error/Canonicalize.hs.
//! Nash adds trait, kind, representation, and nominal-record diagnostics.
use crate::{Doc, Label, Report, Snippet, Source, suggest};
use nash_ast::{Kind, ModuleName, QualifiedName};
use nash_can::{
    BadArityContext, DuplicatePatternContext, Error, KindContext, PossibleNames, VarKind,
};
use nash_region::Region;

#[cfg(test)]
mod tests {
    use super::*;
    use nash_region::Position;
    fn region() -> Region {
        Region::new(Position::new(1, 1), Position::new(1, 5))
    }
    fn snapshot(name: &str, error: Error<'_>) {
        let source = Source::new("name = other\n");
        insta::assert_snapshot!(
            name,
            crate::render_plain(&to_report(&source, &error), &source, "Main.nash")
        );
    }
    #[test]
    fn not_found_var() {
        snapshot(
            "not_found_var",
            Error::NotFoundVar {
                region: region(),
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
        snapshot(
            "not_found_var_with_suggestion",
            Error::NotFoundVar {
                region: region(),
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
    fn missing_module_header() {
        snapshot("missing_module_header", Error::MissingModuleHeader);
    }
    #[test]
    fn kind_mismatch() {
        snapshot(
            "kind_mismatch",
            Error::KindMismatch {
                region: region(),
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
    match error {
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
            label(*first, "once here"),
            label(*second, "and again right here"),
            Doc::reflow(&format!(
                "You are trying to expose `{name}` multiple times! Once here:"
            )),
            Doc::text("Remove one of them and you should be all set!"),
        ),
        Error::ExportNotFound {
            region,
            kind,
            name,
            suggestions,
        } => {
            let (article, thing, display) = to_kind_info(*kind, name);
            let nearby = nearby(name, suggestions, 4);
            let mut report = simple(
                "UNKNOWN EXPORT",
                *region,
                &format!(
                    "You are trying to expose {article} {thing} named {display} but I cannot find its definition."
                ),
                "",
            );
            report.snippet = Snippet::None;
            report.after = suggestion_details(
                &nearby,
                "I do not see any super similar names in this file. Is the definition missing?",
            );
            report.with_suggestions(nearby)
        }
        Error::ExportOpenAlias { region, name } => simple(
            "BAD EXPORT",
            *region,
            &format!(
                "The (..) syntax is for exposing variants of a custom type. It cannot be used with a type alias like `{name}` though."
            ),
            "Remove the (..) and you should be fine!",
        ),
        Error::ImportOpenAlias { region, name } => simple(
            "BAD IMPORT",
            *region,
            &format!("The `{name}` type alias cannot be followed by (..) like this:"),
            "Remove the (..) and it should work.",
        ),
        Error::ImportCtorByName {
            region,
            name,
            type_name,
        } => simple(
            "BAD IMPORT",
            *region,
            &format!("You are trying to import the `{name}` variant by name:"),
            &format!(
                "Try importing {type_name}(..) instead. The dots mean “expose the {type_name} type and all its variants” so it gives you access to {name}."
            ),
        ),
        Error::ImportNotFound { region, module } => simple(
            "UNKNOWN IMPORT",
            *region,
            &format!("I could not find a `{module}` module to import!"),
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
            report.after = suggestion_details(
                &nearby,
                "I cannot find any super similar exposed names. Maybe it is private?",
            );
            report.with_suggestions(nearby)
        }
        Error::BinopFunctionNotFound {
            region,
            op,
            function,
        } => simple(
            "INFIX PROBLEM",
            *region,
            &format!(
                "The ({op}) operator says it is implemented by `{function}`, but I cannot find a `{function}` definition in this file."
            ),
            "Define it, or point the `infix` declaration at an existing top-level value.",
        ),
        Error::BinopConflict { region, op1, op2 } => simple(
            "INFIX PROBLEM",
            *region,
            &format!("You cannot mix ({op1}) and ({op2}) without parentheses."),
            "I do not know how to group these expressions. Add parentheses for me!",
        ),
        Error::NotFoundBinop {
            region,
            name,
            available,
        } => not_found_binop(*region, name, available),
        Error::PatternHasRecordCtor { region, name } => simple(
            "BAD PATTERN",
            *region,
            &format!(
                "You can construct records by using `{name}` as a function, but it is not available in pattern matching like this:"
            ),
            "I recommend matching the record as a variable and unpacking it later.",
        ),
        Error::Shadowing {
            name,
            original,
            new,
        } => Report::pair(
            "SHADOWING",
            label(*original, "first defined here"),
            label(*new, "defined AGAIN here"),
            Doc::reflow(&format!("The name `{name}` is first defined here:")),
            Doc::stack([
                Doc::reflow(
                    "Think of a more helpful name for one of them and you should be all set!",
                ),
                Doc::link(
                    "Note",
                    "Linters advise against shadowing, so Nash makes “best practices” the default. Read",
                    "shadowing",
                    "for more details on this choice.",
                ),
            ]),
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
                "The type annotation for `{name}` says it can accept {}, but the definition says it has {}:",
                args(*index),
                args(index + leftovers)
            ),
            &format!(
                "Is the type annotation missing something? Should some argument{} be deleted? Maybe some parentheses are missing?",
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
            &format!("I found a kind mismatch in {}:", kind_context(context)),
            &format!(
                "This position needs kind `{}`, but the type has kind `{}`. Type arguments must have matching kinds.",
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
            "A type constructor cannot be applied to itself. Check which type is being applied and the kinds of its arguments.",
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
            "No type can satisfy all of these requirements. Change the constraints or the positions where this type variable is used.",
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
            "This parameter controls the constraints needed to form the type. Pass a type variable here so context inference can terminate.",
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
            &format!(
                "The `{trait_}` trait requires this method and does not provide a default. Add a `{name}` definition to this impl."
            ),
        ),
        Error::UnknownMethod {
            region,
            trait_,
            name,
        } => simple(
            "UNKNOWN METHOD",
            *region,
            &format!("The `{trait_}` trait has no `{name}` method:"),
            "Check the method name against the trait declaration. Remove this definition or rename it to the method you intended to implement.",
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
                "I cannot choose which impl to use. Remove one of them, or change their heads so they cannot match the same trait arguments. Adding different context constraints does not disambiguate overlapping heads.",
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
                Doc::reflow(
                    "Remove a superclass dependency to break the cycle. A trait cannot require itself through its superclasses.",
                ),
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
                "I cannot find a visible record alias with exactly these fields: {}.",
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
            "Simplify the impl head so the compiler can compare and resolve its patterns within the supported limit.",
        ),
        Error::NegateWithoutNum { region } => simple(
            "NAMING ERROR",
            *region,
            "I cannot resolve numeric negation here:",
            "Negation requires the `Num` trait. Import the module that defines `Num` before using a negative expression.",
        ),
        Error::DoWithoutMonad { region } => simple(
            "NAMING ERROR",
            *region,
            "I cannot resolve this `do` expression:",
            "A `do` expression requires the `Monad` trait. Import the module that defines `Monad`.",
        ),
        Error::RefutableBindPattern { region } => simple(
            "UNSAFE PATTERN",
            *region,
            "This `do` binding has a pattern that can fail to match:",
            "Use a variable or another irrefutable pattern here. Match individual variants in a `case` expression so every possibility is handled.",
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
            &format!("I cannot canonicalize {feature} yet:"),
            "This syntax is recognized, but its compiler implementation is not available yet.",
        ),
    }
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
        label(first, "one here"),
        label(second, "and another one here"),
        Doc::reflow(&format!("{message} One here:")),
        Doc::text("How can I know which one you want? Rename one of them!"),
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
            Doc::text("Maybe you want"),
            Doc::text(one).dullyellow(),
            Doc::text("instead?"),
        ]),
        _ => Doc::stack([
            Doc::text("These names seem close though:"),
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
    let details = match prefix {
        None => {
            if nearby.is_empty() {
                "Is there an `import` or `exposing` missing up top?".into()
            } else {
                "These names seem close though:".into()
            }
        }
        Some(p) if possible.qualified.iter().any(|(m, _)| *m == p) => format!(
            "The `{p}` module does not expose a `{name}` {thing}.{}",
            if nearby.is_empty() {
                ""
            } else {
                " These names seem close though:"
            }
        ),
        Some(p) => {
            if nearby.is_empty() {
                format!("I cannot find a `{p}` module. Is there an `import` for it?")
            } else {
                format!("I cannot find a `{p}` import. These names seem close though:")
            }
        }
    };
    let mut docs = vec![Doc::reflow(&details)];
    if !nearby.is_empty() {
        docs.push(Doc::indent(
            4,
            Doc::vcat(nearby.iter().map(|n| Doc::text(n).dullyellow())),
        ));
    }
    docs.push(Doc::link(
        "Hint",
        "Read",
        "imports",
        "to see how `import` declarations work in Nash.",
    ));
    Report::snippet(
        "NAMING ERROR",
        region,
        None,
        Doc::reflow(&format!("I cannot find a `{given}` {thing}:")),
        Doc::stack(docs),
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
    match prefix {
        None => Report::snippet(
            "AMBIGUOUS NAME",
            region,
            None,
            Doc::reflow(&format!("This usage of `{name}` is ambiguous:")),
            Doc::stack([
                Doc::reflow(&format!(
                    "This name is exposed by {} of your imports, so I am not sure which one to use:",
                    homes.len()
                )),
                Doc::indent(
                    4,
                    Doc::vcat(
                        homes
                            .iter()
                            .map(|h| Doc::text(to_qual_string(h.name, name)).dullyellow()),
                    ),
                ),
                Doc::reflow(
                    "I recommend using qualified names for imported values. I also recommend having at most one `exposing (..)` per file to make name clashes like this less common in the long run.",
                ),
                Doc::link(
                    "Note",
                    "Check out",
                    "imports",
                    "for more info on the import syntax.",
                ),
            ]),
        ),
        Some(prefix) => Report::snippet(
            "AMBIGUOUS NAME",
            region,
            None,
            Doc::reflow(&format!("This usage of `{prefix}.{name}` is ambiguous.")),
            Doc::stack([
                Doc::reflow(&format!(
                    "It could refer to a {thing} from {} of these imports:",
                    if homes.len() == 2 { "either" } else { "any" }
                )),
                Doc::indent(
                    4,
                    Doc::vcat(homes.iter().map(|h| {
                        Doc::text(if prefix == h.name {
                            format!("import {}", h.name)
                        } else {
                            format!("import {} as {prefix}", h.name)
                        })
                    })),
                ),
                Doc::reflow_link(
                    "Read",
                    "imports",
                    "to learn how to clarify which one you want.",
                ),
            ]),
        ),
    }
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
            "What is missing? Are some parentheses misplaced?"
        } else if actual - expected == 1 {
            "Which is the extra one? Maybe some parentheses are missing?"
        } else {
            "Which are the extra ones? Maybe some parentheses are missing?"
        },
    )
}
fn not_found_binop(region: Region, name: &str, available: &[&str]) -> Report {
    let (before,after,suggestions) = match name {
        "===" => ("Nash does not have a (===) operator like JavaScript.".into(),"Switch to (==) instead.".into(),vec!["==".into()]),
        "!="|"!==" => ("Nash uses a different name for the “not equal” operator:".into(),format!("Switch to (/=) instead. Our (/=) operator is supposed to look like a real “not equal” sign (≠). I hope that history will remember ({name}) as a weird and temporary choice."),vec!["/=".into()]),
        "**" => ("I do not recognize the (**) operator:".into(),"Switch to (^) for exponentiation. Or switch to (*) for multiplication.".into(),vec!["^".into(),"*".into()]),
        // The stdlib names Int.rem and Int.mod are provisional in Plan 06.
        "%" => ("Nash does not use (%) as the remainder operator:".into(),"If you want the behavior of (%) like in JavaScript, use the integer remainder function. If you want modular arithmetic like in math, use the integer modulus function. The difference is how things work when negative numbers are involved.".into(),vec![]),
        _ => {let choices=nearby(name,available,2); let mut after="Is there an `import` and `exposing` entry for it?".to_string(); if !choices.is_empty() {after.push_str(&format!(" Maybe you want {} instead?",choices.iter().map(|s|format!("({s})")).collect::<Vec<_>>().join(" or ")));} (format!("I do not recognize the ({name}) operator."),after,choices)}
    };
    simple("UNKNOWN OPERATOR", region, &before, &after).with_suggestions(suggestions)
}
fn recursive_value(region: Region, name: &str, others: &[&str], is_let: bool) -> Report {
    let before = if others.is_empty() {
        format!(
            "The `{name}` value is defined directly in terms of itself, causing an infinite loop."
        )
    } else if is_let {
        "I do not allow cyclic values in `let` expressions.".into()
    } else {
        format!("The `{name}` definition is causing a very tricky infinite loop.")
    };
    let mut docs = if others.is_empty() {
        vec![
            Doc::reflow(&format!(
                "Are you trying to mutate a variable? Nash does not have mutation, so when I see {name} defined in terms of {name}, I treat it as a recursive definition. Try giving the new value a new name!"
            )),
            Doc::reflow(&format!(
                "Maybe you DO want a recursive value? To define {name} we need to know what {name} is, so let’s expand it. Wait, but now we need to know what {name} is, so let’s expand it... This will keep going infinitely!"
            )),
        ]
    } else {
        vec![
            Doc::reflow(&format!(
                "The `{name}` value depends on itself through the following chain of definitions:"
            )),
            Doc::cycle(4, name, others),
        ]
    };
    docs.push(Doc::link(
        "Hint",
        "The root problem is often a typo in some variable name, but I recommend reading",
        "bad-recursion",
        "for more detailed advice, especially if you actually do need a recursive value.",
    ));
    Report::snippet(
        if is_let {
            "CYCLIC VALUE"
        } else {
            "CYCLIC DEFINITION"
        },
        region,
        None,
        Doc::reflow(&before),
        Doc::stack(docs),
    )
}
fn alias_recursion_report(
    region: Region,
    name: &str,
    args: &[&str],
    typ: &nash_region::Located<nash_source::Type<'_>>,
    others: &[&str],
) -> Report {
    let (before, after) = if others.is_empty() {
        (
            "This type alias is recursive, forming an infinite type!",
            Doc::stack([
                Doc::reflow(
                    "When I expand a recursive type alias, it just keeps getting bigger and bigger. So dealiasing results in an infinitely large type! Try this instead:",
                ),
                Doc::indent(4, alias_to_union_doc(name, args, typ)),
                Doc::link(
                    "Hint",
                    "This is kind of a subtle distinction. I suggested the naive fix, but I recommend reading",
                    "recursive-alias",
                    "for ideas on how to do better.",
                ),
            ]),
        )
    } else {
        (
            "This type alias is part of a mutually recursive set of type aliases.",
            Doc::stack([
                Doc::text("It is part of this cycle of type aliases:"),
                Doc::cycle(4, name, others),
                Doc::reflow(
                    "You need to convert at least one of these type aliases into a `type`.",
                ),
                Doc::link(
                    "Note",
                    "Read",
                    "recursive-alias",
                    "to learn why this `type` vs `type alias` distinction matters. It is subtle but important!",
                ),
            ]),
        )
    };
    Report::snippet("ALIAS PROBLEM", region, None, Doc::text(before), after)
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
            Doc::reflow("You probably need to change the declaration to something like this:"),
            declaration(decl, name, args, &names),
            Doc::reflow(&format!(
                "Why? Well, imagine one `{name}` where `{}` is an Int and another where it is a Bool. When we explicitly list the type variables, the type checker can see that they are actually different types.",
                first.0
            )),
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
                    "I recommend removing {} from the declaration, like this:",
                    unused_names.join(" and ")
                )),
                declaration("type alias", name, &kept, &[]),
                Doc::reflow(
                    "Why? Well, if I allowed `type alias Height 'a = Int` I would need to answer some weird questions. Is `Height Bool` the same as `Int`? Is `Height Bool` the same as `Height Int`? My solution is to not need to ask them!",
                ),
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
                Doc::reflow("My guess is that a definition like this will work better:"),
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
    use nash_region::{Located, Position};
    fn r() -> Region {
        Region::new(Position::new(1, 1), Position::new(1, 5))
    }
    fn r2() -> Region {
        Region::new(Position::new(2, 1), Position::new(2, 6))
    }
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
    fn check(name: &str, error: Error<'_>) {
        let source = Source::new("name = other\nother = name\n");
        let report = to_report(&source, &error);
        insta::assert_snapshot!(name, crate::render_plain(&report, &source, "Main.nash"));
    }
    #[test]
    fn record_literal_no_alias() {
        check(
            "variant_record_literal_no_alias",
            Error::RecordLiteralNoAlias {
                region: r(),
                fields: &["x", "y"],
            },
        );
    }
    #[test]
    fn record_literal_ambiguous() {
        check(
            "variant_record_literal_ambiguous",
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
        check(
            "variant_record_type_outside_alias",
            Error::RecordTypeOutsideAlias { region: r() },
        );
    }
    #[test]
    fn impl_pattern_limit() {
        check(
            "variant_impl_pattern_limit",
            Error::ImplPatternLimit { region: r() },
        );
    }
    #[test]
    fn negate_without_num() {
        check(
            "variant_negate_without_num",
            Error::NegateWithoutNum { region: r() },
        );
    }
    #[test]
    fn do_without_monad() {
        check(
            "variant_do_without_monad",
            Error::DoWithoutMonad { region: r() },
        );
    }
    #[test]
    fn refutable_bind_pattern() {
        check(
            "variant_refutable_bind_pattern",
            Error::RefutableBindPattern { region: r() },
        );
    }
    #[test]
    fn structural_eq_override() {
        check(
            "variant_structural_eq_override",
            Error::StructuralEqOverride {
                head: &Located::at(r(), nash_ast::Type::Var("a")),
            },
        );
    }
    #[test]
    fn reflexive_lift_overlap() {
        check(
            "variant_reflexive_lift_overlap",
            Error::ReflexiveLiftOverlap {
                heads: &[&Located::at(r(), nash_ast::Type::Var("a"))],
            },
        );
    }
    #[test]
    fn missing_superclass() {
        check(
            "variant_missing_superclass",
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
        check(
            "variant_bad_instance_head",
            Error::BadInstanceHead {
                region: r(),
                reason: nash_can::BadHead::BareVariable,
            },
        );
    }
    #[test]
    fn impl_context_var_not_in_head() {
        check(
            "variant_impl_context_var_not_in_head",
            Error::ImplContextVarNotInHead {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn missing_method() {
        check(
            "variant_missing_method",
            Error::MissingMethod {
                region: r(),
                trait_: "Equal",
                name: "name",
            },
        );
    }
    #[test]
    fn unknown_method() {
        check(
            "variant_unknown_method",
            Error::UnknownMethod {
                region: r(),
                trait_: "Equal",
                name: "name",
            },
        );
    }
    #[test]
    fn orphan_impl() {
        check(
            "variant_orphan_impl",
            Error::OrphanImpl {
                region: r(),
                trait_: q(),
                heads: &[nash_ast::HeadCon::Named(q())],
            },
        );
    }
    #[test]
    fn overlapping_impls() {
        check(
            "variant_overlapping_impls",
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
        check(
            "variant_import_open_trait",
            Error::ImportOpenTrait {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn duplicate_trait() {
        check(
            "variant_duplicate_trait",
            Error::DuplicateTrait {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_method() {
        check(
            "variant_duplicate_method",
            Error::DuplicateMethod {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_trait_parameter() {
        check(
            "variant_duplicate_trait_parameter",
            Error::DuplicateTraitParameter {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn superclass_bad_arg() {
        check(
            "variant_superclass_bad_arg",
            Error::SuperclassBadArg {
                region: r(),
                trait_: "Equal",
            },
        );
    }
    #[test]
    fn method_missing_parameter() {
        check(
            "variant_method_missing_parameter",
            Error::MethodMissingParameter {
                region: r(),
                method: "a",
                parameter: "a",
            },
        );
    }
    #[test]
    fn recursive_superclass() {
        check(
            "variant_recursive_superclass",
            Error::RecursiveSuperclass {
                names: &[&Located::at(r(), "First"), &Located::at(r2(), "Second")],
            },
        );
    }
    #[test]
    fn export_open_trait() {
        check(
            "variant_export_open_trait",
            Error::ExportOpenTrait {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn not_found_trait() {
        check(
            "variant_not_found_trait",
            Error::NotFoundTrait {
                region: r(),
                prefix: None,
                name: "name",
            },
        );
    }
    #[test]
    fn ambiguous_trait() {
        check(
            "variant_ambiguous_trait",
            Error::AmbiguousTrait {
                region: r(),
                prefix: None,
                name: "name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn trait_arity() {
        check(
            "variant_trait_arity",
            Error::TraitArity {
                region: r(),
                name: "name",
                expected: 1,
                actual: 2,
            },
        );
    }
    #[test]
    fn context_var_not_in_type() {
        check(
            "variant_context_var_not_in_type",
            Error::ContextVarNotInType {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn kind_mismatch() {
        check(
            "variant_kind_mismatch",
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
        check(
            "variant_kind_infinite",
            Error::KindInfinite {
                region: r(),
                context: &KindContext::Annotation { name: "name" },
            },
        );
    }
    #[test]
    fn representation_mismatch() {
        check(
            "variant_representation_mismatch",
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
        check(
            "variant_contradictory_representation",
            Error::ContradictoryRepresentation {
                region: r(),
                variable: "a",
            },
        );
    }
    #[test]
    fn impl_of_builtin_trait() {
        check(
            "variant_impl_of_builtin_trait",
            Error::ImplOfBuiltinTrait {
                region: r(),
                trait_: q(),
            },
        );
    }
    #[test]
    fn irregular_recursion() {
        check(
            "variant_irregular_recursion",
            Error::IrregularRecursion {
                region: r(),
                constructor: q(),
                parameter: "a",
            },
        );
    }
    #[test]
    fn unsupported() {
        check(
            "variant_unsupported",
            Error::Unsupported {
                feature: "a",
                region: r(),
            },
        );
    }
    #[test]
    fn missing_module_header() {
        check("variant_missing_module_header", Error::MissingModuleHeader);
    }
    #[test]
    fn not_found_type() {
        check(
            "variant_not_found_type",
            Error::NotFoundType {
                region: r(),
                prefix: None,
                name: "name",
                suggestions: PossibleNames {
                    locals: &["name"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn import_not_found() {
        check(
            "variant_import_not_found",
            Error::ImportNotFound {
                region: r(),
                module: "Missing",
            },
        );
    }
    #[test]
    fn ambiguous_type() {
        check(
            "variant_ambiguous_type",
            Error::AmbiguousType {
                region: r(),
                prefix: None,
                name: "name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn bad_arity() {
        check(
            "variant_bad_arity",
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
        check(
            "variant_export_not_found",
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
        check(
            "variant_export_open_alias",
            Error::ExportOpenAlias {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn duplicate_decl() {
        check(
            "variant_duplicate_decl",
            Error::DuplicateDecl {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_type() {
        check(
            "variant_duplicate_type",
            Error::DuplicateType {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_ctor() {
        check(
            "variant_duplicate_ctor",
            Error::DuplicateCtor {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_binop() {
        check(
            "variant_duplicate_binop",
            Error::DuplicateBinop {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn binop_function_not_found() {
        check(
            "variant_binop_function_not_found",
            Error::BinopFunctionNotFound {
                region: r(),
                op: "a",
                function: "a",
            },
        );
    }
    #[test]
    fn duplicate_union_arg() {
        check(
            "variant_duplicate_union_arg",
            Error::DuplicateUnionArg {
                type_name: "a",
                arg_name: "a",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn duplicate_alias_arg() {
        check(
            "variant_duplicate_alias_arg",
            Error::DuplicateAliasArg {
                type_name: "a",
                arg_name: "a",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn recursive_alias() {
        check(
            "variant_recursive_alias",
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
        check(
            "variant_type_vars_unbound_in_union",
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
        check(
            "variant_type_vars_messed_up_in_alias",
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
        check(
            "variant_labeled_ctor_missing_field",
            Error::LabeledCtorMissingField {
                region: r(),
                ctor: "a",
                field: "a",
            },
        );
    }
    #[test]
    fn labeled_ctor_extra_field() {
        check(
            "variant_labeled_ctor_extra_field",
            Error::LabeledCtorExtraField {
                region: r(),
                ctor: "a",
                field: "a",
            },
        );
    }
    #[test]
    fn labeled_ctor_unknown_field() {
        check(
            "variant_labeled_ctor_unknown_field",
            Error::LabeledCtorUnknownField {
                region: r(),
                ctor: "a",
                field: "a",
            },
        );
    }
    #[test]
    fn duplicate_field() {
        check(
            "variant_duplicate_field",
            Error::DuplicateField {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn export_duplicate() {
        check(
            "variant_export_duplicate",
            Error::ExportDuplicate {
                name: "name",
                first: r(),
                second: r2(),
            },
        );
    }
    #[test]
    fn not_found_ctor() {
        check(
            "variant_not_found_ctor",
            Error::NotFoundCtor {
                region: r(),
                prefix: None,
                name: "name",
                suggestions: PossibleNames {
                    locals: &["name"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn ambiguous_ctor() {
        check(
            "variant_ambiguous_ctor",
            Error::AmbiguousCtor {
                region: r(),
                prefix: None,
                name: "name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn pattern_has_record_ctor() {
        check(
            "variant_pattern_has_record_ctor",
            Error::PatternHasRecordCtor {
                region: r(),
                name: "name",
            },
        );
    }
    #[test]
    fn duplicate_pattern() {
        check(
            "variant_duplicate_pattern",
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
        check(
            "variant_not_found_var",
            Error::NotFoundVar {
                region: r(),
                prefix: None,
                name: "name",
                suggestions: PossibleNames {
                    locals: &["name"],
                    qualified: &[],
                },
            },
        );
    }
    #[test]
    fn ambiguous_var() {
        check(
            "variant_ambiguous_var",
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
        check(
            "variant_not_found_binop",
            Error::NotFoundBinop {
                region: r(),
                name: "name",
                available: &["other"],
            },
        );
    }
    #[test]
    fn ambiguous_binop() {
        check(
            "variant_ambiguous_binop",
            Error::AmbiguousBinop {
                region: r(),
                name: "name",
                first_module: home(),
                other_modules: &[other()],
            },
        );
    }
    #[test]
    fn binop_conflict() {
        check(
            "variant_binop_conflict",
            Error::BinopConflict {
                region: r(),
                op1: "a",
                op2: "a",
            },
        );
    }
    #[test]
    fn shadowing() {
        check(
            "variant_shadowing",
            Error::Shadowing {
                name: "name",
                original: r(),
                new: r2(),
            },
        );
    }
    #[test]
    fn recursive_let() {
        check(
            "variant_recursive_let",
            Error::RecursiveLet {
                name: &Located::at(r(), "name"),
                others: &["other"],
            },
        );
    }
    #[test]
    fn recursive_decl() {
        check(
            "variant_recursive_decl",
            Error::RecursiveDecl {
                name: &Located::at(r(), "name"),
                others: &[],
            },
        );
    }
    #[test]
    fn annotation_too_short() {
        check(
            "variant_annotation_too_short",
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
        check(
            "variant_import_exposing_not_found",
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
        check(
            "variant_import_ctor_by_name",
            Error::ImportCtorByName {
                region: r(),
                name: "name",
                type_name: "a",
            },
        );
    }
    #[test]
    fn import_open_alias() {
        check(
            "variant_import_open_alias",
            Error::ImportOpenAlias {
                region: r(),
                name: "name",
            },
        );
    }
}

#[cfg(test)]
mod branches {
    use super::*;
    use nash_region::{Located, Position};
    fn r() -> Region {
        Region::new(Position::new(1, 1), Position::new(1, 5))
    }
    fn snapshot(name: &str, error: Error<'_>) {
        let source = Source::new("name = other\n");
        insta::assert_snapshot!(
            name,
            crate::render_plain(&to_report(&source, &error), &source, "Main.nash")
        );
    }
    #[test]
    fn qualified_missing_import() {
        snapshot(
            "qualified_missing_import",
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
        snapshot(
            "qualified_not_exposed",
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
        snapshot(
            "qualified_ambiguity",
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
        snapshot(
            "too_few_args",
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
        snapshot(
            "too_many_args_plural",
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
        snapshot(
            "recursive_alias_cycle",
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
        snapshot(
            "recursive_decl_cycle",
            Error::RecursiveDecl {
                name: &Located::at(r(), "name"),
                others: &["other"],
            },
        );
    }
    #[test]
    fn recursive_let_self() {
        snapshot(
            "recursive_let_self",
            Error::RecursiveLet {
                name: &Located::at(r(), "name"),
                others: &[],
            },
        );
    }
    #[test]
    fn unused_alias_variable() {
        snapshot(
            "unused_alias_variable",
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
        snapshot(
            "unused_alias_variables",
            Error::TypeVarsMessedUpInAlias {
                region: r(),
                name: "Box",
                args: &["a", "b"],
                unused: &[("a", r()), ("b", r())],
                unbound: &[],
            },
        );
    }
    #[test]
    fn unbound_alias_variable() {
        snapshot(
            "unbound_alias_variable",
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
        snapshot(
            "unbound_union_variables",
            Error::TypeVarsUnboundInUnion {
                region: r(),
                name: "Box",
                args: &[],
                unbound: ("a", r()),
                more_unbound: &[("b", r())],
            },
        );
    }
    #[test]
    fn operator_javascript_equal() {
        snapshot(
            "operator_javascript_equal",
            Error::NotFoundBinop {
                region: r(),
                name: "===",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_javascript_not_equal() {
        snapshot(
            "operator_javascript_not_equal",
            Error::NotFoundBinop {
                region: r(),
                name: "!=",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_javascript_strict_not_equal() {
        snapshot(
            "operator_javascript_strict_not_equal",
            Error::NotFoundBinop {
                region: r(),
                name: "!==",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_power() {
        snapshot(
            "operator_power",
            Error::NotFoundBinop {
                region: r(),
                name: "**",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_percent() {
        snapshot(
            "operator_percent",
            Error::NotFoundBinop {
                region: r(),
                name: "%",
                available: &[],
            },
        );
    }
    #[test]
    fn operator_missing() {
        snapshot(
            "operator_missing",
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
        insta::assert_snapshot!(crate::render_plain(&report, &source, "App/Main.nash"));
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
        insta::assert_snapshot!(
            contexts
                .iter()
                .map(kind_context)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    #[test]
    fn source_pipeline_reports_all_missing_names() {
        let input = "module Main exposing (..)\nfirst = missing\nsecond = absent\n";
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(input);
        let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());
        let module = parser.module().expect("parse");
        let errors = nash_can::canonicalize(&bump, nash_can::Context::default(), &module)
            .expect_err("canonical errors");
        assert_eq!(errors.len(), 2);
        let source = Source::new(input);
        insta::assert_snapshot!(
            errors
                .iter()
                .map(|e| crate::render_plain(&to_report(&source, e), &source, "Main.nash"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    #[test]
    fn bad_impl_head_reasons() {
        for (name, reason) in [
            ("bad_head_function", nash_can::BadHead::Function),
            ("bad_head_record", nash_can::BadHead::Record),
            (
                "bad_head_variable_application",
                nash_can::BadHead::VariableApplication,
            ),
        ] {
            snapshot(
                name,
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
        let arg = Located::at(r(), nash_ast::Type::Var("a"));
        for (name, reason) in [
            ("superclass_cycle", nash_can::EntailmentFailure::Cycle),
            ("superclass_limit", nash_can::EntailmentFailure::Limit),
        ] {
            snapshot(
                name,
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
        for (name, context) in [
            ("duplicate_lambda", DuplicatePatternContext::LambdaArgs),
            (
                "duplicate_function",
                DuplicatePatternContext::FuncArgs("map"),
            ),
            ("duplicate_let", DuplicatePatternContext::LetBinding),
            ("duplicate_destruct", DuplicatePatternContext::Destruct),
        ] {
            snapshot(
                name,
                Error::DuplicatePattern {
                    context,
                    name: "x",
                    first: r(),
                    second: r(),
                },
            );
        }
    }
    #[test]
    fn export_suggestion_counts() {
        snapshot(
            "export_no_suggestions",
            Error::ExportNotFound {
                region: r(),
                kind: VarKind::BadType,
                name: "Box",
                suggestions: &[],
            },
        );
        snapshot(
            "export_multiple_suggestions",
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
        let mut parser = nash_parse::Parser::new(&bump, src.as_bytes());
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
        assert!(
            matches!(&report.snippet, Snippet::Pair { first: a, second: b } if a.region == *first && b.region == *second)
        );
        let rendered = crate::render_plain(&report, &source, "Bad.nash");
        assert!(!rendered.contains("Rename"));
        assert!(rendered.contains("context constraints"));
        insta::assert_snapshot!(rendered);
    }
}
