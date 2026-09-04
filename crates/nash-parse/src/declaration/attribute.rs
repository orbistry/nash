use bumpalo::collections::Vec as BumpVec;
use nash_region::Located;
use nash_source::{Attribute, Expr};

use crate::Parser;
use crate::error;

impl<'a> Parser<'a> {
    pub(super) fn chomp_attributes(&mut self) -> Result<&'a [&'a Attribute<'a>], error::Decl<'a>> {
        let mut attributes = BumpVec::new_in(self.bump);
        loop {
            let next = self.one_of_with_fallback(
                vec![Box::new(|parser: &mut Parser<'a>| {
                    parser
                        .in_context(
                            |bump, error, row, col| {
                                error::Decl::Attribute(bump.alloc(error), row, col)
                            },
                            |parser| parser.word1(b'@', error::Decl::Start),
                            |parser| parser.attribute_body(),
                        )
                        .map(Some)
                })],
                None,
            )?;
            match next {
                Some(attribute) => attributes.push(attribute),
                None => return Ok(attributes.into_bump_slice()),
            }
        }
    }

    fn attribute_body(&mut self) -> Result<&'a Attribute<'a>, error::Attribute<'a>> {
        let name_start = self.get_position();
        let name = self.lower_name(error::Attribute::Name)?;
        let name = self.add_end(name_start, name);
        let args = self.one_of_with_fallback(
            vec![Box::new(|parser: &mut Parser<'a>| {
                parser.word1(b'(', error::Attribute::End)?;
                parser
                    .chomp_and_check_indent(error::Attribute::Space, error::Attribute::IndentArg)?;
                parser.one_of(
                    error::Attribute::End,
                    vec![
                        Box::new(|parser: &mut Parser<'a>| {
                            parser.word1(b')', error::Attribute::End)?;
                            Ok(&[][..])
                        }),
                        Box::new(|parser| parser.attribute_args()),
                    ],
                )
            })],
            &[][..],
        )?;
        self.chomp(error::Attribute::Space)?;
        self.check_fresh_line(error::Attribute::FreshLine)?;
        Ok(self.alloc(Attribute { name, args }))
    }

    fn attribute_args(&mut self) -> Result<&'a [&'a Located<Expr<'a>>], error::Attribute<'a>> {
        let mut args = BumpVec::new_in(self.bump);
        loop {
            let (arg, end) = self.specialize(
                |bump, error, row, col| error::Attribute::Arg(bump.alloc(error), row, col),
                |parser| parser.expression(),
            )?;
            args.push(arg);
            self.check_indent(end.line, end.column, error::Attribute::IndentEnd)?;
            let done = self.one_of(
                error::Attribute::End,
                vec![
                    Box::new(|parser: &mut Parser<'a>| {
                        parser.word1(b',', error::Attribute::End)?;
                        parser.chomp_and_check_indent(
                            error::Attribute::Space,
                            error::Attribute::IndentArg,
                        )?;
                        Ok(false)
                    }),
                    Box::new(|parser| {
                        parser.word1(b')', error::Attribute::End)?;
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
    use super::super::{assert_decl_error_snapshot, assert_decl_snapshot};

    #[test]
    fn derive_union() {
        assert_decl_snapshot!("@derive(Eq, Show)\ntype T = A | B");
    }
    #[test]
    fn inline_value() {
        assert_decl_snapshot!("@inline\nf x = x");
    }
    #[test]
    fn cost_alias() {
        assert_decl_snapshot!("@cost(cpu 10, \"n\")\ntype alias a = int");
    }
    #[test]
    fn doc_then_attribute() {
        assert_decl_snapshot!("{-| doc -}\n@derive(Eq)\ntype T = A");
    }
    #[test]
    fn stacked() {
        assert_decl_snapshot!("@first\n@second(1)\nf = 1");
    }
    #[test]
    fn error_upper_name() {
        assert_decl_error_snapshot!("@Derive(Eq)\ntype T = A");
    }
    #[test]
    fn error_unclosed() {
        assert_decl_error_snapshot!("@derive(Eq\ntype T = A");
    }
    #[test]
    fn error_same_line() {
        assert_decl_error_snapshot!("@derive(Eq) type T = A");
    }
}
