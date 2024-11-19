use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::style::Stylize;

static VERBOSE_OUTPUT: AtomicBool = AtomicBool::new(false);

pub fn enable_verbose_output() {
    VERBOSE_OUTPUT.store(true, Ordering::Release);
}

pub fn is_verbose_output_enabled() -> bool {
    VERBOSE_OUTPUT.load(Ordering::Acquire)
}

static COLOR_OUTPUT: AtomicBool = AtomicBool::new(false);

pub fn enable_color_output() {
    COLOR_OUTPUT.store(true, Ordering::Release);
}

pub fn is_color_output_enabled() -> bool {
    COLOR_OUTPUT.load(Ordering::Acquire)
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        eprintln!("{}: {}", $crate::log::Error, format_args!($($arg)*));
    };
}

pub use error;

#[macro_export]
macro_rules! optimized {
    ($($arg:tt)*) => {
        println!("{} {}", $crate::log::Title::OPTM, format_args!($($arg)*));
    };
}

pub use optimized;

#[macro_export]
macro_rules! reduced {
    ($($arg:tt)*) => {
        println!("{} {}", $crate::log::Title::REDU, format_args!($($arg)*));
    };
}

pub use reduced;

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        if $crate::log::is_verbose_output_enabled() {
            println!("{} {}", $crate::log::Title::INFO, format_args!($($arg)*));
        }
    };
}

pub use info;

pub struct Error;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = "error";
        if is_color_output_enabled() {
            write!(f, "{}", title.dark_red().bold())
        } else {
            write!(f, "{title}")
        }
    }
}

pub struct Title(&'static str);

impl Title {
    pub const OPTM: Self = Self("   Optimized");
    pub const INFO: Self = Self("        Info");
    pub const REDU: Self = Self("     Reduced");
}

impl fmt::Display for Title {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = self.0;
        if is_color_output_enabled() {
            write!(f, "{}", title.dark_blue().bold())
        } else {
            write!(f, "{title}")
        }
    }
}
