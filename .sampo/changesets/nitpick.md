---
cargo/nash-nitpick: minor
cargo/nash-driver: minor
cargo/nash-cli: patch
---

Add Maranget exhaustiveness and redundancy checking across declarations,
trait defaults, impl methods and nested expressions. Render missing-pattern
examples and handle trait-overloaded literals conservatively.
Reject modules with incomplete or redundant patterns after type solving,
before publishing interfaces or retaining solved modules for dependents.
