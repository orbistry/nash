//! Nash-specific inference, trait, kind and representation diagnostics.
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
fn report(
    title: &str,
    region: Region,
    before: String,
    docs: impl IntoIterator<Item = Doc>,
) -> Report {
    Report::snippet(title, region, None, Doc::reflow(&before), Doc::stack(docs))
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
    let mut docs = vec![
        Doc::reflow("The type arguments at this use are:"),
        Doc::indent(4, type_docs(l, args)),
    ];
    let (title, before) = match reason {
        KindProblem::Infinite => {
            docs.push(Doc::reflow("A type constructor cannot be applied to itself in this way: its kind would have to contain itself forever."));
            (
                "INFINITE KIND",
                format!("The use of `{name}` would require an infinite kind:"),
            )
        }
        KindProblem::Mismatch { expected, actual } => {
            docs.extend([Doc::reflow("I need kind:"), Doc::indent(4, Doc::text(kind(expected)).dullyellow()), Doc::reflow("But this use has kind:"), Doc::indent(4, Doc::text(kind(actual)).dullyellow()), Doc::to_simple_hint("A type constructor needs all of its required type arguments before it can be used as a value type. An annotation's quantified kinds cannot be specialized by its body.")]);
            (
                "KIND MISMATCH",
                format!("The type arguments to `{name}` have incompatible kinds:"),
            )
        }
    };
    report(title, region, before, docs)
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
        format!("I cannot determine the type needed by `{name}`:"),
        [
            Doc::reflow("This type variable is still unresolved:"),
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, variable)),
            Doc::reflow("It must satisfy these constraints:"),
            Doc::indent(
                4,
                Doc::vcat(predicates.iter().map(|p| predicate(l, p.trait_, p.args))),
            ),
            Doc::to_simple_hint(
                "Add a type annotation that fixes this type. Each constraint needs enough information to select an impl.",
            ),
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
        format!("`{name}` requires incompatible representations:"),
        [
            Doc::reflow(
                "This type is required to satisfy all of the following representation constraints:",
            ),
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, typ)),
            Doc::indent(
                4,
                Doc::text(
                    requirements
                        .iter()
                        .map(|r| r.name())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            ),
            Doc::reflow(
                "No type can satisfy all of them. Check where this value is used as Big Data and where a little builtin representation is required.",
            ),
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
        format!("The recursive use of `{name}` keeps changing its trait arguments:"),
        [
            Doc::reflow("The growing requirement is:"),
            Doc::indent(4, predicate(l, trait_, args)),
            Doc::reflow(
                "Each trip around this recursive call adds another impl wrapper. I cannot construct a finite set of evidence arguments for it.",
            ),
            Doc::to_simple_hint(
                "Keep the trait arguments the same across recursive calls, or split the work into functions with explicit type annotations.",
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
        format!("I could not establish the constraint required by `{name}`:"),
        [
            Doc::indent(4, predicate(l, trait_, args)),
            Doc::reflow("Type inference finished without a proof for this requirement."),
            Doc::to_simple_hint(
                "Add a type annotation to resolve the remaining type variables, then check that the required impl or annotation constraint is available.",
            ),
        ],
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
        format!("I could not establish the datatype context required by `{name}`:"),
        [
            Doc::reflow("The unresolved type application is:"),
            Doc::indent(
                4,
                Doc::hsep(
                    std::iter::once(type_diff::to_doc(l, Ctx::App, head))
                        .chain(args.iter().map(|ty| type_diff::to_doc(l, Ctx::App, ty))),
                ),
            ),
            Doc::to_simple_hint(
                "Add a type annotation that determines the type constructor and its arguments. Its datatype constraints must be satisfied before this value can be used.",
            ),
        ],
    )
}

pub(super) fn resolution_limit(
    l: &Localizer,
    region: Region,
    name: &str,
    trait_: QualifiedName<'_>,
) -> Report {
    report(
        "IMPL RESOLUTION LIMIT",
        region,
        format!("I reached the impl resolution limit while checking `{name}`:"),
        [
            Doc::reflow(&format!(
                "The search for `{}` evidence exceeded the compiler's work limit.",
                l.to_string(trait_.home, trait_.name)
            )),
            Doc::reflow(
                "This requirement has not been checked completely. The other diagnostics from this compilation still apply.",
            ),
            Doc::to_simple_hint(
                "Check for a cycle or a growing chain of impl constraints, and simplify the requirement before trying again.",
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
    let representation = ReprTrait::of(trait_).is_some();
    let mut result = report(
        "MISSING CONSTRAINT",
        region,
        format!(
            "`{name}` needs a constraint that the annotation for `{}` does not promise:",
            binder.value
        ),
        [
            Doc::indent(4, wanted.clone()),
            Doc::reflow(&format!(
                "The type variables in `{}` must work for every type allowed by its annotation. I cannot assume this {}constraint without it being declared.",
                binder.value,
                if representation {
                    "representation "
                } else {
                    ""
                }
            )),
            Doc::to_simple_hint(&format!(
                "Add `{}` to the context of the `{}` type annotation.",
                wanted.render(80, false),
                binder.value
            )),
        ],
    );
    result.snippet = crate::Snippet::Pair {
        first: crate::Label {
            region: binder.region,
            text: format!("annotation for `{}`", binder.value),
        },
        second: crate::Label {
            region,
            text: format!("needs `{}`", wanted.render(80, false)),
        },
    };
    result
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
        format!(
            "This annotation{} quantifies a type variable fixed by an enclosing scope:",
            name.map_or_else(String::new, |name| format!(" for `{name}`"))
        ),
        [
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, variable)),
            Doc::reflow(
                "The variable cannot stand for every type here because the surrounding definition has already fixed it.",
            ),
            Doc::to_simple_hint(
                "Use the enclosing type variable consistently, or change the annotation so that it does not promise a fresh independent type.",
            ),
        ],
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
        Head::Var(index) => (Doc::text(format!("'a{index}")), false),
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
    let trait_name = l.to_string(trait_.home, trait_.name);
    let args_doc = type_docs(l, args);
    let head = args_doc.render(80, false);
    let mut docs;
    let before;
    if let Some(representation) = ReprTrait::of(trait_) {
        before = format!("`{name}` requires a representation that this type does not provide:");
        let admitted = match representation {
            ReprTrait::Big => "Big",
            ReprTrait::Const => "Const",
            ReprTrait::Term => "Term",
            ReprTrait::Storable => "Big or Const",
            ReprTrait::Little => "Const or Term",
        };
        docs = vec![
            Doc::indent(4, args_doc),
            Doc::reflow(&format!(
                "`{}` accepts {admitted} representations. This argument does not meet that requirement.",
                representation.name()
            )),
            Doc::to_simple_hint(
                "Representation constraints are compiler-owned. Adding an impl cannot change a type's representation; change the datatype or convert the value explicitly.",
            ),
        ];
    } else {
        before = format!("I cannot find an `{trait_name}` impl for `{head}`:");
        let thing = if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii() && nash_parse::symbol::is_binop_char(c as u8))
        {
            format!("The ({name}) operator")
        } else {
            format!("`{name}`")
        };
        docs = vec![
            Doc::reflow(&format!(
                "{thing} needs its arguments to implement `{trait_name}`, and here they are:"
            )),
            Doc::indent(4, args_doc),
            Doc::reflow(&format!(
                "But there is no `impl {trait_name} {head}` in this module or in any import."
            )),
        ];
        if !available.is_empty() {
            docs.push(Doc::reflow(&format!(
                "`{trait_name}` is implemented for these heads:"
            )));
            docs.push(Doc::indent(
                4,
                Doc::vcat(
                    available
                        .iter()
                        .take(4)
                        .map(|heads| Doc::hsep(heads.iter().map(|head| head_doc(l, head, true)))),
                ),
            ));
        }
        docs.extend(derive_hint(l, trait_, args, &trait_name, &head));
    }
    if !because.is_empty() {
        docs.push(Doc::reflow("This requirement came from the following chain, from the original use to the failing requirement:"));
        docs.push(Doc::indent(
            4,
            Doc::vcat(because.iter().map(|reason| requirement(l, reason))),
        ));
    }
    report("MISSING IMPL", region, before, docs)
}

fn derive_hint(
    l: &Localizer,
    trait_: QualifiedName<'_>,
    args: &[&ErrorType<'_>],
    trait_name: &str,
    head: &str,
) -> Vec<Doc> {
    let core_trait =
        trait_.home.package == Some(nash_ast::primitives::CORE) && trait_.home.name == trait_.name;
    let derivable =
        core_trait && matches!(trait_.name, "Eq" | "Ord" | "Show" | "ToData" | "FromData");
    let local_union =
        matches!(args, [ErrorType::Type { home, name, .. }] if l.is_local_union(*home, name));
    let mut docs = Vec::new();
    if derivable && local_union {
        docs.push(Doc::to_simple_hint(&format!("This local datatype is a candidate for `@derive({trait_name})`, but automatic deriving is not available yet. Write the impl by hand:")));
    } else {
        docs.push(Doc::to_simple_hint(&format!(
            "Write an `impl {trait_name} {head}` that provides the trait's methods:"
        )));
    }
    let method = if core_trait && trait_.name == "Eq" {
        "    eq a b = ..."
    } else {
        "    ..."
    };
    docs.push(Doc::indent(
        4,
        Doc::vcat([
            Doc::text(format!("impl {trait_name} {head} where")),
            Doc::text(method),
        ]),
    ));
    docs
}
