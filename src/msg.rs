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
    };
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

/// like `eprintln!` but then calls exit(first argument).
#[macro_export]
macro_rules! die {
    ($code:expr, $($arg:expr),+) => {
        {
            eprintln!($($arg),*);
            use std::process::exit;
            exit($code)
        }
    }
}
