//! `morf approve <config>`: the configuration's imported modules and
//! plugins, each described in plain words and asked about, and the answers
//! recorded as approvals the shell reads at its next start.
//!
//! The configuration is read with every gated API refused: no module it
//! asks about has run with the user's privileges before the answer.

use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use morf_lua::{Approval, Approvals, ModuleDeclaration, Runtime};

use crate::permission_words::{describe, status_words};

/// What `morf approve` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) config: PathBuf,
    /// Approve everything without asking, for scripts.
    pub(crate) yes: bool,
}

/// How an approval run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Every imported module and plugin stands approved.
    Approved,
    /// At least one was not approved.
    Denied,
}

/// `0` when everything stands approved, `1` when something was denied,
/// `2` when the configuration could not be read at all.
pub(crate) fn exit_code(result: &Result<Outcome, String>) -> i32 {
    match result {
        Ok(Outcome::Approved) => 0,
        Ok(Outcome::Denied) => 1,
        Err(_) => 2,
    }
}

/// Reads `morf approve`'s arguments: `[--yes] <config>` or `[--yes] -c NAME`.
pub(crate) fn parse(rest: &[&str]) -> Result<Request, String> {
    let usage = "usage: morf approve [--yes] <shell.lua> | morf approve [--yes] -c <name>";
    let mut yes = false;
    let mut config = None;
    let mut words = rest.iter();
    while let Some(word) = words.next() {
        let named = match *word {
            "--yes" | "-y" => {
                yes = true;
                continue;
            }
            "-c" => {
                let name = words
                    .next()
                    .ok_or_else(|| format!("-c wants a name\n{usage}"))?;
                crate::config::named_config_path(name)?
            }
            flag if flag.starts_with('-') => {
                return Err(format!("morf approve has no option `{flag}`\n{usage}"));
            }
            path => PathBuf::from(path),
        };
        if config.replace(named).is_some() {
            return Err(format!("morf approve takes one configuration\n{usage}"));
        }
    }
    let config = config.ok_or_else(|| format!("morf approve wants a configuration\n{usage}"))?;
    Ok(Request { config, yes })
}

/// How many source files the search for literal `require`s reads.
const MAX_SCANNED: usize = 4096;

/// Runs the walk: `input` answers the prompts, `out` is what is printed,
/// and `store` (when given) replaces the shell's own approvals file.
pub(crate) fn approve(
    request: &Request,
    store: Option<&Path>,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Outcome, String> {
    let config = morf_lua::canonical_path(&request.config);
    if !config.is_file() {
        return Err(format!(
            "{}: no such configuration",
            request.config.display()
        ));
    }
    let (mut runtime, loaded) = morf_host::headless::discover::discover(&config, store);
    let say =
        |out: &mut dyn Write, line: String| writeln!(out, "{line}").map_err(|e| e.to_string());
    if let Err(error) = &loaded {
        say(
            out,
            format!(
                "note: {} stopped loading with gated calls refused: {error}",
                config.display()
            ),
        )?;
        say(
            out,
            "note: the modules it reached before that, and those its sources name, follow"
                .to_owned(),
        )?;
    }
    let records = reachable(&mut runtime, &config);
    let imported: Vec<_> = records
        .into_iter()
        .filter(ModuleDeclaration::imported)
        .collect();
    let note = "note: a module required under a computed name is found only when it runs; \
                run `morf approve` again if one is refused";
    if imported.is_empty() {
        if let Err(error) = loaded {
            return Err(format!("{} did not load: {error}", config.display()));
        }
        say(
            out,
            format!("{}: no imported modules or plugins", config.display()),
        )?;
        say(out, note.to_owned())?;
        return Ok(Outcome::Approved);
    }
    let catalog = runtime.service_catalog().ok();
    let store_path = runtime.approvals_path()?;
    let mut approvals = Approvals::load(&store_path)?;
    let (mut approved, mut denied, mut changed) = (0, 0, false);
    for record in &imported {
        for line in describe(record, catalog.as_deref()) {
            say(out, line)?;
        }
        let status = approvals.status(record);
        if status == Approval::Approved {
            say(out, "  already approved".to_owned())?;
            approved += 1;
            continue;
        }
        if status == Approval::Lapsed {
            say(out, format!("  {}", status_words(status)))?;
        }
        if request.yes || ask(input, out)? {
            approvals.approve(record)?;
            changed = true;
            approved += 1;
            say(out, "  approved".to_owned())?;
        } else {
            denied += 1;
            say(out, "  not approved".to_owned())?;
        }
    }
    if changed {
        approvals.save(&store_path)?;
        say(
            out,
            format!("approvals written to {}", store_path.display()),
        )?;
    }
    say(out, format!("{approved} approved, {denied} not approved"))?;
    say(out, note.to_owned())?;
    Ok(if denied == 0 {
        Outcome::Approved
    } else {
        Outcome::Denied
    })
}

fn ask(input: &mut dyn BufRead, out: &mut dyn Write) -> Result<bool, String> {
    write!(out, "  approve? [y/N] ").map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    let mut answer = String::new();
    input
        .read_line(&mut answer)
        .map_err(|error| error.to_string())?;
    let answer = answer.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// What the runtime loaded, then what the sources it read name in literal
/// `require` calls, transitively.
fn reachable(runtime: &mut Runtime, config: &Path) -> Vec<ModuleDeclaration> {
    let mut records = runtime.module_declarations();
    let mut names: BTreeSet<String> = records.iter().map(|record| record.name.clone()).collect();
    let mut queue: Vec<PathBuf> = std::iter::once(config.to_path_buf())
        .chain(records.iter().map(|record| record.path.clone()))
        .collect();
    let mut scanned = BTreeSet::new();
    while let Some(file) = queue.pop() {
        if scanned.len() >= MAX_SCANNED || !scanned.insert(file.clone()) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        for name in literal_requires(&source) {
            if !names.insert(name.clone()) {
                continue;
            }
            if let Some(record) = runtime.declare_module(&name) {
                queue.push(record.path.clone());
                records.push(record);
            }
        }
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    records
}

/// The module names `source` passes to `require` as a literal string:
/// `require("a.b")`, `require "a.b"`, `pcall(require, "a.b")`.
pub(crate) fn literal_requires(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = source[from..].find("require") {
        let start = from + offset;
        from = start + "require".len();
        let word_before = start > 0 && is_word(bytes[start - 1]);
        if word_before || bytes.get(from).copied().is_some_and(is_word) {
            continue;
        }
        let mut at = skip_space(bytes, from);
        if matches!(bytes.get(at), Some(b'(' | b',')) {
            at = skip_space(bytes, at + 1);
        }
        let Some(&quote) = bytes.get(at).filter(|byte| matches!(byte, b'"' | b'\'')) else {
            continue;
        };
        let rest = &source[at + 1..];
        let Some(end) = rest.find(quote as char) else {
            continue;
        };
        let name = &rest[..end];
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'));
        if valid {
            found.push(name.to_owned());
        }
    }
    found
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}
