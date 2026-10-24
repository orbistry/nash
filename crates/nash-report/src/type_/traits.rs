//! Concise trait, kind, and representation diagnostics.
use super::*;
use nash_ast::primitives::ReprTrait;
use nash_ast::{Head, Kind, QualifiedName};
use nash_constrain::error::{AmbiguousPredicate, KindProblem, Requirement};

fn type_docs(l: &Localizer, types: &[&ErrorType<'_>]) -> Doc {
    Doc::hsep(types.iter().map(|ty| type_diff::to_doc(l, Ctx::App, ty)))
}
fn predicate(l: &Localizer, trait_: QualifiedName<'_>, args: &[&ErrorType<'_>]) -> Doc {
    Doc::hsep(
        std::iter::once(l.to_doc(trait_.home, trait_.name))
            .chain(args.iter().map(|ty| type_diff::to_doc(l, Ctx::App, ty))),
    )
}
// `lead `predicate`tail`. When the sentence does not fit, the predicate moves
// to its own line, indented 4; a sentence that fits renders unchanged.
fn with_predicate(lead: String, predicate: Doc, tail: String) -> Doc {
    Doc::hang(
        4,
        Doc::sep([
            Doc::text(lead),
            Doc::cat([Doc::text("`"), predicate, Doc::text(format!("`{tail}"))]),
        ]),
    )
}
fn report(
    title: &str,
    region: Region,
    message: Doc,
    details: impl IntoIterator<Item = Doc>,
) -> Report {
    Report::snippet(title, region, None, message, Doc::stack(details))
}
fn kind(kind: &Kind<'_>) -> String {
    match kind {
        Kind::Type => "Type".into(),
        Kind::Arrow(arg, result) => format!(
            "{} -> {}",
            if matches!(arg, Kind::Arrow(..)) {
                format!("({})", self::kind(arg))
            } else {
                self::kind(arg)
            },
            self::kind(result)
        ),
    }
}

pub(super) fn bad_kind(
    l: &Localizer,
    region: Region,
    name: &str,
    args: &[&ErrorType<'_>],
    reason: &KindProblem<'_>,
) -> Report {
    let (title, message, hint) = match reason {
        KindProblem::Infinite => (
            "INFINITE KIND",
            format!("`{name}` requires a self-referential kind."),
            "Break the cycle in this type application.",
        ),
        KindProblem::Mismatch { expected, actual } => (
            "KIND MISMATCH",
            format!(
                "Kind mismatch in `{name}`: expected `{}`, found `{}`.",
                kind(expected),
                kind(actual)
            ),
            "Check type-constructor arguments and the annotation's quantified kinds.",
        ),
    };
    report(
        title,
        region,
        Doc::text(message),
        [
            Doc::cat([Doc::text("Type arguments: "), type_docs(l, args)]),
            Doc::text(hint),
        ],
    )
}

pub(super) fn ambiguous_type(
    l: &Localizer,
    region: Region,
    name: &str,
    variable: &ErrorType<'_>,
    predicates: &[AmbiguousPredicate<'_>],
) -> Report {
    report(
        "AMBIGUOUS TYPE",
        region,
        Doc::cat([
            Doc::text("Cannot determine type `"),
            type_diff::to_doc(l, Ctx::None, variable),
            Doc::text(format!("` in `{name}`.")),
        ]),
        [
            Doc::cat([
                Doc::text("Required constraints: "),
                Doc::hsep(predicates.iter().map(|p| predicate(l, p.trait_, p.args))),
            ]),
            Doc::text("Add a type annotation that determines this variable."),
        ],
    )
}

pub(super) fn contradictory_representation(
    l: &Localizer,
    region: Region,
    name: &str,
    typ: &ErrorType<'_>,
    requirements: &[ReprTrait],
) -> Report {
    report(
        "CONTRADICTORY REPRESENTATION",
        region,
        Doc::cat([
            Doc::text(format!(
                "`{name}` requires incompatible representations for `"
            )),
            type_diff::to_doc(l, Ctx::None, typ),
            Doc::text("`."),
        ]),
        [
            Doc::text(format!(
                "Required: {}.",
                requirements
                    .iter()
                    .map(|r| r.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Doc::text("Use a consistent representation or convert the value explicitly."),
        ],
    )
}

pub(super) fn polymorphic_recursion(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
    args: &[&ErrorType<'_>],
) -> Report {
    report(
        "POLYMORPHIC RECURSION",
        region,
        Doc::text(format!(
            "Recursive call to `{name}` grows its trait requirement."
        )),
        [
            predicate(l, trait_, args),
            Doc::text(
                "Keep trait arguments stable across recursive calls, or split the recursion.",
            ),
        ],
    )
}

pub(super) fn unresolved_constraint(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
    args: &[&ErrorType<'_>],
) -> Report {
    report(
        "UNRESOLVED CONSTRAINT",
        region,
        Doc::cat([
            Doc::text("Cannot establish `"),
            predicate(l, trait_, args),
            Doc::text(format!("` for `{name}`.")),
        ]),
        [Doc::text(
            "Specify the unresolved types and provide the required impl or annotation constraint.",
        )],
    )
}

pub(super) fn unresolved_application(
    l: &Localizer,
    region: Region,
    name: &str,
    head: &ErrorType<'_>,
    args: &[&ErrorType<'_>],
) -> Report {
    report(
        "UNRESOLVED TYPE APPLICATION",
        region,
        Doc::cat([
            Doc::text(format!(
                "Cannot establish the datatype context for `{name}`: "
            )),
            Doc::hsep(
                std::iter::once(type_diff::to_doc(l, Ctx::App, head))
                    .chain(args.iter().map(|ty| type_diff::to_doc(l, Ctx::App, ty))),
            ),
        ]),
        [Doc::text(
            "Add an annotation that determines the type constructor and its arguments.",
        )],
    )
}

pub(super) fn out_of_fuel(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
) -> Report {
    report(
        "COMPILATION STEP LIMIT",
        region,
        Doc::text(format!(
            "Resolving `{}` in `{name}` needed more compilation steps than the editor allows.",
            l.to_string(trait_.home, trait_.name)
        )),
        [
            Doc::text("This requirement remains unchecked; other reported errors still apply."),
            Doc::text(
                "`nash check` has no step limit. Simplify cyclic or growing impl constraints.",
            ),
        ],
    )
}

pub(super) fn missing_constraint(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
    args: &[&ErrorType<'_>],
    binder: &nash_region::Located<&str>,
) -> Report {
    let wanted = predicate(l, trait_, args);
    let mut report = report(
        "MISSING CONSTRAINT",
        region,
        with_predicate(
            format!("`{name}` requires undeclared constraint"),
            wanted.clone(),
            ".".into(),
        ),
        [with_predicate(
            "Add".into(),
            wanted,
            format!(" to the annotation for `{}`.", binder.value),
        )],
    );
    report.primary_label = Some("constraint required here".into());
    report.with_label(crate::Label {
        region: binder.region,
        text: format!("annotation for `{}`", binder.value),
    })
}

pub(super) fn annotation_variable_escapes(
    l: &Localizer,
    region: Region,
    name: Option<&str>,
    variable: &ErrorType<'_>,
) -> Report {
    report(
        "ANNOTATION VARIABLE ESCAPES",
        region,
        Doc::cat([
            Doc::text(format!(
                "Annotation{} quantifies type variable `",
                name.map_or_else(String::new, |name| format!(" for `{name}`"))
            )),
            type_diff::to_doc(l, Ctx::None, variable),
            Doc::text("`, already fixed by the enclosing scope."),
        ]),
        [Doc::text(
            "Use the enclosing variable without quantifying it again.",
        )],
    )
}

fn requirement(l: &Localizer, requirement: &Requirement<'_>) -> Doc {
    match requirement {
        Requirement::Trait { trait_, args } => predicate(l, *trait_, args),
        Requirement::Application { head, args } => Doc::hsep(
            std::iter::once(type_diff::to_doc(l, Ctx::App, head))
                .chain(args.iter().map(|ty| type_diff::to_doc(l, Ctx::App, ty))),
        ),
        Requirement::Formation(ty) => {
            Doc::hsep([Doc::text("forming"), type_diff::to_doc(l, Ctx::None, ty)])
        }
    }
}

fn head_doc(l: &Localizer, head: &Head<'_>, nested: bool) -> Doc {
    let (doc, parens) = match head {
        Head::Var { index, repr } => (
            Doc::text(if repr.is_all() {
                format!("'a{index}")
            } else {
                format!("('a{index} : {})", repr.name())
            }),
            false,
        ),
        Head::Named { reference, args } => (
            Doc::hsep(
                std::iter::once(l.to_doc(reference.home, reference.name))
                    .chain(args.iter().map(|head| head_doc(l, head, true))),
            ),
            !args.is_empty(),
        ),
        Head::Tuple(items) => (
            Doc::cat([
                Doc::text("("),
                Doc::hcat(items.iter().enumerate().map(|(index, head)| {
                    Doc::cat([
                        Doc::text(if index == 0 { "" } else { ", " }),
                        head_doc(l, head, false),
                    ])
                })),
                Doc::text(")"),
            ]),
            false,
        ),
        Head::Function(arg, result) => (
            Doc::hsep([
                head_doc(l, arg, true),
                Doc::text("->"),
                head_doc(l, result, false),
            ]),
            true,
        ),
    };
    if nested && parens {
        Doc::cat([Doc::text("("), doc, Doc::text(")")])
    } else {
        doc
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn missing_impl(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
    args: &[&ErrorType<'_>],
    available: &[&[Head<'_>]],
    because: &[Requirement<'_>],
) -> Report {
    let wanted = predicate(l, trait_, args);
    let (message, hint) = if let Some(representation) = ReprTrait::of(trait_) {
        let admitted = match representation {
            ReprTrait::Big => "Big",
            ReprTrait::Const => "Const",
            ReprTrait::Term => "Term",
            ReprTrait::Storable => "Big or Const",
            ReprTrait::Little => "Const or Term",
        };
        (
            with_predicate(
                "Unsatisfied representation constraint".into(),
                wanted,
                ".".into(),
            ),
            Doc::text(format!(
                "Use a type with {admitted} representation; an impl cannot change representation."
            )),
        )
    } else {
        (
            with_predicate("No impl for".into(), wanted.clone(), ".".into()),
            with_predicate("Import or define an impl for".into(), wanted, ".".into()),
        )
    };
    let mut details = Vec::new();
    if !available.is_empty() {
        details.push(Doc::text("Available impl heads:"));
        details.push(Doc::indent(
            4,
            Doc::vcat(
                available
                    .iter()
                    .take(4)
                    .map(|heads| Doc::hsep(heads.iter().map(|head| head_doc(l, head, true)))),
            ),
        ));
        if available.len() > 4 {
            details.push(Doc::text("…"));
        }
    }
    if !because.is_empty() {
        details.push(Doc::text("Requirement chain:"));
        details.push(Doc::indent(
            4,
            Doc::vcat(because.iter().map(|reason| requirement(l, reason))),
        ));
    }
    details.push(hint);
    let mut report = report("MISSING IMPL", region, message, details);
    report.primary_label = Some(if name.is_empty() {
        "required here".into()
    } else {
        format!("required by `{name}`")
    });
    report
}
