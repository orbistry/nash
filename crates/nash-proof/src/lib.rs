//! Verify actual compiled UPLC using PlutusCoreBlaster and CardanoLedgerApiBlaster.
mod lean;
mod runner;
pub use lean::{lakefile, render};
pub use runner::{Config, Outcome, Status, export, run};

use nash_source::Expect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Int,
    Integer,
    Bool,
    Bytes,
    ByteString,
    String,
    Data,
    Spending(u8),
    Minting(u8),
}

impl Domain {
    /// Names are recognized only after resolving to the compiler-bundled Proof module.
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "int" => Self::Int,
            "integer" => Self::Integer,
            "bool" => Self::Bool,
            "bytes" => Self::Bytes,
            "byteString" => Self::ByteString,
            "string" => Self::String,
            "data" => Self::Data,
            "spendingV1" => Self::Spending(1),
            "spendingV2" => Self::Spending(2),
            "spendingV3" => Self::Spending(3),
            "mintingV1" => Self::Minting(1),
            "mintingV2" => Self::Minting(2),
            "mintingV3" => Self::Minting(3),
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub struct ProofProgram {
    pub module: String,
    pub name: String,
    pub expect: Expect,
    pub domains: Vec<Domain>,
    pub flat: Vec<u8>,
    pub postcondition: Option<Postcondition>,
    pub plutus_version: nash_config::PlutusVersion,
}

/// Constant representations that can cross the computation/postcondition boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnDomain {
    Int,
    Bool,
    Bytes,
    String,
    Unit,
    Data,
}

#[derive(Debug)]
pub struct Postcondition {
    pub flat: Vec<u8>,
    pub result: ReturnDomain,
}
