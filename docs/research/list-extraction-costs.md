# List extraction costs

These measurements compare CEK execution budgets using the captured mainnet
Plutus V3 parameters for epoch 656 (protocol 11.0). They are not wall-clock
benchmarks. The completed experiment and its fixtures were removed; the findings
remain here to explain the extraction policy.

Parameter source: [Koios epoch 656 protocol parameters](https://api.koios.rest/api/v1/epoch_params?epoch_no=eq.656&select=epoch_no,protocol_major,protocol_minor,cost_models).

Each strategy receives the same integer list through a strict binding. Multiple
field reads return the sum of the selected integers. Tail-only and head-plus-tail
cases return the extracted values. Every result is checked against the expected
value. Inputs of length 8 and 64 produce identical budgets for these operations.
Core passes through normal codegen lowering; construction, printing, and host
allocation are outside the measured budget. No optimizer pass is benchmarked.

## Results

Each cell is **CPU / memory**. Indices start at zero. Shared builtin traversal
reuses earlier tails; it does not restart every read from the original list.

| Operation | Shared head/tail | dropList + head | Repeated case | dropList + case |
| --- | ---: | ---: | ---: | ---: |
| Read index 0 | 195250 / 832 | 195250 / 832 | 128100 / 900 | 128100 / 900 |
| Read index 1 | 324913 / 1164 | 324913 / 1164 | 192100 / 1300 | 192100 / 1300 |
| Read index 2 | 454576 / 1496 | 395875 / 1336 | 256100 / 1700 | 328725 / 1404 |
| Read index 3 | 584239 / 1828 | 397832 / 1336 | 320100 / 2100 | 330682 / 1404 |
| Read index 4 | 713902 / 2160 | 399789 / 1336 | 384100 / 2500 | 332639 / 1404 |
| Read index 5 | 843565 / 2492 | 401746 / 1336 | 448100 / 2900 | 334596 / 1404 |
| Read indices 0, 1 | 621271 / 1898 | 621271 / 1898 | 357308 / 1702 | 357308 / 1702 |
| Read indices 0, 1, 2, 3 | 1569313 / 4630 | 1569313 / 4630 | 815724 / 3306 | 815724 / 3306 |
| Read indices 0, 3 | 880597 / 2562 | 694190 / 2070 | 485308 / 2502 | 557933 / 2206 |
| Read indices 2, 5, 6 | 1661944 / 4892 | 1416836 / 4240 | 842516 / 4104 | 987766 / 3512 |

| Operation | Builtins CPU / memory | Case CPU / memory |
| --- | ---: | ---: |
| Tail alone | 193763 / 832 | 128100 / 900 |
| Head and tail together | 356913 / 1364 | 160100 / 1100 |

The drop strategies use dropList when the distance from their current cursor is
at least two. A case supplies both head and tail, so its next cursor is already
one element further along than a headList cursor.

## Interpretation

Prefer memory when CPU costs are close, then CPU. Compiler-generated extraction
uses shared native case bindings, with dropList for remaining gaps of two or
more. This selects the lower-memory skip strategy, while adjacent case reads
win both measures. Explicit source builtin calls remain available.

- Shared case wins both CPU and memory for adjacent reads and head-plus-tail.
- A single head or tail builtin uses 68 less memory, but case uses about 34% less
  CPU. This is a tradeoff, not a universal win for either operation.
- At index 3, dropList plus case saves 696 memory for about 3% more CPU than
  repeated case. This is a clear fit for the memory preference.
- At index 2, dropping saves 296 memory but costs about 28% more CPU than repeated
  case. This is a larger tradeoff.
- From index 4 in these examples, dropList plus case wins both over repeated case.
- After dropping, headList saves 68 memory versus case but costs about 20% more
  CPU. The single-head exception therefore remains relevant.

The experiment covered successful, in-bounds extraction from integer lists. It
did not establish equivalent failure behavior on empty or malformed inputs, or
measure every element representation and enclosing program.
