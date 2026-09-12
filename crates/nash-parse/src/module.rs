//! Module parsing for Nash.
//!
//! Ported from Elm's `Parse/Module.hs`.
//!
//! Parses full modules including:
//! - Module header: `module Main exposing (main)`
//! - Imports: `import List exposing (map)`
//! - Declarations: values, types, aliases

use nash_region::{Located, Region};
use nash_source::{
    Alias, Docs, Exposing, Impl, Import, Infix, Module, ModuleKind, Trait, Union, Value,
};

use crate::Parser;
use crate::declaration::Decl;
use crate::error;

impl<'a> Parser<'a> {
    /// Parse a module header.
    ///
    /// Mirrors Elm's `chompHeader` (simplified - no port/effect modules):
    /// ```text
    /// module_header = 'module' module_name 'exposing' exposing_list
    /// ```
    ///
    /// Returns the module name and exports as a tuple.
    pub fn module_header(
        &mut self,
    ) -> Result<(ModuleKind, &'a Located<&'a str>, &'a Located<Exposing<'a>>), error::Module<'a>>
    {
        let kind = self.one_of_with_fallback(
            vec![Box::new(|parser: &mut Parser<'a>| {
                let start = parser.get_position();
                parser.keyword_validator(error::Module::Validator)?;
                let end = parser.get_position();
                parser.chomp_and_check_indent(error::Module::Space, error::Module::Validator)?;
                Ok(ModuleKind::Validator(Region::new(start, end)))
            })],
            ModuleKind::Normal,
        )?;
        // Match 'module' keyword
        self.keyword_module(error::Module::Problem)?;

        self.chomp_and_check_indent(error::Module::Space, error::Module::Name)?;

        // Parse module name (e.g., "Json.Decode")
        let start = self.get_position();
        let name = self.module_name(error::Module::Name)?;
        let module_name = self.add_end(start, name);

        // Consume whitespace
        self.chomp(error::Module::Space)?;

        // Must have 'exposing' keyword
        self.keyword_exposing(|row, col| {
            error::Module::Exposing(self.bump.alloc(error::Exposing::Start(row, col)), row, col)
        })?;

        self.chomp_and_check_indent(error::Module::Space, |row, col| {
            error::Module::Exposing(
                self.bump.alloc(error::Exposing::IndentValue(row, col)),
                row,
                col,
            )
        })?;

        // Parse the exposing list, wrapping errors
        let exposing_start = self.get_position();
        let exposing = self.specialize(
            |bump, err, row, col| error::Module::Exposing(bump.alloc(err), row, col),
            |p| p.exposing(),
        )?;
        let exposing_located = self.add_end(exposing_start, exposing);

        Ok((kind, module_name, exposing_located))
    }

    /// Parse zero or more import statements.
    ///
    /// Mirrors Elm's `chompImports`:
    /// ```haskell
    /// chompImports :: [Src.Import] -> Parser E.Module [Src.Import]
    /// chompImports imports =
    ///   oneOfWithFallback
    ///     [ do  i <- chompImport
    ///           chompImports (i:imports)
    ///     ]
    ///     (reverse imports)
    /// ```
    pub(crate) fn imports(&mut self) -> Result<&'a [&'a Import<'a>], error::Module<'a>> {
        let mut imports = Vec::new();

        loop {
            // Save state in case import keyword doesn't match
            let state = self.save_state();

            match self.import() {
                Ok(import) => {
                    imports.push(import);
                    // import() already ensures fresh line at the end
                }
                Err(error) => {
                    // If we didn't consume input, we're done with imports
                    if self.pos == state.pos {
                        self.restore_state(state);
                        break;
                    }
                    // Otherwise propagate the error
                    return Err(error);
                }
            }
        }

        Ok(self.alloc_slice_copy(&imports))
    }

    /// Parse zero or more declarations.
    ///
    /// Mirrors Elm's `chompDecls`:
    /// ```haskell
    /// chompDecls :: [Decl.Decl] -> Parser E.Decl [Decl.Decl]
    /// chompDecls decls =
    ///   do  (decl, _) <- Decl.declaration
    ///       oneOfWithFallback
    ///         [ do  Space.checkFreshLine E.DeclStart
    ///               chompDecls (decl:decls)
    ///         ]
    ///         (reverse (decl:decls))
    /// ```
    fn declarations(&mut self) -> Result<Vec<Decl<'a>>, error::Module<'a>> {
        let mut decls = Vec::new();

        loop {
            // Save state in case no declaration starts
            let state = self.save_state();

            // Try to parse a declaration
            match self.specialize(
                |bump, err, row, col| error::Module::Declarations(bump.alloc(err), row, col),
                |p| p.declaration(),
            ) {
                Ok((decl, _end)) => {
                    decls.push(decl);

                    // Chomp any trailing whitespace
                    self.chomp(error::Module::Space)?;

                    // Check for fresh line (another declaration might follow)
                    if self.is_eof() {
                        break;
                    }

                    // If not at fresh line, we're done
                    if self.col != 1 {
                        break;
                    }
                }
                Err(error) => {
                    // If we didn't consume input, we're done with declarations
                    if self.pos == state.pos {
                        self.restore_state(state);
                        break;
                    }
                    // Otherwise propagate the error
                    return Err(error);
                }
            }
        }

        Ok(decls)
    }

    /// Parse zero or more infix declarations.
    ///
    /// Infixes are parsed at module level (before regular declarations).
    fn infixes(&mut self) -> Result<Vec<&'a Located<Infix<'a>>>, error::Module<'a>> {
        let mut infixes = Vec::new();

        loop {
            let state = self.save_state();

            match self.infix_decl() {
                Ok(infix) => {
                    infixes.push(infix);
                    // infix_decl already ensures fresh line
                }
                Err(_) => {
                    // If we didn't consume input, we're done
                    if self.pos == state.pos {
                        self.restore_state(state);
                        break;
                    }
                    return Err(error::Module::Infix(self.row, self.col));
                }
            }
        }

        Ok(infixes)
    }

    /// Parse a complete module.
    ///
    /// Mirrors Elm's `chompModule`:
    /// ```text
    /// module = [ module_header ] { import } { infix } { declaration }
    /// ```
    pub fn module(&mut self) -> Result<Module<'a>, error::Module<'a>> {
        // Consume initial whitespace
        self.chomp(error::Module::Space)?;

        let start_pos = self.get_position();

        // Try to parse module header (optional)
        let (kind, name, exports) =
            if self.starts_keyword(b"module") || self.starts_keyword(b"validator") {
                let (kind, name, exports) = self.module_header()?;
                self.chomp(error::Module::Space)?;
                self.check_fresh_line(error::Module::FreshLine)?;
                (kind, Some(name), exports)
            } else {
                let default_exports = self.alloc(Located::at(Region::one(), Exposing::Open));
                (ModuleKind::Normal, None, default_exports)
            };

        // Parse imports
        let imports = self.imports()?;

        // Parse infixes
        let infix_vec = self.infixes()?;
        let binops = self.alloc_slice_copy(&infix_vec);

        // Parse declarations
        let decls = self.declarations()?;

        self.chomp(error::Module::Space)?;
        let tests = self.one_of_with_fallback(
            vec![Box::new(|parser: &mut Parser<'a>| {
                parser.tests_block().map(Some)
            })],
            None,
        )?;
        self.chomp(error::Module::Space)?;
        if !self.is_eof() {
            return Err(error::Module::BadEnd(self.row, self.col));
        }

        // Categorize declarations into values, unions, aliases
        let (values, unions, aliases, traits, impls) = self.categorize_decls(decls);

        // Build docs (simplified: no module-level docs for now)
        let docs = self.alloc(Docs::NoDocs(Region::new(start_pos, self.get_position())));

        Ok(Module {
            kind,
            name,
            exports,
            docs,
            imports,
            values,
            unions,
            aliases,
            traits,
            impls,
            tests,
            binops,
        })
    }

    fn starts_keyword(&self, keyword: &[u8]) -> bool {
        let remaining = self.remaining();
        remaining.starts_with(keyword)
            && remaining
                .get(keyword.len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    }

    /// Categorize declarations into separate slices by type.
    #[allow(clippy::type_complexity)]
    fn categorize_decls(
        &self,
        decls: Vec<Decl<'a>>,
    ) -> (
        &'a [&'a Located<Value<'a>>],
        &'a [&'a Located<Union<'a>>],
        &'a [&'a Located<Alias<'a>>],
        &'a [&'a Located<Trait<'a>>],
        &'a [&'a Located<Impl<'a>>],
    ) {
        let mut values = Vec::new();
        let mut unions = Vec::new();
        let mut aliases = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();

        for decl in decls {
            match decl {
                Decl::Value(_doc, value) => values.push(value),
                Decl::Union(_doc, union) => unions.push(union),
                Decl::Alias(_doc, alias) => aliases.push(alias),
                Decl::Trait(trait_) => traits.push(trait_),
                Decl::Impl(impl_) => impls.push(impl_),
            }
        }

        (
            self.alloc_slice_copy(&values),
            self.alloc_slice_copy(&unions),
            self.alloc_slice_copy(&aliases),
            self.alloc_slice_copy(&traits),
            self.alloc_slice_copy(&impls),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use indoc::indoc;

    macro_rules! assert_module_header_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let src = bump.alloc_str(input);
            let mut parser = Parser::new(&bump, src);
            let result = parser.module_header();
            match result {
                Ok((kind, name, exposing)) => {
                    insta::with_settings!({
                        description => format!("Code:\n\n{}", input),
                        omit_expression => true,
                    }, {
                        insta::assert_debug_snapshot!((kind, name, exposing));
                    });
                }
                Err(e) => {
                    panic!("Expected successful parse, got error: {:?}", e);
                }
            }
        }};
    }

    #[test]
    fn module_header_simple_open() {
        assert_module_header_snapshot!("module Foo exposing (..)");
    }

    #[test]
    fn module_header_dotted() {
        assert_module_header_snapshot!("module Foo.Bar exposing (baz)");
    }

    #[test]
    fn module_header_mixed_exposing() {
        assert_module_header_snapshot!("module Main exposing (main, Msg(..))");
    }

    #[test]
    fn module_header_little_types() {
        assert_module_header_snapshot!("module P exposing (type option(..), type step)");
    }

    #[test]
    fn module_header_deeply_nested() {
        assert_module_header_snapshot!("module Platform.Cmd.Extra exposing (batch, none)");
    }

    #[test]
    fn validator_module_header() {
        assert_module_header_snapshot!("validator module Vesting exposing (main)");
    }

    // =========================================================================
    // Full module parsing tests
    // =========================================================================

    macro_rules! assert_module_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let src = bump.alloc_str(input);
            let mut parser = Parser::new(&bump, src);
            let result = parser.module();
            match result {
                Ok(module) => {
                    insta::with_settings!({
                        description => format!("Code:\n\n{}", input),
                        omit_expression => true,
                    }, {
                        insta::assert_debug_snapshot!(module);
                    });
                }
                Err(e) => {
                    panic!("Expected successful parse, got error: {:?}", e);
                }
            }
        }};
    }

    macro_rules! assert_module_error_snapshot {
        ($input:expr) => {{
            let input = indoc!($input);
            let bump = Bump::new();
            let src = bump.alloc_str(input);
            let mut parser = nash_parse::Parser::new(&bump, src);
            let error = parser.module().expect_err("expected module parse error");
            insta::with_settings!({
                description => format!("Code:\n\n{}", input),
                omit_expression => true,
                info => &"diagnostic",
            }, {
                insta::assert_snapshot!($crate::test_support::render_module_error(src, &error));
            });
        }};
    }

    #[test]
    fn module_header_only() {
        assert_module_snapshot!("module Main exposing (..)\n");
    }

    #[test]
    fn module_with_imports() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            import List
            import Maybe exposing (Maybe(..))
        "#
        );
    }

    #[test]
    fn module_with_value() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            main = 42
        "#
        );
    }

    #[test]
    fn module_with_type() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type Maybe 'a
                = Just 'a
                | Nothing
        "#
        );
    }

    #[test]
    fn module_with_union_then_value() {
        // A definition at column 1 must end the union's variant list, not
        // be swallowed as extra constructor arguments.
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type Wrap 'a
                = Wrap 'a

            f w = w
        "#
        );
    }

    #[test]
    fn module_with_alias() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            type alias Point = { x : Int, y : Int }
        "#
        );
    }

    #[test]
    fn module_full() {
        assert_module_snapshot!(
            r#"
            module Main exposing (main, Model, Msg(..))

            import Html exposing (div)
            import Platform.Cmd as Cmd

            type alias Model = { count : Int }

            type Msg
                = Increment
                | Decrement

            main = 0
        "#
        );
    }

    #[test]
    fn module_no_header() {
        // Without header, defaults to name=None, exports=Open
        assert_module_snapshot!("x = 1\n");
    }

    #[test]
    fn module_with_infix() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            infix left 6 (|>) = apR

            apR f x = f x
        "#
        );
    }

    #[test]
    fn module_with_empty_trait_then_value() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            trait Marker 'a where

            x = 1
        "#
        );
    }
    #[test]
    fn later_constraint_does_not_imply_trait_superclass() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            trait Show 'a where

            id : Eq 'a => 'a -> 'a
            id x = x
        "#
        );
    }

    #[test]
    fn module_with_empty_impl_then_value() {
        assert_module_snapshot!(
            r#"
            module Main exposing (..)

            impl Show unit where

            x = 1
        "#
        );
    }

    #[test]
    fn validator_module_full() {
        assert_module_snapshot!(
            r#"
            validator module Vesting exposing (main)

            import Cardano.Tx exposing (Tx, Output)

            type Datum = Datum { owner : Bytes, deadline : Int }

            type step 'a = Done 'a | Next int 'a

            type alias acc = { total : int, seen : list Int }

            main datum = assert True
        "#
        );
    }

    #[test]
    fn validator_requires_module_keyword() {
        assert_module_error_snapshot!("validator Vesting exposing (main)");
    }

    #[test]
    fn validator_module_must_remain_indented() {
        assert_module_error_snapshot!("validator\nmodule V exposing (..)");
    }

    #[test]
    fn module_preserves_import_alias_error() {
        assert_module_error_snapshot!("import Cardano.Tx as tx");
    }

    #[test]
    fn module_preserves_type_alias_error() {
        assert_module_error_snapshot!("type alias account");
    }

    #[test]
    fn module_preserves_pattern_error() {
        assert_module_error_snapshot!("f (x as) = x");
    }

    #[test]
    fn module_preserves_expression_error() {
        assert_module_error_snapshot!("value = if True then 42");
    }

    #[test]
    fn module_preserves_annotation_name_error() {
        assert_module_error_snapshot!("f : int\ng = 1");
    }
}
