-- systemd-logind: the session, the power buttons, the lid and the backlight.
--
-- A shell needs logind for the things that are not its to decide alone:
-- whether the machine can suspend, telling it to, hearing that it is about
-- to (so the screen is locked *before* the lid closes, not after it opens),
-- hearing `loginctl lock-session`, and changing the backlight without being
-- root. All of it is `org.freedesktop.login1` on the system bus.
--
--   local logind = require("lib.logind")
--   local login = logind.connect()
--   login.on_lock(function() show_lock_screen() end)
--   login.on_prepare_for_sleep(function(going) if going then lock_now() end end)
--   ui.Slider { value = function() return login.state.brightness.percent end,
--               on_changed = function(v) login.set_brightness(v / 100) end }
--
-- Inhibitors and file descriptors. logind's `Inhibit` answers with a file
-- descriptor, and the lock lasts exactly as long as someone holds it open.
-- The engine hands it over as an opaque handle (see the note at the top of
-- `morf_io::dbus_types`): nothing can be read or written through it, only
-- held and closed. So the lock is held by the shell itself and ends when the
-- handle is released, when it is dropped and collected, or when the shell
-- dies for any reason — the kernel closes the descriptor. A lock that
-- outlived a crashed shell would keep a laptop awake in a bag.
--
-- Nothing here waits on logind to act. The power actions, locking and the
-- backlight are sent and answered later (logind may be waiting on polkit, and
-- the shell keeps drawing meanwhile): each returns true once sent, or nil and
-- why not, and takes an optional last argument `done(ok, err)`.
--
-- Brightness is read from `/sys/class/backlight` with `morf.fs` and written
-- with `Session.SetBrightness`, which logind allows an active session to do
-- without privileges. It is re-read when udev reports a backlight change.

local morf = require("morf")
local dbus_client = require("lib.dbus_client")

local logind = {}

local NAME = "org.freedesktop.login1"
local MANAGER_PATH = "/org/freedesktop/login1"
local MANAGER = "org.freedesktop.login1.Manager"
local SESSION = "org.freedesktop.login1.Session"
local AUTO_SESSION = "/org/freedesktop/login1/session/auto"

local u = dbus_client.u

-- The `Can*` questions, by the name this library gives each.
local CAN = {
  suspend = "CanSuspend",
  hibernate = "CanHibernate",
  hybrid_sleep = "CanHybridSleep",
  suspend_then_hibernate = "CanSuspendThenHibernate",
  reboot = "CanReboot",
  power_off = "CanPowerOff",
}

local function empty_session()
  return {
    id = "", path = "", user = "", uid = -1, type = "", class = "", seat = "", vt = 0,
    active = false, locked = false, idle = false, remote = false, state = "",
    desktop = "", service = "",
  }
end

local function trim_number(text)
  if type(text) ~= "string" then return nil end
  return tonumber((text:gsub("%s+", "")))
end

--- Starts watching. Call it while the configuration loads.
---
--- Options: `bus` ("system"), `name` (logind's), `session_path` (skips
--- finding the caller's own session), `backlight_dir`
--- ("/sys/class/backlight"), `backlight` (a device name; the first
--- otherwise), `action_timeout_ms` (how long an action may wait on logind
--- and polkit; 25000), `dbus` (test seam), `udev` (false to skip the
--- backlight watch).
function logind.connect(options)
  options = options or {}
  local name = options.name or NAME
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or "system" })
  local action_timeout = options.action_timeout_ms or 25000
  local backlight_dir = options.backlight_dir or "/sys/class/backlight"

  local can_seed = {}
  for key in pairs(CAN) do can_seed[key] = "" end

  local state = morf.state({
    available = false,
    session = empty_session(),
    can = can_seed,
    lid_closed = false,
    handle_lid_switch = "",
    docked = false,
    idle_hint = false,
    preparing_for_sleep = false,
    preparing_for_shutdown = false,
    brightness = { device = "", value = 0, max = 0, percent = 0 },
    inhibitors = {},
  })

  local login = { state = state }
  local handlers = { lock = {}, unlock = {}, sleep = {}, shutdown = {} }
  local session_path = options.session_path

  local function assign(target, values)
    for key, value in pairs(values) do target[key] = value end
  end

  local function fire(list, ...)
    for _, handler in ipairs(list) do
      local ok, err = pcall(handler, ...)
      if not ok then morf.log.warn("logind handler: " .. tostring(err)) end
    end
  end

  --- Finds the session this shell runs in. `session/auto` names it for
  --- reading, but signals are sent from the session's real path, so the
  --- real path is asked for.
  local function find_session()
    if session_path then return session_path end
    local id = client.get(name, AUTO_SESSION, SESSION, "Id")
    if type(id) ~= "string" or id == "" then return nil end
    local path = client.call1(name, MANAGER_PATH, MANAGER, "GetSession", { id })
    if type(path) == "string" then return path end
  end

  local function read_session()
    if not session_path then
      assign(state.session, empty_session())
      return
    end
    local p = client.get_all(name, session_path, SESSION)
    if not p then
      assign(state.session, empty_session())
      return
    end
    -- `Seat` is `(so)` and `User` is `(uo)`: a name or number, then a path.
    local seat = type(p.Seat) == "table" and p.Seat[1] or ""
    local uid = type(p.User) == "table" and p.User[1] or -1
    assign(state.session, {
      id = p.Id or "", path = session_path, user = p.Name or "", uid = uid,
      type = p.Type or "", class = p.Class or "", seat = seat, vt = p.VTNr or 0,
      active = p.Active == true, locked = p.LockedHint == true, idle = p.IdleHint == true,
      remote = p.Remote == true, state = p.State or "", desktop = p.Desktop or "",
      service = p.Service or "",
    })
  end

  local function read_manager()
    local p = client.get_all(name, MANAGER_PATH, MANAGER)
    if not p then
      state.available = false
      return false
    end
    state.available = true
    state.lid_closed = p.LidClosed == true
    state.handle_lid_switch = p.HandleLidSwitch or ""
    state.docked = p.Docked == true
    state.idle_hint = p.IdleHint == true
    state.preparing_for_sleep = p.PreparingForSleep == true
    state.preparing_for_shutdown = p.PreparingForShutdown == true
    return true
  end

  local function read_can()
    for key, method in pairs(CAN) do
      local answer = client.call1(name, MANAGER_PATH, MANAGER, method)
      state.can[key] = type(answer) == "string" and answer or ""
    end
  end

  local function read_inhibitors()
    local list = client.call1(name, MANAGER_PATH, MANAGER, "ListInhibitors")
    local rows = {}
    for index, entry in ipairs(type(list) == "table" and list or {}) do
      if type(entry) == "table" then
        rows[#rows + 1] = {
          key = index, what = entry[1] or "", who = entry[2] or "", why = entry[3] or "",
          mode = entry[4] or "", uid = entry[5] or -1, pid = entry[6] or -1,
        }
      end
    end
    state.inhibitors:replace(rows)
  end

  --- The backlight devices under `/sys/class/backlight`, each
  --- `{ name, value, max }`.
  function login.backlights()
    local entries = morf.fs.list(backlight_dir)
    local found = {}
    for _, entry in ipairs(type(entries) == "table" and entries or {}) do
      local dir = backlight_dir .. "/" .. entry.name
      local value = trim_number(morf.fs.read(dir .. "/brightness"))
      local max = trim_number(morf.fs.read(dir .. "/max_brightness"))
      if value and max then
        found[#found + 1] = { name = entry.name, value = value, max = max }
      end
    end
    table.sort(found, function(a, b) return a.name < b.name end)
    return found
  end

  local function backlight()
    for _, device in ipairs(login.backlights()) do
      if options.backlight == nil or device.name == options.backlight then return device end
    end
  end

  local function read_brightness()
    local device = backlight()
    if not device then
      assign(state.brightness, { device = "", value = 0, max = 0, percent = 0 })
      return
    end
    assign(state.brightness, {
      device = device.name, value = device.value, max = device.max,
      percent = device.max > 0 and math.floor(device.value * 100 / device.max + 0.5) or 0,
    })
  end

  local function refresh()
    if read_manager() then
      session_path = find_session()
      read_session()
      read_can()
      read_inhibitors()
    else
      assign(state.session, empty_session())
      for key in pairs(CAN) do state.can[key] = "" end
      state.inhibitors:replace({})
    end
    read_brightness()
  end

  local function nothing() end

  --- Sends one action and returns true once it is on its way; `done(ok,
  --- err)` hears how it ended.
  local function send(path, interface, method, arguments, done)
    done = done or nothing
    local sent, err = client.call_async(name, path, interface, method, arguments, action_timeout,
      function(ok, failure) done(ok and true or nil, failure) end)
    if not sent then return nil, err end
    return true
  end

  local function manager_call(method, arguments, done)
    return send(MANAGER_PATH, MANAGER, method, arguments, done)
  end

  local function session_call(method, arguments, done)
    if not session_path then return nil, "no session" end
    return send(session_path, SESSION, method, arguments, done)
  end

  --- Whether logind is on the bus.
  function login.available() return state.available end

  function login.refresh() refresh() end

  --- `CanSuspend` and its siblings, asked now: "yes", "no", "challenge"
  --- (allowed after authenticating) or "na" (not possible here).
  function login.can(what)
    local method = CAN[what]
    if not method then return nil, "unknown action " .. tostring(what) end
    local answer, err = client.call1(name, MANAGER_PATH, MANAGER, method)
    if type(answer) ~= "string" then return nil, err end
    state.can[what] = answer
    return answer
  end

  --- `handler()` when something (`loginctl lock-session`, an idle daemon)
  --- asks this session to lock; `on_unlock` for the reverse.
  function login.on_lock(handler) handlers.lock[#handlers.lock + 1] = handler end
  function login.on_unlock(handler) handlers.unlock[#handlers.unlock + 1] = handler end

  --- `handler(true)` just before suspend, `handler(false)` after resume.
  --- Pair it with a "delay" inhibitor to have time to lock first.
  function login.on_prepare_for_sleep(handler) handlers.sleep[#handlers.sleep + 1] = handler end
  function login.on_prepare_for_shutdown(handler)
    handlers.shutdown[#handlers.shutdown + 1] = handler
  end

  --- Asks logind to lock this session. It answers by sending `Lock` back,
  --- to this shell's own `on_lock` among others — so the lock screen is
  --- drawn from the handler, whichever way the request came.
  function login.lock(done) return session_call("Lock", nil, done) end
  function login.unlock(done) return session_call("Unlock", nil, done) end

  --- What a lock screen tells logind once it is up (and down again), so
  --- `loginctl` and other programs can see the session is locked.
  function login.set_locked_hint(on, done)
    return session_call("SetLockedHint", { on == true }, done)
  end
  function login.set_idle_hint(on, done) return session_call("SetIdleHint", { on == true }, done) end

  --- The power actions. `interactive` (default true) lets polkit ask for a
  --- password when the policy wants one — which is why they are never waited
  --- on: the dialog is drawn by the same shell that would be waiting.
  local function power(method)
    return function(interactive, done)
      return manager_call(method, { interactive ~= false }, done)
    end
  end
  login.suspend = power("Suspend")
  login.hibernate = power("Hibernate")
  login.hybrid_sleep = power("HybridSleep")
  login.suspend_then_hibernate = power("SuspendThenHibernate")
  login.reboot = power("Reboot")
  login.power_off = power("PowerOff")

  --- Takes an inhibitor lock. `what` is a colon-separated list ("sleep",
  --- "idle", "handle-lid-switch", ...), `mode` "block" or "delay". Returns a
  --- handle at once, with `release()`; the lock itself arrives a moment
  --- later as logind answers, and `done(handle)` or `done(nil, err)` hears
  --- it. `handle.held` is true from the answer until the release. A handle
  --- released before the answer came releases the lock as it arrives.
  function login.inhibit(what, who, why, mode, done)
    done = done or nothing
    local handle = {
      what = what or "sleep", who = who or "morf", why = why or "", mode = mode or "block",
      held = false, released = false,
    }
    local fd
    function handle.release()
      if handle.released then return false end
      handle.released = true
      handle.held = false
      if fd then pcall(fd.close, fd) end
      fd = nil
      return true
    end
    local sent, err = client.call1_async(name, MANAGER_PATH, MANAGER, "Inhibit",
      { handle.what, handle.who, handle.why, handle.mode }, action_timeout,
      function(reply, failure)
        if reply == nil or type(reply) ~= "userdata" and type(reply) ~= "table" then
          handle.released = true
          return done(nil, failure or "logind answered without a lock")
        end
        fd = reply
        if handle.released then
          pcall(fd.close, fd)
          fd = nil
          return done(nil, "released before it was granted")
        end
        handle.held = true
        done(handle)
      end)
    if not sent then return nil, err end
    return handle
  end

  --- Sets the backlight. `level` is a fraction, 0 to 1, of the device's
  --- maximum; `device` defaults to the one `state.brightness` shows.
  function login.set_brightness(level, device, done)
    local target = device and { name = device } or backlight()
    if not target then return nil, "no backlight" end
    local max = target.max or state.brightness.max
    if not max or max <= 0 then return nil, "backlight has no range" end
    level = math.max(0, math.min(1, tonumber(level) or 0))
    return login.set_brightness_raw(math.floor(level * max + 0.5), target.name, done)
  end

  --- Sets the backlight to a raw value, in the device's own units.
  function login.set_brightness_raw(value, device, done)
    device = device or state.brightness.device
    if device == "" then return nil, "no backlight" end
    local ok, err = session_call("SetBrightness", { "backlight", device, u(math.floor(value)) },
      function(done_ok, failure)
        -- Wrong about what was asked for: read back what the device has.
        if not done_ok then read_brightness() end
        if done then done(done_ok, failure) end
      end)
    if ok and device == state.brightness.device then
      -- sysfs reads back the new value at once on most drivers, but not
      -- all; the state says what was asked for rather than waiting.
      local max = state.brightness.max
      state.brightness.value = math.floor(value)
      state.brightness.percent = max > 0 and math.floor(value * 100 / max + 0.5) or 0
    end
    return ok, err
  end

  client.watch_name(name, function() refresh() end)
  client.on_signal(name, MANAGER_PATH, MANAGER, "PrepareForSleep", function(body)
    local going = dbus_client.first(body) == true
    state.preparing_for_sleep = going
    fire(handlers.sleep, going)
    if not going then read_brightness() end
  end)
  client.on_signal(name, MANAGER_PATH, MANAGER, "PrepareForShutdown", function(body)
    local going = dbus_client.first(body) == true
    state.preparing_for_shutdown = going
    fire(handlers.shutdown, going)
  end)
  client.on_properties(name, MANAGER_PATH, function() read_manager() end)

  refresh()
  -- The session's own signals, once it is known. A session does not change
  -- path for its lifetime, and neither does the shell's.
  if session_path then
    client.on_signal(name, session_path, SESSION, "Lock", function() fire(handlers.lock) end)
    client.on_signal(name, session_path, SESSION, "Unlock", function() fire(handlers.unlock) end)
    client.on_properties(name, session_path, function() read_session() end)
  end
  if options.udev ~= false and morf.udev then
    pcall(morf.udev.subscribe, "backlight", function() read_brightness() end)
  end
  return login
end

return logind
