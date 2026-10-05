# Initial O2 silence trial

O2 removes source/compiler traces and Core/builtin trace message expressions, then runs the existing O1 pipeline. As explicitly requested, removed messages may fail or diverge. Returned values still evaluate. No broader application motion is included.

Measured with the explicit `silent-experiment` command in `tools/optimizer-perf`, using Plutus V3/PV11, UPLC 1.1.0 and the bundled cost model. Sizes are raw Flat bytes. Each O2 case also checks identical Flat output across three pipeline applications.

| Case | Mode | CPU | Memory | Bytes | Result | Logs |
| --- | --- | ---: | ---: | ---: | --- | --- |
| source | O1 verbose | 2,451,550 | 11,206 | 53 | `(con integer 42)` | 3 |
| source | O0 silent | 2,113,056 | 10,110 | 47 | `(con integer 42)` | 0 |
| source | O1 silent | 1,921,056 | 8,910 | 39 | `(con integer 42)` | 0 |
| source | O2 silent | 1,921,056 | 8,910 | 39 | `(con integer 42)` | 0 |
| builtin | O1 verbose | 2,643,550 | 12,406 | 57 | `(con integer 42)` | 3 |
| builtin | O0 silent | 2,787,550 | 13,306 | 68 | `(con integer 42)` | 3 |
| builtin | O1 silent | 2,643,550 | 12,406 | 57 | `(con integer 42)` | 3 |
| builtin | O2 silent | 1,921,056 | 8,910 | 39 | `(con integer 42)` | 0 |
| failure | O1 verbose | 100 | 100 | 14 | `error: ExplicitErrorTerm` | 0 |
| failure | O0 silent | 100 | 100 | 19 | `error: ExplicitErrorTerm` | 0 |
| failure | O1 silent | 100 | 100 | 14 | `error: ExplicitErrorTerm` | 0 |
| failure | O2 silent | 16,100 | 200 | 6 | `(con integer 42)` | 0 |

1. Source trace loop: O1 silent and O2 produce the same output. The existing silent source policy already omits these messages.
2. Aliased builtin trace loop: O1 silent still emits builtin logs. O2 removes them, exposing the same 39-byte loop as the source-silent case.
3. Failing aliased message: O0/O1 fail before producing a value. O2 intentionally returns 42. Its CPU increase is not a cost regression for equivalent execution; the outcome changes under the authorized O2 rule.

Sources: [SourceTrace](../../tools/optimizer-perf/fixtures/o2/SourceTrace.nash), [BuiltinTrace](../../tools/optimizer-perf/fixtures/o2/BuiltinTrace.nash), [FailingMessage](../../tools/optimizer-perf/fixtures/o2/FailingMessage.nash). [Raw report](../../tools/optimizer-perf/trials/o2-silence.json) includes source, Core, UPLC, logs, costs and revision metadata.

Regression tests additionally cover failing returned values, partial trace calls, builtin aliases, Core traces, traces inside lambdas/delays and a diverging message. The diverging O0 program is rendered without evaluation; O2 returns 42. Build/test configuration conflicts and silent test outcomes are checked through Rust APIs. CLI execution is left for manual testing.

Validation: all 3,896 workspace tests passed. After arranging source snapshots beside their source tests, all 580 codegen tests passed again. Strict Clippy passed for the repository and performance workspace, and all 199 existing O0/O1 performance cases matched their baseline.
