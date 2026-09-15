use direct.{boxed}
use shapes.{Box}
/// A transitive package type retains its qualified wire layout.
validator boxed_value {
  mint(value: Box<Int>, _policy: ByteArray, _transaction: Data) {
    value == boxed(7)
  }
}
