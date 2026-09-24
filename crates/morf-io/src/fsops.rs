//! Filesystem operations a configuration performs directly: listing a
//! folder, asking what a path is, making, moving, copying and removing, and
//! finding the user's own directories.
//!
//! Everything here is synchronous and bounded. A listing stops at
//! [`MAX_ENTRIES`], a read refuses a file larger than the bound it is given,
//! and a glob stops at [`MAX_ENTRIES`] matches, so a configuration that
//! points one of these at `/` gets an answer or a refusal, never a stall.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The most entries one listing or one glob returns.
pub const MAX_ENTRIES: usize = 10_000;
/// The deepest a recursive walk descends.
pub const MAX_DEPTH: usize = 32;

/// What a path is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Dir,
    Link,
    Other,
}

impl EntryKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dir => "dir",
            Self::Link => "link",
            Self::Other => "other",
        }
    }
}

/// One path's facts, as `stat` gives them.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    /// What the path itself is; a link is a link here.
    pub kind: EntryKind,
    /// What a link points at, when it is one and its target exists.
    pub target_kind: Option<EntryKind>,
    pub size: u64,
    /// Seconds since the epoch, with the fraction.
    pub modified: f64,
    pub accessed: f64,
    pub created: Option<f64>,
    /// The permission bits, `0o755` and the like.
    pub mode: u32,
    pub hidden: bool,
}

fn kind_of(file_type: fs::FileType) -> EntryKind {
    if file_type.is_symlink() {
        EntryKind::Link
    } else if file_type.is_dir() {
        EntryKind::Dir
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn seconds(time: io::Result<std::time::SystemTime>) -> Option<f64> {
    let time = time.ok()?;
    Some(match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    })
}

fn entry(path: &Path, meta: &fs::Metadata) -> Entry {
    let kind = kind_of(meta.file_type());
    let target_kind = if kind == EntryKind::Link {
        fs::metadata(path)
            .ok()
            .map(|target| kind_of(target.file_type()))
    } else {
        None
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    Entry {
        hidden: name.starts_with('.'),
        name,
        path: path.to_path_buf(),
        kind,
        target_kind,
        size: meta.len(),
        modified: seconds(meta.modified()).unwrap_or(0.0),
        accessed: seconds(meta.accessed()).unwrap_or(0.0),
        created: seconds(meta.created()),
        mode: meta.permissions().mode() & 0o7777,
    }
}

/// A path's facts, without following a final link.
pub fn stat(path: &Path) -> io::Result<Entry> {
    let meta = fs::symlink_metadata(path)?;
    Ok(entry(path, &meta))
}

/// How a listing is made.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListOptions {
    /// Include names that start with a dot.
    pub hidden: bool,
    /// Descend into directories, to this depth (0 lists the folder itself).
    pub depth: usize,
    /// Follow links to directories while descending.
    pub follow: bool,
}

/// A folder's entries, sorted by name, directories not first — a caller
/// sorts the way it wants. Stops at [`MAX_ENTRIES`] and says so.
pub fn list(path: &Path, options: ListOptions) -> io::Result<(Vec<Entry>, bool)> {
    let mut out = Vec::new();
    let truncated = walk(path, options, 0, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((out, truncated))
}

fn walk(path: &Path, options: ListOptions, depth: usize, out: &mut Vec<Entry>) -> io::Result<bool> {
    let mut children = fs::read_dir(path)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|child| child.file_name());
    for child in children {
        if out.len() >= MAX_ENTRIES {
            return Ok(true);
        }
        let name = child.file_name();
        if !options.hidden && name.to_string_lossy().starts_with('.') {
            continue;
        }
        let child_path = child.path();
        let Ok(meta) = fs::symlink_metadata(&child_path) else {
            continue;
        };
        let found = entry(&child_path, &meta);
        let descend = depth < options.depth.min(MAX_DEPTH)
            && (found.kind == EntryKind::Dir
                || (options.follow && found.target_kind == Some(EntryKind::Dir)));
        out.push(found);
        // A folder that cannot be read is skipped, not an error: one
        // locked subfolder must not cost the listing of everything else.
        if descend && walk(&child_path, options, depth + 1, out).unwrap_or(false) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reads a whole file, refusing one larger than `limit` bytes.
pub fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let meta = fs::metadata(path)?;
    if meta.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!(
                "{} is {} bytes, over the {limit}-byte limit",
                path.display(),
                meta.len()
            ),
        ));
    }
    fs::read(path)
}

/// How a write is made.
#[derive(Clone, Copy, Debug, Default)]
pub struct WriteOptions {
    /// Add to the end instead of replacing.
    pub append: bool,
    /// Write beside the file and rename over it, so a reader never sees half.
    pub atomic: bool,
    /// Make the missing folders on the way.
    pub parents: bool,
    /// Permission bits for a file this creates.
    pub mode: Option<u32>,
}

pub fn write(path: &Path, bytes: &[u8], options: WriteOptions) -> io::Result<()> {
    if options.parents
        && let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    if options.append {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(bytes)?;
        return Ok(());
    }
    if options.atomic {
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let mut temporary = name.to_os_string();
        temporary.push(format!(".morf-{}.tmp", std::process::id()));
        let temporary = path.with_file_name(temporary);
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            if let Some(mode) = options.mode {
                fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
            } else if let Ok(existing) = fs::metadata(path) {
                fs::set_permissions(&temporary, existing.permissions())?;
            }
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        return result;
    }
    fs::write(path, bytes)?;
    if let Some(mode) = options.mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

pub fn mkdir(path: &Path, parents: bool) -> io::Result<()> {
    if parents {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    }
}

/// Removes a file, a link, or a folder. A folder with anything in it goes
/// only when `recursive` says so. Refuses `/` and the home directory
/// whatever it is told: a configuration that computes an empty path should
/// get an error, not an empty disk.
pub fn remove(path: &Path, recursive: bool) -> io::Result<()> {
    let refuse = path.as_os_str().is_empty()
        || path.parent().is_none()
        || home_dir().is_some_and(|home| same_path(&home, path));
    if refuse {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to remove {}", path.display()),
        ));
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        if recursive {
            fs::remove_dir_all(path)
        } else {
            fs::remove_dir(path)
        }
    } else {
        fs::remove_file(path)
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => normalize(a) == normalize(b),
    }
}

pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

/// Copies a file, or a folder and everything in it when `recursive`.
/// Returns the bytes copied.
pub fn copy(from: &Path, to: &Path, recursive: bool) -> io::Result<u64> {
    let meta = fs::metadata(from)?;
    if !meta.is_dir() {
        return fs::copy(from, to);
    }
    if !recursive {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is a folder; copy it with recursive", from.display()),
        ));
    }
    copy_tree(from, to, 0)
}

fn copy_tree(from: &Path, to: &Path, depth: usize) -> io::Result<u64> {
    if depth > MAX_DEPTH {
        return Err(io::Error::other("folder is nested too deeply to copy"));
    }
    fs::create_dir_all(to)?;
    let mut copied = 0;
    for child in fs::read_dir(from)? {
        let child = child?;
        let target = to.join(child.file_name());
        let kind = child.file_type()?;
        if kind.is_dir() {
            copied += copy_tree(&child.path(), &target, depth + 1)?;
        } else if kind.is_symlink() {
            let pointed = fs::read_link(child.path())?;
            let _ = fs::remove_file(&target);
            std::os::unix::fs::symlink(pointed, &target)?;
        } else {
            copied += fs::copy(child.path(), &target)?;
        }
    }
    Ok(copied)
}

pub fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Up to `length` bytes from `offset`; fewer at the end of the file, none
/// past it. A negative-free window, so a reader that remembers where it got
/// to reads only what was appended since -- a log, a transcript.
pub fn read_range(path: &Path, offset: u64, length: u64) -> io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut out = Vec::new();
    file.take(length).read_to_end(&mut out)?;
    Ok(out)
}

pub fn read_link(path: &Path) -> io::Result<PathBuf> {
    fs::read_link(path)
}

pub fn realpath(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

/// `.` and `..` resolved by the text alone, without asking the disk.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !out.pop() && !path.is_absolute() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// `~` and `~/…` become the home directory; `$VAR` and `${VAR}` their value.
pub fn expand(path: &str) -> String {
    let mut text = path.to_owned();
    if let Some(home) = home_dir() {
        let home = home.to_string_lossy();
        if text == "~" {
            text = home.to_string();
        } else if let Some(rest) = text.strip_prefix("~/") {
            text = format!("{home}/{rest}");
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if braced {
            if chars.peek() == Some(&'}') {
                chars.next();
            } else {
                out.push_str("${");
                out.push_str(&name);
                continue;
            }
        }
        if name.is_empty() {
            out.push('$');
            if braced {
                out.push_str("{}");
            }
        } else {
            out.push_str(&std::env::var(&name).unwrap_or_default());
        }
    }
    out
}

/// The user's directories: `config`, `data`, `cache`, `state`, `runtime`,
/// `home`, and the ones `user-dirs.dirs` names — `desktop`, `documents`,
/// `download`, `music`, `pictures`, `publicshare`, `templates`, `videos`.
pub fn user_dir(kind: &str) -> Option<PathBuf> {
    let home = home_dir();
    let env_or = |var: &str, fallback: &str| {
        std::env::var_os(var)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    match kind {
        "home" => home.clone(),
        "config" => env_or("XDG_CONFIG_HOME", ".config"),
        "data" => env_or("XDG_DATA_HOME", ".local/share"),
        "cache" => env_or("XDG_CACHE_HOME", ".cache"),
        "state" => env_or("XDG_STATE_HOME", ".local/state"),
        "runtime" => std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        other => {
            let key = format!("XDG_{}_DIR", other.to_ascii_uppercase());
            let config = env_or("XDG_CONFIG_HOME", ".config")?;
            let text = fs::read_to_string(config.join("user-dirs.dirs")).ok();
            let named = text.as_deref().and_then(|text| {
                text.lines().find_map(|line| {
                    let (name, value) = line.trim().split_once('=')?;
                    (name.trim() == key).then(|| expand(value.trim().trim_matches('"')))
                })
            });
            if let Some(named) = named {
                return Some(PathBuf::from(named));
            }
            let fallback = match other {
                "desktop" => "Desktop",
                "documents" => "Documents",
                "download" | "downloads" => "Downloads",
                "music" => "Music",
                "pictures" => "Pictures",
                "publicshare" => "Public",
                "templates" => "Templates",
                "videos" => "Videos",
                _ => return None,
            };
            home.map(|home| home.join(fallback))
        }
    }
}

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

/// Free and total bytes of the filesystem holding `path`.
pub fn disk_usage(path: &Path) -> io::Result<(u64, u64, u64)> {
    let stat = rustix::fs::statvfs(path).map_err(io::Error::from)?;
    let block = stat.f_frsize.max(1);
    Ok((
        stat.f_blocks * block,
        stat.f_bfree * block,
        stat.f_bavail * block,
    ))
}

/// The owner's uid and gid of a path.
pub fn owner(path: &Path) -> io::Result<(u32, u32)> {
    let meta = fs::metadata(path)?;
    Ok((meta.uid(), meta.gid()))
}
