//! Text form of missing patterns, from Elm's `Reporting/Error/Pattern.hs`
//! (`patternToDoc`, `delist`).

use crate::pattern::{CONS_NAME, Literal, NIL_NAME, PAIR_NAME, Pattern, TRIPLE_NAME, UNIT_NAME};

/// Elm's `Context` in `Reporting.Error.Pattern`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderContext {
    Arg,
    Head,
    Unambiguous,
}

pub fn pattern_to_string(context: RenderContext, pattern: Pattern<'_>) -> String {
    match delist(pattern, Vec::new()) {
        Structure::NonList(Pattern::Anything) => "_".to_string(),
        Structure::NonList(Pattern::Literal(literal)) => literal_to_string(literal),
        Structure::NonList(Pattern::Ctor {
            name: UNIT_NAME, ..
        }) => "()".to_string(),
        Structure::NonList(Pattern::Ctor {
            name: PAIR_NAME | TRIPLE_NAME,
            args,
            ..
        }) => {
            format!("( {} )", join(args, RenderContext::Unambiguous, ", "))
        }
        Structure::NonList(Pattern::Ctor { name, args, .. }) => {
            let mut doc = name.to_string();
            for arg in args {
                doc.push(' ');
                doc.push_str(&pattern_to_string(RenderContext::Arg, *arg));
            }
            if context == RenderContext::Arg && !args.is_empty() {
                format!("({doc})")
            } else {
                doc
            }
        }
        Structure::FiniteList(entries) => {
            format!("[{}]", join(&entries, RenderContext::Unambiguous, ","))
        }
        Structure::Conses(conses, last) => {
            let doc = conses.iter().rev().fold(
                pattern_to_string(RenderContext::Unambiguous, last),
                |tail, head| {
                    format!(
                        "{} :: {}",
                        pattern_to_string(RenderContext::Head, *head),
                        tail
                    )
                },
            );
            if context == RenderContext::Unambiguous {
                doc
            } else {
                format!("({doc})")
            }
        }
    }
}

fn join(patterns: &[Pattern<'_>], context: RenderContext, sep: &str) -> String {
    patterns
        .iter()
        .map(|p| pattern_to_string(context, *p))
        .collect::<Vec<_>>()
        .join(sep)
}

fn literal_to_string(literal: Literal<'_>) -> String {
    match literal {
        Literal::Int(n) => n.to_string(),
        Literal::Str(s) => string_literal(s),
        Literal::Bytes(bytes) => format!("#\"{}\"", hex::encode(bytes)),
    }
}

enum Structure<'a> {
    FiniteList(Vec<Pattern<'a>>),
    Conses(Vec<Pattern<'a>>, Pattern<'a>),
    NonList(Pattern<'a>),
}

/// Elm's `delist`, with a Vec that accumulates entries in source order.
fn delist<'a>(pattern: Pattern<'a>, mut entries: Vec<Pattern<'a>>) -> Structure<'a> {
    match pattern {
        Pattern::Ctor {
            name: NIL_NAME,
            args: [],
            ..
        } => Structure::FiniteList(entries),
        Pattern::Ctor {
            name: CONS_NAME,
            args: [head, tail],
            ..
        } => {
            entries.push(*head);
            delist(*tail, entries)
        }
        _ if entries.is_empty() => Structure::NonList(pattern),
        _ => Structure::Conses(entries, pattern),
    }
}

fn string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            ch if ch.is_control() => {
                use std::fmt::Write;
                write!(out, "\\u{{{:04x}}}", u32::from(ch)).unwrap();
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
