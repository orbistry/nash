---
cargo/nash-solve: patch
---

Stop reporting ambiguous or unresolved trait constraints on values passed to a definition whose body failed to type check. Their types are unknown because of that failure; the body error is still reported, and fixing it reveals any remaining ambiguity.
