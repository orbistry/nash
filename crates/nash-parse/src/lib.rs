use bumpalo::Bump;
use nash_region::{Located, Position, Region};

mod bytes;
mod declaration;
pub mod error;
mod exposing;
mod expression;
mod import;
pub mod keyword;
mod module;
mod number;
mod pattern;
mod space;
mod string;
pub mod symbol;
#[cfg(test)]
pub(crate) mod test_support;
mod tests_block;
mod type_;

pub type Row = usize;
pub type Col = usize;

/// Saved parser state for backtracking.
#[derive(Clone, Copy)]
struct ParserState {
    pos: usize,
    indent: usize,
    row: Row,
    col: Col,
}

/// Parser for Nash source code.
///
/// Combines the arena allocator with parsing state for a unified API.
/// All parsed AST nodes are allocated in the provided bump arena.
///
/// The source text should already be allocated in the arena (via `bump.alloc_str`),
/// so all string slices in the resulting AST share the `'a` lifetime.
pub struct Parser<'a> {
    /// Arena allocator for AST nodes
    bump: &'a Bump,
    /// Source bytes (UTF-8, already in arena)
    src: &'a [u8],
    /// Current byte position
    pos: usize,
    /// Current indentation level (for layout-sensitive parsing)
    indent: usize,
    /// Current row (1-indexed)
    row: Row,
    /// Current column (1-indexed)
    col: Col,
    depth: usize,
    depth_error: Option<(Row, Col)>,
}

impl<'a> Parser<'a> {
    /// Create a new parser for valid UTF-8 source text.
    ///
    /// The source should already be allocated in the arena.
    ///
    /// Arbitrary byte buffers are not a parser input:
    /// ```compile_fail
    /// let bump = bumpalo::Bump::new();
    /// nash_parse::Parser::new(&bump, &[b'"', 0xff, b'"']);
    /// ```
    pub fn new(bump: &'a Bump, src: &'a str) -> Self {
        Parser {
            bump,
            src: src.as_bytes(),
            pos: 0,
            // Elm starts at 0; 1 is behaviorally identical because
            // `checkIndent`'s `col > 1` guard dominates at top level.
            indent: 1,
            row: 1,
            col: 1,
            depth: 0,
            depth_error: None,
        }
    }

    /// Count recursive expression, pattern, and type entries together. This state
    /// is deliberately outside ParserState: backtracking cannot undo exhaustion.
    fn with_depth<T, E>(
        &mut self,
        to_error: impl FnOnce(error::Space, Row, Col) -> E,
        parse: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        const MAX_DEPTH: usize = 64;
        if self.depth == MAX_DEPTH || self.depth_error.is_some() {
            let position = self.position();
            let (row, col) = *self.depth_error.get_or_insert(position);
            return Err(to_error(error::Space::TooDeep, row, col));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        // A speculative parser may swallow an error. Never turn exhaustion into
        // a successful prefix parse, even when it restored the input position.
        let result = match (result, self.depth_error) {
            (Ok(_), Some((row, col))) => Err(to_error(error::Space::TooDeep, row, col)),
            (result, _) => result,
        };
        if self.depth == 0 {
            self.depth_error = None;
        }
        result
    }

    // -------------------------------------------------------------------------
    // Position & State
    // -------------------------------------------------------------------------

    /// Current position as (row, col).
    #[inline]
    pub fn position(&self) -> (Row, Col) {
        (self.row, self.col)
    }

    /// Get the current position as a `Position`.
    #[inline]
    pub fn get_position(&self) -> Position {
        Position::new(self.row, self.col)
    }

    /// Create a `Located` value spanning from `start` to the current position,
    /// allocated directly in the arena.
    #[inline]
    pub fn add_end<T>(&self, start: Position, value: T) -> &'a Located<T> {
        let end = self.get_position();
        self.alloc(Located::at(Region::new(start, end), value))
    }

    /// Current row (1-indexed).
    #[inline]
    pub fn row(&self) -> Row {
        self.row
    }

    /// Current column (1-indexed).
    #[inline]
    pub fn col(&self) -> Col {
        self.col
    }

    /// Current indentation level.
    #[inline]
    pub fn indent(&self) -> usize {
        self.indent
    }

    /// Set the indentation level.
    #[inline]
    pub fn set_indent(&mut self, indent: usize) {
        self.indent = indent;
    }

    /// Run a parser with the current column as the indent level,
    /// then restore the old indent level.
    ///
    /// Mirrors Elm's `withIndent`:
    /// ```haskell
    /// withIndent (Parser parser) =
    ///   Parser $ \(State src pos end oldIndent row col) cok eok cerr eerr ->
    ///     let
    ///       cok' a (State s p e _ r c) = cok a (State s p e oldIndent r c)
    ///       eok' a (State s p e _ r c) = eok a (State s p e oldIndent r c)
    ///     in
    ///     parser (State src pos end col row col) cok' eok' cerr eerr
    /// ```
    pub fn with_indent<T, E>(
        &mut self,
        parser: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let old_indent = self.indent;
        self.indent = self.col;
        let result = parser(self);
        self.indent = old_indent;
        result
    }

    /// Run a parser with indent set to (current column - backset),
    /// then restore the old indent level.
    ///
    /// Mirrors Elm's `withBacksetIndent`:
    /// ```haskell
    /// withBacksetIndent backset (Parser parser) =
    ///   Parser $ \(State src pos end oldIndent row col) cok eok cerr eerr ->
    ///     parser (State src pos end (col - backset) row col) ...
    /// ```
    pub fn with_backset_indent<T, E>(
        &mut self,
        backset: usize,
        parser: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let old_indent = self.indent;
        self.indent = self.col.saturating_sub(backset);
        let result = parser(self);
        self.indent = old_indent;
        result
    }

    /// Check if we've reached the end of input.
    #[inline]
    pub fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// Save current parser state for backtracking.
    #[inline]
    fn save_state(&self) -> ParserState {
        ParserState {
            pos: self.pos,
            indent: self.indent,
            row: self.row,
            col: self.col,
        }
    }

    /// Restore parser state for backtracking.
    #[inline]
    fn restore_state(&mut self, state: ParserState) {
        self.pos = state.pos;
        self.indent = state.indent;
        self.row = state.row;
        self.col = state.col;
    }

    // -------------------------------------------------------------------------
    // Combinators
    // -------------------------------------------------------------------------

    /// Try multiple parsers in order, returning the first success.
    ///
    /// Mirrors Elm's `oneOf`:
    /// ```haskell
    /// oneOf :: (Row -> Col -> x) -> [Parser x a] -> Parser x a
    /// ```
    ///
    /// Key semantics:
    /// - If a parser fails without consuming input, try the next one
    /// - If a parser fails after consuming input, propagate the error (committed)
    /// - If all parsers fail without consuming, call `to_error(row, col)`
    ///
    /// # Example
    /// ```ignore
    /// parser.one_of(
    ///     error::Expr::Start,
    ///     vec![
    ///         Box::new(|p: &mut Parser| p.string(start)),
    ///         Box::new(|p| p.number(start)),
    ///     ],
    /// )
    /// ```
    #[allow(clippy::type_complexity)]
    pub fn one_of<T, E>(
        &mut self,
        to_error: impl FnOnce(Row, Col) -> E,
        parsers: Vec<Box<dyn FnOnce(&mut Self) -> Result<T, E> + '_>>,
    ) -> Result<T, E> {
        let initial_state = self.save_state();

        for parser in parsers {
            let before = self.save_state();
            match parser(self) {
                Ok(value) => return Ok(value),
                Err(e) => {
                    // Did we consume any input?
                    if self.pos != before.pos || self.depth_error.is_some() {
                        // Committed - propagate error
                        return Err(e);
                    }
                    // No input consumed - restore and try next
                    self.restore_state(before);
                }
            }
        }

        // All parsers failed without consuming - restore to initial and return error
        self.restore_state(initial_state);
        let (row, col) = self.position();
        Err(to_error(row, col))
    }

    /// Like `one_of` but returns a fallback value if nothing matches.
    ///
    /// Mirrors Elm's `oneOfWithFallback`:
    /// ```haskell
    /// oneOfWithFallback :: [Parser x a] -> a -> Parser x a
    /// ```
    #[allow(clippy::type_complexity)]
    pub fn one_of_with_fallback<T, E>(
        &mut self,
        parsers: Vec<Box<dyn FnOnce(&mut Self) -> Result<T, E> + '_>>,
        fallback: T,
    ) -> Result<T, E> {
        let initial_state = self.save_state();

        for parser in parsers {
            let before = self.save_state();
            match parser(self) {
                Ok(value) => return Ok(value),
                Err(e) => {
                    // Did we consume any input?
                    if self.pos != before.pos || self.depth_error.is_some() {
                        // Committed - propagate error
                        return Err(e);
                    }
                    // No input consumed - restore and try next
                    self.restore_state(before);
                }
            }
        }

        // All parsers failed without consuming - return fallback
        self.restore_state(initial_state);
        Ok(fallback)
    }

    /// Parse with error context wrapping.
    ///
    /// Mirrors Elm's `inContext`:
    /// ```haskell
    /// inContext :: (x -> Row -> Col -> y) -> Parser y start -> Parser x a -> Parser y a
    /// ```
    ///
    /// 1. Saves the starting position
    /// 2. Runs `start_parser` - if it fails without consuming, returns that error
    /// 3. If start succeeds, runs `body_parser`
    /// 4. If body fails, wraps the error using `add_context` at the original position
    ///
    /// The `add_context` closure receives the bump allocator so it can allocate wrapped errors.
    ///
    /// This is used to provide better error context, e.g., "error in list expression".
    pub fn in_context<T, StartErr, BodyErr, ContextErr>(
        &mut self,
        add_context: impl FnOnce(&'a Bump, BodyErr, Row, Col) -> ContextErr,
        start_parser: impl FnOnce(&mut Self) -> Result<(), StartErr>,
        body_parser: impl FnOnce(&mut Self) -> Result<T, BodyErr>,
    ) -> Result<T, ContextErr>
    where
        StartErr: Into<ContextErr>,
    {
        let (start_row, start_col) = self.position();

        // Try to parse start token
        match start_parser(self) {
            Ok(()) => {
                // Start succeeded, now parse body
                match body_parser(self) {
                    Ok(value) => Ok(value),
                    Err(body_err) => {
                        // Wrap body error with context at original position
                        Err(add_context(self.bump, body_err, start_row, start_col))
                    }
                }
            }
            Err(start_err) => {
                // Start failed - convert to context error type
                Err(start_err.into())
            }
        }
    }

    /// Transform errors from one type to another with position context.
    ///
    /// Mirrors Elm's `specialize`:
    /// ```haskell
    /// specialize :: (x -> Row -> Col -> y) -> Parser x a -> Parser y a
    /// ```
    ///
    /// Runs the parser and wraps any error with the context at the starting position.
    /// The `add_context` closure receives the bump allocator so it can allocate wrapped errors.
    pub fn specialize<T, InnerErr, OuterErr>(
        &mut self,
        add_context: impl FnOnce(&'a Bump, InnerErr, Row, Col) -> OuterErr,
        parser: impl FnOnce(&mut Self) -> Result<T, InnerErr>,
    ) -> Result<T, OuterErr> {
        let (start_row, start_col) = self.position();

        match parser(self) {
            Ok(value) => Ok(value),
            Err(inner_err) => Err(add_context(self.bump, inner_err, start_row, start_col)),
        }
    }

    // -------------------------------------------------------------------------
    // Single-byte parsing
    // -------------------------------------------------------------------------

    /// Parse a single expected byte.
    ///
    /// Mirrors Elm's `word1`:
    /// ```haskell
    /// word1 :: Word8 -> (Row -> Col -> x) -> Parser x ()
    /// ```
    ///
    /// Returns `Ok(())` and advances if the byte matches.
    /// Returns `Err` without consuming if it doesn't match.
    #[inline]
    pub fn word1<E>(
        &mut self,
        expected: u8,
        to_error: impl FnOnce(Row, Col) -> E,
    ) -> Result<(), E> {
        if self.peek() == Some(expected) {
            self.advance();
            Ok(())
        } else {
            let (row, col) = self.position();
            Err(to_error(row, col))
        }
    }

    /// Parse two expected consecutive bytes.
    ///
    /// Mirrors Elm's `word2`:
    /// ```haskell
    /// word2 :: Word8 -> Word8 -> (Row -> Col -> x) -> Parser x ()
    /// ```
    #[inline]
    pub fn word2<E>(
        &mut self,
        b1: u8,
        b2: u8,
        to_error: impl FnOnce(Row, Col) -> E,
    ) -> Result<(), E> {
        if self.peek() == Some(b1) && self.peek_at(1) == Some(b2) {
            self.advance();
            self.advance();
            Ok(())
        } else {
            let (row, col) = self.position();
            Err(to_error(row, col))
        }
    }

    // -------------------------------------------------------------------------
    // Peeking
    // -------------------------------------------------------------------------

    /// Peek at the current byte without consuming it.
    #[inline]
    pub fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// Peek at a byte at the given offset from current position.
    #[inline]
    pub fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos.checked_add(offset)?).copied()
    }

    /// Get the remaining bytes from current position.
    #[inline]
    pub fn remaining(&self) -> &'a [u8] {
        &self.src[self.pos..]
    }

    // -------------------------------------------------------------------------
    // Advancing
    // -------------------------------------------------------------------------

    /// Advance by one byte, updating row/col for newlines.
    #[inline]
    pub fn advance(&mut self) {
        if let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'\n' {
                self.row += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    /// Advance by n bytes, tracking newlines.
    #[inline]
    pub fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    // -------------------------------------------------------------------------
    // Allocation helpers
    // -------------------------------------------------------------------------

    /// Allocate a value in the arena.
    #[inline]
    pub fn alloc<T>(&self, value: T) -> &'a T {
        self.bump.alloc(value)
    }

    /// Allocate a slice in the arena by copying.
    #[inline]
    pub fn alloc_slice_copy<T: Copy>(&self, slice: &[T]) -> &'a [T] {
        self.bump.alloc_slice_copy(slice)
    }

    /// Allocate a string in the arena (for constructed strings like escape sequences).
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &'a str {
        self.bump.alloc_str(s)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nesting_is_bounded_on_a_small_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                for depth in [8, 512] {
                    for (open, leaf, close, kind) in [
                        ("(", "1", ")", 0),
                        ("(-", "1", ")", 0),
                        ("[", "1", "]", 0),
                        ("\\x -> ", "1", "", 0),
                        ("Just (", "x", ")", 1),
                        ("(", "x", ")", 2),
                        ("Int -> ", "Int", "", 3),
                        ("(", "Int", ")", 4),
                    ] {
                        let bump = Bump::new();
                        let source =
                            format!("{}{}{}", open.repeat(depth), leaf, close.repeat(depth));
                        let mut parser = Parser::new(&bump, &source);
                        let result = match kind {
                            0 => parser
                                .expression()
                                .map(|_| ())
                                .map_err(|e| format!("{e:?}")),
                            1 => parser
                                .pattern_expr()
                                .map(|_| ())
                                .map_err(|e| format!("{e:?}")),
                            2 => parser
                                .pattern_term()
                                .map(|_| ())
                                .map_err(|e| format!("{e:?}")),
                            3 => parser.type_expr().map(|_| ()).map_err(|e| format!("{e:?}")),
                            _ => parser.type_term().map(|_| ()).map_err(|e| format!("{e:?}")),
                        };
                        if depth == 8 {
                            assert!(result.is_ok(), "{open}: {result:?}");
                            assert!(parser.is_eof());
                        } else {
                            assert!(result.unwrap_err().contains("TooDeep"), "{open}");
                        }
                        assert_eq!(parser.depth, 0);
                        assert_eq!(parser.depth_error, None);
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn nesting_exhaustion_survives_backtracking_and_resets() {
        let bump = Bump::new();
        let mut parser = Parser::new(&bump, "x");
        let result = parser.with_depth(error::Expr::Space, |p| {
            let saved = p.save_state();
            p.depth_error = Some((1, 1));
            p.restore_state(saved);
            p.one_of_with_fallback(
                vec![Box::new(|_| {
                    Err(error::Expr::Space(error::Space::TooDeep, 1, 1))
                })],
                (),
            )
        });
        assert!(matches!(
            result,
            Err(error::Expr::Space(error::Space::TooDeep, 1, 1))
        ));
        parser.expression().unwrap();
        assert!(parser.is_eof());
    }

    #[test]
    fn flat_sequences_use_bounded_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let expressions = [
                    format!("x{}", ".field".repeat(2_000)),
                    format!("\\{}-> x", "x ".repeat(2_000)),
                    format!("{}0", "if True then 1 else ".repeat(2_000)),
                    format!("let\n{}in x", "    x = 1\n".repeat(2_000)),
                    format!("case x of\n{}", "    _ -> 1\n".repeat(2_000)),
                ];
                for source in expressions {
                    let source = crate::test_support::indent_fragment(&source);
                    let bump = Bump::new();
                    let mut parser = Parser::new(&bump, &source);
                    parser.chomp(|_, _, _| ()).unwrap();
                    parser.expression().unwrap();
                    assert!(parser.is_eof());
                }
                let bump = Bump::new();
                let source = format!("type Many = A{}", " | A".repeat(2_000));
                let mut parser = Parser::new(&bump, &source);
                parser.declaration().unwrap();
                assert!(parser.is_eof());
                let source = format!("{}{}", "{-".repeat(65_536), "-}".repeat(65_536));
                let mut parser = Parser::new(&bump, &source);
                parser.chomp(|_, _, _| ()).unwrap();
                assert!(parser.is_eof());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn coordinates_cover_large_sources() {
        let bump = Bump::new();
        let lines = "\n".repeat(65_536);
        let mut parser = Parser::new(&bump, &lines);
        parser.advance_by(lines.len());
        assert_eq!(parser.row(), 65_537);
        assert_eq!(parser.col(), 1);

        let string = format!("\"{}\"", "x".repeat(65_536));
        let mut parser = Parser::new(&bump, &string);
        parser.expression().expect("long string");
        assert!(parser.is_eof());
        assert_eq!(parser.col(), string.len() + 1);

        let indented = format!("{}x", " ".repeat(65_536));
        let mut parser = Parser::new(&bump, &indented);
        parser.chomp(|_, _, _| ()).expect("long indentation");
        assert_eq!(parser.col(), 65_537);
        parser.expression().expect("indented expression");
        assert!(parser.is_eof());

        let escaped = format!("\"\\u{{{}}}\"", "F".repeat(65_536));
        let mut parser = Parser::new(&bump, &escaped);
        let error::Expr::String(
            error::StringError::Escape(error::Escape::BadUnicodeCode(width)),
            1,
            3,
        ) = parser.expression().expect_err("oversized Unicode escape")
        else {
            panic!("expected an invalid Unicode code with its full width");
        };
        assert_eq!(width, 65_539);
    }

    #[test]
    fn lookahead_offset_cannot_wrap() {
        let bump = Bump::new();
        let mut parser = Parser::new(&bump, "xy");
        parser.advance();
        assert_eq!(parser.peek_at(usize::MAX), None);
    }

    use super::*;

    #[test]
    fn test_parser_new() {
        let bump = Bump::new();
        let src = bump.alloc_str("hello");
        let parser = Parser::new(&bump, src);

        assert_eq!(parser.row(), 1);
        assert_eq!(parser.col(), 1);
        assert_eq!(parser.peek(), Some(b'h'));
        assert!(!parser.is_eof());
    }

    #[test]
    fn test_parser_advance() {
        let bump = Bump::new();
        let src = bump.alloc_str("ab\ncd");
        let mut parser = Parser::new(&bump, src);

        assert_eq!(parser.position(), (1, 1));
        parser.advance(); // 'a'
        assert_eq!(parser.position(), (1, 2));
        parser.advance(); // 'b'
        assert_eq!(parser.position(), (1, 3));
        parser.advance(); // '\n'
        assert_eq!(parser.position(), (2, 1));
        parser.advance(); // 'c'
        assert_eq!(parser.position(), (2, 2));
    }

    #[test]
    fn test_parser_eof() {
        let bump = Bump::new();
        let src = bump.alloc_str("x");
        let mut parser = Parser::new(&bump, src);

        assert!(!parser.is_eof());
        parser.advance();
        assert!(parser.is_eof());
        assert_eq!(parser.peek(), None);
    }
}
