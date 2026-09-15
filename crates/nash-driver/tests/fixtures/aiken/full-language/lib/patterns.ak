use rows.{Row}

fn supply(value: Int, next: fn(Row, Pair<Int, Int>) -> Int) -> Int {
  trace @"supply"
  next(Row { left: value, right: 2, extra: 3 }, Pair(4, 5))
}

fn read_row(Row.Row { left: renamed, .. }: Row) -> Int {
  renamed
}

pub fn patterns_once() -> Int {
  let Row.Row { left: x, .. }, Pair(y, _) <- supply(7)
  x + y + read_row(Row { left: 1, right: 2, extra: 3 })
}

pub fn expect_backpass_refutes() -> Int {
  expect Row.Row { left: 0, .. }, _ <- supply(7)
  1
}

pub fn terminal_expect() -> Void {
  expect Pair(1, _) = Pair(1, 2)
}
