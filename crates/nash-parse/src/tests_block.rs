use bumpalo::collections::Vec as BumpVec;
use nash_region::{Located, Position, Region};
use nash_source::{
    Block, Budget, Expect, Proof, ProofBinder, ProofBody, Proofs, Test, TestBody, Tests, ViaBinder,
};

use crate::Parser;
use crate::error::{self, Test as TestErr, Tests as TestsErr};

impl<'a> Parser<'a> {
    pub(crate) fn tests_block(&mut self) -> Result<&'a Tests<'a>, error::Module<'a>> {
        let (imports, tests) = self.specification_block(false, Self::test_item)?;
        Ok(self.alloc(Tests { imports, tests }))
    }

    pub(crate) fn proofs_block(&mut self) -> Result<&'a Proofs<'a>, error::Module<'a>> {
        let (imports, proofs) = self.specification_block(true, Self::proof_item)?;
        Ok(self.alloc(Proofs { imports, proofs }))
    }

    fn specification_block<T: 'a>(
        &mut self,
        proof: bool,
        parse_item: fn(&mut Self) -> Result<&'a Located<T>, TestErr<'a>>,
    ) -> Result<(&'a [&'a nash_source::Import<'a>], &'a [&'a Located<T>]), error::Module<'a>> {
        self.in_context(
            |bump, error, row, col| error::Module::Tests(bump.alloc(error), row, col),
            |parser| {
                if proof {
                    parser.keyword_proof(error::Module::BadEnd)
                } else {
                    parser.keyword_tests(error::Module::BadEnd)
                }
            },
            |parser| {
                let block_end = parser.get_position();
                parser.chomp(TestsErr::Space)?;
                if parser.is_eof() || parser.col() == 1 {
                    return Ok((&[][..], &[][..]));
                }
                parser.check_indent(block_end.line, block_end.column, TestsErr::IndentStart)?;
                parser.with_indent(|parser| {
                    let imports = parser.specialize(
                        |bump, error, row, col| TestsErr::Import(bump.alloc(error), row, col),
                        |parser| parser.imports(),
                    )?;
                    let mut items = BumpVec::new_in(parser.bump);
                    loop {
                        if parser.is_eof() || parser.col() < parser.indent() {
                            break;
                        }
                        parser.check_aligned(TestsErr::Alignment)?;
                        let item = parser.specialize(
                            |bump, error, row, col| TestsErr::Test(bump.alloc(error), row, col),
                            parse_item,
                        )?;
                        items.push(item);
                    }
                    Ok((imports, items.into_bump_slice()))
                })
            },
        )
    }

    fn specification_header(
        &mut self,
    ) -> Result<(Position, bool, &'a Located<&'a str>, Expect), TestErr<'a>> {
        let start = self.get_position();
        let is_prop = self.one_of(
            TestErr::NameStart,
            vec![
                Box::new(|parser: &mut Self| {
                    parser.keyword_test(TestErr::NameStart)?;
                    Ok(false)
                }),
                Box::new(|parser: &mut Self| {
                    parser.keyword_prop(TestErr::NameStart)?;
                    Ok(true)
                }),
            ],
        )?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentName)?;
        let name_start = self.get_position();
        let name_str = self.string_literal(TestErr::NameStart, TestErr::Name)?;
        let name = self.add_end(name_start, name_str);
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
        let expect = self.test_expect(is_prop)?;
        Ok((start, is_prop, name, expect))
    }

    fn test_item(&mut self) -> Result<&'a Located<Test<'a>>, TestErr<'a>> {
        let (start, is_prop, name, expect) = self.specification_header()?;
        let budget = self.test_budget()?;
        self.word1(b'=', TestErr::Equals)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (body, end) = if is_prop {
            let (binders, body, end) =
                self.property_body(|pattern, generator| ViaBinder { pattern, generator })?;
            (TestBody::Prop { binders, body }, end)
        } else {
            let (block, end) = self.test_block()?;
            (TestBody::Unit(block), end)
        };
        self.chomp(TestErr::Space)?;
        Ok(self.alloc(Located::at(
            Region::new(start, end),
            Test {
                name,
                expect,
                budget,
                body,
            },
        )))
    }

    fn proof_item(&mut self) -> Result<&'a Located<Proof<'a>>, TestErr<'a>> {
        let (start, is_prop, name, expect) = self.specification_header()?;
        if self.next_is_keyword(b"within") {
            let (row, col) = self.position();
            return Err(TestErr::ProofBudget(row, col));
        }
        self.word1(b'=', TestErr::Equals)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (body, end) = if is_prop {
            let (binders, body, end) =
                self.property_body(|pattern, domain| ProofBinder { pattern, domain })?;
            (ProofBody::Prop { binders, body }, end)
        } else {
            let (block, end) = self.test_block()?;
            (ProofBody::Unit(block), end)
        };
        self.chomp(TestErr::Space)?;
        Ok(self.alloc(Located::at(
            Region::new(start, end),
            Proof { name, expect, body },
        )))
    }

    fn test_block(&mut self) -> Result<(&'a Block<'a>, Position), TestErr<'a>> {
        self.keyword_do(TestErr::Do)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let parent_indent = self.indent();
        let (stmts, last, end) = self.specialize(
            |bump, error, row, col| TestErr::Body(bump.alloc(error), row, col),
            |parser| parser.with_indent(|parser| parser.do_body(parent_indent)),
        )?;
        Ok((self.alloc(Block { stmts, last }), end))
    }

    fn test_expect(&mut self, is_prop: bool) -> Result<Expect, TestErr<'a>> {
        self.one_of_with_fallback(
            vec![Box::new(|parser: &mut Parser<'a>| {
                parser.keyword_fail(TestErr::Equals)?;
                parser.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                let (row, col) = parser.position();
                parser.one_of_with_fallback(
                    vec![Box::new(|parser: &mut Parser<'a>| {
                        parser.keyword_once(TestErr::OnceOnUnitTest)?;
                        if !is_prop {
                            return Err(TestErr::OnceOnUnitTest(row, col));
                        }
                        parser.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                        Ok(Expect::FailOnce)
                    })],
                    Expect::Fail,
                )
            })],
            Expect::Pass,
        )
    }

    fn test_budget(&mut self) -> Result<Option<Budget>, TestErr<'a>> {
        self.one_of_with_fallback(
            vec![Box::new(|parser: &mut Parser<'a>| {
                parser.keyword_within(TestErr::Equals)?;
                parser.chomp_and_check_indent(TestErr::Space, TestErr::WithinOpen)?;
                let opening = parser.get_position();
                parser.word1(b'(', TestErr::WithinOpen)?;
                parser.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
                let first = parser.budget_entry()?;
                parser.chomp_and_check_indent(TestErr::Space, |r, c| {
                    TestErr::WithinEnd(opening, r, c)
                })?;
                let budget = parser.one_of(
                    |r, c| TestErr::WithinEnd(opening, r, c),
                    vec![
                        Box::new(|parser: &mut Parser<'a>| {
                            parser.word1(b',', |r, c| TestErr::WithinEnd(opening, r, c))?;
                            parser.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
                            let (row, col) = parser.position();
                            let second = parser.budget_entry()?;
                            parser.chomp_and_check_indent(TestErr::Space, |r, c| {
                                TestErr::WithinEnd(opening, r, c)
                            })?;
                            parser.word1(b')', |r, c| TestErr::WithinEnd(opening, r, c))?;
                            match (first, second) {
                                (Budget::Cpu(cpu), Budget::Mem(mem))
                                | (Budget::Mem(mem), Budget::Cpu(cpu)) => {
                                    Ok(Budget::Both { cpu, mem })
                                }
                                _ => Err(TestErr::WithinDuplicate(row, col)),
                            }
                        }),
                        Box::new(|parser: &mut Parser<'a>| {
                            parser.word1(b')', |r, c| TestErr::WithinEnd(opening, r, c))?;
                            Ok(first)
                        }),
                    ],
                )?;
                parser.chomp_and_check_indent(TestErr::Space, TestErr::IndentEquals)?;
                Ok(Some(budget))
            })],
            None,
        )
    }

    fn budget_entry(&mut self) -> Result<Budget, TestErr<'a>> {
        let constructor = self.one_of(
            TestErr::WithinKind,
            vec![
                Box::new(|parser: &mut Parser<'a>| {
                    parser.keyword_cpu(TestErr::WithinKind)?;
                    Ok(Budget::Cpu as fn(i128) -> Budget)
                }),
                Box::new(|parser: &mut Parser<'a>| {
                    parser.keyword_mem(TestErr::WithinKind)?;
                    Ok(Budget::Mem as fn(i128) -> Budget)
                }),
            ],
        )?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::WithinKind)?;
        let number = self.number_literal(TestErr::WithinKind, TestErr::WithinNumber)?;
        Ok(constructor(number))
    }

    fn property_body<T: 'a>(
        &mut self,
        make: fn(&'a Located<nash_source::Pattern<'a>>, &'a Located<nash_source::Expr<'a>>) -> T,
    ) -> Result<(&'a [&'a Located<T>], &'a Block<'a>, Position), TestErr<'a>> {
        self.keyword_let(TestErr::Let)?;
        let (binders, binders_end) = self.with_backset_indent(3, |parser| {
            parser.chomp_and_check_indent(TestErr::Space, TestErr::IndentBinder)?;
            parser.with_indent(|parser| {
                let mut binders = BumpVec::new_in(parser.bump);
                let (first, mut end) = parser.via_binder(make)?;
                binders.push(first);

                loop {
                    if parser.next_is_keyword(b"in") || parser.col() < parser.indent() {
                        break;
                    }
                    parser.check_aligned(TestErr::BinderAlignment)?;
                    let (binder, binder_end) = parser.via_binder(make)?;
                    binders.push(binder);
                    end = binder_end;
                }
                Ok((binders.into_bump_slice(), end))
            })
        })?;
        self.check_indent(binders_end.line, binders_end.column, TestErr::IndentIn)?;
        self.keyword_in(TestErr::In)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBody)?;
        let (body, end) = self.test_block()?;
        Ok((binders, body, end))
    }

    fn via_binder<T: 'a>(
        &mut self,
        make: fn(&'a Located<nash_source::Pattern<'a>>, &'a Located<nash_source::Expr<'a>>) -> T,
    ) -> Result<(&'a Located<T>, Position), TestErr<'a>> {
        let start = self.get_position();
        let pattern = self.specialize(
            |bump, error, row, col| TestErr::Pattern(bump.alloc(error), row, col),
            |parser| parser.pattern_term(),
        )?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::Via)?;
        self.keyword_via(TestErr::Via)?;
        self.chomp_and_check_indent(TestErr::Space, TestErr::IndentBinder)?;
        let (generator, end) = self.specialize(
            |bump, error, row, col| TestErr::Generator(bump.alloc(error), row, col),
            |parser| parser.expression(),
        )?;
        Ok((
            self.alloc(Located::at(
                Region::new(start, end),
                make(pattern, generator),
            )),
            end,
        ))
    }

    fn next_is_keyword(&self, keyword: &[u8]) -> bool {
        let remaining = self.remaining();
        remaining.starts_with(keyword)
            && remaining
                .get(keyword.len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    }
}

#[cfg(test)]
mod tests {
    use bumpalo::Bump;
    use indoc::indoc;

    use crate::Parser;

    macro_rules! assert_tests_module_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let source = bump.alloc_str(input);
            let mut parser = Parser::new(&bump, source);
            let module = parser.module().expect("expected successful module parse");
            insta::with_settings!({
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
            }, {
                insta::assert_debug_snapshot!(module);
            });
        }};
    }

    macro_rules! assert_tests_module_error_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let source = bump.alloc_str(input);
            let mut parser = nash_parse::Parser::new(&bump, source);
            let error = parser.module().expect_err("expected module parse error");
            insta::with_settings!({
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
                info => &"diagnostic",
            }, {
                insta::assert_snapshot!($crate::test_support::render_module_error(source, &error));
            });
        }};
    }

    #[test]
    fn complete_tests_block() {
        assert_tests_module_snapshot!(
            r#"
            module Main exposing (main)

            main = 1

            tests
                import Prop exposing (int, listOf)

                test "lt is strict" = do
                    assert (not (lt 1 1))

                test "fails" fail = do
                    assert (1 / 0 == 0)

                test "budget" within (cpu 1000, mem 50) = do
                    assert True

                prop "antisym" fail once within (mem 5) =
                    let
                        a via int
                        b via int
                    in
                    do
                        label "x"
                        assert (compare a b == invert (compare b a))

                prop "sorted" =
                    let xs via listOf int in
                    do
                        let
                            ys = sort xs
                        n <- length ys
                        assert (n == length xs)
        "#
        );
    }

    #[test]
    fn tests_without_imports() {
        assert_tests_module_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "truth" = do
                    assert True
        "#
        );
    }

    #[test]
    fn tests_with_imports_only() {
        assert_tests_module_snapshot!(
            r#"
            module Main exposing (..)

            tests
                import Prop exposing (int)
        "#
        );
    }

    #[test]
    fn nested_monadic_do_in_test_block() {
        assert_tests_module_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "nested do" = do
                    r <- do
                        x <- action
                        pure x
                    assert r
        "#
        );
    }

    #[test]
    fn test_body_requires_do() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "t" =
                    assert True
        "#
        );
    }

    #[test]
    fn tests_block_must_be_last() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
            x = 1
        "#
        );
    }

    #[test]
    fn test_name_must_be_string() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test name = do
                    assert True
        "#
        );
    }

    #[test]
    fn property_requires_let() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                prop "p" = assert True
        "#
        );
    }

    #[test]
    fn property_binder_requires_via() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                prop "p" = let x = int in x
        "#
        );
    }

    #[test]
    fn unit_test_rejects_via_let() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "t" = let x via int in do
                    assert True
        "#
        );
    }

    #[test]
    fn property_requires_at_least_one_binder() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                prop "p" = let in do
                    assert True
        "#
        );
    }

    #[test]
    fn once_is_invalid_on_unit_test() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "t" fail once = do
                    assert True
        "#
        );
    }

    #[test]
    fn test_block_must_end_in_expression() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "t" = do
                    x <- e
        "#
        );
    }

    #[test]
    fn budget_kinds_cannot_repeat() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "t" within (cpu 1, cpu 2) = do
                    assert True
        "#
        );
    }

    #[test]
    fn test_items_must_align() {
        assert_tests_module_error_snapshot!(
            r#"
            module Main exposing (..)

            tests
                test "first" = do
                    assert True
                 test "second" = do
                     assert True
        "#
        );
    }

    #[test]
    fn syntax_overview_acceptance() {
        assert_tests_module_snapshot!(
            r#"
            validator module Vesting exposing (main)

            import Cardano.Tx exposing (Tx, Output)

            type Datum = Datum { owner : Bytes, deadline : Int }

            type step 'a = Done 'a | Next int 'a

            type alias acc = { total : int, seen : list Int }

            trait Eq 'a => Ord 'a where
                compare : 'a -> 'a -> ordering

                lt : 'a -> 'a -> bool
                lt a b = compare a b == LT

            impl Ord int where
                compare = Builtin.compareInteger

            @derive(Eq, Show, ToData, FromData)
            type Redeemer = Claim | Cancel

            main : Datum -> Redeemer -> Data -> unit
            main datum redeemer ctx =
                case redeemer of
                    Claim -> assert (lower datum.deadline < currentSlot ctx)
                    Cancel -> assert (signedBy ctx datum.owner)

            tests
                import Prop exposing (int, listOf)

                test "lt is strict" = do
                    assert (not (lt 1 1))

                prop "compare is antisymmetric" =
                    let
                        a via int
                        b via int
                    in
                    do
                        label (if a < b then "lt" else "ge")
                        assert (compare a b == invert (compare b a))

                prop "division by zero fails" fail =
                    let x via int in
                    do
                        assert (x / 0 == 0)
        "#
        );
    }
}

#[cfg(test)]
mod proof_tests {
    #[test]
    fn tests_and_proofs_are_separate_and_either_order_parses() {
        for source in [
            "module Main exposing (..)\ntests\n    test \"t\" = do\n        assert True\nproof\n    prop \"p\" = let x via Proof.int in do\n        assert (x == x)\n",
            "module Main exposing (..)\nproof\n    test \"p\" = do\n        assert True\ntests\n    test \"t\" = do\n        assert True\n",
        ] {
            let bump = bumpalo::Bump::new();
            let module = crate::Parser::new(&bump, source).module().unwrap();
            assert_eq!(module.tests.unwrap().tests.len(), 1);
            assert_eq!(module.proofs.unwrap().proofs.len(), 1);
        }
    }
    #[test]
    fn duplicate_proof_blocks_are_rejected() {
        let bump = bumpalo::Bump::new();
        assert!(
            crate::Parser::new(&bump, "module Main exposing (..)\nproof\nproof\n")
                .module()
                .is_err()
        );
    }
    #[test]
    fn proof_budget_is_rejected_during_parsing() {
        let bump = bumpalo::Bump::new();
        let error = crate::Parser::new(&bump, "module Main exposing (..)\nproof\n    test \"p\" within (cpu 100) = do\n        assert True\n").module().unwrap_err();
        let crate::error::Module::Tests(error, ..) = error else {
            panic!("{error:?}");
        };
        let crate::error::Tests::Test(error, ..) = error else {
            panic!("{error:?}");
        };
        assert!(matches!(error, crate::error::Test::ProofBudget(..)));
    }
}
