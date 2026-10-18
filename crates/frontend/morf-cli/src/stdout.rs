//! Standard output for every verb that prints. A reader that closes its
//! end early (`morf docs | head -3`) ends the process with status 0.

use std::fmt;
use std::io::{self, Write};

/// `println!` to stdout, ending cleanly when the reader has gone.
macro_rules! outln {
    () => { $crate::stdout::write(format_args!(""), true) };
    ($($arg:tt)*) => { $crate::stdout::write(format_args!($($arg)*), true) };
}

/// `print!` to stdout, flushed, ending cleanly when the reader has gone.
macro_rules! out {
    ($($arg:tt)*) => { $crate::stdout::write(format_args!($($arg)*), false) };
}

/// Writes `args` to stdout, then a newline when `newline`.
pub(crate) fn write(args: fmt::Arguments, newline: bool) {
    if let Err(error) = emit(&mut io::stdout().lock(), args, newline) {
        exit_for(&error);
    }
}

/// Writes `args` to `out` and flushes it.
pub(crate) fn emit(out: &mut impl Write, args: fmt::Arguments, newline: bool) -> io::Result<()> {
    out.write_fmt(args)?;
    if newline {
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Whether a failed write means the reader closed its end.
pub(crate) fn reader_gone(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::BrokenPipe
}

fn exit_for(error: &io::Error) -> ! {
    if reader_gone(error) {
        std::process::exit(0);
    }
    eprintln!("morf: could not write to standard output: {error}");
    std::process::exit(1);
}
