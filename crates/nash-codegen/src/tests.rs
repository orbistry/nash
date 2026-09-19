//! Closed roots for module-local unit and property tests.
use std::path::Path;

use nash_ast::{ModuleName, NodeId, ViaBinder, primitives};
use nash_ir::{
    core::*,
    ty::{BigTy, ConstTy, TermTy, Ty},
};
use nash_plutus::{arena::Arena, flat};
use nash_region::{Position, Region};
pub use nash_test::{AssertSite, Capture, Programs, TestProgram};

use crate::{
    build::{Binding, Build, Context, Engine, TraceConfig},
    decision_tree::{self, MatchBranch, MatchInputs},
};

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("unknown test module {0:?}")]
    UnknownModule(ModuleName<'a>),
    #[error("{0}")]
    Build(crate::build::Error<'a>),
    #[error("{0}")]
    Program(crate::program::Error<'a>),
    #[error("could not encode test program: {0}")]
    Encoding(String),
}

impl<'a> From<crate::build::Error<'a>> for Error<'a> {
    fn from(value: crate::build::Error<'a>) -> Self {
        Self::Build(value)
    }
}
impl<'a> From<crate::program::Error<'a>> for Error<'a> {
    fn from(value: crate::program::Error<'a>) -> Self {
        Self::Program(value)
    }
}

/// Compile private test roots directly against their original solved nodes.
/// Tests never enter the module's public interface or definition namespace.
pub fn compile_tests<'a>(
    arena: &'a Arena,
    build: &Build<'a, '_>,
    module: ModuleName<'a>,
    source: &str,
    path: &Path,
    version: nash_config::PlutusVersion,
    trace: TraceConfig,
) -> Result<Vec<TestProgram>, Error<'a>> {
    compile_tests_matching(arena, build, module, source, path, version, trace, |_| true)
}

/// Select declarations before code generation: an unselected property must not
/// impose its native-constructor target requirements on selected unit tests.
#[allow(clippy::too_many_arguments)]
pub fn compile_tests_matching<'a>(
    arena: &'a Arena,
    build: &Build<'a, '_>,
    module: ModuleName<'a>,
    source: &str,
    path: &Path,
    version: nash_config::PlutusVersion,
    trace: TraceConfig,
    mut include: impl FnMut(&nash_ast::Test<'a>) -> bool,
) -> Result<Vec<TestProgram>, Error<'a>> {
    let input = build
        .inputs
        .iter()
        .position(|i| i.module.name == module)
        .ok_or(Error::UnknownModule(module))?;
    let mut result = Vec::new();
    for test in build.inputs[input].module.tests {
        if !include(test) {
            continue;
        }
        let mut engine = Engine::new(build, arena, trace);
        let ctx = engine.test_context(input);
        let programs = if test.binders.is_empty() {
            let root = engine.expr(test.body, &ctx)?;
            Programs::Unit {
                run: encode(&mut engine, root, version)?,
            }
        } else {
            let prng = Binder {
                name: engine.ir.fresh("prng"),
                ty: Ty::Big(&BigTy::Data),
            };
            let run = engine.property(test, 0, &ctx, engine.ir.var(prng.name), false, &[])?;
            let run = engine.ir.lam(&[prng], run);
            let run = encode(&mut engine, run, version)?;

            let mut draw_engine = Engine::new(build, arena, trace);
            let draw_ctx = draw_engine.test_context(input);
            let prng = Binder {
                name: draw_engine.ir.fresh("prng"),
                ty: Ty::Big(&BigTy::Data),
            };
            let draw = draw_engine.property(
                test,
                0,
                &draw_ctx,
                draw_engine.ir.var(prng.name),
                true,
                &[],
            )?;
            let draw = draw_engine.ir.lam(&[prng], draw);
            Programs::Prop {
                draw: encode(&mut draw_engine, draw, version)?,
                run,
            }
        };
        result.push(TestProgram {
            module: module.name.to_owned(),
            name: test.name.value.to_owned(),
            expect: test.expect,
            budget: test.budget,
            region: test.region,
            programs,
            asserts: engine.asserts,
            binder_texts: test
                .binders
                .iter()
                .map(|b| source_region(source, b.pattern.region))
                .collect(),
            plutus_version: version,
            source: source.to_owned(),
            source_path: path.to_owned(),
        });
    }
    Ok(result)
}

fn encode<'a>(
    engine: &mut Engine<'a, '_, '_>,
    root: &'a Core<'a>,
    version: nash_config::PlutusVersion,
) -> Result<Vec<u8>, Error<'a>> {
    let core = engine.finish_root(root)?;
    let version = match version {
        nash_config::PlutusVersion::V1 => nash_plutus::machine::PlutusVersion::V1,
        nash_config::PlutusVersion::V2 => nash_plutus::machine::PlutusVersion::V2,
        nash_config::PlutusVersion::V3 => nash_plutus::machine::PlutusVersion::V3,
    };
    let program = crate::program::assemble_core_for_version(engine.ir.arena, core, version)?;
    flat::encode(program.program).map_err(|e| Error::Encoding(e.to_string()))
}

fn source_region(source: &str, region: Region) -> String {
    fn offset(source: &str, pos: Position) -> Option<usize> {
        let mut start = 0;
        for (i, line) in source.split_inclusive('\n').enumerate() {
            if i + 1 == pos.line {
                let column = pos.column.checked_sub(1)?;
                return line.is_char_boundary(column).then_some(start + column);
            }
            start += line.len();
        }
        None
    }
    offset(source, region.start)
        .zip(offset(source, region.end))
        .and_then(|(start, end)| source.get(start..end))
        .unwrap_or("_")
        .to_owned()
}

impl<'a> Engine<'a, '_, '_> {
    fn property(
        &mut self,
        test: &'a nash_ast::Test<'a>,
        index: usize,
        ctx: &Context<'a>,
        prng: &'a Core<'a>,
        draw: bool,
        shown: &[&'a Core<'a>],
    ) -> Result<&'a Core<'a>, crate::build::Error<'a>> {
        let Some(binder) = test.binders.get(index) else {
            return if draw {
                let strings = self.list(Ty::Const(&ConstTy::String), shown)?;
                Ok(self.ir.constr(0, &[self.ir.constr(0, &[prng, strings])]))
            } else {
                let body = self.expr(test.body, ctx)?;
                let unit = Binder {
                    name: self.ir.fresh("test_result"),
                    ty: Ty::Const(&ConstTy::Unit),
                };
                Ok(self.ir.let_(unit, body, self.ir.constr(0, &[prng])))
            };
        };
        let fuzzer = self.expr(binder.fuzzer, ctx)?;
        let (fuzzer_tag, some_tag, none_tag, function_ty, tuple_ty) =
            self.fuzzer_layout(binder, ctx)?;
        let function = Binder {
            name: self.ir.fresh("fuzzer"),
            ty: function_ty,
        };
        let tuple = Binder {
            name: self.ir.fresh("drawn"),
            ty: tuple_ty,
        };
        let value_ty = self.ty(NodeId::pattern(binder.pattern), ctx)?;
        let value = Binder {
            name: self.ir.fresh("generated"),
            ty: value_ty,
        };
        let next = Binder {
            name: self.ir.fresh("next_prng"),
            ty: Ty::Big(&BigTy::Data),
        };
        let (records, literals) = self.pattern_inputs(binder.pattern, ctx)?;
        let bindings = decision_tree::bindings(
            &self.ir,
            &mut self.types,
            value_ty,
            binder.pattern,
            &records,
        )?;
        let mut child = ctx.clone();
        for (name, bound) in &bindings {
            child.env.insert(name, Binding::Value(*bound));
        }
        let mut shown = shown.to_vec();
        if draw {
            let display = self
                .show_value(
                    NodeId::pattern(binder.pattern),
                    self.ir.var(value.name),
                    ctx,
                )?
                .unwrap_or_else(|| self.string("?"));
            shown.push(display);
        }
        let body = self.property(
            test,
            index + 1,
            &child,
            self.ir.var(next.name),
            draw,
            &shown,
        )?;
        let body = decision_tree::compile(
            &self.ir,
            &mut self.types,
            value_ty,
            self.ir.var(value.name),
            &[MatchBranch {
                pattern: binder.pattern,
                bindings,
                body,
            }],
            MatchInputs {
                record_fields: &records,
                literal_tests: &literals,
            },
            self.ir.error(),
        )?;
        let body = self.ir.let_(
            next,
            self.ir.field(self.ir.var(tuple.name), 0, 2),
            self.ir
                .let_(value, self.ir.field(self.ir.var(tuple.name), 1, 2), body),
        );
        let sampled = self.ir.app(self.ir.var(function.name), &[prng]);
        let sampled = self.ir.case(
            CaseKind::Tag,
            sampled,
            &[
                Branch {
                    test: Test::Tag(some_tag),
                    binders: self.ir.arena.alloc_slice_copy(&[tuple]),
                    body,
                },
                Branch {
                    test: Test::Tag(none_tag),
                    binders: &[],
                    body: self.ir.constr(1, &[]),
                },
            ],
            None,
        );
        Ok(self.ir.case(
            CaseKind::Tag,
            fuzzer,
            &[Branch {
                test: Test::Tag(fuzzer_tag),
                binders: self.ir.arena.alloc_slice_copy(&[function]),
                body: sampled,
            }],
            None,
        ))
    }

    /// Validate and obtain constructor tags from the actual standard-library
    /// metadata, including its native option and tuple representation.
    fn fuzzer_layout(
        &mut self,
        binder: &ViaBinder<'a>,
        ctx: &Context<'a>,
    ) -> Result<(u16, u16, u16, Ty<'a>, Ty<'a>), crate::build::Error<'a>> {
        use crate::build::Error as E;
        let ty = self.ty(NodeId::expr(binder.fuzzer), ctx)?;
        let Ty::Term(TermTy::Adt(adt)) = ty else {
            return Err(E::RuntimeLayout(ty));
        };
        if adt.name.home.package != Some(primitives::CORE)
            || adt.name.home.name != "Fuzz"
            || adt.name.name != "fuzzer"
        {
            return Err(E::InvalidConstructor);
        }
        let union = self
            .build
            .unions
            .get(&adt.name)
            .ok_or(E::InvalidConstructor)?;
        let [constructor] = union.ctors else {
            return Err(E::InvalidConstructor);
        };
        let fuzzer_tag = constructor.index;
        let fields = self.types.layout(*adt)?;
        let [function] = fields[fuzzer_tag as usize] else {
            return Err(E::InvalidConstructor);
        };
        let Ty::Term(TermTy::Fun(_, Ty::Term(TermTy::Adt(option)))) = function else {
            return Err(E::RuntimeLayout(*function));
        };
        let union = self
            .build
            .unions
            .get(&option.name)
            .ok_or(E::InvalidConstructor)?;
        let some = union
            .ctors
            .iter()
            .find(|c| c.name == "Some" && c.arity == 1)
            .ok_or(E::InvalidConstructor)?
            .index;
        let none = union
            .ctors
            .iter()
            .find(|c| c.name == "None" && c.arity == 0)
            .ok_or(E::InvalidConstructor)?
            .index;
        let fields = self.types.layout(*option)?;
        let [tuple @ Ty::Term(TermTy::Tuple([_, _]))] = fields[some as usize] else {
            return Err(E::InvalidConstructor);
        };
        Ok((fuzzer_tag, some, none, *function, *tuple))
    }
}

#[cfg(test)]
mod integration;
