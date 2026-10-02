//! Compile proof bodies as UPLC functions over symbolic input domains.
use crate::{
    build::{Binding, Build, Engine, TraceConfig},
    decision_tree::{self, MatchBranch, MatchInputs},
};
use nash_ast::{Expr, ModuleName, NodeId, ProofObligation, primitives};
use nash_ir::{
    core::*,
    ty::{ConstTy, Ty},
};
use nash_plutus::{arena::Arena, flat};
pub use nash_proof::{Domain, ProofProgram};
use nash_proof::{Postcondition, ReturnDomain};

#[derive(Debug, thiserror::Error)]
pub enum Error<'a> {
    #[error("unknown proof module {0:?}")]
    UnknownModule(ModuleName<'a>),
    #[error(
        "proof input must use a domain from the bundled Proof module; random generators and arbitrary values are not proof domains"
    )]
    Domain,
    #[error("ledger domain version must match the proof's Plutus version")]
    LedgerVersion,
    #[error(
        "Proof.returns requires an integer, Boolean, bytes, string, unit or Data-represented result"
    )]
    PartialResult,
    #[error("{0}")]
    Build(crate::build::Error<'a>),
    #[error("{0}")]
    Program(crate::program::Error<'a>),
    #[error("could not encode proof program: {0}")]
    Encoding(String),
}

impl<'a> From<crate::build::Error<'a>> for Error<'a> {
    fn from(error: crate::build::Error<'a>) -> Self {
        Self::Build(error)
    }
}
impl<'a> From<crate::program::Error<'a>> for Error<'a> {
    fn from(error: crate::program::Error<'a>) -> Self {
        Self::Program(error)
    }
}
impl<'a> From<crate::decision_tree::Error<'a>> for Error<'a> {
    fn from(error: crate::decision_tree::Error<'a>) -> Self {
        Self::Build(error.into())
    }
}

pub fn compile_proofs_matching<'a>(
    arena: &'a Arena,
    build: &Build<'a, '_>,
    module: ModuleName<'a>,
    version: nash_config::PlutusVersion,
    mut include: impl FnMut(&nash_ast::Proof<'a>) -> bool,
) -> Result<Vec<ProofProgram>, Error<'a>> {
    let input = build
        .inputs
        .iter()
        .position(|i| i.module.name == module)
        .ok_or(Error::UnknownModule(module))?;
    let mut programs = Vec::new();
    for proof in build.inputs[input].module.proofs {
        if !include(proof) {
            continue;
        }
        let domains = proof
            .binders
            .iter()
            .map(|binder| {
                let Expr::VarForeign { reference, .. } = binder.domain.value else {
                    return Err(Error::Domain);
                };
                if reference.home.package != Some(primitives::BASE)
                    || reference.home.name != "Proof"
                {
                    return Err(Error::Domain);
                }
                let domain = Domain::named(reference.name).ok_or(Error::Domain)?;
                let target = match version {
                    nash_config::PlutusVersion::V1 => 1,
                    nash_config::PlutusVersion::V2 => 2,
                    nash_config::PlutusVersion::V3 => 3,
                };
                if matches!(domain, Domain::Spending(v) | Domain::Minting(v) if v != target) {
                    return Err(Error::LedgerVersion);
                }
                Ok(domain)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut engine = Engine::new(
            build,
            arena,
            TraceConfig {
                user: crate::build::TraceLevel::Silent,
                compiler: false,
            },
        );
        let mut ctx = engine.test_context(input);
        ctx.test = false; // Ordinary assertions: no test power-assert instrumentation.
        let mut params = Vec::new();
        let mut patterns = Vec::new();
        for binder in proof.binders {
            let ty = engine.ty(NodeId::pattern(binder.pattern), &ctx)?;
            let value = Binder {
                name: engine.ir.fresh("symbolic"),
                ty,
            };
            let (records, literals) = engine.pattern_inputs(binder.pattern, &ctx)?;
            let bindings = decision_tree::bindings(
                &engine.ir,
                &mut engine.types,
                ty,
                binder.pattern,
                &records,
            )?;
            for (name, bound) in &bindings {
                ctx.env.insert(name, Binding::Value(*bound));
            }
            params.push(value);
            patterns.push((binder, value, records, literals, bindings));
        }
        let (computation, condition, expect) = match proof.obligation {
            ProofObligation::Execution { body, expect } => (body, None, expect),
            ProofObligation::Returns {
                computation,
                postcondition,
            } => (computation, Some(postcondition), nash_ast::Expect::Pass),
        };
        let body = engine.expr(computation, &ctx)?;
        let mut postcondition = None;
        let mut checker = None;
        if let Some(condition) = condition {
            let result = match body.ty {
                Ty::Const(ConstTy::Int) => ReturnDomain::Int,
                Ty::Const(ConstTy::Bool) => ReturnDomain::Bool,
                Ty::Const(ConstTy::Bytes) => ReturnDomain::Bytes,
                Ty::Const(ConstTy::String) => ReturnDomain::String,
                Ty::Const(ConstTy::Unit) => ReturnDomain::Unit,
                Ty::Big(_) => ReturnDomain::Data,
                _ => return Err(Error::PartialResult),
            };
            let value = Binder {
                name: engine.ir.fresh("returned"),
                ty: body.ty,
            };
            let condition = engine.expr(condition, &ctx)?;
            let holds = engine.ir.app(
                condition,
                &[engine.ir.var(value.name, value.ty)],
                Ty::Const(&ConstTy::Bool),
            );
            checker = Some((holds, value));
            postcondition = Some(Postcondition {
                flat: Vec::new(),
                result,
            });
        }
        // Both programs bind the same symbolic inputs, but only the condition
        // receives a successfully returned value. Never guard the assertion body.
        let wrap = |engine: &mut Engine<'a, '_, '_>, mut body| -> Result<_, Error<'a>> {
            for (binder, value, records, literals, bindings) in patterns.iter().rev() {
                body = decision_tree::compile(
                    &engine.ir,
                    &mut engine.types,
                    value.ty,
                    engine.ir.var(value.name, value.ty),
                    &[MatchBranch {
                        pattern: binder.pattern,
                        bindings: bindings.clone(),
                        body,
                    }],
                    MatchInputs {
                        record_fields: records,
                        literal_tests: literals,
                    },
                    engine.ir.error(body.ty),
                )?;
            }
            Ok(body)
        };
        let body = wrap(&mut engine, body)?;
        let root = engine.ir.lam(&params, body);
        let checker_root = if let Some((body, value)) = checker {
            let body = wrap(&mut engine, body)?;
            let mut arguments = params.clone();
            arguments.push(value);
            Some(engine.ir.lam(&arguments, body))
        } else {
            None
        };
        let target = match version {
            nash_config::PlutusVersion::V1 => nash_plutus::machine::PlutusVersion::V1,
            nash_config::PlutusVersion::V2 => nash_plutus::machine::PlutusVersion::V2,
            nash_config::PlutusVersion::V3 => nash_plutus::machine::PlutusVersion::V3,
        };
        let core = engine.finish_root(root)?;
        let compiled = crate::program::assemble_core_for_version(arena, core, target)?;
        if let Some(root) = checker_root {
            let core = engine.finish_root(root)?;
            let compiled = crate::program::assemble_core_for_version(arena, core, target)?;
            postcondition.as_mut().unwrap().flat =
                flat::encode(compiled.program).map_err(|e| Error::Encoding(e.to_string()))?;
        }
        programs.push(ProofProgram {
            module: module.name.to_owned(),
            name: proof.name.value.to_owned(),
            expect,
            domains,
            flat: flat::encode(compiled.program).map_err(|e| Error::Encoding(e.to_string()))?,
            postcondition,
            plutus_version: version,
        });
    }
    Ok(programs)
}
