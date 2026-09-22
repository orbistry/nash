# nash-test

## 0.3.0 — 2026-09-22

### Minor changes

- [1d057156](https://github.com/orbistry/nash/commit/1d05715685c5561c798e6bab3df4a7c8788d056a) Use little prng state with native bytes, integers, and lists for property testing.
  Pass native constructor terms between generators and the runner, removing Data
  wrapping from seeded draws and replay. — Thanks @MicroProofs!
- [dccae3df](https://github.com/orbistry/nash/commit/dccae3df5332b8d5628abb785b8b6b361d0974c1) Use a function alias for generators, returning value and next PRNG state per draw.
  Remove the redundant replay count. Prepare properties once and retain their
  state, body function, and deferred display function without rerunning generators.
  Preserve function aliases in typed definitions and captured variables inside
  native constructor and case nodes when returning CEK closures. — Thanks @MicroProofs!

### Patch changes

- [3f9188b0](https://github.com/orbistry/nash/commit/3f9188b01b8bdc18363335274c20c150197d1173) Name property generation Prop throughout the compiler, Base, diagnostics and
  examples. Use direct draws returning values and PRNG states. Remove the identity
  run helper, generator map/bind functions and generator trait instances; ordinary
  Option do notation remains available for explicit state-threaded draws. — Thanks @MicroProofs!
- Updated dependencies: nash-plutus@0.3.1, nash-source@0.8.1

## 0.2.0 — 2026-09-20

### Minor changes

- [0c345bb3](https://github.com/orbistry/nash/commit/0c345bb3a5cfb0391c039efe743313953fd3a9fd) Add deterministic CEK test execution, seeded property generation, exact replay
  caching and choice-sequence shrinking. Report independent budget and generator
  failures, labels, traces, and source-aligned assertions in terminal and JSON. — Thanks @MicroProofs!

### Patch changes

- Updated dependencies: nash-config@0.5.0, nash-plutus@0.3.0, nash-source@0.8.0

