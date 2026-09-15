use aiken/builtin
use annotations
use rows.{Row}
fn add(left: Int, right: Int) -> Int { left + right }
pub fn operations(input: Int) -> Bool {
  let row = Row { left: 1, right: 2, extra: 3 }
  let changed = Row { ..row, right: input }
  let tuple = (input, True)
  let value = input |> add(2)
  trace @"values": tuple.1st, changed
  annotations.renamed(label: value) > 0 && (changed.right == input)?
}
pub fn inspect(raw: Data) -> Bool {
  if raw is value: Int { value > 0 } else { False }
}

pub fn curve_roundtrip() -> Bool {
  let point = #<Bls12_381, G1>"97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb"
  point == builtin.bls12_381_g1_uncompress(builtin.bls12_381_g1_compress(point))
}
