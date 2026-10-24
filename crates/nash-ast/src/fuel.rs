//! The one step counter every compiler search draws from.
//!
//! Impl contexts need not shrink, so trait resolution, overlap checks and code
//! generation can loop on a program that is accepted syntactically. Instead of
//! a budget per search, the driver installs one `Fuel` around each module's
//! compile, and every search loop calls [`tick`]. A pass over an existing type
//! that cannot run away on its own pays with [`charge`], so the steps track
//! time even when the types grow. The command-line tools run `Unlimited`; the
//! language server runs `Limited` so an editor never freezes on a looping
//! program.
//!
//! The counter is thread-local so no signature carries it. That is sound only
//! while the installed section runs on one thread and never awaits; the driver
//! installs it inside its dedicated compiler thread. With nothing installed the
//! fuel is `Unlimited`.
use std::cell::Cell;

/// How many steps a compile may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fuel {
    Unlimited,
    Limited(u64),
}

/// The installed fuel ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfFuel;

thread_local! {
    static REMAINING: Cell<Fuel> = const { Cell::new(Fuel::Unlimited) };
}

/// Spend one step.
#[inline]
pub fn tick() -> Result<(), OutOfFuel> {
    REMAINING.with(|remaining| match remaining.get() {
        Fuel::Unlimited => Ok(()),
        Fuel::Limited(0) => Err(OutOfFuel),
        Fuel::Limited(steps) => {
            remaining.set(Fuel::Limited(steps - 1));
            Ok(())
        }
    })
}

/// Pay for `steps` of work that cannot run away on its own, such as one pass
/// over an existing type. Running out is not reported here: the next `tick`
/// fails, because an empty counter stays empty.
#[inline]
pub fn charge(steps: usize) {
    REMAINING.with(|remaining| {
        if let Fuel::Limited(left) = remaining.get() {
            remaining.set(Fuel::Limited(left.saturating_sub(steps as u64)));
        }
    });
}

/// The fuel left on this thread.
pub fn remaining() -> Fuel {
    REMAINING.with(Cell::get)
}

/// Run `work` with a fresh counter of `fuel` on this thread, then restore the
/// counter that was installed before, even if `work` panics.
pub fn run<T>(fuel: Fuel, work: impl FnOnce() -> T) -> T {
    struct Restore(Fuel);
    impl Drop for Restore {
        fn drop(&mut self) {
            REMAINING.with(|remaining| remaining.set(self.0));
        }
    }
    let _restore = Restore(REMAINING.with(|remaining| remaining.replace(fuel)));
    work()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limited_fuel_runs_out_and_the_previous_counter_returns() {
        assert_eq!(tick(), Ok(()));
        run(Fuel::Limited(2), || {
            assert_eq!(tick(), Ok(()));
            assert_eq!(tick(), Ok(()));
            assert_eq!(tick(), Err(OutOfFuel));
            assert_eq!(remaining(), Fuel::Limited(0));
        });
        run(Fuel::Limited(3), || {
            charge(5);
            assert_eq!(remaining(), Fuel::Limited(0));
            assert_eq!(tick(), Err(OutOfFuel));
        });
        assert_eq!(remaining(), Fuel::Unlimited);
    }
}
