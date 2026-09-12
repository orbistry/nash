use std::cell::RefCell;

/// Explicit Nash inputs used by one snapshot test, in evaluation order.
#[derive(Default)]
pub struct SnapshotInputs(RefCell<Vec<String>>);

impl SnapshotInputs {
    pub fn record<'a>(&self, source: &'a str) -> &'a str {
        let mut inputs = self.0.borrow_mut();
        if !inputs.iter().any(|input| input == source) {
            inputs.push(source.to_owned());
        }
        source
    }

    pub fn description(&self) -> String {
        self.0.borrow().join("\n\n")
    }
}
