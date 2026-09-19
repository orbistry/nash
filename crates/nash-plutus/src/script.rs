//! Ledger script compatibility and hashing.
//!
//! Compatibility is pinned to the van Rossem (protocol 11) baseline. Ledger
//! language versions are distinct from UPLC versions; all support UPLC 1.1.0.
//! Reference: IntersectMBO/plutus, PlutusLedgerApi/Common/Versions.hs,
//! `builtinsIntroducedIn`, batches 1–6 (PV11), and `plcVersionsIntroducedIn`.
//! Batch 7 remains unavailable.
use crate::{constant::Constant, machine::PlutusVersion, program::Program, term::Term, typ::Type};
use cryptoxide::{blake2b::Blake2b, digest::Digest};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error(
    "{feature} is unavailable for {version:?} at the van Rossem (protocol 11) compatibility baseline"
)]
pub struct TargetError {
    pub version: PlutusVersion,
    pub feature: String,
}

/// Cardano script hash: Blake2b-224 of the ledger language discriminator followed
/// by the single CBOR bytestring wrapping Flat UPLC (`flat::to_cbor`).
/// Do not pass the extra bytestring wrapper used by cardano-cli text envelopes.
pub fn script_hash(version: PlutusVersion, single_wrapped_cbor: &[u8]) -> [u8; 28] {
    let tag = match version {
        PlutusVersion::V1 => 1,
        PlutusVersion::V2 => 2,
        PlutusVersion::V3 => 3,
    };
    let mut hash = Blake2b::new(28);
    hash.input(&[tag]);
    hash.input(single_wrapped_cbor);
    let mut result = [0; 28];
    hash.result(&mut result);
    result
}

/// Validate every term and constant type against the explicit PV11 baseline.
/// This does not type-check UPLC or check builtin application saturation.
pub fn validate_program<V>(
    program: &Program<'_, V>,
    version: PlutusVersion,
) -> Result<(), TargetError> {
    if !(program.version.is_v1_0_0() || program.version.is_v1_1_0()) {
        return Err(error(version, "UPLC version (expected 1.0.0 or 1.1.0)"));
    }
    validate_term(program.term, version, program.version.is_v1_1_0())
}
fn error(version: PlutusVersion, feature: impl Into<String>) -> TargetError {
    TargetError {
        version,
        feature: feature.into(),
    }
}
fn validate_term<V>(
    term: &Term<'_, V>,
    version: PlutusVersion,
    uplc_110: bool,
) -> Result<(), TargetError> {
    match term {
        Term::Lambda { body, .. } | Term::Delay(body) | Term::Force(body) => {
            validate_term(body, version, uplc_110)?
        }
        Term::Apply { function, argument } => {
            validate_term(function, version, uplc_110)?;
            validate_term(argument, version, uplc_110)?;
        }
        Term::Constr { fields, .. } => {
            if !uplc_110 {
                return Err(error(version, "constr (requires UPLC 1.1.0)"));
            }
            for term in *fields {
                validate_term(term, version, uplc_110)?;
            }
        }
        Term::Case { constr, branches } => {
            if !uplc_110 {
                return Err(error(version, "case (requires UPLC 1.1.0)"));
            }
            validate_term(constr, version, uplc_110)?;
            for term in *branches {
                validate_term(term, version, uplc_110)?;
            }
        }
        Term::Builtin(fun) => {
            let tag = **fun as u8;
            // Flat tags 0..=100 are exactly upstream batches 1–6. Keep this
            // bound fixed: future runtime builtins must not become ledger-valid
            // merely because they were added to DefaultFunction.
            let allowed = tag <= 100;
            if !allowed {
                return Err(error(version, format!("builtin {fun:?}")));
            }
        }
        Term::Constant(constant) => validate_constant(constant, version)?,
        Term::Var(_) | Term::Error => {}
    }
    Ok(())
}
fn validate_type(typ: &Type<'_>, version: PlutusVersion) -> Result<(), TargetError> {
    match typ {
        Type::List(t) | Type::Array(t) => validate_type(t, version)?,
        Type::Pair(a, b) => {
            validate_type(a, version)?;
            validate_type(b, version)?;
        }
        // Nash cannot encode BLS constant types (including empty containers).
        Type::Bls12_381G1Element | Type::Bls12_381G2Element | Type::Bls12_381MlResult => {
            return Err(error(version, format!("constant type {typ:?}")));
        }
        _ => {}
    }
    Ok(())
}
fn validate_constant(constant: &Constant<'_>, version: PlutusVersion) -> Result<(), TargetError> {
    match constant {
        Constant::ProtoList(t, values) | Constant::ProtoArray(t, values) => {
            validate_type(t, version)?;
            for value in *values {
                validate_constant(value, version)?;
            }
        }
        Constant::ProtoPair(a, b, x, y) => {
            validate_type(a, version)?;
            validate_type(b, version)?;
            validate_constant(x, version)?;
            validate_constant(y, version)?;
        }
        Constant::Bls12_381G1Element(_)
        | Constant::Bls12_381G2Element(_)
        | Constant::Bls12_381MlResult(_) => {
            return Err(error(version, "runtime-only BLS constant"));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
