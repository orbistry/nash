//! Plain diagnostic text for kind and representation checking. The full
//! reporting layer can add source spans and styling without changing errors.
use nash_ast::{Kind, primitives::ReprTrait};
use nash_region::Region;

fn at(source: &str, region: Region, tag: &str, message: String) -> String {
    let line = region.start.line;
    let excerpt = source
        .lines()
        .nth(usize::from(line.saturating_sub(1)))
        .unwrap_or("");
    format!(
        "{tag} at {line}:{}: {message}\n  {excerpt}",
        region.start.column
    )
}

fn kind(value: &Kind<'_>) -> String {
    match value {
        Kind::Type => "Type".into(),
        Kind::Arrow(from, to) => {
            let from = match from {
                Kind::Arrow(..) => format!("({})", kind(from)),
                _ => kind(from),
            };
            format!("{from} -> {}", kind(to))
        }
    }
}

fn admitted(trait_: ReprTrait) -> &'static str {
    match trait_ {
        ReprTrait::Big => "Big",
        ReprTrait::Const => "Const",
        ReprTrait::Term => "Term",
        ReprTrait::Storable => "Big or Const",
        ReprTrait::Little => "Const or Term",
    }
}

fn application(head: String, args: impl IntoIterator<Item = String>) -> String {
    let args: Vec<_> = args.into_iter().collect();
    if args.is_empty() {
        head
    } else {
        format!("({head} {})", args.join(" "))
    }
}

fn typ(value: &nash_constrain::error_type::ErrorType<'_>) -> String {
    use nash_constrain::error_type::{ErrorType, Extension};
    let qualified = |home: nash_ast::ModuleName<'_>, name: &str| {
        if home == nash_ast::primitives::builtin_home() {
            name.to_owned()
        } else {
            format!("{}.{name}", home.name)
        }
    };
    match value {
        ErrorType::FlexVar(name) | ErrorType::RigidVar(name) => format!("'{name}"),
        ErrorType::Type { home, name, args } => {
            application(qualified(*home, name), args.iter().map(|arg| typ(arg)))
        }
        ErrorType::Alias {
            home, name, args, ..
        } => application(qualified(*home, name), args.iter().map(|(_, arg)| typ(arg))),
        ErrorType::VarApp(head, args) => application(typ(head), args.iter().map(|arg| typ(arg))),
        ErrorType::Unit => "()".into(),
        ErrorType::Tuple(first, second, rest) => format!(
            "({})",
            [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .map(typ)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ErrorType::Lambda(first, second, rest) => format!(
            "({})",
            [*first, *second]
                .into_iter()
                .chain(rest.iter().copied())
                .map(typ)
                .collect::<Vec<_>>()
                .join(" -> ")
        ),
        ErrorType::Record { fields, ext } => {
            let fields = fields
                .iter()
                .map(|(name, value)| format!("{name} : {}", typ(value)))
                .collect::<Vec<_>>()
                .join(", ");
            match ext {
                Extension::Closed => format!("{{ {fields} }}"),
                Extension::FlexOpen(name) | Extension::RigidOpen(name) => {
                    format!("{{ '{name} | {fields} }}")
                }
            }
        }
        ErrorType::Infinite => "<infinite type>".into(),
        ErrorType::Error => "<type error>".into(),
    }
}

fn requirement(value: &nash_constrain::error::Requirement<'_>) -> String {
    use nash_constrain::error::Requirement;
    match value {
        Requirement::Trait { trait_, args } => {
            application(trait_.name.into(), args.iter().map(|arg| typ(arg)))
        }
        Requirement::Application { head, args } => format!(
            "application {}",
            application(typ(head), args.iter().map(|arg| typ(arg)))
        ),
        Requirement::Formation(value) => format!("formation of {}", typ(value)),
    }
}

fn context(value: &nash_can::KindContext<'_>) -> String {
    use nash_can::KindContext;
    match value {
        KindContext::TypeAnnotation => "the type annotation".into(),
        KindContext::Annotation { name } => format!("the annotation for {name}"),
        KindContext::BigField { union, ctor, index }
        | KindContext::LittleField { union, ctor, index } => {
            format!("field {} of {ctor} in {union}", usize::from(*index) + 1)
        }
        KindContext::RecordField { alias, field, .. } => format!("field {field} of {alias}"),
        KindContext::AliasCasing { alias, big } => format!(
            "the {}case name of alias {alias}",
            if *big { "upper" } else { "lower" }
        ),
        KindContext::ValuePosition => "a value type".into(),
        KindContext::ParamAnnotation { type_name, param } => {
            format!("parameter '{param} of {type_name}")
        }
        KindContext::ImplHead { trait_, index } => {
            format!("head {} of impl {}", usize::from(*index) + 1, trait_.name)
        }
        KindContext::TypeArg { index, .. } => format!("type argument {}", usize::from(*index) + 1),
    }
}

pub(crate) fn canonical(source: &str, errors: &[nash_can::Error<'_>]) -> String {
    use nash_can::Error;
    errors.iter().map(|error| {
        let (region, tag, message) = match error {
            Error::KindMismatch { region, expected, actual, .. } => (*region, "KindMismatch",
                format!("expected kind {}, but found {}. Type arguments must have matching kinds.", kind(expected), kind(actual))),
            Error::KindInfinite { region, .. } => (*region, "KindInfinite",
                "this application would require an infinite kind. A type constructor cannot be applied to itself.".into()),
            Error::RepresentationMismatch { region, context: origin, required, actual } => (*region, "RepresentationMismatch",
                format!("this position requires {} ({}), but the type has {actual:?} representation. Required by {}.", required.name(), admitted(*required), context(origin))),
            Error::ContradictoryRepresentation { region, variable } => (*region, "ContradictoryRepresentation",
                format!("the representation requirements on '{variable} are incompatible; no type can satisfy all of them.")),
            Error::ImplOfBuiltinTrait { region, trait_ } => (*region, "ImplOfBuiltinTrait",
                format!("{} is compiler-owned. Its representation rules cannot be replaced by an impl.", trait_.name)),
            Error::IrregularRecursion { region, constructor, parameter } => (*region, "IrregularRecursion",
                format!("recursive use of {} constructs the applied-relevant parameter '{parameter}. Pass a type variable here so context inference can terminate.", constructor.name)),
            _ => return format!("{error:?}"),
        };
        at(source, region, tag, message)
    }).collect::<Vec<_>>().join("\n")
}

pub(crate) fn inference(source: &str, errors: &[nash_constrain::error::Error<'_>]) -> String {
    use nash_constrain::error::{Error, KindProblem};
    errors.iter().map(|error| {
        let (region, tag, message) = match error {
            Error::BadKind { region, name, reason, .. } => (*region, "BadKind", match reason {
                KindProblem::Infinite => format!("{name} requires an infinite kind. A type constructor cannot be applied to itself."),
                KindProblem::Mismatch { expected, actual } => format!("{name} requires kind {}, but this use has kind {}. An annotation's quantified kinds cannot be specialized by its body.", kind(expected), kind(actual)),
            }),
            Error::ContradictoryRepresentation { region, name, requirements, .. } => (*region, "ContradictoryRepresentation",
                format!("{name} requires incompatible representations {requirements:?}. No type can satisfy all of them.")),
            Error::MissingImpl { region, name, trait_, args, because, .. }
                if ReprTrait::of(*trait_).is_some() => {
                    let required = ReprTrait::of(*trait_).unwrap();
                    let chain = if because.is_empty() { String::new() } else { format!(" Required through {}.", because.iter().map(requirement).collect::<Vec<_>>().join(" -> ")) };
                    (*region, "MissingImpl", format!("{name} requires {} ({}), but its argument does not have an allowed representation: {}.{chain}", required.name(), admitted(required), args.iter().map(|arg| typ(arg)).collect::<Vec<_>>().join(", ")))
                }
            Error::MissingConstraint { region, name, trait_, binder, .. }
                if ReprTrait::of(*trait_).is_some() => (*region, "MissingConstraint",
                    format!("{name} requires {}, but the annotation for {} does not promise it. Add the representation constraint to that annotation.", trait_.name, binder.value)),
            Error::UnresolvedApplication { region, name, .. } => (*region, "UnresolvedApplication",
                format!("the datatype context required by {name} could not be established for this type application.")),
            _ => return format!("{error:?}"),
        };
        at(source, region, tag, message)
    }).collect::<Vec<_>>().join("\n")
}
