---
cargo/nash-solve: patch
---

Record definition identities and freeze scheme quantifiers at generalization,
so local schemes do not quantify captured variables when an outer definition
later generalizes them. Build exported annotations from the recorded schemes.
