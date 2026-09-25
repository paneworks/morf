//! A terminal nobody types in, listening for colours.
//!
//! Colour tools (pywal, wallust, lule, ...) re-theme every open terminal by
//! writing escape sequences to each `/dev/pts/*`: OSC 4 for the 256 palette
//! colours, OSC 10/11/12 for the foreground, background and cursor. A
//! [`PaletteTty`] is a pseudo-terminal of the engine's own whose slave side
//! stays open, so it is one of those `/dev/pts/*` and hears the sequences
//! like any terminal; a thread reads its master side, picks the colours out
//! and hands them to the loop. The same parser reads a file of the same
//! sequences (pywal's `~/.cache/wal/sequences`), for a tool that also keeps
//! one.

use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::mpsc;

use alacritty_terminal::vte::ansi::{Handler, Processor, Rgb};
use rustix::pty::{OpenptFlags, grantpt, openpt, unlockpt};

/// Where the dynamic colours are numbered, after the 256 of the palette.
pub const FOREGROUND: usize = 256;
pub const BACKGROUND: usize = 257;
pub const CURSOR: usize = 258;

/// One colour set by a sequence: its index (0..=255, or [`FOREGROUND`],
/// [`BACKGROUND`], [`CURSOR`]) and `Some([r, g, b])`, or `None` for a reset.
pub type ColorChange = (usize, Option<[u8; 3]>);

#[derive(Default)]
struct Collect {
    changes: Vec<ColorChange>,
}

impl Handler for Collect {
    fn set_color(&mut self, index: usize, color: Rgb) {
        self.changes
            .push((index, Some([color.r, color.g, color.b])));
    }

    fn reset_color(&mut self, index: usize) {
        self.changes.push((index, None));
    }
}

/// The colours a run of terminal output sets, in order; everything else in
/// it (text, cursor movement, other sequences) is ignored.
pub fn parse_color_sequences(bytes: &[u8]) -> Vec<ColorChange> {
    let mut parser: Processor = Processor::new();
    let mut collect = Collect::default();
    parser.advance(&mut collect, bytes);
    collect.changes
}

/// A pseudo-terminal listening for colour sequences; dropped, it goes.
pub struct PaletteTty {
    path: PathBuf,
    changes: mpsc::Receiver<Vec<ColorChange>>,
    // The slave side, held so the terminal exists (and is written to) for as
    // long as this does; closing it ends the reader thread.
    _slave: OwnedFd,
}

impl PaletteTty {
    /// Opens the terminal and starts listening. Each burst of colours
    /// written to it arrives through [`PaletteTty::take`], and the loop is
    /// woken (`morf_io::wake_all`) when one does.
    pub fn open() -> std::io::Result<Self> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let path = PathBuf::from(
            rustix::pty::ptsname(&master, Vec::new())?
                .to_string_lossy()
                .into_owned(),
        );
        let slave = crate::pty::open_slave(&master)?;
        let (sender, changes) = mpsc::channel();
        std::thread::Builder::new()
            .name("palette-tty".to_owned())
            .spawn(move || {
                let _wake = morf_io::WakeOnDrop;
                let mut master = std::fs::File::from(master);
                let mut parser: Processor = Processor::new();
                let mut buffer = [0u8; 8192];
                // A read fails (EIO) once the slave side is closed: when the
                // listener is dropped.
                while let Ok(read) = master.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    let mut collect = Collect::default();
                    parser.advance(&mut collect, &buffer[..read]);
                    if !collect.changes.is_empty() {
                        if sender.send(collect.changes).is_err() {
                            break;
                        }
                        morf_io::wake_all();
                    }
                }
            })?;
        Ok(Self {
            path,
            changes,
            _slave: slave,
        })
    }

    /// The terminal's device, `/dev/pts/N`.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Every colour heard since the last call, in order.
    pub fn take(&self) -> Vec<ColorChange> {
        let mut out = Vec::new();
        while let Ok(batch) = self.changes.try_recv() {
            out.extend(batch);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn the_sequences_a_colour_tool_writes_are_read() {
        // What pywal's `sequences` file holds, trimmed: palette entries,
        // then foreground, background and cursor, and text around them.
        let text = "\x1b]4;0;#10110c\x1b\\\x1b]4;1;#c7cf9c\x07hello\x1b]10;#f8f9f3\x07\x1b]11;#10110c\x07\x1b]12;#c7cf9c\x07\x1b]104;3\x07";
        let changes = parse_color_sequences(text.as_bytes());
        assert_eq!(
            changes,
            vec![
                (0, Some([0x10, 0x11, 0x0c])),
                (1, Some([0xc7, 0xcf, 0x9c])),
                (FOREGROUND, Some([0xf8, 0xf9, 0xf3])),
                (BACKGROUND, Some([0x10, 0x11, 0x0c])),
                (CURSOR, Some([0xc7, 0xcf, 0x9c])),
                (3, None),
            ]
        );
    }

    #[test]
    fn a_colour_written_to_the_terminal_is_heard() {
        let tty = PaletteTty::open().unwrap();
        assert!(tty.path().starts_with("/dev/pts"));
        let mut device = std::fs::OpenOptions::new()
            .write(true)
            .open(tty.path())
            .unwrap();
        device
            .write_all(b"\x1b]4;1;rgb:c7/cf/9c\x07\x1b]11;#10110c\x07")
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut heard = Vec::new();
        while heard.len() < 2 && std::time::Instant::now() < deadline {
            heard.extend(tty.take());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            heard,
            vec![
                (1, Some([0xc7, 0xcf, 0x9c])),
                (BACKGROUND, Some([0x10, 0x11, 0x0c]))
            ]
        );
    }
}
