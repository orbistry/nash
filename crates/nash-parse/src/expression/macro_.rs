use bumpalo::collections::Vec as BumpVec;
use nash_region::{Located, Position, Region};
use nash_source::{Expr, VarType};

use crate::Parser;
use crate::error::{self, Macro};

impl<'a> Parser<'a> {
    pub(crate) fn macro_call_or_term(
        &mut self,
        start: Position,
        variable: &'a Located<Expr<'a>>,
    ) -> Result<&'a Located<Expr<'a>>, error::Expr<'a>> {
        let (name, module) = match variable.value {
            Expr::Var {
                kind: VarType::LowVar,
                name,
            } => (name, None),
            Expr::VarQual {
                kind: VarType::LowVar,
                module,
                name,
            } => (name, Some(module)),
            _ => return Ok(variable),
        };
        if self.peek() != Some(b'!') || self.peek_at(1) != Some(b'(') {
            return Ok(variable);
        }

        let name_start = Position::new(
            variable.region.end.line,
            variable.region.end.column - u16::try_from(name.len()).expect("identifier too long"),
        );
        let name = self.alloc(Located::at(
            Region::new(name_start, variable.region.end),
            name,
        ));
        self.in_context(
            |bump, error, row, col| error::Expr::Macro(bump.alloc(error), row, col),
            |parser| parser.word2(b'!', b'(', error::Expr::Start),
            |parser| {
                parser.chomp_and_check_indent(Macro::Space, Macro::IndentArg)?;
                let args = parser.one_of(
                    Macro::Open,
                    vec![
                        Box::new(|parser: &mut Parser<'a>| {
                            parser.word1(b')', Macro::Open)?;
                            Ok(&[][..])
                        }),
                        Box::new(|parser| parser.macro_args()),
                    ],
                )?;
                Ok(parser.add_end(start, Expr::MacroCall { name, module, args }))
            },
        )
    }

    fn macro_args(&mut self) -> Result<&'a [&'a Located<Expr<'a>>], Macro<'a>> {
        let mut args = BumpVec::new_in(self.bump);
        loop {
            let (arg, end) = self.specialize(
                |bump, error, row, col| Macro::Arg(bump.alloc(error), row, col),
                |parser| parser.expression(),
            )?;
            args.push(arg);
            self.check_indent(end.line, end.column, Macro::IndentEnd)?;
            let done = self.one_of(
                Macro::End,
                vec![
                    Box::new(|parser: &mut Parser<'a>| {
                        parser.word1(b',', Macro::End)?;
                        parser.chomp_and_check_indent(Macro::Space, Macro::IndentArg)?;
                        Ok(false)
                    }),
                    Box::new(|parser| {
                        parser.word1(b')', Macro::End)?;
                        Ok(true)
                    }),
                ],
            )?;
            if done {
                return Ok(args.into_bump_slice());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::expression::{assert_expression_error_snapshot, assert_expression_snapshot};

    #[test]
    fn record_argument() {
        assert_expression_snapshot!("json!({ a = 1 })");
    }
    #[test]
    fn empty() {
        assert_expression_snapshot!("m!()");
    }
    #[test]
    fn several_arguments() {
        assert_expression_snapshot!("m!(1, \"two\", x)");
    }
    #[test]
    fn qualified() {
        assert_expression_snapshot!("Cardano.Macros.address!(\"addr1\")");
    }
    #[test]
    fn accessible() {
        assert_expression_snapshot!("m!(x).field");
    }
    #[test]
    fn function_argument() {
        assert_expression_snapshot!("f m!(1) y");
    }
    #[test]
    fn spaced_bang_is_operator() {
        assert_expression_snapshot!("x ! (y)");
    }
    #[test]
    fn uppercase_is_operator() {
        assert_expression_snapshot!("Foo!(x)");
    }
    #[test]
    fn error_unclosed() {
        assert_expression_error_snapshot!("m!(");
    }
    #[test]
    fn error_trailing_comma() {
        assert_expression_error_snapshot!("m!(1,)");
    }
    #[test]
    fn error_double_comma() {
        assert_expression_error_snapshot!("m!(1,,2)");
    }
}
