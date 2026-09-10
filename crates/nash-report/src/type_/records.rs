use super::*;
use nash_constrain::type_::FieldContext;

fn label_field(mut report: Report, field: Region) -> Report {
    if field != report.region {
        report.labels.push(crate::Label {
            region: field,
            text: "field used here".into(),
        });
    }
    report
}

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
    let report = match iterated_dealias(actual) {
        ErrorType::Record { fields } => {
            let (details, suggestions) = nearby(l, field, fields);
            Report::snippet(
                "TYPE MISMATCH",
                primary,
                None,
                Doc::text(format!(
                    "Record{} has no field `{field}`.",
                    name.map_or_else(String::new, |name| format!(" `{name}`"))
                )),
                details,
            )
            .with_suggestions(suggestions)
        }
        _ => mismatch_report(l, primary, "record receiver".into(), actual, expected, None),
    }
    .with_region(surroundings);
    let report = label_field(report, field_region);
    if record_region != primary && record_region != field_region {
        report.with_label(crate::Label {
            region: record_region,
            text: "record receiver".into(),
        })
    } else {
        report
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
    if let ErrorType::Record { fields } = iterated_dealias(actual)
        && let Some(update) = updates
            .iter()
            .filter(|update| !fields.iter().any(|(name, _)| *name == update.field.value))
            .min_by_key(|update| update.field.value)
    {
        let field = update.field.value;
        let (details, suggestions) = nearby(l, field, fields);
        return label_field(
            Report::snippet(
                "TYPE MISMATCH",
                primary,
                None,
                Doc::text(format!("Record `{name}` has no field `{field}`.")),
                details,
            )
            .with_suggestions(suggestions)
            .with_region(surroundings),
            update.field.region,
        );
    }
    mismatch_report(
        l,
        primary,
        format!("update of `{name}`"),
        actual,
        expected,
        Some(Doc::text("Rebuild the record to change a field's type.")),
    )
    .with_region(surroundings)
}

fn nearby(l: &Localizer, field: &str, fields: &[(&str, &ErrorType<'_>)]) -> (Doc, Vec<String>) {
    let sorted = crate::suggest::sort(field, |(name, _)| (*name).into(), fields.to_vec());
    let Some(first) = sorted.first() else {
        return (Doc::text("This record has no fields."), vec![]);
    };
    let fields_doc = crate::render_type::vrecord(
        sorted
            .iter()
            .take(4)
            .map(|(name, typ)| (Doc::text(*name), type_diff::to_doc(l, Ctx::None, typ)))
            .collect(),
        // A snippet must not imply this is the record's complete shape.
        (sorted.len() > 4).then(|| Doc::text("…")),
    );
    (
        Doc::stack([
            Doc::text(format!("Did you mean `{}`?", first.0)),
            Doc::cat([Doc::text("Available fields: "), fields_doc]),
        ]),
        sorted
            .iter()
            .take(4)
            .map(|(name, _)| (*name).into())
            .collect(),
    )
}

fn field_context(context: FieldContext<'_>, field: Option<&str>) -> String {
    let field = field.map_or_else(|| "fields".into(), |field| format!("field `{field}`"));
    match context {
        FieldContext::Access {
            maybe_name: Some(name),
            ..
        } => format!("{field} of `{name}`"),
        FieldContext::Access {
            maybe_name: None, ..
        } => format!("{field} access"),
        FieldContext::Accessor => format!("{field} accessor"),
        FieldContext::Update { record } => format!("{field} update of `{record}`"),
        FieldContext::Pattern => format!("{field} pattern"),
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
    let (actual, expected, hint) = if matches!(context, FieldContext::Update { .. }) {
        // The field solver compares the existing field against the new value.
        (
            expected,
            actual,
            Some(Doc::text("Rebuild the record to change a field's type.")),
        )
    } else {
        (actual, expected, None)
    };
    mismatch_report(
        l,
        region,
        field_context(context, Some(field)),
        actual,
        expected,
        hint,
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
    let (details, suggestions) = match iterated_dealias(record) {
        ErrorType::Record { fields } => nearby(l, field, fields),
        _ => {
            let sorted =
                crate::suggest::sort(field, |name| (*name).to_string(), available.to_vec());
            let mut docs = vec![Doc::cat([
                Doc::text("Record type: "),
                type_diff::to_doc(l, Ctx::None, record),
            ])];
            if let Some(nearest) = sorted.first() {
                docs.push(Doc::text(format!("Did you mean `{nearest}`?")));
                docs.push(Doc::text(format!(
                    "Available fields: {}{}.",
                    sorted
                        .iter()
                        .take(4)
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    if sorted.len() > 4 { ", …" } else { "" }
                )));
            }
            (
                Doc::stack(docs),
                sorted.into_iter().take(4).map(String::from).collect(),
            )
        }
    };
    let mut report = Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::text(format!("Record has no field `{field}`.")),
        details,
    )
    .with_suggestions(suggestions);
    report.primary_label = Some(field_context(context, Some(field)));
    report
}

pub(super) fn not_a_record(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: Option<&str>,
    record: &ErrorType<'_>,
) -> Report {
    let mut report = Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::cat([
            Doc::text("Expected a record, found `"),
            type_diff::to_doc(l, Ctx::None, record),
            Doc::text("`."),
        ]),
        Doc::Empty,
    );
    report.primary_label = Some(field_context(context, field));
    report
}

pub(super) fn update_not_record(l: &Localizer, region: Region, record: &ErrorType<'_>) -> Report {
    Report::snippet(
        "TYPE MISMATCH",
        region,
        None,
        Doc::cat([
            Doc::text("Cannot update fields of `"),
            type_diff::to_doc(l, Ctx::None, record),
            Doc::text("`."),
        ]),
        Doc::text("Rebuild the value with its constructor."),
    )
}

pub(super) fn ambiguous_access(
    l: &Localizer,
    region: Region,
    context: FieldContext<'_>,
    field: Option<&str>,
    record: &ErrorType<'_>,
) -> Report {
    let mut report = Report::snippet(
        "AMBIGUOUS RECORD ACCESS",
        region,
        None,
        Doc::cat([
            Doc::text("Cannot determine record type `"),
            type_diff::to_doc(l, Ctx::None, record),
            Doc::text("`."),
        ]),
        Doc::text("Add a type annotation that identifies the record."),
    );
    report.primary_label = Some(field_context(context, field));
    report
}
