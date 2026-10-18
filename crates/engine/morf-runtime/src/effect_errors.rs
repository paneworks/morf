//! Errors effects raised, counted per effect: one that fails on every frame
//! is logged once, then again with its count at 10, 100, 1000, ... repeats.

use std::collections::HashMap;

#[derive(Default)]
pub struct EffectErrors {
    /// Each effect's last message and how many times in a row it was raised.
    seen: HashMap<String, (String, u64)>,
    total: u64,
    /// The errors of the last flush that raised any, as one message.
    last: Option<String>,
}

impl EffectErrors {
    /// Counts one flush's errors. Returns the lines worth logging.
    pub fn note(&mut self, errors: &[(String, String)]) -> Vec<String> {
        if errors.is_empty() {
            return Vec::new();
        }
        self.last = Some(
            errors
                .iter()
                .map(|(effect, message)| format!("{effect}: {message}"))
                .collect::<Vec<_>>()
                .join("; "),
        );
        let mut lines = Vec::new();
        for (effect, message) in errors {
            self.total += 1;
            let entry = self.seen.entry(effect.clone()).or_default();
            if entry.0 != *message {
                *entry = (message.clone(), 1);
                lines.push(format!("{effect}: {message}"));
                continue;
            }
            entry.1 += 1;
            if entry.1 == 10u64.pow(entry.1.ilog10()) {
                lines.push(format!("{effect}: {message} (x{})", entry.1));
            }
        }
        lines
    }

    /// How many effect errors there have been, repeats included.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Takes the message of the last flush that raised errors.
    pub fn take_last(&mut self) -> Option<String> {
        self.last.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeated_error_is_logged_with_its_count() {
        let mut errors = EffectErrors::default();
        let failing = [("label.text".to_owned(), "boom".to_owned())];
        let logged = (0..100)
            .flat_map(|_| errors.note(&failing))
            .collect::<Vec<_>>();
        assert_eq!(
            logged,
            [
                "label.text: boom",
                "label.text: boom (x10)",
                "label.text: boom (x100)"
            ]
        );
        assert_eq!(errors.total(), 100);
        let changed = [("label.text".to_owned(), "other".to_owned())];
        assert_eq!(errors.note(&changed), ["label.text: other"]);
        assert_eq!(errors.take_last().as_deref(), Some("label.text: other"));
    }
}
