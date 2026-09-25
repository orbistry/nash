---
cargo/nash-driver: minor
---

Choose small-biased bit widths before generating Prop.intAtLeast offsets and wide Prop.int magnitudes. Preserve arbitrary precision while replacing the zero-or-huge distribution with useful small nonzero values. Use exact expModInteger powers within each eight-bit band, with CEK budget snapshots comparing CPU and memory. Expose Int.pow2 and use its exact modular fast path in Int.pow for base two.
