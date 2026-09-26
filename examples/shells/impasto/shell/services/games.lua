-- The arcade's registry and best scores.
--
-- Port of impasto's GamesService. A game is a file in `games/`, and a row
-- here: the row carries the board size so the island can take its shape
-- before the game is built. Records live in `games.json` beside the
-- settings, written a moment after a round ends rather than on every change.

local theme = require("theme")
local settings = require("services.settings")

local json = morf.json
local fs = morf.fs

local M = {}

-- ------------------------------------------------------------- catalogue --
--
--   id      file name in `games/`, and the key in the record
--   name    proper noun, not translated
--   icon    glyph on its card
--   tint    palette token, never a hex
--   width   board size in pixels; the island adds the strip and padding
--   height
--   unit    points or moves
--   lower   true where fewer is better
M.catalogue = {
  { id = "whack", name = "Whack-a-Mole", icon = "󰣪", tint = "green",
    width = 520, height = 440, unit = "points", lower = false },
  { id = "snake", name = "Snake Sprint", icon = "󱔎", tint = "green",
    width = 520, height = 520, unit = "points", lower = false },
  { id = "flood", name = "Flood Colors", icon = "󰖌", tint = "blue",
    width = 520, height = 560, unit = "moves", lower = true },
  { id = "mini2048", name = "2048 Mini", icon = "󰎠", tint = "blue",
    width = 480, height = 520, unit = "points", lower = false },
  { id = "lights", name = "Lights Out", icon = "󰛨", tint = "yellow",
    width = 460, height = 500, unit = "moves", lower = true },
  -- Its slabs use the tint plus the other three hues, so it takes the one
  -- that stays distinct from green, yellow and blue.
  { id = "hextris", name = "Hextris", icon = "󰋘", tint = "red",
    width = 520, height = 560, unit = "points", lower = false },
  { id = "bots", name = "Bot Bash", icon = "󰚩", tint = "red",
    width = 520, height = 520, unit = "points", lower = false },
  { id = "target", name = "Target Smash", icon = "󰓾", tint = "red",
    width = 560, height = 460, unit = "points", lower = false },
  { id = "space", name = "Space Blaster", icon = "󱓞", tint = "blue",
    width = 520, height = 600, unit = "points", lower = false },
  { id = "tetris", name = "Tetris", icon = "▙", tint = "accent",
    width = 460, height = 600, unit = "points", lower = false },
  { id = "solitaire", name = "Solitaire", icon = "♠", tint = "green",
    width = 760, height = 560, unit = "moves", lower = true },
}

local by_id = {}
for index, item in ipairs(M.catalogue) do
  item.index = index
  by_id[item.id] = item
end

--- The catalogue row for `id`, or nil.
function M.entry(id) return by_id[id] end

--- The game's tint as a colour; inside a binding it follows the palette.
function M.tint_of(id)
  local item = by_id[id]
  local name = item and item.tint or "accent"
  return theme.color[name]()
end

-- ------------------------------------------------------------------ sizes --
--
-- Declared rather than measured, so the island can morph before the panel
-- that fills it exists.
M.shelf_width = 940
M.shelf_height = 196
M.strip_height = 28
M.strip_gap = 12

--- The island's size for `id`, or the shelf's for none.
function M.panel_size(id)
  local item = by_id[id or ""]
  if not item then return M.shelf_width, M.shelf_height end
  return item.width + 2 * theme.panel_padding,
    item.height + M.strip_height + M.strip_gap + 2 * theme.panel_padding
end

-- The game on screen, or "" for the shelf. `last_opened` is where the shelf
-- scrolls back to.
local playing = morf.signal("impasto.games.playing", "")
local last_opened = morf.signal("impasto.games.last_opened", "")

function M.playing() return playing:get() end
function M.last_opened() return last_opened:get() end

function M.open(id)
  if not by_id[id] then return end
  playing:set(id)
  last_opened:set(id)
end

function M.leave() playing:set("") end

-- ---------------------------------------------------------------- records --
--
-- `{ best, plays, playedAt }` per game played; unplayed games have no entry.
-- One revision signal stands for the whole table: records change a few
-- times a round at most, and every reader wants the latest.

M.path = fs.join(settings.dir, "games.json")

local bests = {}
local revision = morf.signal("impasto.games.revision", 0)

local function touched() revision:set(revision:get() + 1) end

function M.record_of(id)
  revision:get()
  return bests[id]
end

function M.played(id) return M.record_of(id) ~= nil end

function M.best_of(id)
  local record = M.record_of(id)
  return record and record.best or 0
end

function M.plays_of(id)
  local record = M.record_of(id)
  return record and record.plays or 0
end

function M.total_plays()
  local sum = 0
  for _, item in ipairs(M.catalogue) do sum = sum + M.plays_of(item.id) end
  return sum
end

--- Played games, most recent first.
function M.ranked()
  local out = {}
  for _, item in ipairs(M.catalogue) do
    if M.played(item.id) then out[#out + 1] = item end
  end
  table.sort(out, function(a, b) return bests[a.id].playedAt > bests[b.id].playedAt end)
  return out
end

--- The most recently played game's id, or "".
function M.last_played()
  local ranked = M.ranked()
  return ranked[1] and ranked[1].id or ""
end

local saving = false
local function save()
  saving = false
  local ok, err = fs.write(M.path, json.encode({ bests = bests }, true))
  if not ok then morf.log("warn", "impasto: could not save games: " .. tostring(err)) end
end

-- Listeners for a new best, as the original's `newBest` signal.
M.on_new_best = {}

--- Records a finished round. Returns true if it beat the best.
function M.record(id, score)
  local item = by_id[id]
  if not item then return false end
  -- Ignore empty rounds. Move-counted puzzles never score zero.
  if score <= 0 and not item.lower then return false end
  local previous = bests[id]
  local better = previous == nil
    or (item.lower and score < previous.best)
    or (not item.lower and score > previous.best)
  bests[id] = {
    best = better and score or previous.best,
    plays = (previous and previous.plays or 0) + 1,
    playedAt = morf.time.now_ms(),
  }
  touched()
  if not saving then
    saving = true
    morf.timer(120, save, false)
  end
  if better then
    for _, listener in ipairs(M.on_new_best) do listener(id, score) end
    -- No announcement for a first score. The island drops a flash while a
    -- panel is open, as the original's OSD does, so this is heard only
    -- when the round ended with the arcade closed.
    if previous ~= nil then
      require("bar.island_state").flash(item.icon, "New best · " .. item.name .. " " .. score, -1)
    end
  end
  return better
end

-- ---------------------------------------------------------------- storage --

-- morf.json hands arrays and objects back as tagged tables; the records
-- are plain Lua values.
local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

local function load()
  local text = fs.read(M.path)
  if not text or text == "" then return end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then
    morf.log("warn", "impasto: games.json is not JSON; starting with no records")
    return
  end
  decoded = plain(decoded)
  for id, record in pairs(type(decoded.bests) == "table" and decoded.bests or {}) do
    if by_id[id] and type(record) == "table" and tonumber(record.best) then
      bests[id] = {
        best = tonumber(record.best),
        plays = tonumber(record.plays) or 1,
        playedAt = tonumber(record.playedAt) or 0,
      }
    end
  end
end

load()

return M
