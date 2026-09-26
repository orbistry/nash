use nash_ast::{Annotation, Kind, Pred, Type};
use nash_report::{
    localizer::Localizer,
    render_type::{Ctx, can_to_doc},
};

pub fn typ(local: &Localizer, typ: &Type<'_>, context: Ctx) -> String {
    can_to_doc(local, context, typ).render(100, false)
}

pub fn context(local: &Localizer, predicates: &[Pred<'_>]) -> String {
    let items: Vec<_> = predicates
        .iter()
        .filter_map(|pred| {
            let Pred::Trait { trait_, args } = pred else {
                return None;
            };
            let mut text = local.to_doc(trait_.home, trait_.name).render(100, false);
            for arg in *args {
                text.push(' ');
                text.push_str(&typ(local, &arg.value, Ctx::App));
            }
            Some(text)
        })
        .collect();
    match items.as_slice() {
        [] => String::new(),
        [one] => format!("{one} => "),
        _ => format!("({}) => ", items.join(", ")),
    }
}

pub fn annotation(local: &Localizer, annotation: &Annotation<'_>) -> String {
    format!(
        "{}{}",
        context(local, annotation.context),
        typ(local, &annotation.typ.value, Ctx::None)
    )
}

pub fn kind(kind: &Kind<'_>) -> String {
    match kind {
        Kind::Type => "Type".into(),
        Kind::Arrow(from, to) => {
            let left = self::kind(from);
            let left = if matches!(from, Kind::Arrow(..)) {
                format!("({left})")
            } else {
                left
            };
            format!("{left} -> {}", self::kind(to))
        }
    }
}

pub fn constructor(local: &Localizer, ctor: &nash_ast::Ctor<'_>) -> String {
    if let Some(labels) = ctor.labels {
        let fields: Vec<_> = labels
            .iter()
            .zip(ctor.arguments)
            .map(|(name, t)| format!("{name} : {}", typ(local, &t.value, Ctx::None)))
            .collect();
        format!("{} {{ {} }}", ctor.name, fields.join(", "))
    } else {
        let mut text = ctor.name.to_owned();
        for arg in ctor.arguments {
            text.push(' ');
            text.push_str(&typ(local, &arg.value, Ctx::App));
        }
        text
    }
}
