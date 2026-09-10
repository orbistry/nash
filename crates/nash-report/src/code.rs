//! Source text access for reports, from Elm's `Reporting/Render/Code.hs`.
//! Columns are byte-based, matching `nash_parse::Parser::advance`.

mod snippet;

use miette::SourceSpan;
use nash_parse::{Col, Row};
use nash_region::{Position, Region};

pub struct Source<'s> {
    text: &'s str,
    /// Byte offset of every line start; the last entry is `text.len()`
    /// when the text ends in a newline (Elm's `lines ++ [""]`).
    line_starts: Vec<usize>,
}

impl<'s> Source<'s> {
    pub fn new(text: &'s str) -> Source<'s> {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Source { text, line_starts }
    }

    pub fn text(&self) -> &'s str {
        self.text
    }

    /// Byte offset of a 1-based position produced by the parser.
    pub fn offset(&self, position: Position) -> usize {
        let row = usize::from(position.line.saturating_sub(1));
        let Some(&start) = self.line_starts.get(row) else {
            return self.text.len();
        };
        let end = self
            .line_starts
            .get(row + 1)
            .map_or(self.text.len(), |next| next - 1);
        let mut offset = start
            .saturating_add(usize::from(position.column.saturating_sub(1)))
            .min(end);
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// Keep spans on UTF-8 boundaries. Empty spans highlight the next character;
    /// at EOF they remain empty so renderers can draw an insertion caret.
    pub fn span(&self, region: Region) -> SourceSpan {
        let start = self.offset(region.start);
        let mut end = self.offset(region.end).max(start);
        if end == start && start < self.text.len() {
            end += self.text[start..].chars().next().map_or(0, char::len_utf8);
        }
        (start, end - start).into()
    }

    /// Text of a 1-based row, without its newline.
    pub fn line(&self, row: Row) -> Option<&'s str> {
        let start = *self.line_starts.get(usize::from(row.checked_sub(1)?))?;
        let end = self
            .line_starts
            .get(usize::from(row))
            .map_or(self.text.len(), |next| next - 1);
        Some(&self.text[start..end.max(start)])
    }

    /// Elm's `whatIsNext`.
    pub fn what_is_next(&self, row: Row, col: Col) -> Next<'s> {
        let Some(rest) = self
            .line(row)
            .and_then(|line| line.get(usize::from(col.checked_sub(1)?)..))
        else {
            return Next::Other(None);
        };
        let mut chars = rest.chars();
        let Some(c) = chars.next() else {
            return Next::Other(None);
        };
        let inner_len = |s: &str| {
            s.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .map(char::len_utf8)
                .sum::<usize>()
        };
        if c.is_uppercase() {
            Next::Upper(&rest[..c.len_utf8() + inner_len(chars.as_str())])
        } else if c.is_lowercase() {
            let name = &rest[..c.len_utf8() + inner_len(chars.as_str())];
            if nash_parse::keyword::is_reserved(name) {
                Next::Keyword(name)
            } else {
                Next::Lower(name)
            }
        } else if c.is_ascii() && nash_parse::symbol::is_binop_char(c as u8) {
            let len = rest
                .bytes()
                .take_while(|b| nash_parse::symbol::is_binop_char(*b))
                .count();
            Next::Operator(&rest[..len])
        } else {
            match c {
                ')' => Next::Close("parenthesis", ')'),
                ']' => Next::Close("square bracket", ']'),
                '}' => Next::Close("curly brace", '}'),
                other => Next::Other(Some(other)),
            }
        }
    }

    /// Elm's `nextLineStartsWithKeyword`.
    pub fn next_line_starts_with_keyword(&self, keyword: &str, row: Row) -> Option<(Row, Col)> {
        let line = self.line(row.checked_add(1)?)?;
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        let rest = &line[indent..];
        let follows = rest.strip_prefix(keyword)?;
        let boundary = follows
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        boundary.then_some((row + 1, 1 + indent as Col))
    }

    /// Elm's `nextLineStartsWithCloseCurly`.
    pub fn next_line_starts_with_close_curly(&self, row: Row) -> Option<(Row, Col)> {
        let line = self.line(row.checked_add(1)?)?;
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        line[indent..]
            .starts_with('}')
            .then_some((row + 1, 1 + indent as Col))
    }
}

/// Elm's `Next`.
#[derive(Debug, PartialEq, Eq)]
pub enum Next<'s> {
    Keyword(&'s str),
    Operator(&'s str),
    Close(&'static str, char),
    Upper(&'s str),
    Lower(&'s str),
    Other(Option<char>),
}

/// Elm's `toRegion row col`.
pub fn to_region(row: Row, col: Col) -> Region {
    let pos = Position::new(row, col);
    Region::new(pos, pos)
}

/// Elm's `toWiderRegion`.
pub fn to_wider_region(row: Row, col: Col, extra: u16) -> Region {
    Region::new(
        Position::new(row, col),
        Position::new(row, col.saturating_add(extra)),
    )
}

/// Elm's `toKeywordRegion`.
pub fn to_keyword_region(row: Row, col: Col, keyword: &str) -> Region {
    to_wider_region(row, col, keyword.len() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_positions_and_tokens() {
        let source = Source::new("f = x\n    in value");
        assert_eq!(source.offset(Position::new(2, 5)), 10);
        assert_eq!(source.next_line_starts_with_keyword("in", 1), Some((2, 5)));
        assert_eq!(source.what_is_next(2, 5), Next::Keyword("in"));
        assert_eq!(
            Source::new("+ ) Upper lower").what_is_next(1, 1),
            Next::Operator("+")
        );
        assert_eq!(
            Source::new(")").what_is_next(1, 1),
            Next::Close("parenthesis", ')')
        );
        assert_eq!(
            Source::new("Upper").what_is_next(1, 1),
            Next::Upper("Upper")
        );
        assert_eq!(
            Source::new("lower").what_is_next(1, 1),
            Next::Lower("lower")
        );
        assert_eq!(source.what_is_next(1, 99), Next::Other(None));
        assert_eq!(source.line(0), None);
    }
    #[test]
    fn spans_stay_inside_unicode_source_and_eof() {
        let source = Source::new("é\nlast");
        assert_eq!(source.offset(Position::new(1, 99)), 2);
        assert_eq!(source.offset(Position::new(2, 5)), 7);
        let span = source.span(Region::new(Position::new(2, 5), Position::new(2, 5)));
        assert_eq!((span.offset(), span.len()), (7, 0));
        assert_eq!(source.offset(Position::new(1, 2)), 0);
        assert_eq!(Source::new("").span(Region::zero()).len(), 0);
    }
}
