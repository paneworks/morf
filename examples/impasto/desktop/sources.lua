-- What the desk's faces read, one table per module.
--
-- The faces of the original read the same services as the bar
-- (services.audio, battery, brightness, network, bluetooth, media, tasks,
-- notes, timer, updates). Each is asked for with `pcall(require, ...)`, and
-- where one cannot load a small reader over the same library stands in, so
-- the desk still draws something true. Every reader is a plain function, so
-- a binding that calls it follows it.

local settings = require("services.settings")
local theme = require("theme")

local S = {}

local function optional(name)
  local ok, mod = pcall(require, name)
  if ok and type(mod) == "table" then return mod end
  return nil
end

-- A library's state, connected once and only when a face asks.
local connected = {}
local function lib_state(name)
  if connected[name] == nil then
    local ok, lib = pcall(require, "lib." .. name)
    local okc, handle = false, nil
    if ok and lib and lib.connect then okc, handle = pcall(lib.connect) end
    connected[name] = okc and handle or false
  end
  return connected[name] or nil
end

local function call(mod, name, ...)
  local fn = mod and mod[name]
  if type(fn) ~= "function" then return nil end
  local ok, value = pcall(fn, ...)
  if ok then return value end
  return nil
end

-- ------------------------------------------------------------------- clock --

S.clock = {}
--- The time now, as `morf.time.date` fields; a binding follows the minute
--- (the second with `clockShowsSeconds`).
function S.clock.now()
  if settings.clockShowsSeconds then morf.clock:get() else morf.minute_clock:get() end
  return morf.time.date()
end
function S.clock.format(pattern)
  if pattern:find("%%[STr]") then morf.clock:get() else morf.minute_clock:get() end
  return morf.time.format(pattern)
end
--- The clock's own format, with seconds when the setting says.
function S.clock.pattern()
  local format = settings.clockFormat
  if settings.clockShowsSeconds and not format:find("%%S") then
    format = format:gsub("%%M", "%%M:%%S", 1)
  end
  return format
end

-- ----------------------------------------------------------------- battery --

S.battery = {}
local battery_service = optional("services.battery")
local function power() return lib_state("upower") end
local function display()
  local p = power()
  return p and p.state.available and p.state.display or nil
end
function S.battery.available()
  if battery_service then return call(battery_service, "available") == true end
  local d = display()
  return d ~= nil and d.present == true
end
function S.battery.percent()
  if battery_service then return call(battery_service, "percent") or 0 end
  local d = display()
  return d and math.floor((d.percentage or 0) + 0.5) or 0
end
function S.battery.charging()
  if battery_service then return call(battery_service, "charging") == true end
  local d = display()
  return d ~= nil and d.charging == true
end
function S.battery.estimate()
  if battery_service then return call(battery_service, "estimate") or "" end
  local d = display()
  if not d then return "" end
  local ok, up = pcall(require, "lib.upower")
  local fmt = ok and up.format_time or function(s) return math.floor(s / 60) .. " min" end
  if d.charging and (d.time_to_full or 0) > 0 then return fmt(d.time_to_full) .. " to full" end
  if not d.charging and (d.time_to_empty or 0) > 0 then return fmt(d.time_to_empty) .. " left" end
  if d.state == "fully_charged" then return "full" end
  return d.charging and "charging" or ""
end
function S.battery.watts()
  if battery_service then return call(battery_service, "watts") or 0 end
  local d = display()
  return d and math.abs(d.energy_rate or 0) or 0
end
function S.battery.energy()
  if battery_service then return call(battery_service, "energy") or 0 end
  return 0
end
--- Fixed indicator hues, so a colour means the same level on every palette.
function S.battery.tint()
  if S.battery.charging() then return theme.color.indicatorGood end
  local p = S.battery.percent()
  if p <= 10 then return theme.color.indicatorBad end
  if p <= 20 then return theme.color.indicatorWarn end
  return theme.color.text()
end
local LEVELS = { "󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹" }
function S.battery.icon()
  if battery_service and battery_service.icon then return call(battery_service, "icon") or "󰁹" end
  if not S.battery.available() then return "󱉝" end
  if S.battery.charging() then return "󰂄" end
  return LEVELS[math.floor(S.battery.percent() / 10) + 1] or "󰁹"
end

-- ------------------------------------------------------------------- audio --

S.audio = {}
local audio_service = optional("services.audio")
local function sink()
  local a = morf.audio
  if not a or not a.default_sink then return nil end
  local ok, d = pcall(a.default_sink)
  return ok and d or nil
end
function S.audio.available()
  if audio_service then return call(audio_service, "ready") == true end
  return sink() ~= nil
end
function S.audio.volume()
  if audio_service then return call(audio_service, "volume") or 0 end
  local d = sink()
  return d and math.floor((d.volume or 0) * 100 + 0.5) or 0
end
function S.audio.muted()
  if audio_service then return call(audio_service, "muted") == true end
  local d = sink()
  return d ~= nil and d.muted == true
end
function S.audio.icon()
  if audio_service then return call(audio_service, "icon") or "󰕾" end
  if S.audio.muted() then return "󰖁" end
  local v = S.audio.volume()
  return v < 34 and "󰕿" or v < 67 and "󰖀" or "󰕾"
end
function S.audio.set_volume(percent)
  percent = math.max(0, math.min(100, percent))
  if audio_service and audio_service.set_volume then return audio_service.set_volume(percent) end
  local d = sink()
  if d then pcall(morf.audio.set_volume, d.id, percent / 100) end
end
function S.audio.toggle_mute()
  if audio_service and audio_service.toggle_mute then return audio_service.toggle_mute() end
  local d = sink()
  if d then pcall(morf.audio.set_mute, d.id, not d.muted) end
end

-- -------------------------------------------------------------- brightness --

S.brightness = {}
local brightness_service = optional("services.brightness")
local function backlight()
  local l = lib_state("logind")
  return l and l.state.available and l.state.brightness or nil
end
function S.brightness.available()
  if brightness_service then return call(brightness_service, "available") == true end
  local b = backlight()
  return b ~= nil and (b.max or 0) > 0
end
function S.brightness.percent()
  if brightness_service then return call(brightness_service, "percent") or 0 end
  local b = backlight()
  return b and math.floor((b.percent or 0) + 0.5) or 0
end
function S.brightness.icon()
  local p = S.brightness.percent()
  return p < 34 and "󰃞" or p < 67 and "󰃟" or "󰃠"
end
function S.brightness.set(percent)
  percent = math.max(1, math.min(100, percent))
  if brightness_service and brightness_service.set_percent then return brightness_service.set_percent(percent) end
  local l = lib_state("logind")
  if l then pcall(l.set_brightness, percent / 100) end
end

-- ----------------------------------------------------------------- network --

S.network = {}
local network_service = optional("services.network")
local function nm() local n = lib_state("networkmanager") return n and n.state.available and n.state or nil end
function S.network.online()
  if network_service then return call(network_service, "online") == true end
  local s = nm()
  return s ~= nil and (s.connectivity == "full" or s.connectivity == "limited" or s.connectivity == "portal")
end
function S.network.wifi()
  local s = nm()
  return s ~= nil and s.wifi.connected == true
end
function S.network.strength()
  if network_service and network_service.strength then return call(network_service, "strength") or 0 end
  local s = nm()
  return s and s.wifi.strength or 0
end
function S.network.name()
  if network_service and network_service.connection_name then return call(network_service, "connection_name") or "" end
  local s = nm()
  if not s then return "Offline" end
  if s.wifi.connected then return s.wifi.ssid or "Wi-Fi" end
  if s.wired.connected then return "Wired" end
  return s.primary and s.primary.id ~= "" and s.primary.id or "Not connected"
end
function S.network.state_line()
  if network_service and network_service.state_line then return call(network_service, "state_line") or "" end
  local s = nm()
  if not s then return "no NetworkManager" end
  if s.wifi.connected then return string.format("Wi-Fi · %d%%", s.wifi.strength or 0) end
  if s.wired.connected then return "Ethernet" end
  return s.wifi_enabled and "disconnected" or "Wi-Fi off"
end
function S.network.icon()
  if network_service and network_service.icon then return call(network_service, "icon") or "󰤨" end
  local s = nm()
  if not s then return "󰤮" end
  if s.wired.connected and not s.wifi.connected then return "󰈀" end
  if not s.wifi.connected then return "󰤮" end
  local q = s.wifi.strength or 0
  return q < 25 and "󰤟" or q < 50 and "󰤢" or q < 75 and "󰤥" or "󰤨"
end

-- --------------------------------------------------------------- bluetooth --

S.bluetooth = {}
local bluetooth_service = optional("services.bluetooth")
local function bz() local b = lib_state("bluez") return b and b.state or nil end
function S.bluetooth.available()
  if bluetooth_service then return call(bluetooth_service, "available") == true end
  local s = bz()
  return s ~= nil and s.available == true and (s.adapter or "") ~= ""
end
function S.bluetooth.enabled()
  if bluetooth_service then return call(bluetooth_service, "enabled") == true end
  local s = bz()
  return s ~= nil and s.powered == true
end
--- The connected devices: `{ name, battery }`.
function S.bluetooth.devices()
  if bluetooth_service and bluetooth_service.connected_devices then
    local out = {}
    for _, d in ipairs(call(bluetooth_service, "connected_devices") or {}) do
      out[#out + 1] = { name = d.alias or d.name or d.address or "", battery = d.battery }
    end
    return out
  end
  local s = bz()
  local out = {}
  if not s then return out end
  for i = 1, s.devices:len() do
    local d = s.devices:get(i)
    if d.connected then out[#out + 1] = { name = d.alias or d.name or d.address, battery = d.battery } end
  end
  return out
end
function S.bluetooth.summary()
  if bluetooth_service and bluetooth_service.summary then return call(bluetooth_service, "summary") or "" end
  if not S.bluetooth.available() then return "Unavailable" end
  if not S.bluetooth.enabled() then return "Off" end
  local list = S.bluetooth.devices()
  if #list == 0 then return "No devices" end
  if #list == 1 then return list[1].name end
  return #list .. " devices"
end
function S.bluetooth.icon()
  if not S.bluetooth.enabled() then return "󰂲" end
  return #S.bluetooth.devices() > 0 and "󰂱" or "󰂯"
end

-- ------------------------------------------------------------------- media --

S.media = {}
local media_service = optional("services.media")
local function player()
  local m = lib_state("mpris")
  if not m or not m.state.available then return nil end
  local a = m.state.active
  return (a.name or "") ~= "" and a or nil
end
function S.media.available()
  if media_service then return call(media_service, "available") == true end
  return player() ~= nil
end
function S.media.playing()
  if media_service then return call(media_service, "playing") == true end
  local a = player()
  return a ~= nil and a.playing == true
end
local function field(name, service_name)
  return function()
    if media_service then return call(media_service, service_name or name) or "" end
    local a = player()
    return a and a[name] or ""
  end
end
S.media.title = field("title")
S.media.artist = field("artist")
S.media.identity = field("identity")
S.media.album = field("album")
--- A local path for the art, or "".
function S.media.art()
  if media_service and media_service.art then return call(media_service, "art") or "" end
  local a = player()
  local url = a and a.art_url or ""
  if url:sub(1, 7) == "file://" then
    return (url:sub(8):gsub("%%(%x%x)", function(h) return string.char(tonumber(h, 16)) end))
  end
  if url:sub(1, 1) == "/" then return url end
  return ""
end
function S.media.progress()
  if media_service and media_service.progress then return call(media_service, "progress") or 0 end
  local a = player()
  if not a or (a.length or 0) <= 0 then return 0 end
  return math.max(0, math.min(1, (a.position or 0) / a.length))
end
local function transport(name, lib_name)
  return function()
    if media_service and media_service[name] then return media_service[name]() end
    local m = lib_state("mpris")
    if m and m[lib_name] then pcall(m[lib_name]) end
  end
end
S.media.toggle = transport("toggle", "play_pause")
S.media.next = transport("next", "next")
S.media.previous = transport("previous", "previous")

-- ------------------------------------------------------------------- timer --


-- The bar's countdown (services.timer), else a small one of the desk's own.
S.timer = {}
local timer_service = optional("services.timer")
local own = {
  ends = morf.signal("impasto.desk.timer.ends", 0),      -- epoch ms; 0 when idle
  total = morf.signal("impasto.desk.timer.total", 0),
  left = morf.signal("impasto.desk.timer.left", 0),      -- ms left while held
  label = morf.signal("impasto.desk.timer.label", ""),
}
local function own_left()
  if own.left:get() > 0 then return own.left:get() end
  local ends = own.ends:get()
  if ends == 0 then return 0 end
  morf.clock:get()
  return math.max(0, ends - morf.time.now_ms())
end
function S.timer.running()
  if timer_service then return call(timer_service, "running") == true end
  return own.ends:get() > 0 or own.left:get() > 0
end
function S.timer.paused()
  if timer_service then return call(timer_service, "paused") == true end
  return own.left:get() > 0
end
function S.timer.label()
  if timer_service then return call(timer_service, "label") or "" end
  return own.label:get()
end
function S.timer.progress()
  if timer_service then return call(timer_service, "progress") or 0 end
  local total = own.total:get()
  return total > 0 and S.timer.running() and own_left() / total or 0
end
function S.timer.display()
  if timer_service then return call(timer_service, "display") or "" end
  local s = math.ceil(own_left() / 1000)
  local h, m = math.floor(s / 3600), math.floor(s % 3600 / 60)
  if h > 0 then return string.format("%d:%02d:%02d", h, m, s % 60) end
  return string.format("%d:%02d", m, s % 60)
end
function S.timer.start(ms, label)
  if timer_service and timer_service.start then return timer_service.start(ms, label or "") end
  own.total:set(ms)
  own.left:set(0)
  own.label:set(label or "")
  own.ends:set(morf.time.now_ms() + ms)
  local mine = own.ends:get()
  morf.timer(ms, function()
    if own.ends:get() == mine then own.ends:set(0) end
  end, false)
end
function S.timer.toggle()
  if timer_service and timer_service.toggle then return timer_service.toggle() end
  if own.left:get() > 0 then
    local left = own.left:get()
    own.left:set(0)
    own.ends:set(morf.time.now_ms() + left)
    local mine = own.ends:get()
    morf.timer(left, function() if own.ends:get() == mine then own.ends:set(0) end end, false)
  elseif own.ends:get() > 0 then
    own.left:set(own_left())
    own.ends:set(0)
  end
end
function S.timer.cancel()
  if timer_service and timer_service.cancel then return timer_service.cancel() end
  own.ends:set(0)
  own.left:set(0)
end

-- ----------------------------------------------------------------- updates --

-- Pending packages: the updates service (UpdatesService), else a reader over
-- `lib.packages`. Without either the faces say "cannot check".
S.updates = {}
local updates_service = optional("services.updates")
local packages_ok, packages = pcall(require, "lib.packages")
if not packages_ok then packages = nil end
local packages_handle = nil
local function updates_now()
  if not packages then return { available = false, count = 0, packages = {} } end
  if packages_handle == nil then
    local ok, made = pcall(function()
      if packages.new then return packages.new {} end
      return nil
    end)
    packages_handle = ok and made or false
  end
  if not packages_handle then return { available = false, count = 0, packages = {} } end
  local ok, value = pcall(function()
    if packages_handle.get then return packages_handle:get() end
    return packages_handle
  end)
  if not ok or type(value) ~= "table" then return { available = false, count = 0, packages = {} } end
  return value
end
function S.updates.available()
  if updates_service then return call(updates_service, "available") == true end
  local now = updates_now()
  if now.available ~= nil then return now.available == true end
  return type(now.managers) == "table" and #now.managers > 0
end
function S.updates.checking()
  if updates_service then return call(updates_service, "checking") == true end
  return updates_now().checking == true
end
function S.updates.count()
  if updates_service then return tonumber(call(updates_service, "count")) or 0 end
  local now = updates_now()
  return tonumber(now.total or now.count) or 0
end
--- Names of pending packages, up to `n`, across the managers.
function S.updates.packages(n)
  if updates_service then return call(updates_service, "names", n) or {} end
  local now = updates_now()
  local out = {}
  for _, manager in ipairs(now.managers or {}) do
    for _, p in ipairs((now[manager] or {}).updates or {}) do
      if #out >= n then return out end
      out[#out + 1] = type(p) == "table" and (p.name or "") or tostring(p)
    end
  end
  return out
end
--- "5 min ago", "just now", or "" before the first check.
function S.updates.age() return updates_service and call(updates_service, "age") or "" end
--- A face on screen keeps the count checked and the age moving.
function S.updates.subscribe() if updates_service then call(updates_service, "subscribe") end end
function S.updates.release() if updates_service then call(updates_service, "release") end end

-- ------------------------------------------------------------------- tasks --

-- The board's tasks (services.tasks).
S.tasks = {}
local tasks_service = optional("services.tasks")
function S.tasks.available() return tasks_service ~= nil end
function S.tasks.service() return tasks_service end
local function day_key(y, m, d) return string.format("%04d-%02d-%02d", y, m, d) end
S.tasks.day_key = day_key
function S.tasks.today_key()
  local now = S.clock.now()
  return day_key(now.year, now.month, now.day)
end
--- Days of a month with tasks due: a set of day numbers, `{ [11] = "pending" | "done" }`.
function S.tasks.days_with_tasks(year, month)
  if not tasks_service then return {} end
  if tasks_service.days_with_tasks then
    local got = call(tasks_service, "days_with_tasks", year, month)
    if type(got) == "table" then
      local out = {}
      for day, v in pairs(got) do
        if type(v) == "table" then out[day] = (v.pending or 0) > 0 and "pending" or "done"
        else out[day] = v end
      end
      return out
    end
  end
  local out = {}
  local days = morf.time.days_in_month(year, month)
  for d = 1, days do
    local key = day_key(year, month, d)
    local count = call(tasks_service, "count_on", key) or 0
    if count > 0 then
      out[d] = (call(tasks_service, "pending_on", key) or 0) > 0 and "pending" or "done"
    end
  end
  return out
end
function S.tasks.pending_on(key) return tasks_service and call(tasks_service, "pending_on", key) or 0 end
function S.tasks.count_on(key) return tasks_service and call(tasks_service, "count_on", key) or 0 end
function S.tasks.pending() return tasks_service and call(tasks_service, "pending") or 0 end
function S.tasks.count() return tasks_service and call(tasks_service, "count") or 0 end
function S.tasks.overdue()
  local list = tasks_service and call(tasks_service, "overdue")
  return type(list) == "table" and #list or (tonumber(list) or 0)
end
function S.tasks.summary() return tasks_service and call(tasks_service, "summary") or "" end
--- The next `n` open tasks, soonest first: `{ key, text, done, due }`.
function S.tasks.queue(n)
  local list = tasks_service and call(tasks_service, "queue") or {}
  local out = {}
  for _, t in ipairs(type(list) == "table" and list or {}) do
    if #out >= n then break end
    out[#out + 1] = t
  end
  return out
end
--- The tasks due on a day.
function S.tasks.on(key)
  local list = tasks_service and call(tasks_service, "on", key) or {}
  return type(list) == "table" and list or {}
end
--- The next unfinished task due from today on, or nil.
function S.tasks.next() return tasks_service and call(tasks_service, "next") or nil end
function S.tasks.due_label(day) return tasks_service and call(tasks_service, "due_label", day) or (day or "") end
function S.tasks.toggle(key) if tasks_service and tasks_service.toggle then pcall(tasks_service.toggle, key) end end

-- ------------------------------------------------------------------- notes --

-- The notes (services.notes). A note on the desk is read-only: a click
-- opens it in the island.
S.notes = {}
local notes_service = optional("services.notes")
function S.notes.available() return notes_service ~= nil end
--- The note a row names, or the newest: `{ key, title, body, items, updated }`.
function S.notes.note_for(row)
  if not notes_service then return nil end
  -- NotesService.noteFor: the note the row names while it is not archived,
  -- else the newest.
  local key = row and row.note or ""
  local named = key ~= "" and call(notes_service, "entry", key) or nil
  if type(named) == "table" and not named.archived then return named end
  local note = call(notes_service, "newest")
  return type(note) == "table" and note or nil
end
function S.notes.open(key)
  if notes_service and notes_service.open then pcall(notes_service.open, key) end
  local ok, island = pcall(require, "bar.island")
  if ok then pcall(island.open, "notes") end
end
function S.notes.create()
  if notes_service and notes_service.create then pcall(notes_service.create) end
  local ok, island = pcall(require, "bar.island")
  if ok then pcall(island.open, "notes") end
end

-- ----------------------------------------------------------- pets, games --

S.pets = require("services.pets")
S.games = require("services.games")

return S
