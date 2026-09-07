use bumpalo::collections::Vec as BumpVec;
use nash_region::{Located, Position, Region};
use nash_source::{Annotation, Attribute, Constraint, Trait, TraitMethod, Type, TypeParam};

use super::Decl;
use crate::Parser;
use crate::error::{self, Def as DefErr, Trait as TraitErr};

type TraitHead<'a> = (
    &'a [&'a Located<Constraint<'a>>],
    &'a Located<&'a str>,
    &'a [&'a TypeParam<'a>],
);

impl<'a> Parser<'a> {
    pub(super) fn trait_decl(
        &mut self,
        attributes: &'a [&'a Attribute<'a>],
        start: Position,
    ) -> Result<(Decl<'a>, Position), error::Decl<'a>> {
        self.in_context(
            |bump, error, row, col| error::Decl::Trait(bump.alloc(error), row, col),
            |parser| parser.keyword_trait(error::Decl::Start),
            |parser| {
                parser.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentName)?;
                let (supers, name, params) = parser.trait_head()?;
                parser.keyword_where(TraitErr::Where)?;
                let where_end = parser.get_position();
                parser.chomp(TraitErr::Space)?;
                let (methods, end) = parser.trait_body(where_end)?;
                let trait_ = Trait {
                    name,
                    params,
                    supers,
                    methods,
                    attributes,
                };
                Ok((
                    Decl::Trait(parser.alloc(Located::at(Region::new(start, end), trait_))),
                    end,
                ))
            },
        )
    }

    fn trait_head(&mut self) -> Result<TraitHead<'a>, TraitErr<'a>> {
        let supers = self.trait_supers()?;
        let name_start = self.get_position();
        let name = self.upper_name(TraitErr::Name)?;
        let name = self.add_end(name_start, name);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;

        let mut params = BumpVec::new_in(self.bump);
        while matches!(self.peek(), Some(b'\'') | Some(b'(')) {
            let param = self.specialize(
                |bump, error, row, col| TraitErr::Param(bump.alloc(error), row, col),
                |parser| parser.type_param(),
            )?;
            params.push(param);
            self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentWhere)?;
        }

        if params.is_empty() {
            let (row, col) = self.position();
            return Err(TraitErr::Param(
                self.alloc(error::TypeParam::Start(row, col)),
                row,
                col,
            ));
        }

        Ok((supers, name, params.into_bump_slice()))
    }

    fn trait_supers(&mut self) -> Result<&'a [&'a Located<Constraint<'a>>], TraitErr<'a>> {
        let remaining = self.remaining();
        let where_offset =
            remaining
                .windows(b"where".len())
                .enumerate()
                .find_map(|(offset, bytes)| {
                    let before_is_boundary = offset == 0
                        || remaining
                            .get(offset - 1)
                            .is_some_and(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
                    let after_is_boundary = remaining
                        .get(offset + b"where".len())
                        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
                    (bytes == b"where" && before_is_boundary && after_is_boundary).then_some(offset)
                });
        let head = where_offset.map_or(remaining, |offset| &remaining[..offset]);
        let has_context_arrow = head.windows(2).any(|bytes| bytes == b"=>");
        let state = self.save_state();
        let result = if self.peek() == Some(b'(') {
            self.paren_super_context()
        } else {
            self.single_super_context()
        };

        match result {
            Ok(supers) => Ok(supers),
            Err(error) if has_context_arrow => Err(error),
            Err(_) => {
                self.restore_state(state);
                Ok(&[])
            }
        }
    }

    fn single_super_context(&mut self) -> Result<&'a [&'a Located<Constraint<'a>>], TraitErr<'a>> {
        let constraint = self.super_constraint()?;
        self.word2(b'=', b'>', TraitErr::Where)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentName)?;
        Ok(self.alloc_slice_copy(&[constraint]))
    }

    fn paren_super_context(&mut self) -> Result<&'a [&'a Located<Constraint<'a>>], TraitErr<'a>> {
        self.word1(b'(', TraitErr::Name)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
        let mut constraints = BumpVec::new_in(self.bump);
        constraints.push(self.super_constraint()?);

        loop {
            if self.peek() != Some(b',') {
                break;
            }
            self.advance();
            self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
            constraints.push(self.super_constraint()?);
        }

        self.word1(b')', TraitErr::Where)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentWhere)?;
        self.word2(b'=', b'>', TraitErr::Where)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentName)?;
        Ok(constraints.into_bump_slice())
    }

    fn super_constraint(&mut self) -> Result<&'a Located<Constraint<'a>>, TraitErr<'a>> {
        let start = self.get_position();
        let class_name = self.upper_name(TraitErr::Name)?;
        let class = self.add_end(start, class_name);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;

        let mut args = BumpVec::new_in(self.bump);
        while self.peek() == Some(b'\'') {
            let arg_start = self.get_position();
            let variable = self.type_var_name(TraitErr::SuperArg)?;
            args.push(self.add_end(arg_start, Type::Var(variable)));
            self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentParam)?;
        }

        if args.is_empty() {
            let (row, col) = self.position();
            return Err(TraitErr::SuperArg(row, col));
        }

        Ok(self.add_end(
            start,
            Constraint {
                class,
                module: None,
                args: args.into_bump_slice(),
            },
        ))
    }

    fn trait_body(
        &mut self,
        where_end: Position,
    ) -> Result<(&'a [&'a TraitMethod<'a>], Position), TraitErr<'a>> {
        if self.is_eof() || self.col() == 1 {
            return Ok((&[], where_end));
        }
        self.check_indent(where_end.line, where_end.column, TraitErr::IndentMethod)?;

        self.with_indent(|parser| {
            let mut methods = BumpVec::new_in(parser.bump);
            let (first, mut end) = parser.trait_method()?;
            methods.push(first);

            loop {
                if parser.is_eof() || parser.col() == 1 {
                    break;
                }
                parser.check_aligned(TraitErr::Alignment)?;
                let (method, method_end) = parser.trait_method()?;
                methods.push(method);
                end = method_end;
            }

            Ok((methods.into_bump_slice(), end))
        })
    }

    fn trait_method(&mut self) -> Result<(&'a TraitMethod<'a>, Position), TraitErr<'a>> {
        let start = self.get_position();
        let name_str = self.lower_name(TraitErr::MethodName)?;
        let name = self.add_end(start, name_str);
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentColon)?;
        self.word1(b':', TraitErr::Colon)?;
        self.chomp_and_check_indent(TraitErr::Space, TraitErr::IndentType)?;
        let (annotation, signature_end) = self.specialize(
            |bump, error, row, col| TraitErr::Type(bump.alloc(error), row, col),
            |parser| parser.type_scheme(),
        )?;

        let state = self.save_state();
        let default = if self.col() == self.indent() && self.next_name_is(name_str) {
            Some(self.specialize(
                |bump, error, row, col| TraitErr::Default(name_str, bump.alloc(error), row, col),
                |parser| {
                    let definition_start = parser.get_position();
                    let definition_name = parser.chomp_matching_name(name_str)?;
                    parser.chomp_and_check_indent(DefErr::Space, DefErr::IndentEquals)?;
                    parser.chomp_def_args_and_body(definition_start, definition_name, None)
                },
            )?)
        } else {
            self.restore_state(state);
            None
        };

        let end = default.map_or(signature_end, |(_, end)| end);
        Ok((
            self.alloc(TraitMethod {
                name,
                annotation: annotation as &'a Annotation<'a>,
                default: default.map(|(definition, _)| definition),
            }),
            end,
        ))
    }

    fn next_name_is(&self, expected: &str) -> bool {
        let remaining = self.remaining();
        let expected = expected.as_bytes();
        remaining.starts_with(expected)
            && remaining
                .get(expected.len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    }
}

#[cfg(test)]
mod tests {
    use super::super::{assert_decl_error_snapshot, assert_decl_snapshot};

    #[test]
    fn trait_with_superclass_and_default() {
        assert_decl_snapshot!(
            r#"
            trait Eq 'a => Ord 'a where
                compare : 'a -> 'a -> ordering

                lt : 'a -> 'a -> bool
                lt a b = compare a b == LT
        "#
        );
    }

    #[test]
    fn trait_with_higher_kinded_parameter() {
        assert_decl_snapshot!(
            r#"
            trait Functor 'f where
                map : ('a -> 'b) -> 'f 'a -> 'f 'b
        "#
        );
    }

    #[test]
    fn trait_with_multiple_superclasses() {
        assert_decl_snapshot!(
            r#"
            trait (Ord 'k, ToData 'k) => Key 'k where
                hash : 'k -> Bytes
        "#
        );
    }

    #[test]
    fn trait_with_multiple_parameters_and_methods() {
        assert_decl_snapshot!(
            r#"
            trait Lift 'small 'big where
                lift : 'small -> 'big
                lower : 'big -> 'small
        "#
        );
    }

    #[test]
    fn trait_requires_parameter() {
        assert_decl_error_snapshot!("trait Eq where");
    }

    #[test]
    fn trait_requires_uppercase_name() {
        assert_decl_error_snapshot!("trait eq 'a where");
    }

    #[test]
    fn trait_requires_where() {
        assert_decl_error_snapshot!(
            r#"
            trait Eq 'a
                eq : 'a -> bool
        "#
        );
    }

    #[test]
    fn trait_method_requires_signature() {
        assert_decl_error_snapshot!(
            r#"
            trait Eq 'a where
                eq a = true
        "#
        );
    }

    #[test]
    fn trait_methods_must_align() {
        assert_decl_error_snapshot!(
            r#"
            trait Eq 'a where
                eq : 'a -> bool
                 neq : 'a -> bool
        "#
        );
    }

    #[test]
    fn superclass_argument_must_be_variable() {
        assert_decl_error_snapshot!("trait Eq int => Ord 'a where");
    }

    #[test]
    fn constrained_method_does_not_imply_superclass() {
        assert_decl_snapshot!(
            r#"
            trait Show 'a where
                show : Eq 'a => 'a -> string
        "#
        );
    }
}
