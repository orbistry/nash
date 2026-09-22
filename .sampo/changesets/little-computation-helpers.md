---
cargo/nash-driver: minor
cargo/nash-can: patch
---

Keep computation trait instances on little representations. Add mixed Big/little
integer arithmetic and ordering helpers, byte append/ordering helpers, Map.union,
and Option.apply; return little outer representations without converting payloads.
Remove Result/result, its implicit import, and Option.toResult from bundled Base.
