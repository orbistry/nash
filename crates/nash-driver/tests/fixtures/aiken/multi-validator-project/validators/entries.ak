//// Two independent entry points; tool declarations are checked, never run.
use env
use config
/// Parameterized minting entry.
validator alpha(threshold minimum: Int) {
  /// Check the mint redeemer against the public threshold.
  mint(redeemer: Int, _policy: ByteArray, _transaction: Data) {
    redeemer >= minimum
  }
}
/// Independent raw-context entry that may call another validator's handler.
validator beta {
  else(raw: Data) {
    expect n: Int = raw
    let transaction: Data = 0
    n == env.target && n == config.target && alpha.mint(10, 12, #"", transaction)
  }
}
fn sample(size: Int) -> Fuzzer<Int> {
  fn(prng: PRNG) { Some((prng, size)) }
}
test plain() { False }
bench cost(n via sample) { n + 1 }
