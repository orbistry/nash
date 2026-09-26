use crate::doc::{Doc, cat, join, text};
use nash_region::{Position, Region};
use nash_source::{Comment, CommentKind, Module, SourceComment};

pub(crate) struct Printer<'a> {
    pub source: &'a str,
    lines: Vec<usize>,
    comments: &'a [&'a SourceComment<'a>],
    cursor: usize,
}
impl<'a> Printer<'a> {
    pub fn new(source: &'a str, module: &'a Module<'a>) -> Self {
        let mut lines = vec![0];
        lines.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            source,
            lines,
            comments: module.comments,
            cursor: 0,
        }
    }
    pub fn offset(&self, p: Position) -> usize {
        self.lines
            .get(p.line - 1)
            .copied()
            .unwrap_or(self.source.len())
            + p.column
            - 1
    }
    pub fn raw(&self, r: Region) -> &str {
        &self.source[self.offset(r.start)..self.offset(r.end)]
    }
    pub fn multiline(&self, r: Region) -> bool {
        r.start.line != r.end.line
    }
    pub fn has_comment_before(&self, position: Position) -> bool {
        self.comments
            .get(self.cursor)
            .is_some_and(|comment| comment.region.start < position)
    }
    pub fn before(&mut self, p: Position) -> Doc {
        let mut docs = Vec::new();
        while let Some(c) = self.comments.get(self.cursor) {
            if (c.region.start.line, c.region.start.column) >= (p.line, p.column) {
                break;
            }
            docs.push(text(match c.kind {
                CommentKind::Line => format!("--{}", c.text),
                CommentKind::Block => format!("{{-{}-}}", c.text),
            }));
            docs.push(Doc::Hard);
            self.cursor += 1;
        }
        cat(docs)
    }
    pub fn trailing(&mut self, end: Position) -> Doc {
        let Some(comment) = self.comments.get(self.cursor) else {
            return text("");
        };
        if comment.region.start.line != end.line || comment.region.start < end {
            return text("");
        }
        let gap = &self.source[self.offset(end)..self.offset(comment.region.start)];
        if !gap.chars().all(|c| c.is_whitespace() || c == ',') {
            return text("");
        }
        let raw = self.raw(comment.region).to_string();
        self.cursor += 1;
        Doc::Suffix(format!(" {raw}"))
    }
    pub fn documented(&mut self, doc: Option<&Comment<'_>>) -> Doc {
        doc.map_or_else(
            || text(""),
            |d| {
                cat([
                    self.before(d.region.start),
                    text(self.raw(d.region)),
                    Doc::Hard,
                ])
            },
        )
    }
    pub fn literal(&self, r: Region) -> Doc {
        text(self.raw(r))
    }
    pub fn parens(&self, doc: Doc) -> Doc {
        cat([
            text("("),
            cat([Doc::Line(""), doc]).nest(),
            Doc::Line(""),
            text(")"),
        ])
        .group()
    }
    pub fn collection(&self, open: &str, close: &str, docs: Vec<Doc>, broken: bool) -> Doc {
        Self::collection_with_comments(open, close, docs, broken, text(""))
    }
    fn collection_with_comments(
        open: &str,
        close: &str,
        docs: Vec<Doc>,
        broken: bool,
        trailing: Doc,
    ) -> Doc {
        if docs.is_empty() && matches!(&trailing,Doc::Text(s) if s.is_empty()) {
            return text(format!("{open}{close}"));
        }
        let opening_space = if docs.is_empty() { "" } else { " " };
        let separator = cat([if broken { Doc::Hard } else { Doc::Line("") }, text(", ")]);
        cat([
            text(open),
            text(opening_space),
            join(docs, separator),
            if broken { Doc::Hard } else { Doc::Line(" ") },
            trailing,
            text(close),
        ])
        .group()
    }
    pub fn collection_at(
        &mut self,
        open: &str,
        close: &str,
        docs: Vec<Doc>,
        region: Region,
    ) -> Doc {
        let trailing = self.before(region.end);
        let has_comments = !matches!(&trailing,Doc::Cat(ds) if ds.is_empty());
        Self::collection_with_comments(
            open,
            close,
            docs,
            self.multiline(region) || has_comments,
            if has_comments { trailing } else { text("") },
        )
    }
    pub fn args(&self, head: Doc, args: Vec<Doc>, broken: bool) -> Doc {
        if args.is_empty() {
            return head;
        }
        let line = if broken { Doc::Hard } else { Doc::Line(" ") };
        cat([head, cat([line.clone(), join(args, line)]).nest()]).group()
    }
}
