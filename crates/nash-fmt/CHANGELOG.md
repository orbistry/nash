# nash-fmt

## 0.2.1 — 2026-10-10

### Patch changes

- Updated dependencies: nash-parse@0.10.0, nash-report@0.7.0

## 0.2.0 — 2026-09-27

### Minor changes

- [fd56cc68](https://github.com/orbistry/nash/commit/fd56cc68eced010fb5e16d4c8811865249fca40d) Add an AST-based Nash source formatter with 80-column layout, comment preservation,
  and source snapshot tests. Expose `nash format` (`fmt`), in-place and stdin
  formatting, and contextual `--check` diffs through the existing report style.
  Correct whitespace handling after message keywords and before constructor docs.
  Allow aligned explicit continuations inside `do` without merging statements. — Thanks @MicroProofs!

### Patch changes

- [bc3d00cd](https://github.com/orbistry/nash/commit/bc3d00cd223650e60c05ac843bd8110c9df9a38d) Wrap constrained type signatures before the fat arrow, keeping the function type together when it fits. Start multiline exposing lists on the next line. — Thanks @MicroProofs!
- [27d64a27](https://github.com/orbistry/nash/commit/27d64a27f81c0c911bedd831f4a69ad8004b20de) Keep short definitions and arithmetic compact, and place do blocks after assignment and bind operators without extra indentation. — Thanks @MicroProofs!
- Updated dependencies: nash-parse@0.9.0, nash-report@0.6.0, nash-source@0.10.0

