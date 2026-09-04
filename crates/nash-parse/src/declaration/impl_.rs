use bumpalo::collections::Vec as BumpVec;
use nash_region::{Located, Position, Region};
use nash_source::{Attribute, Def, Impl};

use super::Decl;
use crate::Parser;
use crate::error::{self, Def as DefErr, Impl as ImplErr};

impl<'a> Parser<'a> {
    pub(super) fn impl_decl(
        &mut self,
        attributes: &'a [&'a Attribute<'a>],
        start: Position,
    ) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
        self.in_context(
            |bump, error, row, col| error::Decl::Impl(bump.alloc(error), row, col),
            |parser| parser.keyword_impl(error::Decl::Start),
            |parser| {
                parser.chomp_and_check_indent(ImplErr::Space, ImplErr::IndentHead)?;
                let head_start = parser.get_position();
                let (scheme, head_end) = parser.specialize(
                    |bump, error, row, col| ImplErr::Head(bump.alloc(error), row, col),
                    |parser| parser.type_scheme(),
                )?;
                let head = parser
                    .to_constraint(scheme.typ)
                    .ok_or(ImplErr::BadHead(head_start.line, head_start.column))?;
                parser.check_indent(head_end.line, head_end.column, ImplErr::IndentWhere)?;
                parser.keyword_where(ImplErr::Where)?;
                let where_end = parser.get_position();
                parser.chomp(ImplErr::Space)?;
                let (methods, end) = parser.impl_body(where_end)?;
                let impl_ = Impl {
                    context: scheme.constraints,
                    head,
                    methods,
                    attributes,
                };
                Ok((
                    Decl::Impl(parser.alloc(Located::at(Region::new(start, end), impl_))),
                    end,
                ))
            },
        )
    }

    fn impl_body(
        &mut self,
        where_end: Position,
    ) -> Result<(&'a [&'a Located<Def<'a>>], Position), ImplErr<'a>> {
        if self.is_eof() || self.col() == 1 {
            return Ok((&[], where_end));
        }
        self.check_indent(where_end.line, where_end.column, ImplErr::IndentMethod)?;

        self.with_indent(|parser| {
            let mut methods = BumpVec::new_in(parser.bump);
            let (first, mut end) = parser.impl_method()?;
            methods.push(first);

            loop {
                if parser.is_eof() || parser.col() == 1 {
                    break;
                }
                parser.check_aligned(ImplErr::Alignment)?;
                let (method, method_end) = parser.impl_method()?;
                methods.push(method);
                end = method_end;
            }

            Ok((methods.into_bump_slice(), end))
        })
    }

    fn impl_method(&mut self) -> Result<(&'a Located<Def<'a>>, Position), ImplErr<'a>> {
        let start = self.get_position();
        let name_str = self.lower_name(ImplErr::MethodName)?;
        let name = self.add_end(start, name_str);
        self.specialize(
            |bump, error, row, col| ImplErr::Method(name_str, bump.alloc(error), row, col),
            |parser| {
                parser.chomp_and_check_indent(DefErr::Space, DefErr::IndentEquals)?;
                parser.chomp_def_args_and_body(start, name, None)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::{assert_decl_error_snapshot, assert_decl_snapshot};

    #[test]
    fn implementation_simple() {
        assert_decl_snapshot!(
            r#"
            impl Ord int where
                compare = Builtin.compareInteger
        "#
        );
    }

    #[test]
    fn implementation_with_context() {
        assert_decl_snapshot!(
            r#"
            impl Eq 'a => Eq (list 'a) where
                eq xs ys = eqList xs ys
        "#
        );
    }

    #[test]
    fn implementation_with_multiple_head_arguments() {
        assert_decl_snapshot!(
            r#"
            impl Lift int Int where
                lift = liftInt
                lower = lowerInt
        "#
        );
    }

    #[test]
    fn implementation_with_tuple_context_and_head() {
        assert_decl_snapshot!(
            r#"
            impl (Eq 'a, Eq 'b) => Eq ('a, 'b) where
                eq (a, b) (c, d) = a == c && b == d
        "#
        );
    }

    #[test]
    fn implementation_head_requires_trait() {
        assert_decl_error_snapshot!("impl int where");
    }

    #[test]
    fn implementation_head_requires_argument() {
        assert_decl_error_snapshot!("impl Eq where");
    }

    #[test]
    fn implementation_requires_where() {
        assert_decl_error_snapshot!("impl Eq int");
    }

    #[test]
    fn implementation_method_cannot_have_annotation() {
        assert_decl_error_snapshot!(
            r#"
            impl Eq int where
                eq : int -> bool
        "#
        );
    }

    #[test]
    fn implementation_methods_must_align() {
        assert_decl_error_snapshot!(
            r#"
            impl Lift int Int where
                lift = liftInt
                 lower = lowerInt
        "#
        );
    }
}
