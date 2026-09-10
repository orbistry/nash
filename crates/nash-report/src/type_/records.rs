use super::*;
use nash_constrain::type_::FieldContext;

fn highlighted(
    primary: Region,
    surroundings: Region,
    highlight: Region,
    before: Doc,
    after: Doc,
) -> Report {
    let mut report = Report::snippet(
        "TYPE MISMATCH",
        surroundings,
        Some(highlight),
        before,
        after,
    );
    report.region = primary;
    report
}

// Keep the regions explicit: the diagnostic points to the expression, while
// Elm highlights the missing field or the non-record value inside its context.
#[allow(clippy::too_many_arguments)]
pub(super) fn access(
    l: &Localizer,
    primary: Region,
    surroundings: Region,
    record_region: Region,
    name: Option<&str>,
    field_region: Region,
    field: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Report {
    match iterated_dealias(actual) {
        ErrorType::Record { fields } => {
            let (after, suggestions) = nearby(l, name, field, fields);
            highlighted(
                primary,
                surroundings,
                field_region,
                Doc::reflow(&format!(
                    "This {}record does not have a `{field}` field:",
                    name.map_or_else(String::new, |name| format!("`{name}` "))
                )),
                after,
            )
            .with_suggestions(suggestions)
        }
        _ => highlighted(
            primary,
            surroundings,
            record_region,
            Doc::reflow("This is not a record, so it has no fields to access!"),
            lone_type(
                l,
                actual,
                expected,
                Doc::reflow("It is:"),
                vec![Doc::reflow(&format!(
                    "But I need a record with a `{field}` field!"
                ))],
            ),
        ),
    }
}

pub(super) fn update(
    l: &Localizer,
    primary: Region,
    surroundings: Region,
    name: &str,
    updates: &[nash_ast::FieldUpdate<'_>],
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Report {
    match iterated_dealias(actual) {
        ErrorType::Record { fields } => {
            let missing = updates
                .iter()
                .filter(|update| !fields.iter().any(|(name, _)| *name == update.field.value))
                .min_by_key(|update| update.field.value);
            match missing {
                Some(update) => {
                    let field = update.field.value;
                    let (after, suggestions) = nearby(l, Some(name), field, fields);
                    highlighted(
                        primary,
                        surroundings,
                        update.field.region,
                        Doc::reflow(&format!(
                            "The `{name}` record does not have a `{field}` field:"
                        )),
                        after,
                    )
                    .with_suggestions(suggestions)
                }
                None => {
                    let mut report = Report::snippet(
                        "TYPE MISMATCH",
                        surroundings,
                        None,
                        Doc::reflow("Something is off with this record update:"),
                        type_comparison(
                            l,
                            actual,
                            expected,
                            &format!("The `{name}` record is:"),
                            "But this update needs it to be compatible with:",
                            vec![Doc::to_simple_hint(
                                "Add a type annotation to the record and check the types of the fields being updated.",
                            )],
                        ),
                    );
                    report.region = primary;
                    report
                }
            }
        }
        _ => highlighted(
            primary,
            surroundings,
            primary,
            Doc::reflow("This is not a record, so it has no fields to update!"),
            lone_type(
                l,
                actual,
                expected,
                Doc::reflow("It is:"),
                vec![Doc::reflow("But I need a record!")],
            ),
        ),
    }
}

fn nearby(
    l: &Localizer,
    name: Option<&str>,
    field: &str,
    fields: &[(&str, &ErrorType<'_>)],
) -> (Doc, Vec<String>) {
    let sorted = crate::suggest::sort(field, |(name, _)| (*name).into(), fields.to_vec());
    match sorted.split_first() {
        None => (
            Doc::reflow(&format!(
                "In fact, {} is a record with NO fields!",
                name.map_or_else(|| "it".into(), |name| format!("`{name}`"))
            )),
            vec![],
        ),
        Some((first, rest)) => (
            Doc::stack([
                Doc::reflow(&format!(
                    "This is usually a typo. Here are the {}fields that are most similar:",
                    name.map_or_else(String::new, |name| format!("`{name}` "))
                )),
                to_nearby_record(l, *first, rest),
                Doc::fill_sep([
                    Doc::text("So maybe"),
                    Doc::text(field).dullyellow(),
                    Doc::text("should be"),
                    Doc::cat([Doc::text(first.0).green(), Doc::text("?")]),
                ]),
            ]),
            sorted
                .iter()
                .take(4)
                .map(|(field, _)| (*field).into())
                .collect(),
        ),
    }
}
fn to_nearby_record(
    l: &Localizer,
    first: (&str, &ErrorType<'_>),
    rest: &[(&str, &ErrorType<'_>)],
) -> Doc {
    Doc::indent(
        4,
        if rest.len() <= 3 {
            crate::render_type::vrecord(
                std::iter::once(first)
                    .chain(rest.iter().copied())
                    .map(|field| field_to_docs(l, field))
                    .collect(),
                None,
            )
        } else {
            crate::render_type::vrecord_snippet(
                field_to_docs(l, first),
                rest.iter()
                    .take(3)
                    .map(|field| field_to_docs(l, *field))
                    .collect(),
            )
        },
    )
}
fn field_to_docs(l: &Localizer, (field, tipe): (&str, &ErrorType<'_>)) -> (Doc, Doc) {
    (Doc::text(field), type_diff::to_doc(l, Ctx::None, tipe))
}

fn field_context(context: FieldContext<'_>, field: Option<&str>) -> String {
    let field = field.map_or_else(
        || "these fields".into(),
        |field| format!("the `{field}` field"),
    );
    match context {
        FieldContext::Access {
            maybe_name: Some(name),
            ..
        } => format!("accessing {field} of `{name}`"),
        FieldContext::Access {
            maybe_name: None, ..
        } => format!("accessing {field} of this value"),
        FieldContext::Accessor => format!("using the field access function for {field}"),
        FieldContext::Update { record } => format!("updating {field} of `{record}`"),
        FieldContext::Pattern => format!("matching {field} in this record pattern"),
    }
}

pub(super) fn field_mismatch(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: &str,
    actual: &ErrorType<'_>,
    expected: &ErrorType<'_>,
) -> Report {
    if matches!(context, FieldContext::Update { .. }) {
        return Report::snippet(
            "TYPE MISMATCH",
            region,
            None,
            Doc::reflow(&format!("I cannot update the `{field}` field like this:")),
            type_comparison(
                l,
                expected,
                actual,
                &format!("You are trying to update `{field}` to be:"),
                "But it should be:",
                vec![Doc::to_simple_note(
                    "The record update syntax does not allow you to change the type of fields. You can achieve that with record constructors or the record literal syntax.",
                )],
            ),
        );
    }
    let before = format!(
        "Something is off when {}:",
        field_context(context, Some(field))
    );
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::reflow(&before),
        type_comparison(
            l,
            actual,
            expected,
            &format!("The `{field}` field is:"),
            "But it needs to be:",
            vec![],
        ),
    )
}

pub(super) fn missing_field(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: &str,
    record: &ErrorType<'_>,
    available: &[&str],
) -> Report {
    let name = match context {
        FieldContext::Access { maybe_name, .. } => maybe_name,
        FieldContext::Update { record } => Some(record),
        FieldContext::Accessor | FieldContext::Pattern => None,
    };
    let (after, suggestions) = match iterated_dealias(record) {
        ErrorType::Record { fields } => nearby(l, name, field, fields),
        _ => {
            let sorted =
                crate::suggest::sort(field, |name| (*name).to_string(), available.to_vec());
            let mut docs = vec![
                Doc::reflow("This value has type:"),
                Doc::indent(4, type_diff::to_doc(l, Ctx::None, record)),
            ];
            if sorted.is_empty() {
                docs.push(Doc::reflow("It has no available record fields."));
            } else {
                docs.push(Doc::reflow(&format!(
                    "The available fields most similar to `{field}` are: {}.",
                    sorted
                        .iter()
                        .take(4)
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            (
                Doc::stack(docs),
                sorted.into_iter().take(4).map(String::from).collect(),
            )
        }
    };
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::reflow(&format!(
            "I cannot find `{field}` when {}:",
            field_context(context, Some(field))
        )),
        after,
    )
    .with_suggestions(suggestions)
}

pub(super) fn not_a_record(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: Option<&str>,
    record: &ErrorType<'_>,
) -> Report {
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::reflow(&format!(
            "This value is not a record, so I cannot use it when {}:",
            field_context(context, field)
        )),
        Doc::stack([
            Doc::reflow("It has type:"),
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, record)),
            Doc::reflow("But I need a value with record fields!"),
        ]),
    )
}
pub(super) fn update_not_record(l: &Localizer, region: Region, record: &ErrorType<'_>) -> Report {
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::reflow("This value does not support record updates:"),
        Doc::stack([
            Doc::reflow("It has type:"),
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, record)),
            Doc::reflow(
                "I need a record alias for this update. Rebuild this value with its constructor instead.",
            ),
        ]),
    )
}
pub(super) fn ambiguous_access(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: Option<&str>,
    record: &ErrorType<'_>,
) -> Report {
    Report::snippet(
        "AMBIGUOUS RECORD ACCESS",
        region,
        None,
        Doc::reflow(&format!(
            "I cannot determine the record type when {}:",
            field_context(context, field)
        )),
        Doc::stack([
            Doc::reflow("The type is still:"),
            Doc::indent(4, type_diff::to_doc(l, Ctx::None, record)),
            Doc::to_simple_hint(
                "Add a type annotation to tell me which record type provides these fields.",
            ),
        ]),
    )
}
