//! Elm's `Reporting/Render/Code.hs` source drawings for JSON messages.

use super::Source;
use crate::{Doc, Snippet};
use nash_region::Region;
use unicode_width::UnicodeWidthChar;

impl Source<'_> {
    /// Render source using Elm's line numbers, red carets, and multiline arrows.
    pub fn snippet_doc(&self, snippet: &Snippet) -> Doc {
        match snippet {
            Snippet::Region { region, highlight } => self.region_doc(*region, *highlight),
            Snippet::Pair { first, second } => self.pair_doc(first.region, second.region),
            Snippet::None => Doc::Empty,
        }
    }

    /// Elm's `render`, `makeUnderline`, and `drawLines`.
    pub fn region_doc(&self, region: Region, highlight: Option<Region>) -> Doc {
        let start = region.start.line.max(1);
        let end = region.end.line.max(start);
        let lines: Vec<_> = (start..=end)
            .map_while(|row| self.line(row).map(|line| (row, line)))
            .collect();
        let Some(&(last, _)) = lines.last() else {
            return Doc::Empty;
        };
        let width = last.to_string().len();
        let highlight = highlight.unwrap_or(region);
        let underline = highlight.start.line == highlight.end.line && highlight.end.line >= end;
        let mut docs: Vec<_> = lines
            .into_iter()
            .map(|(row, line)| {
                let spacer =
                    if !underline && highlight.start.line <= row && row <= highlight.end.line {
                        Doc::text(">").red()
                    } else {
                        Doc::text(" ")
                    };
                Doc::cat([
                    Doc::text(format!("{row:>width$}|")),
                    spacer,
                    Doc::text(display_line(line)),
                ])
            })
            .collect();
        docs.push(if underline {
            let line = self.line(highlight.start.line).unwrap_or("");
            let (start, end) = visual_range(line, highlight);
            Doc::cat([
                Doc::text(" ".repeat(start + width + 2)),
                Doc::text("^".repeat(end - start)).red(),
            ])
        } else {
            Doc::Empty
        });
        Doc::vcat(docs)
    }

    /// Elm's `renderPair`: one line with two underlines, or two code chunks.
    /// The report's primary region is independent of this source ordering.
    pub fn pair_doc(&self, first: Region, second: Region) -> Doc {
        let (first, second) =
            if (first.start.line, first.start.column) <= (second.start.line, second.start.column) {
                (first, second)
            } else {
                (second, first)
            };
        if first.start.line == first.end.line
            && first.end.line == second.start.line
            && second.start.line == second.end.line
        {
            let row = first.start.line;
            let Some(line) = self.line(row) else {
                return Doc::Empty;
            };
            let width = row.to_string().len();
            let (first_start, first_end) = visual_range(line, first);
            let (second_start, second_end) = visual_range(line, second);
            Doc::vcat([
                Doc::text(format!("{row}| {}", display_line(line))),
                Doc::cat([
                    Doc::text(" ".repeat(first_start + width + 2)),
                    Doc::text("^".repeat(first_end - first_start)).red(),
                    Doc::text(" ".repeat(second_start.saturating_sub(first_end))),
                    Doc::text("^".repeat(second_end - second_start)).red(),
                ]),
            ])
        } else {
            Doc::stack([self.region_doc(first, None), self.region_doc(second, None)])
        }
    }
}

// Match miette's default four-cell tab stops, measured from the source text
// rather than the line-number gutter. Regions remain one-based byte columns.
const TAB_WIDTH: usize = 4;

fn char_width(ch: char, column: usize) -> usize {
    if ch == '\t' {
        TAB_WIDTH - column % TAB_WIDTH
    } else {
        ch.width().unwrap_or(0)
    }
}

fn display_line(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut column = 0;
    for ch in line.chars() {
        let width = char_width(ch, column);
        if ch == '\t' {
            text.extend(std::iter::repeat_n(' ', width));
        } else {
            text.push(ch);
        }
        column += width;
    }
    text
}

fn visual_column(line: &str, byte_column: u16) -> usize {
    let mut offset = usize::from(byte_column.saturating_sub(1)).min(line.len());
    // Match Source::offset when an invalid input points inside a UTF-8 scalar.
    while !line.is_char_boundary(offset) {
        offset -= 1;
    }
    line[..offset]
        .chars()
        .fold(0, |column, ch| column + char_width(ch, column))
}

fn visual_range(line: &str, region: Region) -> (usize, usize) {
    let start = visual_column(line, region.start.column);
    let end = visual_column(line, region.end.column).max(start + 1);
    (start, end)
}

#[cfg(test)]
mod tests {
    use crate::{Doc, Label, Snippet, Source};
    use nash_region::{Position, Region};

    fn region(sr: u16, sc: u16, er: u16, ec: u16) -> Region {
        Region::new(Position::new(sr, sc), Position::new(er, ec))
    }

    #[test]
    fn single_accent_uses_visible_columns() {
        assert_eq!(
            Source::new("é x")
                .region_doc(region(1, 4, 1, 5), None)
                .render(80, false),
            "1| é x\n     ^"
        );
        assert_eq!(
            Source::new("é x")
                .region_doc(region(1, 1, 1, 3), None)
                .render(80, false),
            "1| é x\n   ^"
        );
    }

    #[test]
    fn single_astral_uses_two_cells() {
        assert_eq!(
            Source::new("😀 x")
                .region_doc(region(1, 6, 1, 7), None)
                .render(80, false),
            "1| 😀 x\n      ^"
        );
        assert_eq!(
            Source::new("😀 x")
                .region_doc(region(1, 1, 1, 5), None)
                .render(80, false),
            "1| 😀 x\n   ^^"
        );
    }

    #[test]
    fn single_cjk_uses_two_cells() {
        assert_eq!(
            Source::new("界 x")
                .region_doc(region(1, 5, 1, 6), None)
                .render(80, false),
            "1| 界 x\n      ^"
        );
        assert_eq!(
            Source::new("界 x")
                .region_doc(region(1, 1, 1, 4), None)
                .render(80, false),
            "1| 界 x\n   ^^"
        );
    }

    #[test]
    fn single_tabs_expand_from_source_column() {
        assert_eq!(
            Source::new("\tx")
                .region_doc(region(1, 2, 1, 3), None)
                .render(80, false),
            "1|     x\n       ^"
        );
        assert_eq!(
            Source::new("é\t界 x")
                .region_doc(region(1, 8, 1, 9), None)
                .render(80, false),
            "1| é   界 x\n          ^"
        );
    }

    #[test]
    fn pair_accent_uses_visible_columns() {
        assert_eq!(
            Source::new("é x")
                .pair_doc(region(1, 1, 1, 3), region(1, 4, 1, 5))
                .render(80, false),
            "1| é x\n   ^ ^"
        );
    }

    #[test]
    fn pair_astral_uses_two_cells() {
        assert_eq!(
            Source::new("😀 x")
                .pair_doc(region(1, 1, 1, 5), region(1, 6, 1, 7))
                .render(80, false),
            "1| 😀 x\n   ^^ ^"
        );
    }

    #[test]
    fn pair_cjk_uses_two_cells() {
        assert_eq!(
            Source::new("界 x")
                .pair_doc(region(1, 1, 1, 4), region(1, 5, 1, 6))
                .render(80, false),
            "1| 界 x\n   ^^ ^"
        );
    }

    #[test]
    fn pair_tabs_expand_from_source_column() {
        assert_eq!(
            Source::new("\tx")
                .pair_doc(region(1, 1, 1, 2), region(1, 2, 1, 3))
                .render(80, false),
            "1|     x\n   ^^^^^"
        );
        assert_eq!(
            Source::new("é\t界 x")
                .pair_doc(region(1, 1, 1, 3), region(1, 8, 1, 9))
                .render(80, false),
            "1| é   界 x\n   ^      ^"
        );
    }

    #[test]
    fn combining_marks_and_unicode_insertion() {
        let source = Source::new("e\u{301} x");
        assert_eq!(
            source
                .region_doc(region(1, 5, 1, 6), None)
                .render(80, false),
            "1| e\u{301} x\n     ^"
        );
        assert_eq!(
            source
                .region_doc(region(1, 6, 1, 6), None)
                .render(80, false),
            "1| e\u{301} x\n      ^"
        );
    }

    #[test]
    fn unicode_pair_on_separate_lines() {
        insta::assert_snapshot!(
            Source::new("é x\n界\ty")
                .pair_doc(region(1, 4, 1, 5), region(2, 5, 2, 6))
                .render(80, false)
        );
    }

    #[test]
    fn single_line_and_insertion_carets() {
        let source = Source::new("value = missing");
        assert_eq!(
            source
                .region_doc(region(1, 9, 1, 16), None)
                .render(80, false),
            "1| value = missing\n           ^^^^^^^"
        );
        assert_eq!(
            source
                .region_doc(region(1, 16, 1, 16), None)
                .render(80, false),
            "1| value = missing\n                  ^"
        );
    }

    #[test]
    fn multiline_highlight_arrows_and_line_number_width() {
        let source = Source::new("\n\n\n\n\n\n\n\nfirst\nsecond\nthird");
        assert_eq!(
            source
                .region_doc(region(9, 1, 11, 6), Some(region(10, 1, 10, 7)))
                .render(80, false),
            " 9| first\n10|>second\n11| third\n"
        );
        assert_eq!(
            source
                .region_doc(region(9, 1, 11, 6), Some(region(11, 1, 11, 6)))
                .render(80, false),
            " 9| first\n10| second\n11| third\n    ^^^^^"
        );
    }

    #[test]
    fn same_line_pair_and_separate_chunks() {
        let source = Source::new("first second\nthird");
        assert_eq!(
            source
                .pair_doc(region(1, 1, 1, 6), region(1, 7, 1, 13))
                .render(80, false),
            "1| first second\n   ^^^^^ ^^^^^^"
        );
        assert_eq!(
            source
                .pair_doc(region(1, 1, 1, 6), region(2, 1, 2, 6))
                .render(80, false),
            "1| first second\n   ^^^^^\n\n2| third\n   ^^^^^"
        );
    }

    #[test]
    fn pair_accepts_reverse_source_order() {
        let source = Source::new("first second");
        assert_eq!(
            source
                .pair_doc(region(1, 7, 1, 13), region(1, 1, 1, 6))
                .render(80, false),
            "1| first second\n   ^^^^^ ^^^^^^"
        );
    }

    #[test]
    fn empty_source_and_absent_snippet() {
        assert_eq!(
            Source::new("")
                .region_doc(region(1, 1, 1, 1), None)
                .render(80, false),
            "1|\n   ^"
        );
        assert_eq!(Source::new("").snippet_doc(&Snippet::None), Doc::Empty);
    }

    #[test]
    fn pair_snippet_snapshot() {
        let source = Source::new("a = x\nb = y");
        let snippet = Snippet::Pair {
            first: Label {
                region: region(1, 5, 1, 6),
                text: "first use".into(),
            },
            second: Label {
                region: region(2, 5, 2, 6),
                text: "second use".into(),
            },
        };
        insta::assert_snapshot!(source.snippet_doc(&snippet).render(80, false));
    }
}
