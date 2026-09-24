//! Terminals for morf: a program on a pseudo-terminal, and the screen it
//! draws.
//!
//! Two halves that meet only in whoever drives them. [`Pty`] starts a child
//! on a pseudo-terminal and hands its master side to morf's I/O reactor, so
//! output arrives as reactor events like any child's. [`Emulator`] is the VT
//! emulator (`alacritty_terminal`) those bytes are fed to, and what turns its
//! grid into the renderer's [`morf_scene::TerminalScreen`]. [`input`] is the
//! other direction: keys, the pointer and pastes, as the bytes a terminal
//! program reads.
//!
//! Nothing here knows about Lua or a surface; `morf-lua` puts the two halves
//! behind `ui.Terminal`.

mod emulator;
pub mod input;
mod pty;

pub use emulator::{Emulator, MAX_SCROLLBACK, Palette, ScreenStyle, TerminalEvent};
pub use input::{Modifiers, MouseAction, MouseButton, MouseModes};
pub use pty::{Pty, PtyOptions, PtySize};

#[cfg(test)]
mod tests;
