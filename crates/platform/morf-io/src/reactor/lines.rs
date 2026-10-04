//! Cutting a byte stream into bounded lines.

/// Cuts a byte stream into lines of at most `max` bytes.
///
/// A line longer than that is delivered cut at `max`, and the rest of it,
/// up to its newline, is dropped. The newline itself is never part of a line.
#[derive(Debug)]
pub struct LineSplitter {
    buffer: Vec<u8>,
    max: usize,
    skipping: bool,
}

impl LineSplitter {
    pub fn new(max: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max: max.max(1),
            skipping: false,
        }
    }

    pub fn push(&mut self, mut data: &[u8], mut emit: impl FnMut(Vec<u8>)) {
        while let Some(newline) = data.iter().position(|&byte| byte == b'\n') {
            if self.skipping {
                self.skipping = false;
            } else {
                let room = self.max - self.buffer.len();
                self.buffer.extend_from_slice(&data[..newline.min(room)]);
                emit(std::mem::take(&mut self.buffer));
            }
            data = &data[newline + 1..];
        }
        if self.skipping || data.is_empty() {
            return;
        }
        let room = self.max - self.buffer.len();
        if data.len() > room {
            self.buffer.extend_from_slice(&data[..room]);
            emit(std::mem::take(&mut self.buffer));
            self.skipping = true;
        } else {
            self.buffer.extend_from_slice(data);
        }
    }

    /// What is left at the end of the stream, as a last line.
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        self.skipping = false;
        (!self.buffer.is_empty()).then(|| std::mem::take(&mut self.buffer))
    }
}
