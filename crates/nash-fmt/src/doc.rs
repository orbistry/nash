//! Small document algebra: groups flatten when they fit; hard lines never flatten.
#[derive(Clone)]
pub(crate) enum Doc {
    Text(String),
    Suffix(String),
    Line(&'static str),
    Hard,
    Cat(Vec<Doc>),
    Nest(Box<Doc>),
    Group(Box<Doc>),
}
use Doc::*;
pub(crate) fn text(value: impl ToString) -> Doc {
    Text(value.to_string())
}
pub(crate) fn cat(docs: impl IntoIterator<Item = Doc>) -> Doc {
    Cat(docs.into_iter().collect())
}
pub(crate) fn join(docs: impl IntoIterator<Item = Doc>, separator: Doc) -> Doc {
    let mut out = Vec::new();
    for doc in docs {
        if !out.is_empty() {
            out.push(separator.clone());
        }
        out.push(doc);
    }
    Cat(out)
}
impl Doc {
    pub(crate) fn nest(self) -> Self {
        Nest(Box::new(self))
    }
    pub(crate) fn group(self) -> Self {
        Group(Box::new(self))
    }
    fn width(&self) -> Option<usize> {
        match self {
            Text(s) if !s.contains('\n') => Some(s.chars().count()),
            Text(_) | Suffix(_) | Hard => None,
            Line(s) => Some(s.len()),
            Nest(d) | Group(d) => d.width(),
            Cat(ds) => ds.iter().try_fold(0usize, |n, d| n.checked_add(d.width()?)),
        }
    }
    pub(crate) fn render(&self, width: usize) -> String {
        struct Output {
            text: String,
            column: usize,
            pending: Option<usize>,
            width: usize,
            suffix: String,
        }
        fn write(d: &Doc, indent: usize, flat: bool, out: &mut Output) {
            match d {
                Text(s) => {
                    if s.is_empty() {
                        return;
                    }
                    if let Some(n) = out.pending.take() {
                        out.text.extend(std::iter::repeat_n(' ', n));
                        out.column = n;
                    }
                    out.text.push_str(s);
                    out.column = s
                        .rsplit_once('\n')
                        .map_or(out.column + s.chars().count(), |(_, last)| {
                            last.chars().count()
                        });
                }
                Suffix(s) => out.suffix.push_str(s),
                Line(s) if flat => write(&text(s), indent, flat, out),
                Line(_) | Hard => {
                    out.text.push_str(&std::mem::take(&mut out.suffix));
                    out.text.push('\n');
                    out.pending = Some(indent);
                    out.column = indent;
                }
                Cat(ds) => {
                    for d in ds {
                        write(d, indent, flat, out);
                    }
                }
                Nest(d) => write(d, indent + 4, flat, out),
                Group(d) => write(
                    d,
                    indent,
                    flat || d
                        .width()
                        .is_some_and(|n| n <= out.width.saturating_sub(out.column)),
                    out,
                ),
            }
        }
        let mut out = Output {
            text: String::new(),
            column: 0,
            pending: None,
            width,
            suffix: String::new(),
        };
        write(self, 0, false, &mut out);
        out.text.push_str(&out.suffix);
        while out.text.ends_with('\n') {
            out.text.pop();
        }
        if !out.text.is_empty() {
            out.text.push('\n');
        }
        out.text
    }
}
