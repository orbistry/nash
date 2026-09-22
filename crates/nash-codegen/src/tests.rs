//! Closed roots for module-local unit and property tests.
use std::path::Path;

use nash_ast::{ModuleName, NodeId, Type};
use nash_ir::{
    core::*,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, flat};
use nash_region::{Located, Position, Region};
pub use nash_test::{AssertSite, Capture, Programs, TestProgram};

use crate::{
    build::{Binding, Build, Context, Engine, TraceConfig},
    decision_tree::{self, MatchBranch, MatchInputs},
    ty_of::Substitution,
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
            let prepare = engine.property(test, &ctx)?;
            Programs::Prop {
                prepare: encode(&mut engine, prepare, version)?,
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
        ctx: &Context<'a>,
    ) -> Result<&'a Core<'a>, crate::build::Error<'a>> {
        let (last, prefix) = test.binders.split_last().expect("property has generators");
        let mut generator = self.expr(last.generator, ctx)?;
        let mut typ = self.substitute(
            self.can_type(NodeId::pattern(last.pattern), ctx)?,
            &ctx.subst,
        )?;
        for binder in prefix.iter().rev() {
            let head_type = self.substitute(
                self.can_type(NodeId::pattern(binder.pattern), ctx)?,
                &ctx.subst,
            )?;
            let first = self.expr(binder.generator, ctx)?;
            let unit = Binder {
                name: self.ir.fresh("unit"),
                ty: Ty::Const(&ConstTy::Unit),
            };
            let rest = self.ir.lam(&[unit], generator);
            let both = self.base_function(
                "Test",
                "both",
                Substitution::from([("a", head_type), ("b", typ)]),
            )?;
            generator = self.ir.app(both, &[first, rest]);
            typ = self.ir.arena.alloc(Located::at_zero(Type::Tuple {
                first: head_type,
                second: typ,
                rest: &[],
            }));
        }
        let body = self.property_callback(test, ctx, typ, false)?;
        let display = self.property_callback(test, ctx, typ, true)?;
        let prepare = self.base_function("Test", "prepare", Substitution::from([("a", typ)]))?;
        Ok(self.ir.app(prepare, &[generator, body, display]))
    }

    fn property_callback(
        &mut self,
        test: &'a nash_ast::Test<'a>,
        ctx: &Context<'a>,
        typ: &'a Located<Type<'a>>,
        display: bool,
    ) -> Result<&'a Core<'a>, crate::build::Error<'a>> {
        let argument = Binder {
            name: self.ir.fresh("values"),
            ty: self.types.ty(typ, &Substitution::new())?,
        };
        let mut remaining = self.ir.var(argument.name);
        let mut child = ctx.clone();
        let mut patterns = Vec::new();
        let mut shown = Vec::new();
        for (index, binder) in test.binders.iter().enumerate() {
            let value_ty = self.ty(NodeId::pattern(binder.pattern), ctx)?;
            let value = Binder {
                name: self.ir.fresh("generated"),
                ty: value_ty,
            };
            let input = if index + 1 == test.binders.len() {
                remaining
            } else {
                let first = self.ir.field(remaining, 0, 2);
                remaining = self.ir.field(remaining, 1, 2);
                first
            };
            let (records, literals) = self.pattern_inputs(binder.pattern, ctx)?;
            let bindings = decision_tree::bindings(
                &self.ir,
                &mut self.types,
                value_ty,
                binder.pattern,
                &records,
            )?;
            for (name, bound) in &bindings {
                child.env.insert(name, Binding::Value(*bound));
            }
            if display {
                let rendered = self
                    .show_value(
                        NodeId::pattern(binder.pattern),
                        self.ir.var(value.name),
                        ctx,
                    )?
                    .unwrap_or_else(|| self.string("?"));
                shown.push(rendered);
            }
            patterns.push((binder, value, input, bindings, records, literals));
        }
        let mut body = if display {
            self.list(Ty::Const(&ConstTy::String), &shown)?
        } else {
            self.expr(test.body, &child)?
        };
        for (binder, value, input, bindings, records, literals) in patterns.into_iter().rev() {
            body = decision_tree::compile(
                &self.ir,
                &mut self.types,
                value.ty,
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
            body = self.ir.let_(value, input, body);
        }
        Ok(self.ir.lam(&[argument], body))
    }
}

#[cfg(test)]
mod integration;
