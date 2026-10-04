//! Starting processes and connections through the hub, and reading the
//! options `spawn`, `run` and `connect` share.

use super::*;

impl Starter {
    /// Starts a child. The outer error is a refusal (too many running);
    /// the inner one a program that would not start, which `spawn` returns
    /// and `run` answers through its callback.
    pub(super) fn spawn<'gc>(
        &self,
        ctx: Context<'gc>,
        options: SpawnOptions,
        kind: Kind,
    ) -> Result<Result<UserData<'gc>, String>, HostError> {
        let spawned = self.state.borrow_mut().io.spawn(options, kind);
        let link = match spawned.map_err(HostError)? {
            Ok(link) => link,
            Err(message) => return Ok(Err(message)),
        };
        let userdata = UserData::new_static(&ctx, IoToken(link));
        userdata.set_metatable(ctx, Some(ctx.fetch(&self.process_metatable)));
        Ok(Ok(userdata))
    }

    pub(super) fn connect<'gc>(
        &self,
        ctx: Context<'gc>,
        options: ConnectOptions,
        kind: Kind,
    ) -> Result<UserData<'gc>, HostError> {
        let link = self
            .state
            .borrow_mut()
            .io
            .connect(options, kind)
            .map_err(HostError)?;
        let userdata = UserData::new_static(&ctx, IoToken(link));
        userdata.set_metatable(ctx, Some(ctx.fetch(&self.connection_metatable)));
        Ok(userdata)
    }
}

/// What `spawn` and `run` share: the argv and where and how it runs.
pub(super) fn spawn_options<'gc>(
    ctx: Context<'gc>,
    command: Table<'gc>,
    options: Option<Table<'gc>>,
) -> Result<SpawnOptions, HostError> {
    let command = table_string_array(ctx, command, 256).map_err(HostError)?;
    if command.is_empty() || command[0].is_empty() {
        return Err(HostError("command cannot be empty".into()));
    }
    let mut spawn = SpawnOptions::new(command);
    let Some(options) = options else {
        return Ok(spawn);
    };
    match options.get_value(ctx, "env") {
        LuaValue::Nil => {}
        LuaValue::Table(env) => {
            spawn.environment = table_string_map(ctx, env, 256).map_err(HostError)?;
        }
        _ => return Err(HostError("env must be a table".into())),
    }
    spawn.clear_environment = table_bool(ctx, options, "clear_env", false).map_err(HostError)?;
    match options.get_value(ctx, "cwd") {
        LuaValue::Nil => {}
        LuaValue::String(cwd) => {
            spawn.working_directory = Some(PathBuf::from(cwd.display_lossy().to_string()));
        }
        _ => return Err(HostError("cwd must be a string".into())),
    }
    spawn.stdin = match options.get_value(ctx, "stdin") {
        LuaValue::Nil | LuaValue::Boolean(false) => StdinMode::Null,
        LuaValue::Boolean(true) => StdinMode::Pipe,
        LuaValue::String(text) if text.as_bytes() == b"pipe" => StdinMode::Pipe,
        LuaValue::String(text) => {
            if text.as_bytes().len() > morf_io::MAX_OUTGOING {
                return Err(HostError("stdin is too large".into()));
            }
            StdinMode::Data(text.as_bytes().to_vec())
        }
        _ => return Err(HostError("stdin must be a string, \"pipe\" or nil".into())),
    };
    if let Some(max_line) = positive(ctx, options, "max_line")? {
        spawn.max_line = (max_line as usize).min(MAX_LINE_LIMIT);
    }
    spawn.timeout = positive(ctx, options, "timeout_ms")?.map(Duration::from_millis);
    spawn.detached = table_bool(ctx, options, "detached", false).map_err(HostError)?;
    Ok(spawn)
}

pub(super) fn endpoint_of<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
) -> Result<Endpoint, HostError> {
    match (
        options.get_value(ctx, "path"),
        options.get_value(ctx, "host"),
        options.get_value(ctx, "port"),
    ) {
        (LuaValue::String(path), LuaValue::Nil, LuaValue::Nil) => Ok(Endpoint::Unix(
            PathBuf::from(path.display_lossy().to_string()),
        )),
        (LuaValue::Nil, LuaValue::String(host), LuaValue::Integer(port))
            if (1..=65535).contains(&port) =>
        {
            Ok(Endpoint::Tcp {
                host: host.display_lossy().to_string(),
                port: port as u16,
            })
        }
        _ => Err(HostError(
            "connect needs path = \"...\" or host = \"...\", port = n".into(),
        )),
    }
}

pub(super) fn positive<'gc>(
    ctx: Context<'gc>,
    options: Table<'gc>,
    field: &str,
) -> Result<Option<u64>, HostError> {
    match options.get_value(ctx, field) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(value) if value > 0 => Ok(Some(value as u64)),
        LuaValue::Number(value) if value.is_finite() && value >= 1.0 => Ok(Some(value as u64)),
        _ => Err(HostError(format!("{field} must be a positive number"))),
    }
}

/// A signal as `kill` takes it: a name, a number, or nil for TERM.
pub(super) fn signal_of(value: LuaValue<'_>) -> Result<i32, HostError> {
    match value {
        LuaValue::Nil => morf_io::signal_number("TERM"),
        LuaValue::Integer(number) => i32::try_from(number)
            .ok()
            .and_then(|number| morf_io::signal_number(&number.to_string())),
        LuaValue::String(name) => morf_io::signal_number(&name.display_lossy().to_string()),
        _ => None,
    }
    .ok_or_else(|| HostError("kill takes a signal name or number".into()))
}
