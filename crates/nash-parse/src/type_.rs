//! Type parsing for Nash.
//!
//! Ported from Elm's `Parse/Type.hs`.
//!
//! Provides:
//! - `type_term` - atomic types (variables, named types, tuples, records)
//! - `type_expr` - full types including function arrows and type application

use bumpalo::collections::Vec as BumpVec;
use nash_region::{Located, Position, Region};
use nash_source::{FieldType, Kind, Type, TypeParam};

use crate::Parser;
use crate::error::{self, TRecord, TTuple};

/// Qualified or unqualified uppercase name (for types).
enum TypeName<'a> {
    Unqualified(&'a str),
    Qualified(&'a str, &'a str), // (module, name)
}

enum Head<'a> {
    Name(TypeName<'a>),
    Var(&'a str),
}

impl<'a> Parser<'a> {
    // -------------------------------------------------------------------------
    // Type expressions (with arrows)
    // -------------------------------------------------------------------------

    /// Parse a type expression including function arrows.
    ///
    /// Returns `(type, end)` where `end` is the position at end of type (before any chomp).
    ///
    /// Mirrors Elm's `Type.expression`:
    /// ```haskell
    /// expression :: Space.Parser E.Type Src.Type
    /// expression =
    ///   do  start <- getPosition
    ///       term1@(tipe1, end1) <- oneOf E.TStart [ app start, term... ]
    ///       oneOfWithFallback [ arrow... ] term1
    /// ```
    pub fn type_expr(&mut self) -> Result<(&'a Located<Type<'a>>, Position), error::Type<'a>> {
        let start = self.get_position();

        // Parse first term - either type application or simple term
        let (tipe1, end1) = self.one_of(
            error::Type::Start,
            vec![
                // Type application: Maybe Int, Result String Int, etc.
                Box::new(|p: &mut Parser<'a>| p.type_app(start)),
                // Simple term
                Box::new(|p: &mut Parser<'a>| {
                    let term = p.type_term()?;
                    let end = p.get_position();
                    p.chomp(error::Type::Space)?;
                    Ok((term, end))
                }),
            ],
        )?;

        // Try to parse function arrow
        self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.check_indent(end1.line, end1.column, error::Type::IndentStart)?;
                p.word2(0x2D, 0x3E, error::Type::Start)?; // ->
                p.chomp_and_check_indent(error::Type::Space, error::Type::IndentStart)?;

                let (tipe2, end2) = p.type_expr()?;
                let tipe = p.alloc(Located::at(
                    Region::new(start, end2),
                    Type::Lambda {
                        from: tipe1,
                        to: tipe2,
                    },
                ));
                Ok((tipe, end2))
            })],
            (tipe1, end1),
        )
    }

    // -------------------------------------------------------------------------
    // Type application
    // -------------------------------------------------------------------------

    /// Parse type application: `Maybe Int`, `Result String Int`, etc.
    ///
    /// Mirrors Elm's `Type.app`.
    fn type_app(
        &mut self,
        start: Position,
    ) -> Result<(&'a Located<Type<'a>>, Position), error::Type<'a>> {
        let head = self.one_of(
            error::Type::Start,
            vec![
                Box::new(|p: &mut Parser<'a>| p.type_name(error::Type::Start).map(Head::Name)),
                Box::new(|p: &mut Parser<'a>| {
                    p.type_var_name(error::Type::VarStart).map(Head::Var)
                }),
            ],
        )?;
        let head_end = self.get_position();
        self.chomp(error::Type::Space)?;

        let (args, end) = self.type_chomp_args(head_end)?;

        let region = Region::new(start, head_end);
        let tipe = match head {
            Head::Name(TypeName::Unqualified(name)) => Type::Type { region, name, args },
            Head::Name(TypeName::Qualified(module, name)) => Type::TypeQual {
                region,
                module,
                name,
                args,
            },
            Head::Var(name) if args.is_empty() => Type::Var(name),
            Head::Var(name) => Type::VarApp { region, name, args },
        };

        Ok((self.alloc(Located::at(Region::new(start, end), tipe)), end))
    }

    /// Chomp type arguments for application.
    ///
    /// Mirrors Elm's `Type.chompArgs`.
    fn type_chomp_args(
        &mut self,
        mut end: Position,
    ) -> Result<(&'a [&'a Located<Type<'a>>], Position), error::Type<'a>> {
        let mut args: BumpVec<'a, &'a Located<Type<'a>>> = BumpVec::new_in(self.bump);

        loop {
            let result = self.one_of_with_fallback(
                vec![Box::new(|p: &mut Parser<'a>| {
                    // Check CURRENT position (after chomp), not the end of previous token
                    let (row, col) = p.position();
                    p.check_indent(row, col, error::Type::IndentStart)?;
                    let arg = p.type_term()?;
                    let new_end = p.get_position();
                    p.chomp(error::Type::Space)?;
                    Ok(Some((arg, new_end)))
                })],
                None,
            )?;

            match result {
                Some((arg, new_end)) => {
                    args.push(arg);
                    end = new_end;
                }
                None => break,
            }
        }

        Ok((args.into_bump_slice(), end))
    }

    // -------------------------------------------------------------------------
    // Type terms (atomic)
    // -------------------------------------------------------------------------

    /// Parse an atomic type (no arrows, no application).
    ///
    /// Mirrors Elm's `Type.term`:
    /// - Named types: `Int`, `Maybe`, `Module.Type`
    /// - Type variables: `a`, `msg`
    /// - Tuples: `()`, `(Int, String)`
    /// - Records: `{}`, `{ name : String }`, `{ a | name : String }`
    pub fn type_term(&mut self) -> Result<&'a Located<Type<'a>>, error::Type<'a>> {
        let start = self.get_position();

        self.one_of(
            error::Type::Start,
            vec![
                // Named type (no args in term - args handled by app)
                Box::new(|p: &mut Parser<'a>| {
                    let name = p.type_name(error::Type::Start)?;
                    let end = p.get_position();
                    let region = Region::new(start, end);

                    let tipe = match name {
                        TypeName::Unqualified(name) => {
                            let empty: &'a [&'a Located<Type<'a>>] = &[];
                            Type::Type {
                                region,
                                name,
                                args: empty,
                            }
                        }
                        TypeName::Qualified(module, name) => {
                            let empty: &'a [&'a Located<Type<'a>>] = &[];
                            Type::TypeQual {
                                region,
                                module,
                                name,
                                args: empty,
                            }
                        }
                    };

                    Ok(p.add_end(start, tipe))
                }),
                // Type variable
                Box::new(|p: &mut Parser<'a>| {
                    let var = p.type_var_name(error::Type::VarStart)?;
                    Ok(p.add_end(start, Type::Var(var)))
                }),
                // Tuple (or unit, or parenthesized)
                Box::new(|p: &mut Parser<'a>| p.type_tuple(start)),
                // Record
                Box::new(|p: &mut Parser<'a>| p.type_record(start)),
            ],
        )
    }

    // -------------------------------------------------------------------------
    // Tuples
    // -------------------------------------------------------------------------

    /// Parse a tuple type: `()`, `(a)`, `(a, b)`, `(a, b, c)`
    fn type_tuple(&mut self, start: Position) -> Result<&'a Located<Type<'a>>, error::Type<'a>> {
        self.in_context(
            |bump, tuple_err, row, col| error::Type::Tuple(bump.alloc(tuple_err), row, col),
            |p| p.word1(0x28, error::Type::Start), // (
            |p| p.type_tuple_body(start),
        )
    }

    /// Parse tuple type body after `(`.
    fn type_tuple_body(&mut self, start: Position) -> Result<&'a Located<Type<'a>>, TTuple<'a>> {
        self.chomp_and_check_indent(TTuple::Space, TTuple::IndentType1)?;

        self.one_of(
            TTuple::Open,
            vec![
                // Unit: `()`
                Box::new(|p: &mut Parser<'a>| {
                    p.word1(0x29, TTuple::Open)?; // )
                    Ok(p.add_end(start, Type::Unit))
                }),
                // Type (might be parenthesized or tuple)
                Box::new(|p: &mut Parser<'a>| {
                    let (first, end) = p.type_tuple_entry()?;
                    p.check_indent(end.line, end.column, TTuple::IndentEnd)?;
                    p.type_tuple_help(start, first)
                }),
            ],
        )
    }

    /// Parse a type inside a tuple.
    fn type_tuple_entry(&mut self) -> Result<(&'a Located<Type<'a>>, Position), TTuple<'a>> {
        self.specialize(
            |bump, type_err, row, col| TTuple::Type(bump.alloc(type_err), row, col),
            |p| p.type_expr(),
        )
    }

    /// Parse remaining tuple elements.
    fn type_tuple_help(
        &mut self,
        start: Position,
        first: &'a Located<Type<'a>>,
    ) -> Result<&'a Located<Type<'a>>, TTuple<'a>> {
        let mut rest: BumpVec<'a, &'a Located<Type<'a>>> = BumpVec::new_in(self.bump);

        loop {
            self.chomp(TTuple::Space)?;

            let done = self.one_of(
                TTuple::End,
                vec![
                    // Comma - another type
                    Box::new(|p: &mut Parser<'a>| {
                        p.word1(0x2C, TTuple::End)?; // ,
                        p.chomp_and_check_indent(TTuple::Space, TTuple::IndentTypeN)?;

                        let (tipe, end) = p.type_tuple_entry()?;
                        rest.push(tipe);

                        p.check_indent(end.line, end.column, TTuple::IndentEnd)?;
                        Ok(false)
                    }),
                    // Close paren
                    Box::new(|p: &mut Parser<'a>| {
                        p.word1(0x29, TTuple::End)?; // )
                        Ok(true)
                    }),
                ],
            )?;

            if done {
                break;
            }
        }

        if rest.is_empty() {
            // Just parenthesized type
            Ok(first)
        } else {
            // Tuple
            let second = rest.remove(0);
            let others = rest.into_bump_slice();
            Ok(self.add_end(
                start,
                Type::Tuple {
                    first,
                    second,
                    rest: others,
                },
            ))
        }
    }

    // -------------------------------------------------------------------------
    // Records
    // -------------------------------------------------------------------------

    /// Parse a record type: `{}`, `{ name : String }`, `{ a | name : String }`
    fn type_record(&mut self, start: Position) -> Result<&'a Located<Type<'a>>, error::Type<'a>> {
        self.in_context(
            |bump, record_err, row, col| error::Type::Record(bump.alloc(record_err), row, col),
            |p| p.word1(0x7B, error::Type::Start), // {
            |p| p.type_record_body(start),
        )
    }

    /// Parse record type body after `{`.
    fn type_record_body(&mut self, start: Position) -> Result<&'a Located<Type<'a>>, TRecord<'a>> {
        self.chomp_and_check_indent(TRecord::Space, TRecord::IndentOpen)?;

        self.one_of(
            TRecord::Open,
            vec![
                // Empty record: `{}`
                Box::new(|p: &mut Parser<'a>| {
                    p.word1(0x7D, TRecord::Open)?; // }
                    let empty: &'a [&'a FieldType<'a>] = &[];
                    Ok(p.add_end(start, Type::Record(empty)))
                }),
                // Non-empty record
                Box::new(|p: &mut Parser<'a>| {
                    let field = p.type_record_field()?;
                    let fields = p.type_record_end(field)?;
                    Ok(p.add_end(start, Type::Record(fields)))
                }),
            ],
        )
    }

    /// Parse a type inside a record field.
    fn type_record_type_entry(&mut self) -> Result<(&'a Located<Type<'a>>, Position), TRecord<'a>> {
        self.specialize(
            |bump, type_err, row, col| TRecord::Type(bump.alloc(type_err), row, col),
            |p| p.type_expr(),
        )
    }

    /// Parse a record field: `name : Type`
    fn type_record_field(&mut self) -> Result<&'a FieldType<'a>, TRecord<'a>> {
        let name_start = self.get_position();
        let name = self.lower_name(TRecord::Field)?;
        let name_loc = self.add_end(name_start, name);

        self.chomp_and_check_indent(TRecord::Space, TRecord::IndentColon)?;
        self.word1(0x3A, TRecord::Colon)?; // :
        self.chomp_and_check_indent(TRecord::Space, TRecord::IndentType)?;

        let (tipe, end) = self.type_record_type_entry()?;
        self.check_indent(end.line, end.column, TRecord::IndentEnd)?;

        Ok(self.alloc(FieldType {
            field: name_loc,
            typ: tipe,
        }))
    }

    /// Parse remaining record fields.
    fn type_record_end(
        &mut self,
        first: &'a FieldType<'a>,
    ) -> Result<&'a [&'a FieldType<'a>], TRecord<'a>> {
        let mut fields: BumpVec<'a, &'a FieldType<'a>> = BumpVec::new_in(self.bump);
        fields.push(first);

        loop {
            self.chomp(TRecord::Space)?;

            let done = self.one_of(
                TRecord::End,
                vec![
                    // Comma - another field
                    Box::new(|p: &mut Parser<'a>| {
                        p.word1(0x2C, TRecord::End)?; // ,
                        p.chomp_and_check_indent(TRecord::Space, TRecord::IndentField)?;

                        let field = p.type_record_field()?;
                        fields.push(field);
                        Ok(false)
                    }),
                    // Close brace
                    Box::new(|p: &mut Parser<'a>| {
                        p.word1(0x7D, TRecord::End)?; // }
                        Ok(true)
                    }),
                ],
            )?;

            if done {
                break;
            }
        }

        Ok(fields.into_bump_slice())
    }

    /// Parse a declaration type parameter, with an optional kind annotation.
    pub(crate) fn type_param(&mut self) -> Result<&'a TypeParam<'a>, error::TypeParam<'a>> {
        let start = self.get_position();
        self.one_of(
            error::TypeParam::Start,
            vec![
                Box::new(|p: &mut Parser<'a>| {
                    let name = p.type_var_name(error::TypeParam::Start)?;
                    Ok(p.alloc(TypeParam {
                        name: p.add_end(start, name),
                        kind: None,
                    }))
                }),
                Box::new(|p: &mut Parser<'a>| {
                    p.word1(b'(', error::TypeParam::Start)?;
                    p.chomp_and_check_indent(
                        error::TypeParam::Space,
                        error::TypeParam::IndentColon,
                    )?;
                    let name_start = p.get_position();
                    let name = p.type_var_name(error::TypeParam::Start)?;
                    let name = p.add_end(name_start, name);
                    p.chomp_and_check_indent(
                        error::TypeParam::Space,
                        error::TypeParam::IndentColon,
                    )?;
                    p.word1(b':', error::TypeParam::Colon)?;
                    p.chomp_and_check_indent(
                        error::TypeParam::Space,
                        error::TypeParam::IndentKind,
                    )?;
                    let (kind, end) = p.specialize(
                        |bump, e, row, col| error::TypeParam::Kind(bump.alloc(e), row, col),
                        |p| p.kind_expr(),
                    )?;
                    p.check_indent(end.line, end.column, error::TypeParam::IndentEnd)?;
                    p.word1(b')', error::TypeParam::End)?;
                    Ok(p.alloc(TypeParam {
                        name,
                        kind: Some(kind),
                    }))
                }),
            ],
        )
    }

    /// Parse a kind expression.
    fn kind_expr(&mut self) -> Result<(&'a Located<Kind<'a>>, Position), error::Kind<'a>> {
        let start = self.get_position();
        let atom = self.kind_atom()?;
        let end1 = self.get_position();
        self.chomp(error::Kind::Space)?;
        self.one_of_with_fallback(
            vec![Box::new(|p: &mut Parser<'a>| {
                p.check_indent(end1.line, end1.column, error::Kind::IndentStart)?;
                p.word2(b'-', b'>', error::Kind::Start)?;
                p.chomp_and_check_indent(error::Kind::Space, error::Kind::IndentStart)?;
                let (to, end2) = p.kind_expr()?;
                Ok((
                    p.alloc(Located::at(
                        Region::new(start, end2),
                        Kind::Arrow { from: atom, to },
                    )),
                    end2,
                ))
            })],
            (atom, end1),
        )
    }

    /// Parse a base or parenthesized kind.
    fn kind_atom(&mut self) -> Result<&'a Located<Kind<'a>>, error::Kind<'a>> {
        let start = self.get_position();
        self.one_of(
            error::Kind::Start,
            vec![
                Box::new(|p: &mut Parser<'a>| {
                    let (row, col) = p.position();
                    let name = p.upper_name(error::Kind::Start)?;
                    let kind = match name {
                        "Big" => Kind::Big,
                        "Const" => Kind::Const,
                        "Term" => Kind::Term,
                        "Storable" => Kind::Storable,
                        other => return Err(error::Kind::Name(other, row, col)),
                    };
                    Ok(p.add_end(start, kind))
                }),
                Box::new(|p: &mut Parser<'a>| {
                    p.in_context(
                        |bump, e, row, col| error::Kind::Paren(bump.alloc(e), row, col),
                        |p| p.word1(b'(', error::Kind::Start),
                        |p| {
                            p.chomp_and_check_indent(error::Kind::Space, error::Kind::IndentStart)?;
                            let (kind, end) = p.kind_expr()?;
                            p.check_indent(end.line, end.column, error::Kind::End)?;
                            p.word1(b')', error::Kind::End)?;
                            Ok(kind)
                        },
                    )
                }),
            ],
        )
    }

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    /// Parse a possibly-qualified uppercase name (for types).
    ///
    /// Mirrors Elm's `Var.foreignUpper`.
    fn type_name<E>(&mut self, to_error: impl FnOnce(u16, u16) -> E) -> Result<TypeName<'a>, E> {
        let (row, col) = self.position();
        let start_pos = self.pos;

        match self.peek() {
            Some(b) if b.is_ascii_lowercase() => {
                Ok(TypeName::Unqualified(self.lower_name(to_error)?))
            }
            Some(b) if b.is_ascii_uppercase() => {
                self.advance();
                self.chomp_inner_chars();

                // Check for qualification
                if self.is_dot_upper() {
                    self.chomp_qualified_upper_for_type(start_pos)
                } else if self.is_dot_lower() {
                    // Can't have lowercase after dot for types
                    Err(to_error(row, col))
                } else {
                    let name = self.slice_from(start_pos);
                    Ok(TypeName::Unqualified(name))
                }
            }
            _ => Err(to_error(row, col)),
        }
    }

    /// Chomp through Module.Module... chain for type names.
    fn chomp_qualified_upper_for_type<E>(&mut self, start_pos: usize) -> Result<TypeName<'a>, E> {
        loop {
            if self.is_dot_upper() {
                self.advance(); // consume dot
                self.advance(); // consume first uppercase char
                self.chomp_inner_chars();
            } else {
                // No more dots - split into module and name
                let full = self.slice_from(start_pos);
                if let Some(last_dot) = full.rfind('.') {
                    let module = &full[..last_dot];
                    let name = &full[last_dot + 1..];
                    return Ok(TypeName::Qualified(module, name));
                } else {
                    return Ok(TypeName::Unqualified(full));
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
macro_rules! assert_type_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = $crate::Parser::new(&bump, src.as_bytes());
        let (result, _end) = parser.type_expr().expect("expected successful parse");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }};
}

#[cfg(test)]
macro_rules! assert_type_error_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let src = bump.alloc_str(indoc::indoc!($code));
        let mut parser = $crate::Parser::new(&bump, src.as_bytes());
        let result = parser.type_expr().expect_err("expected parse error");

        insta::with_settings!({
            description => format!("Code:\n\n{}", indoc::indoc!($code)),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }};
}

/// Snapshot test macro for multiline types, laid out as they would appear
/// indented inside a declaration (see `test_support::indent_fragment`).
#[cfg(test)]
macro_rules! assert_indented_type_snapshot {
    ($code:expr) => {{
        let bump = bumpalo::Bump::new();
        let fragment = indoc::indoc!($code);
        let indented = $crate::test_support::indent_fragment(fragment);
        let src = bump.alloc_str(&indented);
        let mut parser = $crate::Parser::new(&bump, src.as_bytes());
        parser
            .chomp(|_, _, _| "space error")
            .expect("expected leading indent");
        let (result, _end) = parser.type_expr().expect("expected successful parse");

        insta::with_settings!({
            description => format!("Code (indented inside a declaration):\n\n{}", indented),
            omit_expression => true,
        }, {
            insta::assert_debug_snapshot!(result);
        });
    }};
}

#[cfg(test)]
mod tests {
    // Type variables
    #[test]
    fn type_var_simple() {
        assert_type_snapshot!("'a");
    }

    #[test]
    fn type_var_msg() {
        assert_type_snapshot!("'msg");
    }

    // Named types (no args)
    #[test]
    fn named_type_int() {
        assert_type_snapshot!("Int");
    }

    #[test]
    fn named_type_string() {
        assert_type_snapshot!("String");
    }

    #[test]
    fn named_type_qualified() {
        assert_type_snapshot!("Dict.Dict");
    }

    #[test]
    fn named_type_multi_qualified() {
        assert_type_snapshot!("Data.Map.Map");
    }

    // Type application
    #[test]
    fn type_app_maybe() {
        assert_type_snapshot!("Maybe Int");
    }

    #[test]
    fn type_app_result() {
        assert_type_snapshot!("Result String Int");
    }

    #[test]
    fn type_app_nested() {
        assert_type_snapshot!("Maybe (List Int)");
    }

    #[test]
    fn type_app_qualified() {
        assert_type_snapshot!("Dict.Dict String Int");
    }

    // Function types
    #[test]
    fn function_simple() {
        assert_type_snapshot!("'a -> 'b");
    }

    #[test]
    fn function_multi() {
        assert_type_snapshot!("'a -> 'b -> 'c");
    }

    #[test]
    fn function_with_types() {
        assert_type_snapshot!("Int -> String -> Bool");
    }

    #[test]
    fn function_with_app() {
        assert_type_snapshot!("Maybe 'a -> Result 'e 'a");
    }

    // Unit
    #[test]
    fn unit() {
        assert_type_snapshot!("()");
    }

    // Tuple types
    #[test]
    fn tuple_pair() {
        assert_type_snapshot!("(Int, String)");
    }

    #[test]
    fn tuple_triple() {
        assert_type_snapshot!("(Int, String, Bool)");
    }

    #[test]
    fn tuple_nested() {
        assert_type_snapshot!("((Int, String), Bool)");
    }

    #[test]
    fn tuple_with_function() {
        assert_type_snapshot!("('a -> 'b, 'c)");
    }

    #[test]
    fn tuple_multiline() {
        assert_indented_type_snapshot!(
            "(
                Int,
                String,
                Bool
            )"
        );
    }

    // Record types
    #[test]
    fn record_empty() {
        assert_type_snapshot!("{}");
    }

    #[test]
    fn record_single() {
        assert_type_snapshot!("{ name : String }");
    }

    #[test]
    fn record_multiple() {
        assert_type_snapshot!("{ name : String, age : Int }");
    }

    #[test]
    fn record_with_function() {
        assert_type_snapshot!("{ onClick : 'msg -> Cmd 'msg }");
    }

    #[test]
    fn record_multiline() {
        assert_indented_type_snapshot!(
            "{
                name : String,
                age : Int,
                active : Bool
            }"
        );
    }

    // Parenthesized
    #[test]
    fn parenthesized() {
        assert_type_snapshot!("(Int)");
    }

    #[test]
    fn parenthesized_function() {
        assert_type_snapshot!("('a -> 'b) -> List 'a -> List 'b");
    }

    // Complex combinations
    #[test]
    fn complex_model_msg() {
        assert_type_snapshot!("{ model : Model, update : Msg -> Model -> Model }");
    }

    #[test]
    fn little_type() {
        assert_type_snapshot!("a");
    }

    #[test]
    fn little_type_application() {
        assert_type_snapshot!("list int");
    }

    #[test]
    fn type_variable_application() {
        assert_type_snapshot!("'f 'a");
    }

    #[test]
    fn mixed_type_names() {
        assert_type_snapshot!("option 'a -> 'a");
        assert_type_snapshot!("Map 'k (List 'v)");
        assert_type_snapshot!("{ x : int, y : Int }");
    }

    #[test]
    fn error_empty_type_variable() {
        assert_type_error_snapshot!("'");
    }

    #[test]
    fn error_upper_type_variable() {
        assert_type_error_snapshot!("'A");
    }

    #[test]
    fn error_record_extension() {
        assert_type_error_snapshot!("{ r | x : int }");
    }
}
