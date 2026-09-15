pub type Redeemer {
  @tag(9)
  Redeemer(Int)
}

validator custom {
  mint(redeemer: Redeemer, _policy: ByteArray, _transaction: Data) {
    trace @"custom"
    when redeemer is {
      Redeemer(value) -> value > 0
    }
  }
}
