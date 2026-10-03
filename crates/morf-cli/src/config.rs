use morf_io::{IpcRequest, IpcValue as WireValue, ipc_call};
use morf_lua::LogLevel;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{commands::*, supervisor::*};

pub(crate) fn usage() -> &'static str {
    "morf - reactive Wayland shell runtime\n\nusage: morf [--no-plugin | --clean] [-d | --daemonize] [shell.lua] [-- args...]\n       morf <shell|lock|greet> [-c NAME] [-- args...]\n       morf --lock <ipc|log|info|...>   (talk to this display's lock process)\n       morf -i <display> <client command>\n       morf list [-j|--json] [--show-dead]\n       morf info\n       morf app <app.lua> [-- args...]   (run a configuration as an application: its windows, one process)\n       morf [--no-plugin | --clean] -c <name>[/shell|/lock|/greet] [-- args...]\n       morf ipc call <target> [args...]\n       morf ipc verbs\n       morf log [-f|--follow] [--level <debug|info|warn|error>]\n       morf log --bindings\n       morf kill\n       morf bundle <shell.lua> [-o <output>] [--with <path>]...\n       morf check <shell.lua> [--size WxH] [--screens N] [--ipc 'VERB ARGS']... [--after MS] [--wait MS] [--strict] [--no-dbus | --private-bus] [--isolate] [-- args...]\n       morf render <shell.lua> -o <out.png> [--size WxH] [--scale S] [--surface NAME|INDEX|screen] [--ipc 'VERB ARGS']... [--after MS] [--wait MS] [--no-dbus | --private-bus] [--isolate] [-- args...]\n       morf test <spec.lua>... [--filter PATTERN] [--size WxH] [--scale S] [--snapshots DIR] [--no-dbus | --private-bus] [--no-isolate]\n       morf --help\n       morf --version\n\nA bundle is morf and a configuration in one file, which then takes only the\nconfiguration's own arguments after `--`.\n\ncheck, render and test run a configuration with no compositor: nothing\nconnects to Wayland and time is virtual. See docs/TESTING.md."
}

pub(crate) fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    // A bundle runs what it carries, and there is nothing else it could be
    // asked to run: its arguments are the configuration's, after the `--`,
    // with morf's own leading options still its own.
    if let Some(payload) = crate::bundle::embedded()? {
        let config = crate::bundle::unpack(&payload)?;
        let strings = args
            .iter()
            .map(|value| {
                value
                    .to_str()
                    .ok_or_else(|| "arguments must be UTF-8".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let name = config
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match strings.as_slice() {
            ["-h" | "--help"] => {
                println!(
                    "a morf bundle of {name}\n\nusage: <bundle> [--no-plugin] [-d | --daemonize] \
                     [-- args...]\n\nEverything after `--` is {name}'s own."
                );
                return Ok(());
            }
            ["-V" | "--version"] => {
                println!("morf {} bundling {name}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
        let (policy, daemonize, rest) = leading_options(&strings)?;
        morf_lua::arguments::install(own(rest)?);
        if daemonize {
            detach()?;
        }
        carry_fonts(&config);
        let source = fs::read(&config)
            .map_err(|error| format!("could not read {}: {error}", config.display()))?;
        // Nothing outside the bundle: that is what a bundle is for.
        return supervise(
            config,
            source,
            LoadPolicy {
                plugins: policy.plugins,
                external_roots: false,
            },
        );
    }
    match parse_command(&args)? {
        Command::Help => println!("{}", usage()),
        Command::Version => println!("morf {}", env!("CARGO_PKG_VERSION")),
        Command::Run(path, policy, arguments, daemonize) => {
            if daemonize {
                // Before anything else: a fork after threads exist takes only
                // the forking thread with it, and the supervisor is about to
                // start several.
                detach()?;
            }
            // Before anything is loaded, so the very first line of the very
            // first configuration can already ask what it was started with.
            morf_lua::arguments::install(arguments);
            let path = followed(path);
            carry_fonts(&path);
            let source = fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            supervise(path, source, policy)?;
        }
        Command::Bundle {
            config,
            output,
            extras,
        } => {
            let bundled = crate::bundle::write(&config, &output, &extras)?;
            println!(
                "{}: {} files, {} bytes of configuration appended",
                bundled.output.display(),
                bundled.files,
                bundled.payload
            );
        }
        Command::Log { follow, level } => follow_logs(follow, level)?,
        Command::List { json, show_dead } => list_instances(json, show_dead)?,
        Command::Info => print_info()?,
        Command::Types(dir) => write_types(&dir)?,
        Command::Runner(args) => return crate::runners::run(args),
        Command::Client(request) => {
            let reply = ipc_call(socket_path()?, &request).map_err(|error| error.to_string())?;
            println!(
                "{}",
                String::from_utf8_lossy(&reply.to_wire().map_err(|e| e.to_string())?)
            );
        }
    }
    Ok(())
}

/// Reads the options `morf log` takes.
///
/// Its own function because it is the only subcommand with more than one, and
/// folding it into the match above would put four cases of flag parsing in the
/// middle of a table of shapes.
fn parse_log(rest: &[&str]) -> Result<Command, String> {
    let mut follow = false;
    let mut level = LogLevel::Debug;
    let mut rest = rest.iter();
    while let Some(argument) = rest.next() {
        match *argument {
            "-f" | "--follow" => follow = true,
            "--level" => {
                let Some(name) = rest.next() else {
                    return Err("--level wants a name".to_owned());
                };
                level =
                    LogLevel::parse(name).ok_or_else(|| format!("unknown log level `{name}`"))?;
            }
            other => return Err(format!("unknown option `{other}` for `morf log`")),
        }
    }
    Ok(Command::Log { follow, level })
}

#[derive(Debug)]
pub(crate) enum Command {
    Help,
    Version,
    /// The configuration to run, how much of the world to load, and the
    /// arguments that are the configuration's own rather than morf's.
    Run(PathBuf, LoadPolicy, Vec<String>, bool),
    /// A configuration and this executable, written as one file.
    Bundle {
        config: PathBuf,
        output: PathBuf,
        extras: Vec<PathBuf>,
    },
    /// Reading the shell's log, once or until interrupted.
    Log {
        follow: bool,
        level: LogLevel,
    },
    /// Every running instance on this machine.
    List {
        json: bool,
        show_dead: bool,
    },
    /// Everything about this machine, this display and the instance on it.
    Info,
    /// The API as Lua language server definitions, written into a folder.
    Types(PathBuf),
    /// `check`, `render` or `test`: a configuration with no compositor.
    Runner(crate::runner_args::RunnerArgs),
    Client(IpcRequest),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoadPolicy {
    pub(crate) plugins: bool,
    pub(crate) external_roots: bool,
}

impl Default for LoadPolicy {
    fn default() -> Self {
        Self {
            plugins: true,
            external_roots: true,
        }
    }
}

pub(crate) fn parse_command(args: &[std::ffi::OsString]) -> Result<Command, String> {
    let strings = args
        .iter()
        .map(|value| {
            value
                .to_str()
                .ok_or_else(|| "arguments must be UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (policy, daemonize, strings) = leading_options(&strings)?;
    match strings {
        ["-h" | "--help"] => Ok(Command::Help),
        ["-V" | "--version"] => Ok(Command::Version),
        [part @ ("shell" | "lock" | "greet"), "-c", name, rest @ ..] => {
            if name.contains('/') {
                return Err("use a shell name with a role, e.g. morf lock -c caelestia".to_owned());
            }
            Ok(Command::Run(
                named_config_path(&format!("{name}/{part}"))?,
                policy,
                own(rest)?,
                daemonize,
            ))
        }
        [part @ ("shell" | "lock" | "greet"), rest @ ..] => Ok(Command::Run(
            if *part == "shell" {
                env::var_os("MORF_CONFIG")
                    .map(PathBuf::from)
                    .map(Ok)
                    .unwrap_or_else(default_config_path)?
            } else {
                named_config_path(&format!("default/{part}"))?
            },
            policy,
            own(rest)?,
            daemonize,
        )),
        ["-c", name, rest @ ..] => Ok(Command::Run(
            named_config_path(name)?,
            policy,
            own(rest)?,
            daemonize,
        )),
        ["ipc", "verbs"] => Ok(Command::Client(IpcRequest::Verbs)),
        ["ipc", "call", target, args @ ..] => Ok(Command::Client(IpcRequest::Call {
            target: (*target).to_owned(),
            args: args
                .iter()
                .map(|value| WireValue::String((*value).to_owned()))
                .collect(),
        })),
        ["log", "--bindings"] => Ok(Command::Client(IpcRequest::Bindings)),
        ["log", rest @ ..] => parse_log(rest),
        ["kill"] => Ok(Command::Client(IpcRequest::Kill)),
        ["list", rest @ ..] => parse_list(rest),
        ["bundle", rest @ ..] => parse_bundle(rest),
        ["info"] => Ok(Command::Info),
        ["types", dir] => Ok(Command::Types(PathBuf::from(dir))),
        ["app", path, rest @ ..] if !path.starts_with('-') => {
            crate::app::enter();
            Ok(Command::Run(PathBuf::from(path), policy, own(rest)?, daemonize))
        }
        ["check", rest @ ..] => runner(crate::runner_args::Runner::Check, rest, policy),
        ["render", rest @ ..] => runner(crate::runner_args::Runner::Render, rest, policy),
        ["test", rest @ ..] => runner(crate::runner_args::Runner::Test, rest, policy),
        // The configuration itself has to look like a path. Without this an
        // unknown flag becomes a filename, and `morf --colour` fails by saying
        // it could not read a file called `--colour` rather than that there is
        // no such option.
        [path, rest @ ..] if !path.starts_with('-') => Ok(Command::Run(
            PathBuf::from(path),
            policy,
            own(rest)?,
            daemonize,
        )),
        // With nothing named, the environment may name it: `MORF_CONFIG` is
        // how a session file or a display manager points a shell at its
        // configuration without a command line to put it on. What follows the
        // `--` is still that configuration's.
        [] | ["--", ..] => Ok(Command::Run(
            match env::var_os("MORF_CONFIG") {
                Some(path) => PathBuf::from(path),
                None => default_config_path()?,
            },
            policy,
            own(strings)?,
            daemonize,
        )),
        _ => Err(usage().to_owned()),
    }
}

/// A headless runner, with morf's leading `--clean` or `--no-plugin`.
fn runner(
    kind: crate::runner_args::Runner,
    rest: &[&str],
    policy: LoadPolicy,
) -> Result<Command, String> {
    let mut args = crate::runner_args::parse_runner(kind, rest)?;
    args.policy = policy;
    Ok(Command::Runner(args))
}

/// Reads morf's own leading options, in any order and any combination. Each
/// is about how morf runs rather than what it runs, so they come before the
/// command and the command sees none of them.
fn leading_options<'a>(
    strings: &'a [&'a str],
) -> Result<(LoadPolicy, bool, &'a [&'a str]), String> {
    let mut policy = LoadPolicy::default();
    let mut daemonize = false;
    let mut strings = strings;
    loop {
        match strings {
            ["--no-plugin", rest @ ..] => {
                policy.plugins = false;
                strings = rest;
            }
            ["--clean", rest @ ..] => {
                policy.plugins = false;
                policy.external_roots = false;
                strings = rest;
            }
            ["-d" | "--daemonize", rest @ ..] => {
                daemonize = true;
                strings = rest;
            }
            ["--lock", rest @ ..] => {
                LOCK_TARGET.store(true, std::sync::atomic::Ordering::Relaxed);
                strings = rest;
            }
            ["-i" | "--instance", display, rest @ ..] => {
                select_instance(display)?;
                strings = rest;
            }
            _ => break,
        }
    }
    Ok((policy, daemonize, strings))
}

/// Everything after `--` belongs to the configuration.
///
/// morf takes the few arguments it needs to find the file at all and stops
/// looking. What the rest mean is not something a shell runtime can know: they
/// are addressed to whatever was written in Lua, and they are handed over
/// exactly as typed. The `--` is the one line between the two: before it every
/// word is morf's and an unknown one is an error, after it every word is the
/// configuration's and none is. So `morf greeter.lua -- lock` says `lock` to
/// the greeter, and `morf greeter.lua lock` is refused rather than silently
/// handed to a file that may not have been written to look at it.
fn own(rest: &[&str]) -> Result<Vec<String>, String> {
    match rest {
        [] => Ok(Vec::new()),
        ["--", after @ ..] => Ok(after.iter().map(|word| (*word).to_owned()).collect()),
        [word, ..] => Err(format!(
            "unexpected argument `{word}`: arguments for the configuration go after `--`"
        )),
    }
}

/// The configuration where it really is: a link, or a folder that is one,
/// followed to the file. Its modules, fonts and watched folder are beside
/// that file -- so `~/.config/morf/default` can be a link to `caelestia` and
/// `require` finds caelestia's modules. A path that is not there is kept as
/// given, for the error to name.
pub(crate) fn followed(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

pub(crate) fn config_root() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("morf"));
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/morf"))
        .ok_or_else(|| "HOME and XDG_CONFIG_HOME are unset".to_owned())
}

/// User configuration first, then the system XDG configuration directories.
/// This lets the greeter account run the same installed shell without access
/// to another user's home or an embedded copy of the runtime.
fn config_roots() -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![config_root()?];
    let dirs = env::var_os("XDG_CONFIG_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/etc/xdg".into());
    roots.extend(
        env::split_paths(&dirs)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("morf")),
    );
    Ok(roots)
}

/// The parts one named shell may have, each a folder with its own
/// `init.lua`: the shell, its lock screen and its greeter.
pub(crate) const PARTS: [&str; 3] = ["shell", "lock", "greet"];

/// `-c NAME` or `-c NAME/PART`: `~/.config/morf/NAME/PART/init.lua`, the
/// part `shell` when none is named. A shell laid out the old way, as
/// `NAME/shell.lua`, is still found.
pub(crate) fn named_config_path(name: &str) -> Result<PathBuf, String> {
    let (name, part) = match name.split_once('/') {
        Some((name, part)) => (name, part),
        None => (name, "shell"),
    };
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err("config name must be NAME or NAME/PART".to_owned());
    }
    if !PARTS.contains(&part) {
        return Err(format!(
            "`{part}` is not a part of a shell: {}",
            PARTS.join(", ")
        ));
    }
    let roots = config_roots()?;
    for root in &roots {
        let folder = root.join(name);
        let path = folder.join(part).join("init.lua");
        if path.is_file() {
            return Ok(path);
        }
        let old = folder.join("shell.lua");
        if part == "shell" && old.is_file() {
            return Ok(old);
        }
    }
    Ok(roots[0].join(name).join(part).join("init.lua"))
}

/// What a bare `morf` runs: `~/.config/morf/shell.lua` if there is one (the
/// old layout), else the shell of whatever `~/.config/morf/default` is --
/// `make apply` makes that a link to a named shell.
pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    let root = config_root()?;
    let old = root.join("shell.lua");
    if old.exists() {
        return Ok(old);
    }
    named_config_path("default/shell")
}

/// The instance `-i` named, if any.
///
/// A process-wide choice rather than a parameter threaded through every
/// client command, the same way the configuration's own arguments are held:
/// it is decided once, before anything runs, and read from one place.
static INSTANCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn select_instance(display: &str) -> Result<(), String> {
    if display.is_empty() || display.contains('/') {
        return Err("an instance is named by its WAYLAND_DISPLAY, one path component".to_owned());
    }
    INSTANCE
        .set(display.to_owned())
        .map_err(|_| "-i given twice".to_owned())
}

/// Where every instance keeps its socket: one file per `WAYLAND_DISPLAY`.
///
/// The directory is the registry. There is no separate list to keep in step
/// with reality; an instance is running exactly when its socket answers.
pub(crate) fn socket_dir() -> Result<PathBuf, String> {
    env::var_os("XDG_RUNTIME_DIR")
        .map(|runtime| PathBuf::from(runtime).join("morf"))
        .ok_or_else(|| "XDG_RUNTIME_DIR is unset".to_owned())
}

pub(crate) fn socket_path() -> Result<PathBuf, String> {
    let display = INSTANCE.get().map(String::as_str);
    if LOCK_TARGET.load(std::sync::atomic::Ordering::Relaxed) {
        return lock_socket_path_for(display);
    }
    socket_path_for(display)
}

/// Whether `--lock` asked the client commands for this display's lock
/// process rather than its shell.
static LOCK_TARGET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The socket a lock process binds: its display's, with `-lock` on the name,
/// because the shell on the same display already holds the plain one.
pub(crate) fn lock_socket_path() -> Result<PathBuf, String> {
    lock_socket_path_for(INSTANCE.get().map(String::as_str))
}

pub(crate) fn lock_socket_path_for(display: Option<&str>) -> Result<PathBuf, String> {
    Ok(lock_variant(&socket_path_for(display)?))
}

/// `wayland-1.sock` -> `wayland-1-lock.sock`, in the same directory.
pub(crate) fn lock_variant(plain: &Path) -> PathBuf {
    let stem = plain
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    plain.with_file_name(format!("{stem}-lock.sock"))
}

/// The instance name for a `WAYLAND_DISPLAY`. libwayland also takes an
/// absolute path there, the socket itself (a sandbox's, a nested
/// compositor's, a client pointed across runtime directories), and the shell
/// refused to start under one: such a display is named by its last component
/// and a hash of the whole path, so two sockets of the same name elsewhere
/// stay two instances.
pub(crate) fn display_instance(raw: &str) -> Result<String, String> {
    if raw.starts_with('/') {
        let last = Path::new(raw)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "WAYLAND_DISPLAY names no socket".to_owned())?;
        let hash = crate::socket_path::fnv1a(raw.as_bytes()) as u32;
        return Ok(format!("{last}-{hash:08x}"));
    }
    if raw.is_empty() || raw.contains('/') {
        return Err("WAYLAND_DISPLAY must be one path component".to_owned());
    }
    Ok(raw.to_owned())
}

/// The socket for a named instance, or for this display when none is named.
pub(crate) fn socket_path_for(display: Option<&str>) -> Result<PathBuf, String> {
    let display = match display {
        Some(display) => display.to_owned(),
        None => display_instance(
            &env::var("WAYLAND_DISPLAY").map_err(|_| "WAYLAND_DISPLAY is unset".to_owned())?,
        )?,
    };
    if display.is_empty() || display.contains('/') {
        return Err("WAYLAND_DISPLAY must be one path component".to_owned());
    }
    // Too long a path for a socket moves somewhere short; see `socket_path`.
    crate::socket_path::fitting_socket_path(
        socket_dir()?.join(format!("{display}.sock")),
        &display,
        &crate::socket_path::fallback_dir(),
    )
}

/// Puts a `fonts` directory beside the configuration, if there is one, on
/// the font path, ahead of whatever was there. A bundle carries its faces
/// this way, and a configuration on disk may keep its own beside it too.
/// Set before any renderer exists, which is when the font database is read,
/// and before any thread, which is when setting the environment is sound.
fn carry_fonts(config: &Path) {
    let Some(fonts) = config.parent().map(|parent| parent.join("fonts")) else {
        return;
    };
    if !fonts.is_dir() {
        return;
    }
    let mut paths = vec![fonts];
    if let Some(existing) = env::var_os("MORF_FONT_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(joined) = env::join_paths(paths) {
        // SAFETY: called from `run` before the supervisor starts its threads.
        unsafe { env::set_var("MORF_FONT_PATH", joined) };
    }
}

/// Reads what `morf bundle` takes: the configuration, where to write, and
/// anything beyond `lib`, `assets`, `plugin`, `fonts` and the faces the
/// configuration names to carry along.
fn parse_bundle(rest: &[&str]) -> Result<Command, String> {
    let mut config = None;
    let mut output = None;
    let mut extras = Vec::new();
    let mut rest = rest.iter();
    while let Some(argument) = rest.next() {
        match *argument {
            "-o" | "--output" => {
                output = Some(PathBuf::from(
                    rest.next().ok_or_else(|| "-o wants a path".to_owned())?,
                ));
            }
            "--with" => {
                extras.push(PathBuf::from(
                    rest.next()
                        .ok_or_else(|| "--with wants a path".to_owned())?,
                ));
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}` for `morf bundle`"));
            }
            path if config.is_none() => config = Some(PathBuf::from(path)),
            other => return Err(format!("unexpected argument `{other}` for `morf bundle`")),
        }
    }
    let config = config.ok_or_else(|| "morf bundle wants a configuration".to_owned())?;
    // Named after the configuration when not named: `greeter.lua` bundles to
    // `greeter`, beside wherever the command was run.
    let output = match output {
        Some(output) => output,
        None => PathBuf::from(
            config
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| "the configuration needs a name to bundle under".to_owned())?,
        ),
    };
    Ok(Command::Bundle {
        config,
        output,
        extras,
    })
}

/// Reads the options `morf list` takes.
fn parse_list(rest: &[&str]) -> Result<Command, String> {
    let mut json = false;
    let mut show_dead = false;
    for argument in rest {
        match *argument {
            "-j" | "--json" => json = true,
            "--show-dead" => show_dead = true,
            other => return Err(format!("unknown option `{other}` for `morf list`")),
        }
    }
    Ok(Command::List { json, show_dead })
}

/// `morf types DIR`: this build's API as LuaLS definitions (`---@meta`), so
/// an editor pointed at DIR completes and checks `morf.*` and `ui.*`.
fn write_types(dir: &std::path::Path) -> Result<(), String> {
    for (relative, contents) in morf_lua::generate_types() {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(())
}
