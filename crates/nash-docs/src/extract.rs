use crate::{Block, Declaration, DeclarationKind as K, DocsWarning, Extraction, ModuleDocs, types};
use nash_can::Interface;
use nash_report::{
    localizer::Localizer,
    render_type::{Ctx, src_to_doc},
};
use nash_source::{Comment, Docs, Module};

fn comment(comment: Option<&Comment<'_>>) -> String {
    comment.map_or_else(String::new, |c| {
        String::from_utf8_lossy(c.snippet.data).trim().to_owned()
    })
}

/// Extract public declarations. Warnings never discard documentation.
pub fn extract(source: &Module<'_>, interface: &Interface<'_>) -> Extraction {
    let name = interface.home.name.to_owned();
    let local = Localizer::from_module(source, &[]).with_package(interface.home.package);
    let mut declarations = Vec::new();
    let mut warnings = Vec::new();
    let mut add = |line, name: &str, kind, signature, doc: String, require_doc| {
        if require_doc && doc.is_empty() {
            warnings.push(DocsWarning {
                module: interface.home.name.into(),
                name: name.into(),
                message: "Export has no documentation comment.".into(),
            });
        }
        let type_kind = match kind {
            K::Union => interface
                .unions
                .iter()
                .find(|u| u.name == name)
                .map(|u| types::kind(u.kind)),
            K::Alias => interface
                .aliases
                .iter()
                .find(|a| a.name == name)
                .map(|a| types::kind(a.kind)),
            _ => None,
        };
        declarations.push((
            line,
            Declaration {
                name: name.into(),
                kind,
                signature,
                type_kind,
                doc,
            },
        ));
    };
    for value in interface.values {
        let definition = source
            .values
            .iter()
            .find(|v| v.value.name.value == value.name);
        add(
            definition.map_or(0, |v| v.region.start.line),
            value.name,
            K::Value,
            format!(
                "{} : {}",
                value.name,
                types::annotation(&local, value.annotation)
            ),
            comment(definition.and_then(|v| v.value.docs)),
            true,
        );
    }
    for union in interface.unions.iter().filter_map(|u| u.to_public()) {
        let definition = source
            .unions
            .iter()
            .find(|u| u.value.name.value == union.name);
        let mut signature = format!(
            "type {}{}{}",
            types::context(&local, union.context),
            union.name,
            parameters(union.parameters)
        );
        if !union.ctors.is_empty() {
            signature.push_str("\n    = ");
            signature.push_str(
                &union
                    .ctors
                    .iter()
                    .map(|c| types::constructor(&local, c))
                    .collect::<Vec<_>>()
                    .join("\n    | "),
            );
        }
        add(
            definition.map_or(0, |v| v.region.start.line),
            union.name,
            K::Union,
            signature,
            comment(definition.and_then(|v| v.value.docs)),
            true,
        );
    }
    for alias in interface.aliases.iter().filter_map(|a| a.to_public()) {
        let definition = source
            .aliases
            .iter()
            .find(|a| a.value.name.value == alias.name);
        add(
            definition.map_or(0, |v| v.region.start.line),
            alias.name,
            K::Alias,
            format!(
                "type alias {}{}{} = {}",
                types::context(&local, alias.context),
                alias.name,
                parameters(alias.parameters),
                types::typ(&local, &alias.typ.value, Ctx::None)
            ),
            comment(definition.and_then(|v| v.value.docs)),
            true,
        );
    }
    for trait_ in interface.traits.iter().filter(|t| t.exported) {
        let definition = source
            .traits
            .iter()
            .find(|t| t.value.name.value == trait_.name);
        let mut signature = format!(
            "trait {}{}{} where",
            types::context(&local, trait_.supers),
            trait_.name,
            parameters(trait_.parameters)
        );
        for method in trait_.methods {
            // The enclosing trait supplies its own predicate.
            let context = &method.annotation.context[1..];
            signature.push_str(&format!(
                "\n    {} : {}{}",
                method.name,
                types::context(&local, context),
                types::typ(&local, &method.annotation.typ.value, Ctx::None)
            ));
        }
        add(
            definition.map_or(0, |v| v.region.start.line),
            trait_.name,
            K::Trait,
            signature,
            comment(definition.and_then(|v| v.value.docs)),
            true,
        );
    }
    for operator in interface.binops {
        let definition = source.binops.iter().find(|b| b.value.op == operator.symbol);
        let doc = comment(
            source
                .values
                .iter()
                .find(|v| {
                    operator.function.home == interface.home
                        && v.value.name.value == operator.function.name
                })
                .and_then(|v| v.value.docs),
        );
        let doc = if doc.is_empty() {
            format!(
                "Infix form of `{}.{}`.",
                operator.function.home.name, operator.function.name
            )
        } else {
            doc
        };
        let associativity = match operator.associativity {
            nash_ast::Associativity::Left => "left",
            nash_ast::Associativity::Right => "right",
            nash_ast::Associativity::None => "non",
        };
        add(
            definition.map_or(0, |v| v.region.start.line),
            &format!("({})", operator.symbol),
            K::Operator,
            format!(
                "({}) : {}\ninfix {associativity} {} ({}) = {}",
                operator.symbol,
                types::annotation(&local, operator.annotation),
                operator.precedence.0,
                operator.symbol,
                local
                    .to_doc(operator.function.home, operator.function.name)
                    .render(100, false)
            ),
            doc,
            false,
        );
    }
    for implementation in source.impls {
        let head = constraint(&implementation.value.head.value);
        let context = implementation
            .value
            .context
            .iter()
            .map(|c| constraint(&c.value))
            .collect::<Vec<_>>();
        let context = match context.as_slice() {
            [] => String::new(),
            [one] => format!("{one} => "),
            _ => format!("({}) => ", context.join(", ")),
        };
        let signature = format!("impl {context}{head}");
        add(
            implementation.region.start.line,
            &signature,
            K::Implementation,
            signature.clone(),
            comment(implementation.value.docs),
            false,
        );
    }
    declarations.sort_by_key(|(line, _)| *line);
    let overview = match source.docs {
        Docs::YesDocs { overview, .. } => comment(Some(overview)),
        Docs::NoDocs(_) => String::new(),
    };
    let mut pending: Vec<_> = declarations.into_iter().map(|(_, d)| Some(d)).collect();
    let mut blocks = Vec::new();
    let mut prose = String::new();
    let code_ranges: Vec<_> = pulldown_cmark::Parser::new(&overview)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(
                event,
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_))
            )
            .then_some(range)
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    let mut lines = overview
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line.trim_end_matches('\n')))
        })
        .peekable();
    while let Some((offset, line)) = lines.next() {
        let trimmed = line.trim_start();
        let directive = trimmed
            .strip_prefix("@docs")
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
        if let Some(rest) = directive.filter(|_| {
            !code_ranges
                .iter()
                .any(|range| range.contains(&(offset + line.len() - trimmed.len())))
        }) {
            push_prose(&mut blocks, &mut prose);
            let mut names = rest.trim().to_owned();
            while names.is_empty() || names.ends_with(',') {
                let Some((_, next)) = lines.peek().filter(|(_, next)| doc_names(next.trim()))
                else {
                    break;
                };
                names.push_str(next.trim());
                lines.next();
            }
            if names.is_empty() {
                warnings.push(DocsWarning {
                    module: name.clone(),
                    name: "@docs".into(),
                    message: "@docs requires at least one exported name.".into(),
                });
            }
            for item in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let item = item.to_owned();
                if !seen.insert(item.clone()) {
                    warnings.push(DocsWarning {
                        module: name.clone(),
                        name: item,
                        message: "Repeated @docs entry.".into(),
                    });
                } else if let Some(entry) = pending
                    .iter_mut()
                    .find(|d| d.as_ref().is_some_and(|d| d.name == item))
                {
                    blocks.push(Block::Declaration(entry.take().unwrap()));
                } else {
                    warnings.push(DocsWarning {
                        module: name.clone(),
                        name: item,
                        message: "@docs name is not exported by this module.".into(),
                    });
                }
            }
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    push_prose(&mut blocks, &mut prose);
    blocks.extend(pending.into_iter().flatten().map(Block::Declaration));
    Extraction {
        module: ModuleDocs { name, blocks },
        warnings,
    }
}

fn doc_names(line: &str) -> bool {
    !line.is_empty()
        && line.trim_end_matches(',').split(',').all(|part| {
            let name = part.trim();
            if name.starts_with('(') && name.ends_with(')') {
                name.len() > 2
                    && name[1..name.len() - 1]
                        .chars()
                        .all(|c| "+-/*=.$<>:&|^?%!~#".contains(c))
            } else {
                name.starts_with(char::is_alphabetic)
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
            }
        })
}

fn push_prose(blocks: &mut Vec<Block>, prose: &mut String) {
    if !prose.trim().is_empty() {
        blocks.push(Block::Text(prose.trim_matches('\n').to_owned()));
    }
    prose.clear();
}
fn parameters(parameters: &[&str]) -> String {
    parameters.iter().map(|p| format!(" '{p}")).collect()
}
fn constraint(constraint: &nash_source::Constraint<'_>) -> String {
    let mut out = constraint.module.map_or_else(
        || constraint.class.value.to_owned(),
        |m| format!("{m}.{}", constraint.class.value),
    );
    for arg in constraint.args {
        out.push(' ');
        out.push_str(&src_to_doc(Ctx::App, arg).render(100, false));
    }
    out
}

/// Compiler-owned modules have no source file, so document their actual catalog.
pub fn primitives() -> Vec<ModuleDocs> {
    use nash_ast::primitives::{BUILTINS, COERCE, PRIMITIVES, ReprTrait};
    let local = Localizer::from_names(["Primitive", "Builtin"]);
    let builtins = BUILTINS
        .iter()
        .map(|b| {
            Block::Declaration(Declaration {
                name: b.name.into(),
                kind: K::Builtin,
                type_kind: None,
                signature: format!(
                    "{} : {}{}",
                    b.name,
                    types::context(&local, b.context),
                    types::typ(&local, &b.typ.value, Ctx::None)
                ),
                doc: format!("Plutus Core `{}` builtin.", b.variant),
            })
        })
        .collect();
    let mut primitive: Vec<_> = PRIMITIVES
        .iter()
        .map(|p| {
            let mut doc = format!(
                "{} representation; kind `{}`.",
                match p.repr {
                    nash_ast::primitives::Repr::Big => "Data",
                    nash_ast::primitives::Repr::Const => "Constant",
                    nash_ast::primitives::Repr::Term => "Term",
                },
                types::kind(p.kind)
            );
            for (index, repr) in p.context {
                doc.push_str(&format!(
                    " Parameter {} requires `{}`.",
                    index + 1,
                    repr.name()
                ));
            }
            let mut signature = format!(
                "type {}{}",
                p.name,
                (0..p.kind.arity())
                    .map(|index| format!(" '{}", char::from(b'a' + index as u8)))
                    .collect::<String>()
            );
            if !p.ctors.is_empty() {
                signature.push_str(" = ");
                signature.push_str(
                    &p.ctors
                        .iter()
                        .map(|c| types::constructor(&local, c))
                        .collect::<Vec<_>>()
                        .join(" | "),
                );
            }
            Block::Declaration(Declaration {
                name: p.name.into(),
                kind: K::Primitive,
                signature,
                type_kind: Some(types::kind(p.kind)),
                doc,
            })
        })
        .collect();
    for repr in ReprTrait::ALL {
        primitive.push(Block::Declaration(Declaration {
            name: repr.name().into(),
            kind: K::Trait,
            type_kind: None,
            signature: format!("trait {} 'a", repr.name()),
            doc: "Compiler-checked representation constraint; no runtime test is emitted.".into(),
        }));
    }
    primitive.push(Block::Declaration(Declaration {
        name: "coerce".into(),
        kind: K::Value,
        type_kind: None,
        signature: format!("coerce : {}", types::annotation(&local, &COERCE)),
        doc: "Unchecked coercion. The caller is responsible for the runtime representation.".into(),
    }));
    vec![
        ModuleDocs {
            name: "Builtin".into(),
            blocks: builtins,
        },
        ModuleDocs {
            name: "Primitive".into(),
            blocks: primitive,
        },
    ]
}
