pub use nash_config::PlutusVersion;
pub use nash_plutus::machine::ExBudget;
use nash_region::Region;
pub use nash_source::{Budget, Expect};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct TestProgram {
    pub module: String,
    pub name: String,
    pub expect: Expect,
    pub budget: Option<Budget>,
    pub region: Region,
    pub programs: Programs,
    pub asserts: Vec<AssertSite>,
    pub binder_texts: Vec<String>,
    pub plutus_version: PlutusVersion,
    pub source: String,
    pub source_path: PathBuf,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Programs {
    Unit { run: Vec<u8> },
    Prop { prepare: Vec<u8> },
}
#[derive(Debug, Clone, PartialEq)]
pub struct AssertSite {
    pub id: u32,
    pub region: Region,
    pub captures: Vec<Capture>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub index: u32,
    pub region: Region,
    pub shown: bool,
}
#[derive(Debug, Clone)]
pub struct Config {
    pub seed: u32,
    pub max_success: usize,
    pub jobs: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            seed: 0,
            max_success: 100,
            jobs: 1,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub test: TestProgram,
    pub status: Status,
    pub budget: ExBudget,
    pub iterations: usize,
    pub labels: BTreeMap<String, usize>,
    pub traces: Vec<String>,
    pub assert: Option<AssertReport>,
    pub counterexample: Option<Vec<(String, String)>>,
    pub expected_failure: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Pass,
    Fail(Failure),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Body,
    BudgetExceeded { limit: Budget, used: ExBudget },
    Generator { message: String },
    NoCounterexample,
    InvalidProgram { message: String },
}
pub type FailureKind = Failure;
#[derive(Debug, Clone, PartialEq)]
pub struct AssertReport {
    pub site: AssertSite,
    pub values: Vec<(u32, String)>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Labels,
    Tests,
}
