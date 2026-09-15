//! Arena-backed source metadata retained until the driver resolves public types.
use nash_region::{Located, Region};
use nash_source::Type;

#[derive(Clone, Copy, Debug)]
pub struct SourceBoundaryBinding<'a> {
    pub name: &'a str,
    pub label: &'a str,
    pub position: usize,
    pub annotation: Option<&'a Located<Type<'a>>>,
    pub region: Region,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceHandler<'a> {
    pub purpose: &'a str,
    pub function: &'a str,
    pub docs: Option<&'a str>,
    pub arguments: &'a [SourceBoundaryBinding<'a>],
    pub region: Region,
}

#[derive(Clone, Copy, Debug)]
pub struct SourceEntryPoint<'a> {
    pub name: &'a str,
    pub function: &'a str,
    pub docs: Option<&'a str>,
    pub parameters: &'a [SourceBoundaryBinding<'a>],
    pub handlers: &'a [SourceHandler<'a>],
    pub region: Region,
}
