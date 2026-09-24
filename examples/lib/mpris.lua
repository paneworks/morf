-- Media players, over MPRIS, as reactive state a shell can bind to.
--
-- Every player that wants a place in a panel — Spotify, a browser tab, mpv —
-- takes a name under `org.mpris.MediaPlayer2.` on the session bus and
-- publishes one object, `/org/mpris/MediaPlayer2`, with the same two
-- interfaces. This lists those players, picks the one a "now playing" widget
-- should show, and carries the buttons back.
--
--   local mpris = require("lib.mpris")
--   local media = mpris.connect()
--   ui.Text { text = function() return media.state.active.title end }
--   ui.Button { on_clicked = function() media.play_pause() end }
--
-- Which player is "active": one that is playing — the one that started most
-- recently, if several are — else the one that changed most recently, else
-- the first by name. `set_active(name)` overrides that until the player
-- leaves.
--
-- Position is not a property anyone announces: a player says where it was
-- and at what rate, and says again only when it jumps (`Seeked`). So the
-- library keeps the last reading and its time, and `state.active.position`
-- is advanced by a timer (once a second by default) while the player plays.
-- `position(name)` computes it exactly, for a caller that wants it now.
--
-- Every player emits on the same path, so a `PropertiesChanged` is told
-- apart by its sender. The engine routes a signal only to the subscriptions
-- that named whoever sent it, so each player's handler hears that player and
-- re-reads only it (debounced). A player's subscriptions are made when it
-- appears and closed when it leaves, so a browser that invents a new name per
-- launch leaves nothing behind.
--
-- The buttons do not wait for the player: a hung player cannot freeze the
-- shell. Each returns true once sent (or nil and why not), and the player's
-- own `PropertiesChanged` moves the state.
--
-- `playerctld` is skipped by default: it is a proxy that re-publishes
-- another player, and listing it would show every track twice.

local morf = require("morf")
local dbus_client = require("lib.dbus_client")

local mpris = {}

local PREFIX = "org.mpris.MediaPlayer2."
local PATH = "/org/mpris/MediaPlayer2"
local ROOT_IFACE = "org.mpris.MediaPlayer2"
local PLAYER = "org.mpris.MediaPlayer2.Player"

local typed, x, o = dbus_client.typed, dbus_client.x, dbus_client.o

local function list_text(value)
  if type(value) == "table" then
    local parts = {}
    for _, each in ipairs(value) do parts[#parts + 1] = tostring(each) end
    return table.concat(parts, ", ")
  end
  return value and tostring(value) or ""
end

--- A metadata dictionary as plain fields. Lengths are microseconds on the
--- wire and seconds here; the raw value is kept as `length_us`.
function mpris.metadata(meta)
  meta = type(meta) == "table" and meta or {}
  local length = tonumber(meta["mpris:length"]) or 0
  return {
    track_id = meta["mpris:trackid"] and tostring(meta["mpris:trackid"]) or "",
    title = meta["xesam:title"] and tostring(meta["xesam:title"]) or "",
    artist = list_text(meta["xesam:artist"]),
    album_artist = list_text(meta["xesam:albumArtist"]),
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
  }
end

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
  local prefix = options.prefix or PREFIX
  local ignore = {}
  for _, each in ipairs(options.ignore or { "playerctld" }) do ignore[prefix .. each] = true end
  local client = dbus_client.new({ dbus = options.dbus, bus = options.bus or "session" })
  local timeout = options.timeout_ms or 5000
  local clock = options.clock
  if not clock then
    local elapsed = morf.elapsed_timer()
    clock = function() return elapsed:elapsed_ms() end
  end

  local state = morf.state({
    available = false,
    count = 0,
    players = {},
    active = empty_active(),
  })

  local media = { state = state }
  local players = {} -- bus name -> reading
  local subscribed = {} -- bus name -> its subscription handles
  local pinned
  local sequence = 0 -- orders "most recently changed" without trusting clocks

  local function assign(target, values)
    for key, value in pairs(values) do target[key] = value end
  end

  local function position_of(player)
    local position = player.position_us
    if player.status == "playing" then
      position = position + (clock() - player.stamp_ms) * 1000 * player.rate
    end
    if player.length_us > 0 and position > player.length_us then position = player.length_us end
    if position < 0 then position = 0 end
    return position
  end

  local schedule

  local function subscribe(name)
    if subscribed[name] then return end
    local changed = function() schedule(name) end
    subscribed[name] = {
      client.on_properties(name, PATH, changed),
      client.on_signal(name, PATH, PLAYER, "Seeked", changed),
    }
  end

  local function unsubscribe(name)
    for _, handle in pairs(subscribed[name] or {}) do handle.close() end
    subscribed[name] = nil
  end

  --- Reads one player whole. Returns nil if it does not answer as a player.
  local function read(name)
    local player = client.get_all(name, PATH, PLAYER)
    if not player then return nil end
    local root = client.get_all(name, PATH, ROOT_IFACE) or {}
    local meta = mpris.metadata(player.Metadata)
    local reading = {
      name = name,
      identity = root.Identity or name:sub(#prefix + 1),
      desktop_entry = root.DesktopEntry or "",
      can_raise = root.CanRaise == true,
      status = string.lower(player.PlaybackStatus or "stopped"),
      loop = string.lower(player.LoopStatus or "none"),
      shuffle = player.Shuffle == true,
      rate = tonumber(player.Rate) or 1,
      volume = tonumber(player.Volume) or 0,
      position_us = tonumber(player.Position) or 0,
      stamp_ms = clock(),
      can_play = player.CanPlay == true,
      can_pause = player.CanPause == true,
      can_go_next = player.CanGoNext == true,
      can_go_previous = player.CanGoPrevious == true,
      can_seek = player.CanSeek == true,
      can_control = player.CanControl == true,
    }
    assign(reading, meta)
    reading.playing = reading.status == "playing"
    return reading
  end

  local function choose()
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

  local function row_of(player)
    return {
      name = player.name, identity = player.identity, desktop_entry = player.desktop_entry,
      status = player.status, playing = player.playing, title = player.title,
      artist = player.artist, album = player.album, art_url = player.art_url,
      length = player.length, position = position_of(player) / 1e6,
      volume = player.volume, can_play = player.can_play, can_pause = player.can_pause,
      can_go_next = player.can_go_next, can_go_previous = player.can_go_previous,
      can_seek = player.can_seek,
    }
  end

  local function publish()
    local list = {}
    for _, player in pairs(players) do list[#list + 1] = player end
    table.sort(list, function(a, b) return a.name < b.name end)
    local rows = {}
    for index, player in ipairs(list) do rows[index] = row_of(player) end
    state.available = #rows > 0
    state.count = #rows
    state.players:replace(rows, "name")
    local active = choose()
    if not active then
      assign(state.active, empty_active())
      return
    end
    local fields = {}
    for key in pairs(empty_active()) do fields[key] = active[key] end
    fields.position = position_of(active) / 1e6
    assign(state.active, fields)
  end

  --- Re-reads the named players (all of them if none named).
  local function refresh(names)
    names = names or players
    for name in pairs(names) do
      local previous = players[name]
      local reading = read(name)
      if reading then
        -- "Changed" is a new status or a new track, not a volume nudge.
        local changed = not previous or previous.status ~= reading.status
          or previous.track_id ~= reading.track_id or previous.title ~= reading.title
        if changed then
          sequence = sequence + 1
          reading.last_active = sequence
        else
          reading.last_active = previous.last_active
        end
        players[name] = reading
      else
        players[name] = nil
      end
    end
    publish()
  end

  -- The players that changed since the last re-read; `schedule()` with no
  -- name means all of them.
  local dirty, dirty_all = {}, false
  local reread = dbus_client.debounce(options.debounce_ms or 50, function()
    local names = dirty
    if dirty_all then names = nil end
    dirty, dirty_all = {}, false
    refresh(names)
  end)
  schedule = function(name)
    if name then dirty[name] = true else dirty_all = true end
    reread()
  end

  local function add(name)
    if ignore[name] or name:sub(1, #prefix) ~= prefix then return end
    subscribe(name)
    refresh({ [name] = true })
  end

  local function target(name)
    local player = players[name or ""] or choose()
    if not player then return nil end
    return player
  end

  local function control(name, method, arguments)
    local player = target(name)
    if not player then return nil, "no player" end
    local player_name = player.name
    return client.call_async(player_name, PATH, PLAYER, method, arguments, timeout,
      function() schedule(player_name) end)
  end

  local function set(name, property, value)
    local player = target(name)
    if not player then return nil, "no player" end
    local player_name = player.name
    return client.set_async(player_name, PATH, PLAYER, property, value, timeout,
      function() schedule(player_name) end)
  end

  --- Whether any player is on the bus.
  function media.available() return state.available end

  --- The players as plain tables, keyed by bus name.
  function media.players() return players end

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
  function media.set_shuffle(on, name) return set(name, "Shuffle", on == true) end

  --- "none", "track" or "playlist".
  function media.set_loop(loop, name)
    local word = ({ none = "None", track = "Track", playlist = "Playlist" })[loop] or loop
    return set(name, "LoopStatus", word)
  end

  --- Brings the player's window forward, if it has one.
  function media.raise(name)
    local player = target(name)
    if not player then return nil, "no player" end
    return client.call_async(player.name, PATH, ROOT_IFACE, "Raise", nil, timeout)
  end

  client.on_owner_changed(function(name, _, new_owner)
    if name:sub(1, #prefix) ~= prefix or ignore[name] then return end
    if new_owner ~= "" then
      add(name)
    else
      unsubscribe(name)
      if players[name] then
        players[name] = nil
        client.forget(PATH, name)
        publish()
      end
    end
  end)

  -- The position, moving. Only the active player's row is advanced; the
  -- list's positions are as of the last reading.
  morf.timer(options.tick_ms or 1000, function()
    local active = choose()
    if active and active.playing then
      state.active.position = position_of(active) / 1e6
    end
  end)

  for _, name in ipairs(client.list_names()) do add(name) end
  publish()
  return media
end

return mpris
