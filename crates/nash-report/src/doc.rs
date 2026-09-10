//! Elm's `Reporting/Doc.hs`: a small Wadler-style pretty printer with the
//! handful of combinators the reports use.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doc {
    Empty,
    Text(String),
    Styled(Style, Box<Doc>),
    Cat(Vec<Doc>),
    Nest(usize, Box<Doc>),
    Align(Box<Doc>),
    Line,
    LineBreak,
    HardLine,
    Group(Box<Doc>),
    Fill(Vec<Doc>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub underline: bool,
    pub color: Option<Color>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub base: BaseColor,
    pub vivid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl From<&str> for Doc {
    fn from(s: &str) -> Doc {
        Doc::text(s)
    }
}

impl Doc {
    /// `P.text` / `fromChars` / `fromName`. Panics on '\n': use `vcat`.
    pub fn text(s: impl Into<String>) -> Doc {
        let s = s.into();
        assert!(!s.contains('\n'), "Doc::text must not contain newlines");
        Doc::Text(s)
    }

    pub fn from_int(n: impl std::fmt::Display) -> Doc {
        Doc::text(n.to_string())
    }

    /// `a <> b`.
    pub fn cat(docs: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::Cat(docs.into_iter().collect())
    }
    /// `P.hsep`.
    pub fn hsep(docs: impl IntoIterator<Item = Doc>) -> Doc {
        intersperse(docs, Doc::text(" "))
    }
    /// `P.hcat`.
    pub fn hcat(docs: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::cat(docs)
    }
    /// `P.vcat`.
    pub fn vcat(docs: impl IntoIterator<Item = Doc>) -> Doc {
        intersperse(docs, Doc::HardLine)
    }
    /// `P.sep` = `group (vsep docs)`.
    pub fn sep(docs: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::Group(Box::new(intersperse(docs, Doc::Line)))
    }
    /// `P.fillSep`.
    pub fn fill_sep(docs: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::Fill(docs.into_iter().collect())
    }
    /// `P.indent`.
    pub fn indent(n: usize, doc: Doc) -> Doc {
        Doc::hang(n, Doc::cat([Doc::text(" ".repeat(n)), doc]))
    }
    /// `P.hang`.
    pub fn hang(n: usize, doc: Doc) -> Doc {
        Doc::Align(Box::new(Doc::Nest(n, Box::new(doc))))
    }
    /// `P.align`.
    pub fn align(doc: Doc) -> Doc {
        Doc::Align(Box::new(doc))
    }

    /// Elm `stack`: paragraphs separated by blank lines.
    pub fn stack(docs: impl IntoIterator<Item = Doc>) -> Doc {
        intersperse(docs, Doc::cat([Doc::HardLine, Doc::HardLine]))
    }

    /// Elm `reflow`: words wrapped to the width.
    pub fn reflow(paragraph: &str) -> Doc {
        Doc::fill_sep(paragraph.split_whitespace().map(Doc::text))
    }

    /// Elm `commaSep`.
    pub fn comma_sep(conjunction: &str, style: impl Fn(Doc) -> Doc, names: Vec<Doc>) -> Vec<Doc> {
        match names.len() {
            0 => Vec::new(),
            1 => names.into_iter().map(style).collect(),
            2 => {
                let mut it = names.into_iter();
                vec![
                    style(it.next().unwrap()),
                    Doc::text(conjunction),
                    style(it.next().unwrap()),
                ]
            }
            n => {
                let mut docs: Vec<Doc> = names
                    .iter()
                    .take(n - 1)
                    .map(|name| Doc::cat([style(name.clone()), Doc::text(",")]))
                    .collect();
                docs.push(Doc::text(conjunction));
                docs.push(style(names[n - 1].clone()));
                docs
            }
        }
    }

    // STYLES (Elm's P.red .. P.dullyellow)

    fn styled(self, f: impl FnOnce(&mut Style)) -> Doc {
        let mut style = Style::default();
        f(&mut style);
        Doc::Styled(style, Box::new(self))
    }
    fn color(self, base: BaseColor, vivid: bool) -> Doc {
        self.styled(|s| s.color = Some(Color { base, vivid }))
    }
    pub fn red(self) -> Doc {
        self.color(BaseColor::Red, true)
    }
    pub fn dullred(self) -> Doc {
        self.color(BaseColor::Red, false)
    }
    pub fn green(self) -> Doc {
        self.color(BaseColor::Green, true)
    }
    pub fn yellow(self) -> Doc {
        self.color(BaseColor::Yellow, true)
    }
    pub fn dullyellow(self) -> Doc {
        self.color(BaseColor::Yellow, false)
    }
    pub fn cyan(self) -> Doc {
        self.color(BaseColor::Cyan, true)
    }
    pub fn dullcyan(self) -> Doc {
        self.color(BaseColor::Cyan, false)
    }
    pub fn blue(self) -> Doc {
        self.color(BaseColor::Blue, true)
    }
    pub fn magenta(self) -> Doc {
        self.color(BaseColor::Magenta, true)
    }
    pub fn black(self) -> Doc {
        self.color(BaseColor::Black, true)
    }
    pub fn bold(self) -> Doc {
        self.styled(|s| s.bold = true)
    }
    pub fn underline(self) -> Doc {
        self.styled(|s| s.underline = true)
    }

    // NOTES, HINTS, LINKS

    /// `toFancyNote`: `fillSep (underline "Note" <> ":" : chunks)`.
    pub fn to_fancy_note(chunks: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::fill_sep(
            std::iter::once(Doc::cat([Doc::text("Note").underline(), Doc::text(":")]))
                .chain(chunks),
        )
    }
    pub fn to_simple_note(message: &str) -> Doc {
        Doc::to_fancy_note(message.split_whitespace().map(Doc::text))
    }
    pub fn to_fancy_hint(chunks: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::fill_sep(
            std::iter::once(Doc::cat([Doc::text("Hint").underline(), Doc::text(":")]))
                .chain(chunks),
        )
    }
    pub fn to_simple_hint(message: &str) -> Doc {
        Doc::to_fancy_hint(message.split_whitespace().map(Doc::text))
    }

    /// `link word before fileName after`.
    pub fn link(word: &str, before: &str, file_name: &str, after: &str) -> Doc {
        Doc::fill_sep(
            std::iter::once(Doc::cat([Doc::text(word).underline(), Doc::text(":")]))
                .chain(before.split_whitespace().map(Doc::text))
                .chain(std::iter::once(Doc::text(make_link(file_name))))
                .chain(after.split_whitespace().map(Doc::text)),
        )
    }
    pub fn fancy_link(word: &str, before: Vec<Doc>, file_name: &str, after: Vec<Doc>) -> Doc {
        Doc::fill_sep(
            std::iter::once(Doc::cat([Doc::text(word).underline(), Doc::text(":")]))
                .chain(before)
                .chain(std::iter::once(Doc::text(make_link(file_name))))
                .chain(after),
        )
    }
    pub fn reflow_link(before: &str, file_name: &str, after: &str) -> Doc {
        Doc::fill_sep(
            before
                .split_whitespace()
                .map(Doc::text)
                .chain(std::iter::once(Doc::text(make_link(file_name))))
                .chain(after.split_whitespace().map(Doc::text)),
        )
    }

    /// Elm `cycle`: the boxed dependency cycle drawing.
    pub fn cycle(indent: usize, name: &str, names: &[&str]) -> Doc {
        let to_ln = |n: &str| Doc::cat([Doc::text("│    "), Doc::text(n).dullyellow()]);
        let mut lines = vec![Doc::text("┌─────┐")];
        for (i, n) in std::iter::once(name)
            .chain(names.iter().copied())
            .enumerate()
        {
            if i > 0 {
                lines.push(Doc::text("│     ↓"));
            }
            lines.push(to_ln(n));
        }
        lines.push(Doc::text("└─────┘"));
        Doc::indent(indent, Doc::vcat(lines))
    }

    // RENDERING

    /// Elm `toString` / `toAnsi` at the given width.
    pub fn render(&self, width: usize, color: bool) -> String {
        render(self, width, color)
    }
    /// Elm `Doc.encode`: styled chunks for JSON.
    pub fn chunks(&self, width: usize) -> Vec<Chunk> {
        chunks(self, width)
    }
}

pub fn make_link(file_name: &str) -> String {
    format!("<https://nash-script.dev/hints/{file_name}>")
}
pub fn make_naked_link(file_name: &str) -> String {
    format!("https://nash-script.dev/hints/{file_name}")
}

/// Elm `args`.
pub fn args(n: usize) -> String {
    format!("{n} argument{}", if n == 1 { "" } else { "s" })
}
/// Elm `moreArgs`.
pub fn more_args(n: usize) -> String {
    format!("{n} more argument{}", if n == 1 { "" } else { "s" })
}
/// Elm `ordinal` over a zero-based index.
pub fn ordinal(index: usize) -> String {
    int_to_ordinal(index + 1)
}
/// Elm `intToOrdinal`.
pub fn int_to_ordinal(number: usize) -> String {
    let ending = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{number}{ending}")
}

fn intersperse(docs: impl IntoIterator<Item = Doc>, sep: Doc) -> Doc {
    let mut out = Vec::new();
    for (i, doc) in docs.into_iter().enumerate() {
        if i > 0 {
            out.push(sep.clone());
        }
        out.push(doc);
    }
    Doc::Cat(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAGRAPH: &str = "I cannot find this variable in the current module. Check the spelling of its name and make sure that the module which defines it is imported. These suggestions may help you find the value you intended to use.";

    #[test]
    fn fill_allows_first_child_group_to_break() {
        let doc = Doc::fill_sep([Doc::sep(["long", "long"].map(Doc::text)), Doc::text("x")]);
        assert_eq!(doc.render(5, false), "long\nlong\nx");
        assert_eq!(doc.render(6, false), "long\nlong x");
    }

    #[test]
    fn fill_allows_later_child_group_to_break() {
        let doc = Doc::fill_sep([Doc::text("a"), Doc::sep(["long", "long"].map(Doc::text))]);
        assert_eq!(doc.render(5, false), "a\nlong\nlong");
        assert_eq!(doc.render(6, false), "a long\nlong");
    }

    #[test]
    fn fill_preserves_nested_fill_choices() {
        let doc = Doc::fill_sep([Doc::text("x"), Doc::fill_sep(["ab", "cd"].map(Doc::text))]);
        assert_eq!(doc.render(4, false), "x ab\ncd");
    }

    #[test]
    fn fill_nested_layout_keeps_styles_and_alignment() {
        let doc = Doc::cat([
            Doc::text("x "),
            Doc::align(Doc::fill_sep([
                Doc::sep(["long", "long"].map(Doc::text)).green(),
                Doc::text("z"),
            ])),
        ]);
        assert_eq!(doc.render(8, false), "x long\n  long z");
        assert!(
            doc.chunks(8)
                .iter()
                .any(|c| matches!(c, Chunk::Styled {text,..} if text == "long"))
        );
    }

    #[test]
    fn group_fits_when_suffix_group_can_break() {
        let doc = Doc::cat([
            Doc::sep(["a", "b"].map(Doc::text)),
            Doc::sep(["c", "dddd"].map(Doc::text)),
        ]);
        assert_eq!(doc.render(4, false), "a bc\ndddd");
    }

    #[test]
    fn reflow_wraps_at_80() {
        insta::assert_snapshot!(Doc::reflow(PARAGRAPH).render(80, false));
    }
    #[test]
    fn reflow_inside_indent_keeps_indent() {
        insta::assert_snapshot!(Doc::indent(4, Doc::reflow(PARAGRAPH)).render(80, false));
    }
    #[test]
    fn stack_separates_with_blank_line() {
        insta::assert_snapshot!(
            Doc::stack([Doc::text("First."), Doc::text("Second.")]).render(80, false)
        );
    }
    #[test]
    fn sep_flat_when_fits() {
        insta::assert_snapshot!(Doc::sep(["a", "->", "b"].map(Doc::text)).render(80, false));
    }
    #[test]
    fn sep_breaks_when_too_wide() {
        insta::assert_snapshot!(Doc::sep((0..30).map(|_| Doc::text("longword"))).render(80, false));
    }
    #[test]
    fn hang_aligns_continuations() {
        insta::assert_snapshot!(
            Doc::cat([
                Doc::text("0123456789"),
                Doc::hang(4, Doc::sep(["argument", "result"].map(Doc::text)))
            ])
            .render(20, false)
        );
    }
    #[test]
    fn cycle_box() {
        insta::assert_snapshot!(Doc::cycle(4, "a", &["b", "c"]).render(80, false));
    }
    #[test]
    fn fancy_note_underlines_word() {
        assert!(
            Doc::to_simple_note("A note.")
                .render(80, true)
                .contains("\x1b[4mNote")
        );
    }
    #[test]
    fn chunks_merge_plain_runs() {
        insta::assert_debug_snapshot!(
            Doc::cat([
                Doc::text("a"),
                Doc::text("b"),
                Doc::text("c").dullyellow(),
                Doc::text("d").dullyellow(),
                Doc::text("e")
            ])
            .chunks(80)
        );
    }
    #[test]
    fn int_to_ordinal_table() {
        assert_eq!(
            [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111]
                .map(int_to_ordinal)
                .join(" "),
            "1st 2nd 3rd 4th 11th 12th 13th 21st 22nd 23rd 101st 111th"
        );
    }
    #[test]
    fn comma_sep_three() {
        assert_eq!(
            Doc::comma_sep("and", |d| d, ["a", "b", "c"].map(Doc::text).to_vec())
                .iter()
                .map(|d| d.render(80, false))
                .collect::<Vec<_>>(),
            ["a,", "b,", "and", "c"]
        );
    }
    #[test]
    fn group_accounts_for_suffix() {
        assert_eq!(
            Doc::cat([Doc::sep(["ab", "cd"].map(Doc::text)), Doc::text("ef")]).render(6, false),
            "ab\ncdef"
        );
    }
    #[test]
    fn line_break_disappears_only_in_flat_mode() {
        let d = Doc::Group(Box::new(Doc::cat([
            Doc::text("ab"),
            Doc::LineBreak,
            Doc::text("cd"),
        ])));
        assert_eq!(d.render(4, false), "abcd");
        assert_eq!(d.render(3, false), "ab\ncd");
    }
    #[test]
    fn hardline_survives_group() {
        assert_eq!(
            Doc::Group(Box::new(Doc::vcat([Doc::text("a"), Doc::text("b")]))).render(80, false),
            "a\nb"
        );
    }
    #[test]
    fn unicode_width_counts_characters() {
        assert_eq!(Doc::reflow("éé λλ x").render(5, false), "éé λλ\nx");
    }
    #[test]
    fn nested_styles_restore_outer() {
        let chunks = Doc::cat([Doc::text("a"), Doc::text("b").green(), Doc::text("c")])
            .dullred()
            .underline()
            .chunks(80);
        assert_eq!(chunks.len(), 3);
        assert!(
            matches!(&chunks[2],Chunk::Styled{style,text} if style.underline && style.color == Some(Color{base:BaseColor::Red,vivid:false}) && text == "c")
        );
    }
    #[test]
    fn trims_spaces_across_styles() {
        assert_eq!(
            Doc::vcat([
                Doc::cat([Doc::text("a "), Doc::text(" ").red()]),
                Doc::text("b  ")
            ])
            .render(80, false),
            "a\nb"
        );
    }
    #[test]
    fn links_use_nash_hint_site() {
        assert_eq!(
            Doc::reflow_link("Read", "imports", "for help.").render(80, false),
            "Read <https://nash-script.dev/hints/imports> for help."
        );
    }
    #[test]
    fn json_preserves_dull_and_vivid_colors() {
        assert_eq!(
            Doc::cat([
                Doc::text("plain"),
                Doc::text("dull").dullyellow(),
                Doc::text("vivid").yellow(),
                Doc::text("bold").bold()
            ])
            .encode(),
            serde_json::json!([
                "plain",
                {"bold":false,"underline":false,"color":"yellow","string":"dull"},
                {"bold":false,"underline":false,"color":"YELLOW","string":"vivid"},
                {"bold":true,"underline":false,"color":null,"string":"bold"}
            ])
        );
    }
    #[test]
    fn outer_group_counts_fill_separators() {
        let d = Doc::sep([Doc::fill_sep(["ab", "cd"].map(Doc::text)), Doc::text("ef")]);
        assert_eq!(d.render(7, false), "ab cd\nef");
        assert_eq!(d.render(8, false), "ab cd ef");
    }
    #[test]
    fn indent_at_current_column() {
        assert_eq!(
            Doc::cat([
                Doc::text("prefix "),
                Doc::indent(2, Doc::vcat(["a", "b"].map(Doc::text)))
            ])
            .render(80, false),
            "prefix   a\n         b"
        );
    }
    #[test]
    fn narrow_width_preserves_long_words() {
        assert_eq!(Doc::reflow("long word").render(0, false), "long\nword");
    }
    #[test]
    fn empty_documents_render_empty() {
        for d in [
            Doc::Empty,
            Doc::text(""),
            Doc::fill_sep([]),
            Doc::sep([]),
            Doc::stack([]),
        ] {
            assert_eq!(d.render(0, false), "");
            assert!(d.chunks(80).is_empty());
        }
    }
    #[test]
    fn paragraph_blank_lines_have_no_indentation() {
        assert_eq!(
            Doc::indent(4, Doc::stack(["a", "b"].map(Doc::text))).render(80, false),
            "    a\n\n    b"
        );
    }
    #[test]
    fn argument_and_ordinal_helpers() {
        assert_eq!(args(0), "0 arguments");
        assert_eq!(args(1), "1 argument");
        assert_eq!(more_args(1), "1 more argument");
        assert_eq!(more_args(2), "2 more arguments");
        assert_eq!(ordinal(0), "1st");
    }
    #[test]
    #[should_panic(expected = "must not contain newlines")]
    fn text_rejects_newlines() {
        Doc::text("a\nb");
    }
}

/// A rendered run, suitable for Elm's JSON message array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Plain(String),
    Styled { style: Style, text: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

#[derive(Clone, Copy)]
enum Work<'a> {
    Doc(usize, Mode, Style, &'a Doc),
    Fill(usize, Mode, Style, &'a [Doc]),
}

fn merge(outer: Style, inner: Style) -> Style {
    Style {
        bold: outer.bold || inner.bold,
        underline: outer.underline || inner.underline,
        color: inner.color.or(outer.color),
    }
}

/// Probe the entire pending line, including the suffix outside a group.
fn fits(width: usize, mut column: usize, mut work: Vec<Work<'_>>) -> bool {
    while let Some(item) = work.pop() {
        if column > width {
            return false;
        }
        match item {
            Work::Fill(indent, mode, style, docs) => {
                if let Some((first, rest)) = docs.split_first() {
                    if mode == Mode::Break {
                        // A fill separator is `group line`, so the pending
                        // line can always end here if its flat choice fails.
                        return true;
                    }
                    column = column.saturating_add(1);
                    work.push(Work::Fill(indent, mode, style, rest));
                    work.push(Work::Doc(indent, mode, style, first));
                }
            }
            Work::Doc(indent, mode, style, doc) => match doc {
                Doc::Empty => {}
                Doc::Text(s) => column = column.saturating_add(s.chars().count()),
                Doc::Styled(st, inner) => {
                    work.push(Work::Doc(indent, mode, merge(style, *st), inner))
                }
                Doc::Nest(n, inner) => {
                    work.push(Work::Doc(indent.saturating_add(*n), mode, style, inner))
                }
                Doc::Align(inner) => work.push(Work::Doc(column, mode, style, inner)),
                Doc::Cat(docs) => {
                    work.extend(docs.iter().rev().map(|d| Work::Doc(indent, mode, style, d)))
                }
                // Only a flat ancestor flattens this group. An unflattened
                // suffix may break: probing its broken branch suffices because
                // the flat choice is selected only when that whole line fits.
                Doc::Group(inner) => work.push(Work::Doc(indent, mode, style, inner)),
                Doc::Line if mode == Mode::Flat => column = column.saturating_add(1),
                Doc::LineBreak if mode == Mode::Flat => {}
                Doc::Line | Doc::LineBreak | Doc::HardLine => return true,
                Doc::Fill(docs) => {
                    if let Some((first, rest)) = docs.split_first() {
                        work.push(Work::Fill(indent, mode, style, rest));
                        work.push(Work::Doc(indent, mode, style, first));
                    }
                }
            },
        }
    }
    column <= width
}

fn push_run(runs: &mut Vec<(Style, String)>, style: Style, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some((last_style, last_text)) = runs.last_mut()
        && *last_style == style
    {
        last_text.push_str(text);
    } else {
        runs.push((style, text.to_owned()));
    }
}

fn trim_spaces(runs: &mut Vec<(Style, String)>) {
    while let Some((_, text)) = runs.last_mut() {
        text.truncate(text.trim_end_matches(' ').len());
        if !text.is_empty() {
            break;
        }
        runs.pop();
    }
}

fn newline(runs: &mut Vec<(Style, String)>, indent: usize) {
    trim_spaces(runs);
    push_run(runs, Style::default(), "\n");
    push_run(runs, Style::default(), &" ".repeat(indent));
}

fn layout(doc: &Doc, width: usize) -> Vec<(Style, String)> {
    let mut runs = Vec::new();
    let mut column: usize = 0;
    let mut work = vec![Work::Doc(0, Mode::Break, Style::default(), doc)];
    while let Some(item) = work.pop() {
        match item {
            Work::Fill(indent, mode, style, docs) => {
                if let Some((first, rest)) = docs.split_first() {
                    let mut probe = work.clone();
                    probe.push(Work::Fill(indent, mode, style, rest));
                    probe.push(Work::Doc(indent, mode, style, first));
                    if mode == Mode::Flat || fits(width, column.saturating_add(1), probe) {
                        push_run(&mut runs, style, " ");
                        column = column.saturating_add(1);
                    } else {
                        newline(&mut runs, indent);
                        column = indent;
                    }
                    work.push(Work::Fill(indent, mode, style, rest));
                    work.push(Work::Doc(indent, mode, style, first));
                }
            }
            Work::Doc(indent, mode, style, doc) => match doc {
                Doc::Empty => {}
                Doc::Text(s) => {
                    push_run(&mut runs, style, s);
                    column = column.saturating_add(s.chars().count());
                }
                Doc::Styled(st, inner) => {
                    work.push(Work::Doc(indent, mode, merge(style, *st), inner))
                }
                Doc::Nest(n, inner) => {
                    work.push(Work::Doc(indent.saturating_add(*n), mode, style, inner))
                }
                Doc::Align(inner) => work.push(Work::Doc(column, mode, style, inner)),
                Doc::Cat(docs) => {
                    work.extend(docs.iter().rev().map(|d| Work::Doc(indent, mode, style, d)))
                }
                Doc::Group(inner) => {
                    let mut probe = work.clone();
                    probe.push(Work::Doc(indent, Mode::Flat, style, inner));
                    let chosen = if mode == Mode::Flat || fits(width, column, probe) {
                        Mode::Flat
                    } else {
                        Mode::Break
                    };
                    work.push(Work::Doc(indent, chosen, style, inner));
                }
                Doc::Line if mode == Mode::Flat => {
                    push_run(&mut runs, style, " ");
                    column = column.saturating_add(1);
                }
                Doc::LineBreak if mode == Mode::Flat => {}
                Doc::Line | Doc::LineBreak | Doc::HardLine => {
                    newline(&mut runs, indent);
                    column = indent;
                }
                Doc::Fill(docs) => {
                    if let Some((first, rest)) = docs.split_first() {
                        work.push(Work::Fill(indent, mode, style, rest));
                        work.push(Work::Doc(indent, mode, style, first));
                    }
                }
            },
        }
    }
    trim_spaces(&mut runs);
    // Trimming may expose adjacent runs with the same style.
    let mut merged = Vec::new();
    for (style, text) in runs {
        push_run(&mut merged, style, &text);
    }
    merged
}

fn ansi_open(style: Style) -> String {
    let mut codes = Vec::new();
    if style.bold {
        codes.push(1);
    }
    if style.underline {
        codes.push(4);
    }
    if let Some(color) = style.color {
        let base = match color.base {
            BaseColor::Black => 30,
            BaseColor::Red => 31,
            BaseColor::Green => 32,
            BaseColor::Yellow => 33,
            BaseColor::Blue => 34,
            BaseColor::Magenta => 35,
            BaseColor::Cyan => 36,
            BaseColor::White => 37,
        };
        codes.push(base + if color.vivid { 60 } else { 0 });
    }
    format!(
        "\x1b[{}m",
        codes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(";")
    )
}

fn render(doc: &Doc, width: usize, color: bool) -> String {
    let mut out = String::new();
    for (style, text) in layout(doc, width) {
        if color && style != Style::default() {
            out.push_str(&ansi_open(style));
            out.push_str(&text);
            out.push_str("\x1b[0m");
        } else {
            out.push_str(&text);
        }
    }
    out
}

fn chunks(doc: &Doc, width: usize) -> Vec<Chunk> {
    layout(doc, width)
        .into_iter()
        .map(|(style, text)| {
            if style == Style::default() {
                Chunk::Plain(text)
            } else {
                Chunk::Styled { style, text }
            }
        })
        .collect()
}

impl Doc {
    /// Elm's `toLine`: disable optional wrapping, retaining explicit lines.
    pub fn to_line(&self) -> String {
        self.render(usize::MAX / 2, false)
    }

    /// Elm's `encode`, including its case-sensitive color names.
    pub fn encode(&self) -> serde_json::Value {
        serde_json::Value::Array(self.chunks(80).into_iter().map(|chunk| match chunk {
            Chunk::Plain(text) => serde_json::Value::String(text),
            Chunk::Styled {style,text} => serde_json::json!({"bold":style.bold,"underline":style.underline,"color":style.color.map(Color::json_name),"string":text}),
        }).collect())
    }
}

impl Color {
    pub fn json_name(self) -> String {
        let name = match self.base {
            BaseColor::Black => "black",
            BaseColor::Red => "red",
            BaseColor::Green => "green",
            BaseColor::Yellow => "yellow",
            BaseColor::Blue => "blue",
            BaseColor::Magenta => "magenta",
            BaseColor::Cyan => "cyan",
            BaseColor::White => "white",
        };
        if self.vivid {
            name.to_ascii_uppercase()
        } else {
            name.to_owned()
        }
    }
}
