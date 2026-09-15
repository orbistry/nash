validator positive {
  mint(redeemer: Int, _policy: ByteArray, _transaction: Data) {
    trace @"mint"
    redeemer > 0
  }

  else(_) {
    False
  }
}
