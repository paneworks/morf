//! What `morf check`, `morf render` and `morf test` are asked to do.
//!
//! One parser for the three, because they take mostly the same options --
//! a screen size, how long to run, whether to reach the session bus -- and a
//! flag that means one thing to `check` should not mean another to `render`.

use std::path::PathBuf;
use std::time::Duration;

/// Which of the three runners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Runner {
    Check,
    Render,
    Test,
}

impl Runner {
    fn name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Render => "render",
            Self::Test => "test",
        }
    }
}

/// Everything a headless runner was asked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunnerArgs {
    pub(crate) runner: Runner,
    /// The configuration (`check`, `render`) or the spec files (`test`).
    pub(crate) files: Vec<PathBuf>,
    pub(crate) size: (u32, u32),
    pub(crate) screens: usize,
    pub(crate) scale: u32,
    /// Virtual time to run before reporting or drawing.
    pub(crate) after: Duration,
    /// Wall time to spread over that, for answers from real processes.
    pub(crate) wait: Duration,
    pub(crate) no_dbus: bool,
    /// Whether a `dbus-daemon` of the run's own stands in for the session
    /// bus.
    pub(crate) private_bus: bool,
    /// Whether the XDG directories point at a scratch folder.
    pub(crate) isolate: bool,
    pub(crate) strict: bool,
    /// `--kit`: check the configuration's `kit` module against the widget
    /// contract (`library/lib/kit/contract.lua`).
    pub(crate) kit: bool,
    /// `--a11y`: every control a screen reader reaches has a name.
    pub(crate) a11y: bool,
    pub(crate) output: Option<PathBuf>,
    pub(crate) surface: Option<String>,
    pub(crate) filter: Option<String>,
    /// IPC calls to make after loading, before `--after`: each a verb and
    /// its arguments, split on whitespace.
    pub(crate) ipc: Vec<Vec<String>>,
    /// Where `test.snapshot` writes.
    pub(crate) snapshots: Option<PathBuf>,
    /// The configuration's own arguments, after `--`.
    pub(crate) args: Vec<String>,
    /// morf's own `--clean` or `--no-plugin`, given before the runner.
    pub(crate) policy: crate::config::LoadPolicy,
}

impl RunnerArgs {
    fn new(runner: Runner) -> Self {
        Self {
            runner,
            files: Vec::new(),
            size: (1920, 1080),
            screens: 1,
            scale: 1,
            after: Duration::from_millis(match runner {
                Runner::Check => 500,
                Runner::Render => 250,
                Runner::Test => 0,
            }),
            wait: Duration::ZERO,
            no_dbus: false,
            private_bus: false,
            // A test must not write to the settings of the person running it;
            // a check or a picture usually wants to read them.
            isolate: runner == Runner::Test,
            strict: false,
            kit: false,
            a11y: false,
            output: None,
            surface: None,
            filter: None,
            ipc: Vec::new(),
            snapshots: None,
            args: Vec::new(),
            policy: crate::config::LoadPolicy::default(),
        }
    }
}

/// Reads `WxH`.
pub(crate) fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let wrong = || format!("a size is WIDTHxHEIGHT, not `{text}`");
    let (width, height) = text.split_once(['x', 'X']).ok_or_else(wrong)?;
    let parse = |part: &str| {
        part.trim()
            .parse::<u32>()
            .ok()
            .filter(|value| (1..=16_384).contains(value))
            .ok_or_else(wrong)
    };
    Ok((parse(width)?, parse(height)?))
}

/// Reads a count of milliseconds.
fn parse_ms(option: &str, text: &str) -> Result<Duration, String> {
    text.parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|_| format!("{option} wants milliseconds, not `{text}`"))
}

/// The default look's check configuration (library/lib/kit/skins/default/
/// check.lua): in the project's library beside the working directory, a
/// `MORF_RUNTIME_PATH` root, or an installed library -- the user's, then each
/// of `XDG_DATA_DIRS` (where a package, a Nix profile among them, puts it).
fn default_kit_config() -> Result<PathBuf, String> {
    let tail = std::path::Path::new("lib/kit/skins/default/check.lua");
    let here = std::env::current_dir().map_err(|e| e.to_string())?.join("x");
    let mut roots: Vec<PathBuf> = morf_host::morf_lua::project_library(&here).into_iter().collect();
    roots.extend(
        std::env::var_os("MORF_RUNTIME_PATH")
            .into_iter()
            .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>()),
    );
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(data) = data {
        roots.push(data.join("morf/library"));
    }
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(
        std::env::split_paths(&dirs)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("morf/library")),
    );
    roots
        .into_iter()
        .map(|root| root.join(tail))
        .find(|path| path.is_file())
        .ok_or_else(|| "`--kit default`: no library with lib/kit/skins/default/check.lua found".to_owned())
}

/// Reads the options of one runner, after its name.
pub(crate) fn parse_runner(runner: Runner, rest: &[&str]) -> Result<RunnerArgs, String> {
    let mut parsed = RunnerArgs::new(runner);
    let mut rest = rest.iter();
    let name = runner.name();
    let value = |rest: &mut std::slice::Iter<&str>, option: &str| {
        rest.next()
            .map(|value| (*value).to_owned())
            .ok_or_else(|| format!("{option} wants a value"))
    };
    while let Some(argument) = rest.next() {
        match (*argument, runner) {
            ("--", Runner::Check | Runner::Render) => {
                parsed.args = rest.map(|word| (*word).to_owned()).collect();
                break;
            }
            ("--size", _) => parsed.size = parse_size(&value(&mut rest, "--size")?)?,
            ("--screens", Runner::Check | Runner::Render) => {
                let text = value(&mut rest, "--screens")?;
                // `morf check --screens 0` loads it as the shell does once
                // every output is gone; a render needs something to draw.
                let least = usize::from(runner == Runner::Render);
                parsed.screens = text
                    .parse::<usize>()
                    .ok()
                    .filter(|count| (least..=16).contains(count))
                    .ok_or_else(|| format!("--screens wants {least} to 16, not `{text}`"))?;
            }
            ("--scale", Runner::Render | Runner::Test) => {
                let text = value(&mut rest, "--scale")?;
                parsed.scale = text
                    .parse::<u32>()
                    .ok()
                    .filter(|scale| (1..=4).contains(scale))
                    .ok_or_else(|| format!("--scale wants 1 to 4, not `{text}`"))?;
            }
            ("--after", Runner::Check | Runner::Render) => {
                parsed.after = parse_ms("--after", &value(&mut rest, "--after")?)?;
            }
            ("--wait", Runner::Check | Runner::Render) => {
                parsed.wait = parse_ms("--wait", &value(&mut rest, "--wait")?)?;
            }
            ("--ipc", Runner::Check | Runner::Render) => {
                let call = value(&mut rest, "--ipc")?;
                let words = call
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if words.is_empty() {
                    return Err("--ipc wants a verb".to_owned());
                }
                parsed.ipc.push(words);
            }
            ("--no-dbus", _) => parsed.no_dbus = true,
            ("--private-bus", _) => parsed.private_bus = true,
            ("--isolate", _) => parsed.isolate = true,
            ("--no-isolate", _) => parsed.isolate = false,
            ("--strict", Runner::Check) => parsed.strict = true,
            ("--kit", Runner::Check) => {
                parsed.kit = true;
                // `--kit default`: the library's own default look, checked
                // with nothing else loaded.
                if rest.as_slice().first() == Some(&"default") {
                    rest.next();
                    parsed.files.push(default_kit_config()?);
                }
            }
            ("--a11y", Runner::Check) => parsed.a11y = true,
            ("-o" | "--output", Runner::Render) => {
                parsed.output = Some(PathBuf::from(value(&mut rest, "-o")?));
            }
            ("--surface", Runner::Render) => parsed.surface = Some(value(&mut rest, "--surface")?),
            ("--filter", Runner::Test) => parsed.filter = Some(value(&mut rest, "--filter")?),
            ("--snapshots", Runner::Test) => {
                parsed.snapshots = Some(PathBuf::from(value(&mut rest, "--snapshots")?));
            }
            (other, _) if other.starts_with('-') => {
                return Err(format!("unknown option `{other}` for `morf {name}`"));
            }
            (path, Runner::Test) => parsed.files.push(PathBuf::from(path)),
            (path, _) if parsed.files.is_empty() => parsed.files.push(PathBuf::from(path)),
            (other, _) => {
                return Err(format!(
                    "unexpected argument `{other}` for `morf {name}`: \
                     arguments for the configuration go after `--`"
                ));
            }
        }
    }
    if parsed.files.is_empty() {
        return Err(match runner {
            Runner::Test => "morf test wants one or more spec files".to_owned(),
            _ => format!("morf {name} wants a configuration"),
        });
    }
    if runner == Runner::Render && parsed.output.is_none() {
        return Err("morf render wants `-o <file.png>`".to_owned());
    }
    Ok(parsed)
}
