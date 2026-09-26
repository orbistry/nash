# nash-test

## 0.4.1 — 2026-09-26

### Patch changes

- Updated dependencies: nash-plutus@0.3.2

## 0.4.0 — 2026-09-25

### Minor changes

- [04cb160c](https://github.com/orbistry/nash/commit/04cb160ceb213899c13b13e5e7b9b3d4e8b27a8c) Use consumed-draw feedback to repair unsuccessful property reductions. Reconstruct boundary proposals in Nash and validate them with strict replay. Add adaptive draw deletion, joint numeric reduction, whole-draw reordering, and coordinated decrement/deletion passes. — Thanks @MicroProofs!
- [feeb1edb](https://github.com/orbistry/nash/commit/feeb1edbecdb0b402023d99d5867e671eb57b336) Use nested Choice/Group traces for property generation, strict replay, and reduction. Compose generator functions through ordinary Functor, Applicative, and Monad instances in Nash. Preserve reduced replay trees in runner outcomes. Retain concrete little type layouts when specializing generic trait helpers. — Thanks @MicroProofs!

### Patch changes

- [940d8d58](https://github.com/orbistry/nash/commit/940d8d58e5df0dcb08e6dd14a1b9761c2329f4c5) Evaluate rebuilt property candidates without regenerating them through replay, cache their complete outcomes, and express list element grouping through Monad.bind. — Thanks @MicroProofs!
- [661376a3](https://github.com/orbistry/nash/commit/661376a3f9d003420b3868c322c09cd277b2ed51) Normalize recorded property traces in Rust instead of reversing groups during Nash execution. Remove the unused Test.assertFailed helper. — Thanks @MicroProofs!
- Updated dependencies: nash-source@0.9.0

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

