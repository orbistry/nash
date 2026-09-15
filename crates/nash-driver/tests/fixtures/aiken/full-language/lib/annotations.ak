pub fn partial(value: a, _other) -> a {
  value
}

pub fn holes(left: _, right: _) -> (_, _) {
  (left, right)
}

pub fn renamed(label internal: Int) -> Int {
  internal
}

pub type Box {
  Box { value: Int }
}

pub fn destructured(Box { value }: Box, value: Int) -> Int {
  value
}

fn ints(prng: PRNG) -> Option<(PRNG, Int)> {
  Some((prng, 1))
}

fn sized(_size: Int) -> Fuzzer<Int> {
  ints
}

test boolean_result() {
  True
}

test void_result() {
  Void
}

test generated(value via ints) {
  value == 1
}

bench sampled(value via sized) {
  value + 1
}

const monomorphic_identity = fn(value: Int) -> Int { value }

pub fn use_annotations() -> Bool {
  let (first, second) = holes(1, True)
  partial(first, second) == renamed(label: monomorphic_identity(1))
}

pub fn lambda_keeps_int() -> Int {
  let action = fn() -> Data { 1 }
  action()
}

pub fn lexical_annotation_scope() {
  let value: a = { fail }
  let action: fn(a) -> a = fn(item) { item }
  action(value)
}

pub type Tagged<marker> = Int

pub fn phantom(value: Tagged<Int>) -> Int {
  value
}
