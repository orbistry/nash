use nash_region::{Located, Position, Region};
use nash_source::Expr;

use crate::error;
use crate::{Col, Parser, Row};

type KeywordParser<'a> =
    fn(&mut Parser<'a>, fn(Row, Col) -> error::Expr<'a>) -> Result<(), error::Expr<'a>>;

impl<'a> Parser<'a> {
    fn keyword_body(
        &mut self,
        start: Position,
        wrap: fn(&'a error::Keyword<'a>, Row, Col) -> error::Expr<'a>,
        keyword: KeywordParser<'a>,
        build: fn(&'a Located<Expr<'a>>) -> Expr<'a>,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            move |bump, error, row, col| wrap(bump.alloc(error), row, col),
            |parser| keyword(parser, error::Expr::Start),
            |parser| {
                parser.chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentBody)?;
                let (body, end) = parser.specialize(
                    |bump, error, row, col| error::Keyword::Body(bump.alloc(error), row, col),
                    |parser| parser.expression(),
                )?;
                Ok((
                    parser.alloc(Located::at(Region::new(start, end), build(body))),
                    end,
                ))
            },
        )
    }

    pub(crate) fn assert_(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_body(
            start,
            error::Expr::Assert,
            Parser::keyword_assert,
            Expr::Assert,
        )
    }

    pub(crate) fn comptime(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_body(
            start,
            error::Expr::Comptime,
            Parser::keyword_comptime,
            Expr::Comptime,
        )
    }

    fn keyword_message(
        &mut self,
        start: Position,
        wrap: fn(&'a error::Keyword<'a>, Row, Col) -> error::Expr<'a>,
        keyword: KeywordParser<'a>,
        build: fn(Option<&'a Located<Expr<'a>>>) -> Expr<'a>,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            move |bump, error, row, col| wrap(bump.alloc(error), row, col),
            |parser| keyword(parser, error::Expr::Start),
            |parser| {
                let keyword_end = parser.get_position();
                parser.chomp(error::Keyword::Space)?;
                let message = parser.one_of_with_fallback(
                    vec![Box::new(|parser: &mut Parser<'a>| {
                        let (row, col) = parser.position();
                        parser.check_indent(row, col, error::Keyword::IndentMessage)?;
                        let term = parser.specialize(
                            |bump, error, row, col| {
                                error::Keyword::Message(bump.alloc(error), row, col)
                            },
                            |parser| parser.term(),
                        )?;
                        Ok(Some(term))
                    })],
                    None,
                )?;
                let end = message.map_or(keyword_end, |message| message.region.end);
                Ok((
                    parser.alloc(Located::at(Region::new(start, end), build(message))),
                    end,
                ))
            },
        )
    }

    pub(crate) fn fail(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_message(start, error::Expr::Fail, Parser::keyword_fail, Expr::Fail)
    }

    pub(crate) fn todo(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.keyword_message(start, error::Expr::Todo, Parser::keyword_todo, Expr::Todo)
    }

    pub(crate) fn trace(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Expr<'a>>, Position), error::Expr<'a>> {
        self.in_context(
            |bump, error, row, col| error::Expr::Trace(bump.alloc(error), row, col),
            |parser| parser.keyword_trace(error::Expr::Start),
            |parser| {
                parser
                    .chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentMessage)?;
                let message = parser.specialize(
                    |bump, error, row, col| error::Keyword::Message(bump.alloc(error), row, col),
                    |parser| parser.term(),
                )?;
                parser.chomp_and_check_indent(error::Keyword::Space, error::Keyword::IndentBody)?;
                let (body, end) = parser.specialize(
                    |bump, error, row, col| error::Keyword::Body(bump.alloc(error), row, col),
                    |parser| parser.expression(),
                )?;
                Ok((
                    parser.alloc(Located::at(
                        Region::new(start, end),
                        Expr::Trace { message, body },
                    )),
                    end,
                ))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::expression::{
        assert_expression_error_snapshot, assert_expression_snapshot,
        assert_indented_expression_snapshot,
    };

    #[test]
    fn assert_parenthesized() {
        assert_expression_snapshot!("assert (x > 0)");
    }
    #[test]
    fn assert_binop() {
        assert_expression_snapshot!("assert x == y");
    }
    #[test]
    fn fail_empty() {
        assert_expression_snapshot!("fail");
    }
    #[test]
    fn fail_message() {
        assert_expression_snapshot!("fail \"boom\"");
    }
    #[test]
    fn todo_empty() {
        assert_expression_snapshot!("todo");
    }
    #[test]
    fn trace_call() {
        assert_expression_snapshot!("trace \"m\" (f x)");
    }
    #[test]
    fn trace_binop_body() {
        assert_expression_snapshot!("trace \"m\" x + 1");
    }
    #[test]
    fn comptime_call() {
        assert_expression_snapshot!("comptime (fib 20)");
    }
    #[test]
    fn fail_after_operator() {
        assert_expression_snapshot!("a + fail");
    }
    #[test]
    fn fail_does_not_swallow_case_branch() {
        assert_indented_expression_snapshot!(
            r#"
            case action of
                Cancel -> fail
                Continue -> 1
        "#
        );
    }
    #[test]
    fn error_assert_without_body() {
        assert_expression_error_snapshot!("assert");
    }
    #[test]
    fn error_trace_without_body() {
        assert_expression_error_snapshot!("trace \"m\"");
    }
    #[test]
    fn error_comptime_without_body() {
        assert_expression_error_snapshot!("comptime");
    }
}
