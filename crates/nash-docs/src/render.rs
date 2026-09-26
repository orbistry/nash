use crate::highlight::{escape, highlight};
use crate::{Block, DeclarationKind, ModuleDocs};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::collections::BTreeMap;

pub const STYLE: &str = include_str!("assets/style.css");
pub const SEARCH: &str = include_str!("assets/search.js");

#[derive(Clone, Copy, Debug)]
pub enum Format {
    Html,
    Markdown,
}

#[derive(Serialize)]
struct SearchEntry<'a> {
    module: &'a str,
    name: &'a str,
    kind: &'static str,
    signature: &'a str,
    summary: &'a str,
    url: String,
}

pub fn anchor(name: &str) -> String {
    format!(
        "entry-{}",
        name.bytes().map(|b| format!("{b:02x}")).collect::<String>()
    )
}
fn path(name: &str, extension: &str) -> String {
    format!("{}.{extension}", name.replace('.', "/"))
}
fn kind(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Value => "value",
        DeclarationKind::Union => "type",
        DeclarationKind::Alias => "alias",
        DeclarationKind::Operator => "operator",
        DeclarationKind::Trait => "trait",
        DeclarationKind::Implementation => "implementation",
        DeclarationKind::Builtin => "builtin",
        DeclarationKind::Primitive => "primitive",
    }
}

/// Render a complete site or Markdown collection. I/O belongs to the frontend.
pub fn render(modules: &[ModuleDocs], format: Format) -> BTreeMap<String, String> {
    let mut modules: Vec<_> = modules.iter().collect();
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    let mut files = BTreeMap::new();
    match format {
        Format::Markdown => {
            let mut index = "# Nash documentation\n\n".to_owned();
            for module in &modules {
                let filename = path(&module.name, "md");
                index.push_str(&format!("- [{}]({filename})\n", module.name));
                files.insert(filename, markdown(module));
            }
            files.insert("index.md".into(), index);
        }
        Format::Html => {
            let mut index = "<h1>Nash documentation</h1><p>Public modules and their APIs.</p><ul class=\"module-list\">".to_owned();
            let mut search = Vec::new();
            for module in &modules {
                let filename = path(&module.name, "html");
                index.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    escape(&filename),
                    escape(&module.name)
                ));
                let root = "../".repeat(module.name.matches('.').count());
                let mut body = format!("<h1>{}</h1>", escape(&module.name));
                let overview = module
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        Block::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let overview_parser = Parser::new(&overview);
                for block in &module.blocks {
                    match block {
                        Block::Text(text) => body
                            .push_str(&prose(text, Some(overview_parser.reference_definitions()))),
                        Block::Declaration(entry) => {
                            let id = anchor(&entry.name);
                            body.push_str(&format!("<section id=\"{id}\"><header><span class=\"kind\">{}</span><h2><a href=\"#{id}\">{}</a></h2></header><pre><code class=\"language-nash\">{}</code></pre>", kind(entry.kind), escape(&entry.name), highlight(&entry.signature)));
                            if let Some(kind) = &entry.type_kind {
                                body.push_str(&format!(
                                    "<p class=\"type-kind\">Kind: <code>{}</code></p>",
                                    escape(kind)
                                ));
                            }
                            body.push_str(&prose(&entry.doc, None));
                            body.push_str("</section>\n");
                            search.push(SearchEntry {
                                module: &module.name,
                                name: &entry.name,
                                kind: kind(entry.kind),
                                signature: &entry.signature,
                                summary: entry.doc.lines().next().unwrap_or(""),
                                url: format!("{filename}#{id}"),
                            });
                        }
                    }
                }
                files.insert(filename, page(&module.name, &root, &modules, &body));
            }
            index.push_str("</ul>");
            files.insert(
                "index.html".into(),
                page("Nash documentation", "", &modules, &index),
            );
            files.insert(
                "search.json".into(),
                serde_json::to_string_pretty(&search).expect("documentation strings serialize"),
            );
            files.insert("style.css".into(), STYLE.into());
            files.insert("search.js".into(), SEARCH.into());
        }
    }
    files
}

fn page(title: &str, root: &str, modules: &[&ModuleDocs], body: &str) -> String {
    let links = modules
        .iter()
        .map(|module| {
            format!(
                "<a href=\"{}{}\">{}</a>",
                root,
                escape(&path(&module.name, "html")),
                escape(&module.name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Nash</title>
<link rel="stylesheet" href="{root}style.css">
<script defer src="{root}search.js"></script>
</head>
<body>
<aside>
<a class="brand" href="{root}index.html">Nash <span>documentation</span></a>
<label for="search">Search APIs</label>
<input id="search" type="search" placeholder="Name, module, or type" autocomplete="off">
<p id="search-status" role="status"></p>
<nav id="search-results" aria-label="Search results" hidden></nav>
<nav aria-label="Modules">{links}</nav>
</aside>
<main>{body}</main>
</body>
</html>
"#,
        title = escape(title)
    )
}

fn safe_link(url: &str) -> bool {
    !url.chars().any(char::is_control)
        && url.split_once(':').is_none_or(|(scheme, _)| {
            matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "mailto"
            )
        })
}
fn prose(source: &str, references: Option<&pulldown_cmark::RefDefs<'_>>) -> String {
    let resolve = |link: pulldown_cmark::BrokenLink<'_>| {
        references
            .and_then(|refs| refs.get(&link.reference))
            .map(|definition| {
                (
                    definition.dest.to_string().into(),
                    definition.title.as_deref().unwrap_or("").to_string().into(),
                )
            })
    };
    let mut parser = Parser::new_with_broken_link_callback(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
        Some(resolve),
    )
    .peekable();
    let mut events = Vec::new();
    while let Some(event) = parser.next() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language)))
                if language.split_whitespace().next() == Some("nash") =>
            {
                let mut code = String::new();
                for part in parser.by_ref() {
                    match part {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text) => code.push_str(&text),
                        _ => {}
                    }
                }
                events.push(Event::Html(
                    format!(
                        "<pre><code class=\"language-nash\">{}</code></pre>\n",
                        highlight(&code)
                    )
                    .into(),
                ));
            }
            Event::Html(html) | Event::InlineHtml(html) => events.push(Event::Text(html)),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => events.push(Event::Start(Tag::Link {
                link_type,
                dest_url: if safe_link(&dest_url) {
                    dest_url
                } else {
                    "".into()
                },
                title,
                id,
            })),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => events.push(Event::Start(Tag::Image {
                link_type,
                dest_url: if safe_link(&dest_url) {
                    dest_url
                } else {
                    "".into()
                },
                title,
                id,
            })),
            other => events.push(other),
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

pub fn markdown(module: &ModuleDocs) -> String {
    let mut out = format!("# {}\n\n", module.name);
    for block in &module.blocks {
        match block {
            Block::Text(text) => {
                out.push_str(text);
                out.push_str("\n\n");
            }
            Block::Declaration(entry) => {
                out.push_str(&format!(
                    "<a id=\"{}\"></a>\n\n## {}\n\n",
                    anchor(&entry.name),
                    entry.name
                ));
                let fence = "`".repeat(
                    entry
                        .signature
                        .split(|c| c != '`')
                        .map(str::len)
                        .max()
                        .unwrap_or(0)
                        .max(2)
                        + 1,
                );
                out.push_str(&format!("{fence}nash\n{}\n{fence}\n\n", entry.signature));
                if let Some(kind) = &entry.type_kind {
                    out.push_str(&format!("Kind: `{kind}`\n\n"));
                }
                if !entry.doc.is_empty() {
                    out.push_str(&entry.doc);
                    out.push_str("\n\n");
                }
            }
        }
    }
    out
}
