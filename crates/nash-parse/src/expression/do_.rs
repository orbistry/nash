use nash_region::{Located, Position, Region};
use nash_source::{Expr, Stmt};

use crate::Parser;
use crate::error::{self, Do, Let};

pub(crate) type DoBody<'a> = (&'a [&'a Located<Stmt<'a>>], &'a Located<Expr<'a>>, Position);

impl<'a> Parser<'a> {
    pub(crate) fn do_(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            |bump, error, row, col| error::Expr::Do(bump.alloc(error), row, col),
            |parser| parser.keyword_do(error::Expr::Start),
            |parser| {
                parser.chomp_and_check_indent(Do::Space, Do::IndentStmt)?;
                let parent_indent = parser.indent;
                let (stmts, last, end) =
                    parser.with_indent(|parser| parser.do_body(parent_indent))?;
                Ok((
                    parser.alloc(Located::at(
                        Region::new(start, end),
                        Expr::Do { stmts, last },
                    )),
                    end,
                ))
            },
        )
    }

    /// Parse aligned statements ending in an expression.
    pub(crate) fn do_body(&mut self, parent_indent: u16) -> Result<DoBody<'a>, Do<'a>> {
        let (first, mut end) = self.do_stmt()?;
        let mut statements = vec![first];

        loop {
            if self.is_eof() || self.col <= parent_indent {
                break;
            }
            if self.col != self.indent {
                return Err(Do::Alignment(self.indent, self.row, self.col));
            }
            let (statement, new_end) = self.do_stmt()?;
            statements.push(statement);
            end = new_end;
        }

        let (last, initial) = statements.split_last().expect("do has a statement");
        let last = match last.value {
            Stmt::Expr(expr) => expr,
            Stmt::Bind { .. } | Stmt::Let(_) => {
                return Err(Do::LastNotExpr(
                    last.region.start.line,
                    last.region.start.column,
                ));
            }
        };
        Ok((self.alloc_slice_copy(initial), last, end))
    }

    fn do_stmt(&mut self) -> Result<(&'a Located<Stmt<'a>>, Position), Do<'a>> {
        let start = self.get_position();
        if let Some(statement) = self.do_let_stmt(start)? {
            return Ok(statement);
        }

        let saved = self.save_state();
        if let Ok((pattern, pattern_end)) = self.pattern_expr()
            && self
                .check_indent(pattern_end.line, pattern_end.column, |_, _| ())
                .is_ok()
            && self.word2(b'<', b'-', |_, _| ()).is_ok()
        {
            self.chomp_and_check_indent(Do::Space, Do::IndentExpr)?;
            let (expr, end) = self.specialize(
                |bump, error, row, col| Do::Expr(bump.alloc(error), row, col),
                |parser| parser.expression(),
            )?;
            return Ok((
                self.alloc(Located::at(
                    Region::new(start, end),
                    Stmt::Bind { pattern, expr },
                )),
                end,
            ));
        }

        self.restore_state(saved);
        let (expr, end) = self.specialize(
            |bump, error, row, col| Do::Expr(bump.alloc(error), row, col),
            |parser| parser.expression(),
        )?;
        Ok((
            self.alloc(Located::at(Region::new(start, end), Stmt::Expr(expr))),
            end,
        ))
    }

    fn do_let_stmt(
        &mut self,
        start: Position,
    ) -> Result<Option<(&'a Located<Stmt<'a>>, Position)>, Do<'a>> {
        if self.keyword_let(|_, _| ()).is_err() {
            return Ok(None);
        }

        let (defs, defs_end) = self.specialize(
            |bump, error, row, col| Do::Let(bump.alloc(error), row, col),
            |parser| {
                parser.with_backset_indent(3, |parser| {
                    parser.chomp_and_check_indent(Let::Space, Let::IndentDef)?;
                    parser.with_indent(|parser| {
                        let (first, mut end) = parser.chomp_let_def()?;
                        let mut defs = vec![first];
                        loop {
                            let saved = parser.save_state();
                            if parser.check_aligned(|_, _, _| ()).is_err() {
                                break;
                            }
                            match parser.chomp_let_def() {
                                Ok((def, new_end)) => {
                                    defs.push(def);
                                    end = new_end;
                                }
                                Err(_) => {
                                    parser.restore_state(saved);
                                    break;
                                }
                            }
                        }
                        Ok((defs, end))
                    })
                })
            },
        )?;

        let has_in = self
            .check_indent(defs_end.line, defs_end.column, |_, _| ())
            .is_ok()
            && self.keyword_in(|_, _| ()).is_ok();
        let defs = self.alloc_slice_copy(&defs);
        if !has_in {
            return Ok(Some((
                self.alloc(Located::at(Region::new(start, defs_end), Stmt::Let(defs))),
                defs_end,
            )));
        }

        self.chomp_and_check_indent(Do::Space, Do::IndentExpr)?;
        let (body, end) = self.specialize(
            |bump, error, row, col| Do::Expr(bump.alloc(error), row, col),
            |parser| parser.expression(),
        )?;
        let let_expr = self.alloc(Located::at(
            Region::new(start, end),
            Expr::Let { defs, body },
        ));
        Ok(Some((
            self.alloc(Located::at(Region::new(start, end), Stmt::Expr(let_expr))),
            end,
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::expression::assert_indented_expression_snapshot;

    macro_rules! assert_indented_do_error_snapshot {
        ($code:expr) => {{
            let bump = bumpalo::Bump::new();
            let indented = crate::test_support::indent_fragment(indoc::indoc!($code));
            let source = bump.alloc_str(&indented);
            let mut parser = crate::Parser::new(&bump, source);
            parser
                .chomp(|_, _, _| "space error")
                .expect("expected leading indent");
            let error = parser.expression().expect_err("expected do parse error");
            insta::with_settings!({
                description => format!("Code (indented inside a def):\n\n{}", indented),
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(error);
            });
        }};
    }

    #[test]
    fn bind_and_expressions() {
        assert_indented_expression_snapshot!(
            r#"
            do
                x <- fuzz int
                label "small"
                assert (x < 100)
        "#
        );
    }

    #[test]
    fn single_expression() {
        assert_indented_expression_snapshot!(
            r#"
            do
                pure 1
        "#
        );
    }

    #[test]
    fn tuple_bind() {
        assert_indented_expression_snapshot!(
            r#"
            do
                (a, b) <- pair
                pure a
        "#
        );
    }

    #[test]
    fn constructor_bind() {
        assert_indented_expression_snapshot!(
            r#"
            do
                Just x <- m
                pure x
        "#
        );
    }

    #[test]
    fn nested_do() {
        assert_indented_expression_snapshot!(
            r#"
            do
                x <- do
                    y <- action
                    pure y
                pure x
        "#
        );
    }

    #[test]
    fn do_after_operator() {
        assert_indented_expression_snapshot!(
            r#"
            a + do
                x <- action
                pure x
        "#
        );
    }

    #[test]
    fn let_statement() {
        assert_indented_expression_snapshot!(
            r#"
            do
                let
                    twice = x * 2
                    name = "n"
                label name
                assert (twice > x)
        "#
        );
    }

    #[test]
    fn let_expression_statement() {
        assert_indented_expression_snapshot!(
            r#"
            do
                let y = 1 in pure y
        "#
        );
    }

    #[test]
    fn error_trailing_bind() {
        assert_indented_do_error_snapshot!(
            r#"
            do
                x <- e
        "#
        );
    }

    #[test]
    fn error_trailing_let() {
        assert_indented_do_error_snapshot!(
            r#"
            do
                let y = 1
        "#
        );
    }

    #[test]
    fn error_misaligned_statement() {
        assert_indented_do_error_snapshot!(
            r#"
            do
                x <- e
              y
        "#
        );
    }

    #[test]
    fn error_empty_do() {
        assert_indented_do_error_snapshot!("do");
    }
}
