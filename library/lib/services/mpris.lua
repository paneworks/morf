-- Media players, over MPRIS, as reactive state a shell can bind to.
--
-- Every player that wants a place in a panel — Spotify, a browser tab, mpv —
-- takes a name under `org.mpris.MediaPlayer2.` on the session bus and
-- publishes one object, `/org/mpris/MediaPlayer2`, with the same two
-- interfaces. This lists those players, picks the one a "now playing" widget
-- should show, and carries the buttons back.
--
--   local media = morf.service("media")          -- or require(...).connect()
--   ui.Text { text = function() return media.state.active.title end }
--   ui.Button { on_clicked = function() media.play_pause() end }
--
-- This module is a registry descriptor (lib/services/registry.lua). The data
-- part says what the generic binder does: every bus name under the prefix
-- is a member, found by `ListNames` and followed by `NameOwnerChanged`; each
-- member gets its own `PropertiesChanged` and `Seeked` subscriptions, made
-- when it appears and closed when it leaves (so a browser that invents a new
-- name per launch leaves nothing behind), and any of them re-reads only that
-- player, debounced. Every player emits on the same path, so the engine
-- routes a signal only to the subscriptions that named its sender.
--
-- `bind`, the descriptor's escape hatch, is what no descriptor field says:
--
-- * Which player is "active": one that is playing — the one that started
--   most recently, if several are — else the one that changed most
--   recently, else the first by name. `set_active(name)` overrides that
--   until the player leaves.
-- * Position is not a property anyone announces: a player says where it was
--   and at what rate, and says again only when it jumps (`Seeked`). So the
--   last reading keeps its time, and `state.active.position` is advanced by
--   a timer (once a second by default) only while the player plays.
--   `position(name)` computes it exactly; the list's positions are as of
--   the last reading.
-- * The buttons do not wait for the player: a hung player cannot freeze the
--   shell. Each returns true once sent (or nil and why not), and the
--   player's own `PropertiesChanged` moves the state.
--
-- `playerctld` is skipped by default: it is a proxy that re-publishes
-- another player, and listing it would show every track twice.

local morf = require("morf")
local dbus_client = require("lib.services.dbus")
local registry = require("lib.services.registry")

local mpris = {}

local PREFIX = "org.mpris.MediaPlayer2."
local PATH = "/org/mpris/MediaPlayer2"
local ROOT_IFACE = "org.mpris.MediaPlayer2"
local PLAYER = "org.mpris.MediaPlayer2.Player"

local typed, x, o = dbus_client.typed, dbus_client.x, dbus_client.o

--- A metadata dictionary as plain fields. Lengths are microseconds on the
--- wire and seconds here; the raw value is kept as `length_us`.
function mpris.metadata(meta)
  meta = type(meta) == "table" and meta or {}
  local length = tonumber(meta["mpris:length"]) or 0
  local text = registry.coerce.list_text
  return {
    track_id = meta["mpris:trackid"] and tostring(meta["mpris:trackid"]) or "",
    title = meta["xesam:title"] and tostring(meta["xesam:title"]) or "",
    artist = text(meta["xesam:artist"], {}),
    album_artist = text(meta["xesam:albumArtist"], {}),
    album = meta["xesam:album"] and tostring(meta["xesam:album"]) or "",
    art_url = meta["mpris:artUrl"] and tostring(meta["mpris:artUrl"]) or "",
    url = meta["xesam:url"] and tostring(meta["xesam:url"]) or "",
    length_us = length,
    length = length / 1e6,
  }
end

local function empty_active()
  return {
    name = "", identity = "", desktop_entry = "", status = "stopped", playing = false,
    title = "", artist = "", album_artist = "", album = "", art_url = "", url = "",
    track_id = "", length = 0, position = 0, rate = 1, volume = 0, shuffle = false,
    loop = "none", can_play = false, can_pause = false, can_go_next = false,
    can_go_previous = false, can_seek = false, can_control = false, can_raise = false,
    -- Writes the player took and then ignored (Spotify's client does this
    -- with Shuffle and LoopStatus): a control for them does nothing there.
    ignores_shuffle = false, ignores_loop = false,
  }
end

local function meta(key, as, extra)
  local spec = { prop = { "Metadata", key }, as = as }
  for name, value in pairs(extra or {}) do spec[name] = value end
  return spec
end

-- One player, read whole. `identity` stays raw: the escape hatch falls back
-- to the name's tail when the root interface does not say.
local READING = {
  name = { meta = "key" },
  identity = { from = "identity", prop = "Identity" },
  desktop_entry = { from = "identity", prop = "DesktopEntry", as = "string" },
  can_raise = { from = "identity", prop = "CanRaise", as = "bool" },
  status = { prop = "PlaybackStatus", as = "lower", default = "stopped" },
  loop = { prop = "LoopStatus", as = "lower", default = "none" },
  shuffle = { prop = "Shuffle", as = "bool" },
  rate = { prop = "Rate", as = "number", default = 1 },
  volume = { prop = "Volume", as = "number" },
  position_us = { prop = "Position", as = "number" },
  position = { prop = "Position", as = "divide", by = 1e6 },
  can_play = { prop = "CanPlay", as = "bool" },
  can_pause = { prop = "CanPause", as = "bool" },
  can_go_next = { prop = "CanGoNext", as = "bool" },
  can_go_previous = { prop = "CanGoPrevious", as = "bool" },
  can_seek = { prop = "CanSeek", as = "bool" },
  can_control = { prop = "CanControl", as = "bool" },
  track_id = meta("mpris:trackid", "string"),
  title = meta("xesam:title", "string"),
  artist = meta("xesam:artist", "list_text"),
  album_artist = meta("xesam:albumArtist", "list_text"),
  album = meta("xesam:album", "string"),
  art_url = meta("mpris:artUrl", "string"),
  url = meta("xesam:url", "string"),
  length_us = meta("mpris:length", "number"),
  length = meta("mpris:length", "divide", { by = 1e6 }),
}

local ROW = {
  "name", "identity", "desktop_entry", "status", "playing", "title", "artist", "album",
  "art_url", "length", "position", "volume", "can_play", "can_pause", "can_go_next",
  "can_go_previous", "can_seek",
}

local function assign(target, values)
  for key, value in pairs(values) do target[key] = value end
end

-- The escape hatch: the active player, its moving position, and the buttons.
local function bind(media)
  local options = media.options
  local prefix = media.descriptor.sets.players.names.prefix
  local timeout = options.timeout_ms or 5000
  local clock = options.clock
  if not clock then
    local elapsed = morf.time.elapsed()
    clock = function() return elapsed:elapsed_ms() end
  end
  local state = media.state
  local ignored = {} -- "busname|Property" -> true: a write it took and ignored
  local pinned
  local sequence = 0 -- orders "most recently changed" without trusting clocks

  media.on_read("players", function(name, reading, previous)
    if reading.identity == nil then
      reading.identity = name:sub(#prefix + 1)
    else
      reading.identity = tostring(reading.identity)
    end
    reading.playing = reading.status == "playing"
    reading.stamp_ms = clock()
    -- "Changed" is a new status or a new track, not a volume nudge.
    if not previous or previous.status ~= reading.status
      or previous.track_id ~= reading.track_id or previous.title ~= reading.title then
      sequence = sequence + 1
      reading.last_active = sequence
    else
      reading.last_active = previous.last_active
    end
  end)

  local function position_of(player)
    local position = player.position_us
    if player.status == "playing" then
      position = position + (clock() - player.stamp_ms) * 1000 * player.rate
    end
    if player.length_us > 0 and position > player.length_us then position = player.length_us end
    if position < 0 then position = 0 end
    return position
  end

  local function choose()
    local players = media.members("players")
    if pinned and players[pinned] then return players[pinned] end
    pinned = nil
    local best
    for _, player in pairs(players) do
      if not best then
        best = player
      elseif player.playing ~= best.playing then
        if player.playing then best = player end
      elseif player.last_active ~= best.last_active then
        if player.last_active > best.last_active then best = player end
      elseif player.name < best.name then
        best = player
      end
    end
    return best
  end

  -- Only the active player's position moves, and only while it plays: an
  -- idle desk is not woken every second to learn that nothing moved.
  local ticker
  local function ensure_ticker()
    local active = choose()
    local playing = active and active.playing
    if playing and not ticker then
      ticker = media.timer(options.tick_ms or 1000, function()
        local now = choose()
        if now and now.playing then
          state.active.position = position_of(now) / 1e6
        else
          media.cancel(ticker)
          ticker = nil
        end
      end, true)
    elseif not playing and ticker then
      media.cancel(ticker)
      ticker = nil
    end
  end

  local function publish()
    ensure_ticker()
    local active = choose()
    if not active then return assign(state.active, empty_active()) end
    local fields = {}
    for key in pairs(empty_active()) do fields[key] = active[key] end
    fields.position = position_of(active) / 1e6
    fields.ignores_shuffle = ignored[active.name .. "|Shuffle"] == true
    fields.ignores_loop = ignored[active.name .. "|LoopStatus"] == true
    assign(state.active, fields)
  end
  media.on_publish(publish)

  local function target(name)
    return media.members("players")[name or ""] or choose()
  end

  local function control(name, method, arguments)
    local player = target(name)
    if not player then return nil, "no player" end
    local player_name = player.name
    return media.client.call_async(player_name, PATH, PLAYER, method, arguments, timeout,
      function() media.reread("players", player_name) end)
  end

  -- A write the player answered and then did not act on, by
  -- "busname|Property". Checked a moment after each write of these.
  local CHECKED = { Shuffle = "shuffle", LoopStatus = "loop" }
  local function set(name, property, value, wanted)
    local player = target(name)
    if not player then return nil, "no player" end
    local player_name = player.name
    return media.client.set_async(player_name, PATH, PLAYER, property, value, timeout, function()
      media.reread("players", player_name)
      local field = CHECKED[property]
      if field == nil or wanted == nil then return end
      media.timer(900, function()
        local now = media.members("players")[player_name]
        if not now then return end
        local key = player_name .. "|" .. property
        local was = ignored[key]
        ignored[key] = now[field] ~= wanted or nil
        if was ~= ignored[key] then publish() end
      end, false)
    end)
  end

  --- The players as plain tables, keyed by bus name.
  function media.players() return media.members("players") end

  --- The active player's bus name, or nil.
  function media.active()
    local player = choose()
    return player and player.name
  end

  --- Makes a player the active one until it leaves (nil to go back to the
  --- automatic choice).
  function media.set_active(name)
    pinned = name
    publish()
  end

  --- Where a player is, in seconds, as of now.
  function media.position(name)
    local player = target(name)
    return player and position_of(player) / 1e6 or 0
  end

  function media.play_pause(name) return control(name, "PlayPause") end
  function media.play(name) return control(name, "Play") end
  function media.pause(name) return control(name, "Pause") end
  function media.stop(name) return control(name, "Stop") end
  function media.next(name) return control(name, "Next") end
  function media.previous(name) return control(name, "Previous") end

  --- Moves by `offset` seconds, forward or back.
  function media.seek(offset, name)
    return control(name, "Seek", { x(math.floor(offset * 1e6)) })
  end

  --- Jumps to `seconds` into the current track. MPRIS wants the track id
  --- with it, so a jump meant for one track cannot land in the next.
  function media.set_position(seconds, name)
    local player = target(name)
    if not player then return nil, "no player" end
    if player.track_id == "" then return nil, "the player names no track" end
    return control(player.name, "SetPosition",
      { o(player.track_id), x(math.floor(seconds * 1e6)) })
  end

  --- Sets the volume, 0 to 1.
  function media.set_volume(volume, name) return set(name, "Volume", typed("d", volume)) end
  function media.set_shuffle(on, name) return set(name, "Shuffle", on == true, on == true) end

  --- "none", "track" or "playlist".
  function media.set_loop(loop, name)
    local word = ({ none = "None", track = "Track", playlist = "Playlist" })[loop] or loop
    return set(name, "LoopStatus", word, string.lower(word))
  end

  --- Brings the player's window forward, if it has one.
  function media.raise(name)
    local player = target(name)
    if not player then return nil, "no player" end
    return media.client.call_async(player.name, PATH, ROOT_IFACE, "Raise", nil, timeout)
  end
end

mpris.descriptor = registry.define {
  name = "media",
  bus = "session",
  debounce_ms = 50,
  interfaces = {
    player = { set = "players", path = PATH, interface = PLAYER, changes = "reread" },
    identity = { set = "players", path = PATH, interface = ROOT_IFACE, changes = "reread",
                 optional = true },
  },
  objects = {
    players = { names = { prefix = PREFIX, ignore = { "playerctld" } }, from = "player",
                fields = READING },
  },
  signals = {
    { from = "player", member = "Seeked" },
  },
  fields = {
    available = { from = "players", meta = "available" },
    count = { from = "players", meta = "count" },
    players = { from = "players", only = ROW },
  },
  state = { active = empty_active() },
  bind = bind,
}

--- Starts watching. Call it while the configuration loads.
---
--- Options: `bus` ("session"), `prefix` (the MPRIS name prefix; a test uses
--- its own so it never sees, or presses, a real player), `ignore` (names
--- after the prefix to skip; `{ "playerctld" }`), `tick_ms` (1000),
--- `debounce_ms` (50), `clock` (a function returning milliseconds; for
--- tests), `timeout_ms` (how long a button waits for the player; 5000),
--- `dbus` (test seam).
function mpris.connect(options)
  options = options or {}
  local objects
  if options.prefix or options.ignore then
    objects = { players = { names = {
      prefix = options.prefix or PREFIX, ignore = options.ignore or { "playerctld" },
    } } }
  end
  return registry.connect("media", {
    dbus = options.dbus, bus = options.bus, debounce_ms = options.debounce_ms, objects = objects,
    tick_ms = options.tick_ms, clock = options.clock, timeout_ms = options.timeout_ms,
  })
end

return mpris
