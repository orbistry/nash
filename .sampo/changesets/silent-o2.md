---
cargo/nash-config: minor
cargo/nash-codegen: minor
cargo/nash-driver: minor
cargo/nash-cli: minor
---

Add explicit O2 compilation for build and test. O2 requires silent settings, removes user and compiler traces, and discards trace message computations even when they fail or diverge before applying the O1 optimizer. Reject compact, verbose and compiler tracing with O2. Keep O1 as the default.
