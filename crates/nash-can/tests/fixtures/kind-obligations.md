# Historical kind checker non-termination cases

These sources are regression inputs for the replacement Haskell 98 checker.
Every fenced case now rejects with `KindInfinite` at declaration checking;
`tests/kinds.rs::self_application_cases` asserts that result. The timing and
mechanism below describe the deleted engine, not the current compiler.

Each block is a complete `src/Main.nash` for an application project
(`{"type":"application","sourceDirectories":["src"]}` in `nash.jsonc`).
With the retained-obligation memo fix (`fix(can): terminate retained constructor kind obligations`),
`cargo run -p nash-cli -- check <project>` does not return on any of them (killed at 8 s; the
first case was also run to 90 s, CPU-bound).

Found by generating 1,750 declarations of the form `type s 'f 'g 'a = S (<body>)` with bodies
over `'f`, `'g`, `'a`, `tag` and application depth <= 2, applied as `s s s`, `s s tag`, etc.
19 of 1,750 hang; the rest report a kind error. All 19 are in the second section.

## Historical mechanism

`s` retains obligations `Apply(kf, kf, r1)`, `Apply(r1, ka, r2)`, `Apply(kg, r2, r3)`.
Settling `Apply(kf, kf, r1)` at `kf := s` opens `s` with one argument, so the opened
instance's remaining parameters are fresh and never supplied. The instance replays
`Apply(s [s], ka', r2')` with `ka'` a fresh unbound variable. `same_kind` treats distinct
unbound variables as distinct, so the memo key is new at every level and the scheme is
reopened forever. Each level is the previous one up to renaming of an unbound variable.

## Minimal and supplied-argument variants

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f 'f 'a))
type w = W (s s s)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f 'f 'a))
type w = W (s s s tag)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f 'f 'a))
type w = W (s s tag tag)
```

Historical control: `type w = W (s tag s tag)` with the same `s` terminated
with `KindMismatch` in the old engine. Haskell 98 rejects `s` itself with
`KindInfinite`, independently of its uses.
Control: the arity-2 cousin `type s 'f 'a = S ('f 'f 'a)` terminates at both `s s` and `s s tag`.

## All 19 fuzz hits

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)))
type w = W (s s s)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)))
type w = W (s (s s) tag)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('g) ('f ('f)))
type w = W (s tag (s tag))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('g ('f)) ('f ('f) ('a)))
type w = W (s s (s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('g)) ('f ('f) ('a)))
type w = W (s s (s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('g) ('g)))
type w = W (s s s)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('a)) ('f ('f) ('a)))
type w = W (s s (s s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('a)) ('f ('f) ('a)))
type w = W (s (s s) (s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('g ('f) (tag)) ('f ('f) ('a)))
type w = W (s s tag)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('f)))
type w = W (s (s s) (s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('f ('f ('f)) ('g ('a) ('g)))
type w = W (s (s tag tag) s)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('f ('f ('g)) ('g ('a) ('g)))
type w = W (s (s tag tag) s)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('f) ('a)))
type w = W (s s (s s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('a) ('a)))
type w = W (s s (s s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('f) ('g)))
type w = W (s (s s) tag)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('g)))
type w = W (s (s s tag) (s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('g (tag) ('g)))
type w = W (s s (s s s))
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('g ('a) ('g)) ('f ('f) ('a)))
type w = W (s (s s s) tag)
```

```
module Main exposing (..)
type tag 'a = Tag
type s 'f 'g 'a = S ('g ('f ('f) ('a)) ('f ('f) ('g)))
type w = W (s (s s s) tag)
```
