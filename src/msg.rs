use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

pub fn set_quiet(x: bool) {
    QUIET.store(x, Ordering::Relaxed);
}

/// only executes its argument if quiet mode is disabled.
#[macro_export]
macro_rules! noisy {
    ($x:block) => {
        if !(quiet()) {
           $x
        }
    }
}

/// like `eprint!` but only if `-q` has not been specified.
#[macro_export]
macro_rules! msg {
    ($($arg:expr),+) => {
        noisy!({
            eprint!($($arg),*);
        })
    }
}

pub struct Progress {
    increment: usize,
    target: usize,
}

impl Progress {
    pub fn new(max: usize) -> Progress {
        let increment = (max / 100) + 1;
        Progress {
            increment,
            target: increment,
        }
    }

    pub fn print(&mut self, current: usize) {
        if current > self.target {
            self.target += self.increment;
            eprint!("{}%\r", current / self.increment);
        }
    }
}
