//! The compiler stack.
//!
//! Compiler passes recurse over source structure, as Elm's do. Elm runs on a
//! stack that grows on the heap; Rust fixes the stack size when a thread
//! starts. Every thread that runs compiler work therefore reserves
//! `COMPILER_STACK` bytes. The reserve is address space: the system commits a
//! page only when the thread touches it. Input that needs more ends the process
//! with a stack overflow.

pub const COMPILER_STACK: usize = 128 * 1024 * 1024;

/// Run compiler work on a new thread with the compiler stack.
pub fn run<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("nash-compiler".into())
            .stack_size(COMPILER_STACK)
            .spawn_scoped(scope, work)
            .expect("cannot start the compiler thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Use at least 1 KiB of stack for each level.
    fn descend(levels: usize) -> usize {
        let frame = std::hint::black_box([1u8; 1024]);
        if levels == 0 {
            0
        } else {
            descend(levels - 1) + usize::from(frame[0])
        }
    }

    #[test]
    fn run_gives_the_compiler_stack() {
        // At least 32 MiB of frames: more than a default thread or main stack.
        assert_eq!(run(|| descend(32 * 1024)), 32 * 1024);
    }

    #[test]
    #[should_panic(expected = "from compiler work")]
    fn run_resumes_a_panic_from_the_work() {
        run(|| panic!("from compiler work"));
    }
}
