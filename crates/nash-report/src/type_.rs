//! Type error prose from Elm's `Reporting/Error/Type.hs`, adapted to Nash.

mod operators;
mod records;
#[cfg(test)]
mod tests;
mod traits;

use nash_constrain::error::{
    Category, Context, Error, Expected, MaybeName, PCategory, PContext, PExpected, SubContext,
};
use nash_constrain::error_type::{ErrorType, iterated_dealias};
use nash_region::Region;

use crate::doc::args;
use crate::localizer::Localizer;
use crate::render_type::Ctx;
use crate::type_diff::{self, Direction, Problem};
use crate::{Doc, Report};

/// Elm's `toReport`, including Nash's trait, representation and record errors.
pub fn to_report(localizer: &Localizer, error: &Error<'_>) -> Report {
    let report = match error {
        Error::InvalidCall { region, reason } => Report::snippet(
            "INVALID CALL",
            *region,
            None,
            Doc::reflow(reason),
            Doc::reflow("Use the callable's declared argument labels and arity."),
        ),
        Error::PrivateTypeLeak {
            region,
            declaration,
            name,
        } => {
            let mut report = Report::snippet(
                "PRIVATE TYPE IN PUBLIC SIGNATURE",
                *region,
                None,
                Doc::reflow(&format!(
                    "This public signature exposes the private type `{name}`."
                )),
                Doc::reflow("Make the type public or keep the declaration private."),
            );
            if let Some(region) = declaration {
                report.labels.push(crate::Label {
                    region: *region,
                    text: "private type declared here".into(),
                });
            }
            report
        }
        Error::PolymorphicModuleConstant { region, typ } => Report::snippet(
            "POLYMORPHIC MODULE CONSTANT",
            *region,
            None,
            Doc::stack([
                Doc::reflow(
                    "A function stored in a module constant must have a fully concrete type:",
                ),
                type_diff::to_doc(localizer, Ctx::None, typ),
            ]),
            Doc::reflow("Annotate its parameters and result, or declare a named function instead."),
        ),
        Error::InvalidTupleIndex { region, index, typ } => Report::snippet(
            "INVALID TUPLE INDEX",
            *region,
            None,
            Doc::stack([
                Doc::reflow(&format!(
                    "Element {} is not available on this type:",
                    index + 1
                )),
                type_diff::to_doc(localizer, Ctx::None, typ),
            ]),
            Doc::reflow(
                "The operand must have a known tuple or pair type and the index must be in range.",
            ),
        ),
        Error::InvalidDataCast {
            region,
            reason,
            typ,
        } => Report::snippet(
            "INVALID DATA CONVERSION",
            *region,
            None,
            Doc::stack([
                Doc::reflow(reason),
                type_diff::to_doc(localizer, Ctx::None, typ),
            ]),
            Doc::reflow("Use a concrete serializable type at this conversion boundary."),
        ),
        Error::InvalidRunnable {
            region,
            message,
            typ,
        } => Report::snippet(
            "INVALID RUNNABLE",
            *region,
            None,
            Doc::stack([
                Doc::reflow(message),
                type_diff::to_doc(localizer, Ctx::None, typ),
            ]),
            Doc::reflow(
                "Tests and benchmarks are checked without executing their generators or bodies.",
            ),
        ),
        Error::IllegalDataType { region, typ } => Report::snippet(
            "ILLEGAL DATA TYPE",
            *region,
            None,
            Doc::stack([
                Doc::reflow("This value contains a type that cannot be stored in Data:"),
                type_diff::to_doc(localizer, Ctx::None, typ),
            ]),
            Doc::reflow(
                "Functions and Miller-loop results cannot occur inside lists, pairs, tuples, or other Data values.",
            ),
        ),
        Error::IllegalComparison { region, typ } | Error::IllegalTraceArgument { region, typ } => {
            let comparison = matches!(error, Error::IllegalComparison { .. });
            Report::snippet(
                if comparison {
                    "ILLEGAL COMPARISON"
                } else {
                    "ILLEGAL TRACE ARGUMENT"
                },
                *region,
                None,
                Doc::stack([
                    Doc::reflow(if comparison {
                        "Equality is not defined for this type:"
                    } else {
                        "This type cannot be rendered in a trace:"
                    }),
                    type_diff::to_doc(localizer, Ctx::None, typ),
                ]),
                Doc::reflow(
                    "Functions and Miller-loop results cannot be compared or rendered as Data, including when nested in another type.",
                ),
            )
        }
        Error::MainParameterIsTerm { region, index, typ } => Report::snippet(
            "BAD MAIN PARAMETER",
            *region,
            None,
            Doc::stack([
                Doc::reflow(&format!("Parameter {} of main has a Term type:", index + 1)),
                crate::render_type::can_to_doc(localizer, Ctx::None, &typ.value),
            ]),
            Doc::reflow(
                "A script can only receive constants. Use Data, a Big type, or a Const type for this parameter, and convert it inside main.",
            ),
        ),
        Error::BadExpr(region, category, actual, expected) => {
            to_expr_report(localizer, *region, *category, actual, expected)
        }
        Error::BadPattern(region, category, actual, expected) => {
            to_pattern_report(localizer, *region, *category, actual, expected)
        }
        Error::InfiniteType {
            region,
            name,
            overall_type,
        } => to_infinite_report(localizer, *region, name, overall_type),
        Error::FieldMismatch {
            region,
            context,
            field,
            actual,
            expected,
        } => records::field_mismatch(localizer, *region, *context, field, actual, expected),
        Error::MissingField {
            region,
            context,
            field,
            record,
            available,
        } => records::missing_field(localizer, *region, *context, field, record, available),
        Error::NotARecord {
            region,
            context,
            field,
            record,
        } => records::not_a_record(localizer, *region, *context, *field, record),
        Error::UpdateNotRecord { region, record } => {
            records::update_not_record(localizer, *region, record)
        }
        Error::AmbiguousRecordAccess {
            region,
            context,
            field,
            record,
        } => records::ambiguous_access(localizer, *region, *context, *field, record),
        Error::BadKind {
            region,
            name,
            args,
            reason,
        } => traits::bad_kind(localizer, *region, name, args, reason),
        Error::AmbiguousType {
            region,
            name,
            variable,
            predicates,
        } => traits::ambiguous_type(localizer, *region, name, variable, predicates),
        Error::ContradictoryRepresentation {
            region,
            name,
            typ,
            requirements,
        } => traits::contradictory_representation(localizer, *region, name, typ, requirements),
        Error::PolymorphicRecursion {
            region,
            name,
            trait_,
            args,
        } => traits::polymorphic_recursion(localizer, *region, name, *trait_, args),
        Error::UnresolvedConstraint {
            region,
            name,
            trait_,
            args,
        } => traits::unresolved_constraint(localizer, *region, name, *trait_, args),
        Error::UnresolvedApplication {
            region,
            name,
            head,
            args,
        } => traits::unresolved_application(localizer, *region, name, head, args),
        Error::MissingImpl {
            region,
            name,
            trait_,
            args,
            available,
            because,
        } => traits::missing_impl(localizer, *region, name, *trait_, args, available, because),
        Error::ImplResolutionLimit {
            region,
            name,
            trait_,
        } => traits::resolution_limit(localizer, *region, name, *trait_),
        Error::MissingConstraint {
            region,
            name,
            trait_,
            args,
            binder,
        } => traits::missing_constraint(localizer, *region, name, *trait_, args, binder),
        Error::AnnotationVariableEscapes {
            region,
            name,
            variable,
        } => traits::annotation_variable_escapes(localizer, *region, *name, variable),
    };
    let mut report = report;
    let origin = match error {
        Error::BadExpr(_, _, _, Expected::FromAnnotation(_, region, _, _, _)) => {
            Some((*region, "declared type"))
        }
        Error::BadExpr(
            _,
            _,
            _,
            Expected::FromContext(_, Context::ListEntry(_, Some(region)), _),
        ) => Some((*region, "previous list element")),
        Error::BadExpr(
            _,
            _,
            _,
            Expected::FromContext(
                _,
                Context::IfBranch(_, Some(region)) | Context::CaseBranch(_, Some(region)),
                _,
            ),
        ) => Some((*region, "previous branch")),
        Error::BadPattern(
            _,
            _,
            _,
            PExpected::FromContext(_, PContext::TypedArg(_, _, region), _),
        ) => Some((*region, "declared argument type")),
        _ => None,
    };
    if let Some((region, text)) = origin.filter(|(region, _)| *region != report.region) {
        report.labels.push(crate::Label {
            region,
            text: text.into(),
        });
    }
    report.with_code(match error {
        Error::InvalidCall { .. } => "nash::type::invalid_call",
        Error::PrivateTypeLeak { .. } => "nash::type::private_type_leak",
        Error::PolymorphicModuleConstant { .. } => "nash::type::polymorphic_module_constant",
        Error::InvalidTupleIndex { .. } => "nash::type::invalid_tuple_index",
        Error::InvalidDataCast { .. } => "nash::type::invalid_data_cast",
        Error::InvalidRunnable { .. } => "nash::type::invalid_runnable",
        Error::IllegalComparison { .. } => "nash::type::illegal_comparison",
        Error::IllegalTraceArgument { .. } => "nash::type::illegal_trace_argument",
        Error::IllegalDataType { .. } => "nash::type::illegal_data_type",
        Error::MainParameterIsTerm { .. } => "nash::validator::main_parameter_is_term",
        Error::FieldMismatch { .. } => "nash::type::field_mismatch",
        Error::MissingField { .. } => "nash::type::missing_field",
        Error::NotARecord { .. } => "nash::type::not_a_record",
        Error::UpdateNotRecord { .. } => "nash::type::update_not_record",
        Error::AmbiguousRecordAccess { .. } => "nash::type::ambiguous_record_access",
        Error::BadKind { .. } => "nash::type::bad_kind",
        Error::AmbiguousType { .. } => "nash::type::ambiguous_type",
        Error::ContradictoryRepresentation { .. } => "nash::type::contradictory_representation",
        Error::PolymorphicRecursion { .. } => "nash::type::polymorphic_recursion",
        Error::UnresolvedConstraint { .. } => "nash::type::unresolved_constraint",
        Error::UnresolvedApplication { .. } => "nash::type::unresolved_application",
        Error::MissingImpl { .. } => "nash::type::missing_impl",
        Error::ImplResolutionLimit { .. } => "nash::type::impl_resolution_limit",
        Error::MissingConstraint { .. } => "nash::type::missing_constraint",
        Error::AnnotationVariableEscapes { .. } => "nash::type::annotation_variable_escapes",
        Error::BadExpr(..) => "nash::type::mismatch",
        Error::BadPattern(..) => "nash::type::pattern_mismatch",
        Error::InfiniteType { .. } => "nash::type::infinite_type",
    })
}

fn to_pattern_report(
    localizer: &Localizer,
    region: Region,
    category: PCategory<'_>,
    actual: &ErrorType<'_>,
    expected: &PExpected<'_, &ErrorType<'_>>,
) -> Report {
    let (surroundings, label, expected) = match expected {
        PExpected::NoExpectation(expected) => (region, pattern_label(category), *expected),
        PExpected::FromContext(surroundings, context, expected) => {
            let label = match context {
                PContext::TypedArg(name, index, _) | PContext::CtorArg(name, index) => {
                    format!("argument {} of `{name}`", index + 1)
                }
                PContext::CaseMatch(index) => format!("case pattern {}", index + 1),
                PContext::ListEntry(index) => format!("list pattern {}", index + 1),
                PContext::Tail => "list tail pattern".into(),
            };
            (*surroundings, label, *expected)
        }
    };
    mismatch_report(localizer, region, label, actual, expected, None).with_region(surroundings)
}

fn pattern_label(category: PCategory<'_>) -> String {
    match category {
        PCategory::Record => "record pattern".into(),
        PCategory::Unit => "unit pattern".into(),
        PCategory::Tuple => "tuple pattern".into(),
        PCategory::List => "list pattern".into(),
        PCategory::Ctor(name) => format!("`{name}` pattern"),
        PCategory::Int => "integer pattern".into(),
        PCategory::Bytes => "bytes pattern".into(),
        PCategory::Str => "string pattern".into(),
        PCategory::Bool => "boolean pattern".into(),
    }
}

/// Keep the full structural comparison; only differing parts receive emphasis.
fn comparison_docs(
    localizer: &Localizer,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> (Doc, Option<Doc>) {
    let (actual, expected, problems) = type_diff::to_comparison(localizer, actual, expected);
    let comparison = Doc::Group(Box::new(Doc::cat([
        Doc::text("Type mismatch: expected `"),
        expected,
        Doc::text("`,"),
        Doc::Line,
        Doc::text("found `"),
        actual,
        Doc::text("`."),
    ])));
    (comparison, problems_to_hint(&problems).into_iter().next())
}

fn mismatch_report(
    localizer: &Localizer,
    region: Region,
    label: String,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
    hint: Option<Doc>,
) -> Report {
    let (message, specific_hint) = comparison_docs(localizer, actual, expected);
    let mut report = Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        message,
        specific_hint.or(hint).unwrap_or(Doc::Empty),
    );
    report.primary_label = Some(label);
    report
}

fn category_label(category: Category<'_>) -> String {
    match category {
        Category::Local(name) | Category::Foreign(name) => format!("`{name}`"),
        Category::Access(field) => format!("field `{field}`"),
        Category::Accessor(field) => format!("accessor `.{field}`"),
        Category::If => "if expression".into(),
        Category::Case => "case expression".into(),
        Category::List => "list".into(),
        Category::String => "string".into(),
        Category::Lambda => "function".into(),
        Category::Record => "record".into(),
        Category::Tuple => "tuple".into(),
        Category::Unit => "unit".into(),
        Category::CallResult(name) => format!("result of {}", function_name(name)),
    }
}

fn function_name(name: MaybeName<'_>) -> String {
    match name {
        MaybeName::NoName => "this function".into(),
        MaybeName::FuncName(name) | MaybeName::CtorName(name) => format!("`{name}`"),
        MaybeName::OpName(op) => format!("({op})"),
    }
}

fn to_expr_report(
    localizer: &Localizer,
    region: Region,
    category: Category<'_>,
    actual: &ErrorType<'_>,
    expected: &Expected<'_, &ErrorType<'_>>,
) -> Report {
    let (surroundings, label, expected, hint) = match expected {
        Expected::NoExpectation(expected) => (region, category_label(category), *expected, None),
        Expected::FromAnnotation(name, _, _, context, expected) => {
            let label = match context {
                SubContext::TypedIfBranch(index) => format!("if branch {}", index + 1),
                SubContext::TypedCaseBranch(index) => format!("case branch {}", index + 1),
                SubContext::TypedBody => format!("body of `{name}`"),
            };
            (region, label, *expected, None)
        }
        Expected::FromContext(surroundings, context, expected) => {
            let mut hint = None;
            let label = match context {
                Context::ListEntry(index, _) => format!("list element {}", index + 1),
                Context::IfCondition => "if condition".into(),
                Context::IfBranch(index, _) => format!("if branch {}", index + 1),
                Context::CaseBranch(index, _) => format!("case branch {}", index + 1),
                Context::CallArg(name, index) => {
                    format!("argument {} of {}", index + 1, function_name(*name))
                }
                Context::CallArity(name, given) => {
                    let count = count_args(actual);
                    let function = function_name(*name);
                    let message = if count == 0 {
                        format!("{function} is not a function; received {}.", args(*given))
                    } else {
                        format!("{function} expects {}, but received {given}.", args(count))
                    };
                    return Report::snippet(
                        "TOO MANY ARGS",
                        region,
                        None,
                        Doc::reflow(&message),
                        Doc::Empty,
                    )
                    .with_region(*surroundings);
                }
                Context::OpLeft(op) | Context::OpRight(op) => {
                    hint = operators::hint(
                        op,
                        matches!(context, Context::OpRight(_)),
                        actual,
                        expected,
                    );
                    let side = if matches!(context, Context::OpLeft(_)) {
                        "left"
                    } else {
                        "right"
                    };
                    format!("{side} operand of ({op})")
                }
                Context::RecordAccess {
                    record_region,
                    maybe_name,
                    field_region,
                    field,
                } => {
                    return records::access(
                        localizer,
                        region,
                        *surroundings,
                        *record_region,
                        *maybe_name,
                        *field_region,
                        field,
                        actual,
                        expected,
                    );
                }
                Context::RecordUpdateKeys(name, fields) => {
                    return records::update(
                        localizer,
                        region,
                        *surroundings,
                        name,
                        fields,
                        actual,
                        expected,
                    );
                }
                Context::RecordUpdateValue(field) => {
                    hint = Some(Doc::text("Rebuild the record to change a field's type."));
                    format!("update of `{field}`")
                }
                Context::RecordField(name, field) => format!("field `{field}` of `{name}`"),
                Context::Destructure => "destructuring pattern".into(),
            };
            (*surroundings, label, *expected, hint)
        }
    };
    mismatch_report(localizer, region, label, actual, expected, hint).with_region(surroundings)
}

fn count_args(tipe: &ErrorType<'_>) -> usize {
    match iterated_dealias(tipe) {
        ErrorType::Lambda(_, _, rest) => 1 + rest.len(),
        _ => 0,
    }
}

fn to_infinite_report(
    localizer: &Localizer,
    region: Region,
    name: &str,
    overall_type: &ErrorType<'_>,
) -> Report {
    Report::snippet(
        "INFINITE TYPE",
        region,
        None,
        Doc::cat([
            Doc::text(format!("`{name}` has a self-referential type: ")),
            type_diff::to_doc(localizer, Ctx::None, overall_type).dullyellow(),
        ]),
        Doc::text("Break the type cycle; ∞ marks a repeated part of the type."),
    )
}

fn problems_to_hint(problems: &[Problem<'_>]) -> Vec<Doc> {
    problems.first().map_or_else(Vec::new, problem_to_hint)
}

fn problem_to_hint(problem: &Problem<'_>) -> Vec<Doc> {
    let hint = match problem {
        Problem::AnythingToBool => "Use an explicit comparison to produce a `bool`.".into(),
        Problem::AnythingFromOption => "Use `case` to handle both option variants.".into(),
        Problem::ArityMismatch(actual, expected) => {
            format!("Check the function's argument count: expected {expected}, found {actual}.")
        }
        Problem::BadRigidVar(name, typ) => match typ {
            ErrorType::Infinite | ErrorType::Error | ErrorType::FlexVar(_) => return vec![],
            ErrorType::RigidVar(other) => format!(
                "The annotation keeps `'{name}` and `'{other}` independent; use one variable if they must be equal."
            ),
            _ => format!(
                "Keep the implementation generic over `'{name}`, or specialize its annotation."
            ),
        },
        Problem::FieldsMissing(fields) => {
            if fields.is_empty() {
                return vec![];
            }
            format!(
                "Add the missing fields: {}.",
                fields
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Problem::FieldTypo(typo, possibilities) => {
            let ranked = crate::suggest::sort(typo, |s| (*s).to_string(), possibilities.clone());
            let Some(nearest) = ranked.first() else {
                return vec![];
            };
            format!("Replace field `{typo}` with `{nearest}`.")
        }
        Problem::BigLittle {
            big,
            little,
            direction,
        } => match direction {
            Direction::Have => format!(
                "Use `lower` to convert `{big}` to `{little}` where a `Lift` impl is available."
            ),
            Direction::Need => format!(
                "Use `lift` to convert `{little}` to `{big}` where a `Lift` impl is available."
            ),
        },
    };
    vec![Doc::text(hint)]
}
