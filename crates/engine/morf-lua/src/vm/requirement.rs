//! Which engine a shell or a theme asks for: comparators joined by commas,
//! `>=0.3`, `>=0.3.0, <0.5`, `=0.3`. A version on its own is a minimum. A
//! missing part counts as zero, except after `=`, which matches the parts
//! given: `=0.3` is any 0.3.

/// This engine's own version.
pub(crate) const ENGINE: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    AtLeast,
    Above,
    AtMost,
    Below,
    Exactly,
}

/// A version as written: one to three numbers.
pub(crate) fn parse_version(text: &str) -> Result<Vec<u64>, String> {
    let parts = text
        .trim()
        .split('.')
        .map(|part| part.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("`{text}` is not a version"))?;
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!("`{text}` is not a version"));
    }
    Ok(parts)
}

fn padded(parts: &[u64]) -> [u64; 3] {
    let mut full = [0; 3];
    full[..parts.len()].copy_from_slice(parts);
    full
}

fn comparator(text: &str) -> Result<(Op, Vec<u64>), String> {
    let text = text.trim();
    let (op, rest) = [
        (">=", Op::AtLeast),
        ("<=", Op::AtMost),
        (">", Op::Above),
        ("<", Op::Below),
        ("=", Op::Exactly),
    ]
    .into_iter()
    .find_map(|(sign, op)| text.strip_prefix(sign).map(|rest| (op, rest)))
    .unwrap_or((Op::AtLeast, text));
    Ok((op, parse_version(rest)?))
}

/// Whether `version` meets every comparator in `requirement`.
pub(crate) fn satisfies(requirement: &str, version: &str) -> Result<bool, String> {
    let have = parse_version(version)?;
    let full = padded(&have);
    let mut met = true;
    for part in requirement.split(',') {
        let (op, want) = comparator(part)?;
        met &= match op {
            Op::AtLeast => full >= padded(&want),
            Op::Above => full > padded(&want),
            Op::AtMost => full <= padded(&want),
            Op::Below => full < padded(&want),
            Op::Exactly => full[..want.len()] == want[..],
        };
    }
    Ok(met)
}

/// Whether `version` is past the release line `tested` names: `0.3` is
/// passed by 0.4.0 and not by 0.3.9.
pub(crate) fn beyond(tested: &str, version: &str) -> Result<bool, String> {
    let tested = parse_version(tested)?;
    let have = padded(&parse_version(version)?);
    Ok(have[..tested.len()] > tested[..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_version_is_a_minimum() {
        assert!(satisfies("0.3", "0.3.0").unwrap());
        assert!(satisfies("0.3", "1.0.0").unwrap());
        assert!(!satisfies("0.3", "0.2.9").unwrap());
    }

    #[test]
    fn comparators_join_with_commas() {
        assert!(satisfies(">=0.3.0, <0.5", "0.4.2").unwrap());
        assert!(!satisfies(">=0.3.0, <0.5", "0.5.0").unwrap());
        assert!(satisfies(">0.3, <=0.4.1", "0.4.1").unwrap());
        assert!(!satisfies(">0.3", "0.3.0").unwrap());
    }

    #[test]
    fn exactly_matches_the_parts_given() {
        assert!(satisfies("=0.3", "0.3.7").unwrap());
        assert!(!satisfies("=0.3", "0.4.0").unwrap());
        assert!(satisfies("=0.3.7", "0.3.7").unwrap());
    }

    #[test]
    fn nonsense_is_refused() {
        assert!(satisfies(">=zero", "0.3.0").is_err());
        assert!(satisfies(">=0.3.0.1", "0.3.0").is_err());
        assert!(satisfies("", "0.3.0").is_err());
    }

    #[test]
    fn beyond_a_release_line_is_a_later_one() {
        assert!(!beyond("0.3", "0.3.9").unwrap());
        assert!(beyond("0.3", "0.4.0").unwrap());
        assert!(beyond("0.3.1", "0.3.2").unwrap());
        assert!(!beyond("1", "1.9.0").unwrap());
    }
}
