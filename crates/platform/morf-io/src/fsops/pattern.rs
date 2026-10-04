//! Shell patterns: matching one name, and globbing a pattern over the disk.

use std::fs;
use std::path::{Path, PathBuf};

use super::{MAX_DEPTH, MAX_ENTRIES, expand};

/// Whether `name` matches a shell pattern: `*`, `?`, `[abc]`, `[a-z]`,
/// `[!x]`, and `{a,b}` alternatives. Case-sensitive.
pub fn matches(pattern: &str, name: &str) -> bool {
    expand_braces(pattern)
        .iter()
        .any(|pattern| match_one(pattern.as_bytes(), name.as_bytes()))
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let mut depth = 0;
    let mut close = None;
    for (index, c) in pattern[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_owned()];
    };
    let (head, inner, tail) = (
        &pattern[..open],
        &pattern[open + 1..close],
        &pattern[close + 1..],
    );
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (index, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
        .into_iter()
        .flat_map(|part| expand_braces(&format!("{head}{part}{tail}")))
        .take(256)
        .collect()
}

fn match_one(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let (mut star, mut resume) = (None, 0);
    while n < name.len() {
        if p < pattern.len() {
            match pattern[p] {
                b'*' => {
                    star = Some(p);
                    resume = n;
                    p += 1;
                    continue;
                }
                b'?' => {
                    p += 1;
                    n += 1;
                    continue;
                }
                b'[' => {
                    if let Some((matched, next)) = class(&pattern[p..], name[n]) {
                        if matched {
                            p += next;
                            n += 1;
                            continue;
                        }
                    } else if name[n] == b'[' {
                        p += 1;
                        n += 1;
                        continue;
                    }
                }
                b'\\' if p + 1 < pattern.len() => {
                    if pattern[p + 1] == name[n] {
                        p += 2;
                        n += 1;
                        continue;
                    }
                }
                c if c == name[n] => {
                    p += 1;
                    n += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some(at) => {
                p = at + 1;
                resume += 1;
                n = resume;
            }
            None => return false,
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// A `[...]` class at the start of `pattern`: whether `c` is in it and how
/// long the class is. `None` when the bracket never closes.
fn class(pattern: &[u8], c: u8) -> Option<(bool, usize)> {
    let mut i = 1;
    let negate = matches!(pattern.get(i), Some(b'!' | b'^'));
    if negate {
        i += 1;
    }
    let mut found = false;
    let mut first = true;
    while i < pattern.len() {
        if pattern[i] == b']' && !first {
            return Some((found != negate, i + 1));
        }
        first = false;
        if i + 2 < pattern.len() && pattern[i + 1] == b'-' && pattern[i + 2] != b']' {
            if (pattern[i]..=pattern[i + 2]).contains(&c) {
                found = true;
            }
            i += 3;
        } else {
            if pattern[i] == c {
                found = true;
            }
            i += 1;
        }
    }
    None
}

/// Paths matching `pattern`, where each segment may hold `*`, `?`, `[..]`
/// and `{a,b}`, and a segment that is exactly `**` matches any depth.
/// Hidden names match only a segment that starts with a dot itself.
pub fn glob(pattern: &str) -> (Vec<PathBuf>, bool) {
    let pattern = expand(pattern);
    let absolute = pattern.starts_with('/');
    let segments = pattern
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let start = if absolute {
        PathBuf::from("/")
    } else {
        PathBuf::from(".")
    };
    let mut out = Vec::new();
    let truncated = glob_walk(&start, &segments, 0, &mut out, !absolute);
    out.sort();
    out.dedup();
    (out, truncated)
}

fn is_pattern(segment: &str) -> bool {
    segment.contains(['*', '?', '[', '{'])
}

fn glob_walk(
    base: &Path,
    segments: &[&str],
    depth: usize,
    out: &mut Vec<PathBuf>,
    relative: bool,
) -> bool {
    if out.len() >= MAX_ENTRIES {
        return true;
    }
    let clean = |path: PathBuf| {
        if relative {
            path.strip_prefix("./")
                .map(Path::to_path_buf)
                .unwrap_or(path)
        } else {
            path
        }
    };
    let Some((segment, rest)) = segments.split_first() else {
        out.push(clean(base.to_path_buf()));
        return false;
    };
    if depth > MAX_DEPTH * 2 {
        return false;
    }
    if *segment == "**" {
        // Zero folders deep, then every folder below, each trying the rest.
        if glob_walk(base, rest, depth + 1, out, relative) {
            return true;
        }
        let Ok(children) = fs::read_dir(base) else {
            return false;
        };
        let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(|child| child.file_name());
        for child in children {
            let name = child.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if child.file_type().is_ok_and(|kind| kind.is_dir())
                && glob_walk(&child.path(), segments, depth + 1, out, relative)
            {
                return true;
            }
        }
        return false;
    }
    if !is_pattern(segment) {
        let next = base.join(segment);
        if rest.is_empty() {
            if fs::symlink_metadata(&next).is_ok() {
                out.push(clean(next));
            }
            return out.len() >= MAX_ENTRIES;
        }
        return glob_walk(&next, rest, depth + 1, out, relative);
    }
    let Ok(children) = fs::read_dir(base) else {
        return false;
    };
    let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
    children.sort_by_key(|child| child.file_name());
    for child in children {
        let name = child.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && !segment.starts_with('.') {
            continue;
        }
        if !matches(segment, &name) {
            continue;
        }
        let path = child.path();
        if rest.is_empty() {
            out.push(clean(path));
            if out.len() >= MAX_ENTRIES {
                return true;
            }
        } else if fs::metadata(&path).is_ok_and(|meta| meta.is_dir())
            && glob_walk(&path, rest, depth + 1, out, relative)
        {
            return true;
        }
    }
    false
}
