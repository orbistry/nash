use aiken/builtin.{add_integer as add} as b

pub fn add_one(value: Int) -> Int {
  add(value, 1)
}

pub fn equal(left: ByteArray, right: ByteArray) -> Bool {
  b.equals_bytearray(left, right)
}
