-- The player worth showing, flattened.
--
-- Port of MediaService.qml over `lib.mpris`. The library already picks one
-- player: the one playing (the most recent to start), otherwise the most
-- recently changed, so a paused track stays on the island. Position is
-- interpolated by the library and advanced by its own timer while playing,
-- which is the polling the original ran only while something subscribed.
--
-- Transport is wired to clicks only, through `services.act`.

local mpris = require("lib.services.mpris")
local act = require("services.act")

local M = {}

local media = mpris.connect()
M.lib = media
local a = media.state.active

function M.available() return media.state.available and (a.name or "") ~= "" end
function M.playing() return M.available() and a.playing == true end
function M.title() return M.available() and (a.title or "") or "" end
function M.artist() return M.available() and (a.artist or "") or "" end
function M.album() return M.available() and (a.album or "") or "" end
function M.identity() return M.available() and (a.identity or "") or "" end
function M.can_next() return M.available() and a.can_go_next end
function M.can_previous() return M.available() and a.can_go_previous end
function M.can_toggle() return M.available() and (a.can_play or a.can_pause) end

--- A local path for the artwork, or "". Remote art (http) is not fetched
--- here; the placeholder glyph stands in.
function M.art()
  if not M.available() then return "" end
  local url = a.art_url or ""
  if url:sub(1, 7) == "file://" then
    return (url:sub(8):gsub("%%(%x%x)", function(hex) return string.char(tonumber(hex, 16)) end))
  end
  if url:sub(1, 1) == "/" then return url end
  return ""
end

--- Seconds; a stream has no length.
function M.length() return M.available() and (a.length or 0) or 0 end
function M.position() return M.available() and (a.position or 0) or 0 end
function M.seekable() return M.length() > 0 end
function M.can_seek() return M.available() and a.can_seek and M.seekable() end
function M.progress()
  if not M.seekable() then return 0 end
  return math.max(0, math.min(1, M.position() / M.length()))
end

--- "4:04" for a number of seconds.
function M.clock(seconds)
  local total = math.max(0, math.floor(tonumber(seconds) or 0))
  return string.format("%d:%02d", total // 60, total % 60)
end

-- The original counted subscribers to poll the position; the library ticks
-- by itself while playing, so these only keep the call sites.
function M.subscribe() end
function M.release() end

function M.toggle()
  if M.can_toggle() then return act.run("toggling playback", media.play_pause) end
end
function M.next()
  if M.can_next() then return act.run("skipping the track", media.next) end
end
function M.previous()
  if M.can_previous() then return act.run("going back a track", media.previous) end
end
--- `fraction` of the track's length.
function M.seek(fraction)
  if not M.can_seek() then return end
  local seconds = math.max(0, math.min(1, fraction)) * M.length()
  return act.run("seeking", media.set_position, seconds)
end

return M
