-- Pets: a family of small creatures, one out on the bar, the rest on the
-- shelf.
--
-- Port of PetService.qml. Experience comes only from feeding, playing and
-- keeping the pet out on the bar. There is no death and no penalty for
-- neglect: a neglected pet is hungry, lonely or asleep, and a week away costs
-- nothing. Nothing is ever reset either; the family grows by one egg at each
-- level milestone, rolled from the species it lacks.
--
-- Only the pet that is out earns experience and gets hungry; the rest keep
-- their timestamps frozen on the shelf. Everything is stored in one JSON
-- file beside the settings (`pet.json`, the original's own format:
-- `{ family, active }` with `active` counted from zero); moods are derived
-- from timestamps, never stored.
--
-- Records are plain tables: `{ species, name, level, xp, fedAt, playedAt,
-- hatchedAt, restedAt }`. The family is replaced whole on every change and a
-- revision signal is bumped, so a binding that reads anything here follows
-- it. Indices are 1-based in Lua; the file keeps the original's 0-based
-- `active` so a save moves between the two shells.

local settings = require("services.settings")
-- Loaded here, not inside the effects below: a first require runs a
-- module's own top level, which makes signals, and an effect may not.
local bar = require("bar.bar")
local island_state = require("bar.island_state")

local json, fs = morf.json, morf.fs

local M = {}

-- ----------------------------------------------------------------- species --

-- Tints are palette tokens, so each species follows the wallpaper. Labels
-- are proper names and are not translated.
M.species = {
  { id = "dot", label = "Dot", tint = "accent", ears = "round" },
  { id = "sprout", label = "Sprout", tint = "green", ears = "leaf" },
  { id = "ember", label = "Ember", tint = "red", ears = "tuft" },
  { id = "sol", label = "Sol", tint = "yellow", ears = "none" },
  { id = "drift", label = "Drift", tint = "blue", ears = "droop" },
}

--- The species a record is, or the first when it names none we know.
function M.species_of(record)
  local wanted = record and record.species or ""
  for _, kind in ipairs(M.species) do
    if kind.id == wanted then return kind end
  end
  return M.species[1]
end

-- How a creature is drawn, whichever species it is: one file per style in
-- `pets/`, and `settings.petStyle` holds the id. The species decide colour
-- and silhouette, the style decides the finish.
M.styles = {
  { id = "creature", label = "Creature", note = "A different animal for each species." },
  { id = "plush", label = "Plush", note = "One round body, shaded." },
  { id = "paper", label = "Paper", note = "Flat, cut from two tones." },
  { id = "pixel", label = "Pixel", note = "A sprite, sixteen cells across." },
}

-- ------------------------------------------------------------------ state --

M.path = fs.join(settings.dir, "pet.json")

local family = {}
local active = 1
local revision = morf.signal("impasto.pets.revision", 0)

-- The minute clock moods are read through. A signal rather than reading the
-- time inside a binding, so a mood re-derives once a minute and not on
-- every unrelated repaint.
local now = morf.signal("impasto.pets.now", morf.time.now_ms())

local function touch()
  revision:set(revision:get() + 1)
end

--- The family, in the order found.
function M.family()
  revision:get()
  return family
end

--- How many pets there are.
function M.count()
  revision:get()
  return #family
end

--- The index of the pet that is out, clamped to the family.
function M.active_index()
  revision:get()
  return math.max(1, math.min(active, #family))
end

--- The record at `index`, or nil.
function M.record_at(index)
  revision:get()
  return family[index]
end

--- The pet that is out.
function M.pet() return M.record_at(M.active_index()) end

function M.name() local p = M.pet() return p and p.name or "" end
function M.level() local p = M.pet() return p and p.level or 1 end
function M.xp() local p = M.pet() return p and p.xp or 0 end
function M.species_info() return M.species_of(M.pet()) end

--- An egg hatches at its first meal or game (`reward` sets the stamp).
function M.hatched()
  local p = M.pet()
  return p ~= nil and (p.hatchedAt or 0) > 0
end

-- ------------------------------------------------------------- collection --

-- Total family level at which each next egg is laid. Summed across the
-- family, and only the pet that is out earns, so progress means rotating
-- them.
M.milestones = { 6, 16, 30, 50 }

local function total_of(list)
  local sum = 0
  for _, entry in ipairs(list) do sum = sum + entry.level end
  return sum
end

function M.total_level() revision:get() return total_of(family) end
function M.complete() return M.count() >= #M.species end

function M.next_egg_at()
  if M.complete() then return 0 end
  return M.milestones[math.max(1, M.count())]
end

function M.levels_to_next_egg()
  if M.complete() then return 0 end
  return math.max(0, M.next_egg_at() - M.total_level())
end

function M.egg_progress()
  if M.complete() then return 1 end
  local n = M.count()
  local from = n < 2 and 0 or M.milestones[n - 1]
  local span = math.max(1, M.next_egg_at() - from)
  return math.max(0, math.min(1, (M.total_level() - from) / span))
end

-- ----------------------------------------------------------------- events --

-- `celebrated(level)`, `played()`, `laid()`, `brought(index)`: what the
-- faces hop at.
local listeners = { celebrated = {}, played = {}, laid = {}, brought = {} }

--- Listens for one of the events above.
function M.on(event, fn)
  local list = listeners[event]
  list[#list + 1] = fn
end

-- What a face hops at, as a count: a face is a node that can be destroyed,
-- and a node cannot unsubscribe, but a binding on a signal goes when its
-- node does.
local hops = morf.signal("impasto.pets.hops", 0)

--- Bumped when the pet is played with, levels up or is brought out.
function M.hops() return hops:get() end

local function emit(event, ...)
  if event ~= "laid" then hops:set(hops:get() + 1) end
  for _, fn in ipairs(listeners[event]) do
    local ok, err = pcall(fn, ...)
    if not ok then morf.log("warn", "impasto: pet listener failed: " .. tostring(err)) end
  end
end

-- ---------------------------------------------------------------- storage --

local saving = false

local function save()
  saving = false
  local out = {}
  for i, record in ipairs(family) do out[i] = record end
  local ok, err = fs.write(M.path, json.encode({ family = out, active = active - 1 }, true))
  if not ok then morf.log("warn", "impasto: could not save the pets: " .. tostring(err)) end
end

-- Two changes in one turn (a swap is two) are one write, a moment later.
local function schedule_save()
  if saving then return end
  saving = true
  morf.timer(120, save, false)
end

local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

local function number(value, fallback)
  if type(value) ~= "number" then return fallback end
  return math.floor(value)
end

-- Whatever the file held, as a record with every field and whole numbers
-- (JSON numbers come back as floats, which would print as "Lv 3.0").
local function record_from(entry)
  entry = type(entry) == "table" and entry or {}
  return {
    species = type(entry.species) == "string" and entry.species or M.species[1].id,
    name = type(entry.name) == "string" and entry.name or "",
    level = math.max(1, number(entry.level, 1)),
    xp = math.max(0, number(entry.xp, 0)),
    fedAt = number(entry.fedAt, 0),
    playedAt = number(entry.playedAt, 0),
    hatchedAt = number(entry.hatchedAt, 0),
    restedAt = number(entry.restedAt, 0),
  }
end

-- ---------------------------------------------------------------- writing --

-- Species rolled from those the family lacks, when the egg is laid, so the
-- speckles can hint at the coat.
local function newborn(list)
  local taken = {}
  for _, entry in ipairs(list) do taken[entry.species] = true end
  local left = {}
  for _, kind in ipairs(M.species) do
    if not taken[kind.id] then left[#left + 1] = kind end
  end
  local kind = #left > 0 and left[math.random(#left)] or M.species[1]
  return {
    species = kind.id, name = "", level = 1, xp = 0,
    fedAt = 0, playedAt = 0, hatchedAt = 0, restedAt = 0,
  }
end

-- A loop: one reward can cross two milestones.
local function earned(list)
  while #list < #M.species do
    if total_of(list) < M.milestones[#list] then break end
    list[#list + 1] = newborn(list)
  end
  return list
end

local function copy(record, changes)
  local out = {}
  for k, v in pairs(record) do out[k] = v end
  for k, v in pairs(changes or {}) do out[k] = v end
  return out
end

-- ----------------------------------------------------------------- levels --

--- Linear and shallow.
function M.threshold_for(level) return 40 + 20 * level end
function M.threshold() return M.threshold_for(M.level()) end
function M.progress() return math.max(0, math.min(1, M.xp() / M.threshold())) end

--- The one write for a stamp, the experience, level-ups and any eggs laid.
function M.reward(amount, changes)
  local pet = family[M.active_index()]
  if not pet then return end
  local stamp = morf.time.now_ms()
  now:set(stamp)
  local grown = copy(pet, changes)
  if grown.hatchedAt <= 0 then grown.hatchedAt = stamp end
  grown.xp = grown.xp + amount
  local reached = 0
  while grown.xp >= M.threshold_for(grown.level) do
    grown.xp = grown.xp - M.threshold_for(grown.level)
    grown.level = grown.level + 1
    reached = grown.level
  end
  local list = {}
  for i, entry in ipairs(family) do list[i] = entry end
  list[M.active_index()] = grown
  local before = #list
  family = earned(list)
  touch()
  schedule_save()
  if reached > 0 then emit("celebrated", reached) end
  if #family > before then
    emit("laid")
    -- Eggs can arrive while nothing shows the pet, so announce them.
    if M.on_bar() then
      island_state.flash("󰪯", "A new egg on the shelf", -1)
    end
  end
end

-- ------------------------------------------------------------------- care --

-- Generous: feeding and playing are the whole active game.
M.feed_cooldown = 90 * 60000
M.play_cooldown = 5 * 60000

--- Whether the pet is on either side of the bar.
function M.on_bar()
  for _, side in ipairs { "barLeft", "barRight" } do
    local ids = settings.get(side)
    if type(ids) ~= "table" then ids = side == "barLeft" and bar.default_left or bar.default_right end
    for _, entry in ipairs(ids) do
      if entry == "pet" or (type(entry) == "table" and entry.id == "pet") then return true end
    end
  end
  return false
end

-- A zero stamp reads as "just now", so a fresh hatch is not starving.
local function ago_of(stamp)
  if not stamp or stamp <= 0 then return 0 end
  return math.max(0, now:get() - stamp)
end

function M.fed_ago() local p = M.pet() return ago_of(p and p.fedAt) end
function M.played_ago() local p = M.pet() return ago_of(p and p.playedAt) end

function M.can_feed()
  local p = M.pet()
  return p ~= nil and (p.fedAt <= 0 or M.fed_ago() >= M.feed_cooldown)
end

function M.can_play()
  local p = M.pet()
  return p ~= nil and (p.playedAt <= 0 or M.played_ago() >= M.play_cooldown)
end

function M.feed()
  now:set(morf.time.now_ms())
  if not M.can_feed() then return end
  M.reward(25, { fedAt = morf.time.now_ms() })
end

--- Time since the last game pays a capped bonus, so coming back later is
--- worth more than grinding the cooldown.
function M.play()
  now:set(morf.time.now_ms())
  if not M.can_play() then return end
  local bonus = math.min(18, math.floor(M.played_ago() / 3600000) * 3)
  M.reward(12 + bonus, { playedAt = morf.time.now_ms() })
  emit("played")
end

--- Names the pet at `index`; empty means the species' name. `typing`
--- keeps a trailing space (the next word is on its way); the name is
--- trimmed when the typing ends.
function M.rename_at(index, text, typing)
  if index < 1 or index > #family then return end
  text = tostring(text or ""):gsub("^%s+", "")
  if not typing then text = text:gsub("%s+$", "") end
  -- 24 characters, not bytes: never cut a letter in half.
  local cut = utf8 and utf8.offset and utf8.offset(text, 25)
  if cut then text = text:sub(1, cut - 1) end
  local list = {}
  for i, entry in ipairs(family) do list[i] = entry end
  list[index] = copy(family[index], { name = text })
  family = list
  touch()
  schedule_save()
end

function M.rename(text) M.rename_at(M.active_index(), text) end

-- ------------------------------------------------------------------ shelf --

--- The pet going back records when it fell asleep; the one waking has that
--- time added to its stamps, so time on the shelf does not count.
function M.bring_out(index)
  local current = M.active_index()
  if index < 1 or index > #family or index == current then return end
  local stamp = morf.time.now_ms()
  now:set(stamp)
  local list = {}
  for i, entry in ipairs(family) do list[i] = entry end
  list[current] = copy(list[current], { restedAt = stamp })
  local waking = copy(list[index])
  local slept = waking.restedAt > 0 and math.max(0, stamp - waking.restedAt) or 0
  if waking.fedAt > 0 then waking.fedAt = waking.fedAt + slept end
  if waking.playedAt > 0 then waking.playedAt = waking.playedAt + slept end
  waking.restedAt = 0
  list[index] = waking
  family = list
  active = index
  touch()
  schedule_save()
  emit("brought", index)
end

-- ------------------------------------------------------------------- mood --

--- Derived, never stored. Checked in order: long neglect reads as asleep,
--- and hunger outranks loneliness.
function M.mood()
  local fed, played = M.fed_ago(), M.played_ago()
  if fed > 16 * 3600000 and played > 16 * 3600000 then return "asleep" end
  if fed > 6 * 3600000 then return "peckish" end
  if played > 8 * 3600000 then return "lonely" end
  if fed < 3 * 3600000 and played < 2 * 3600000 then return "beaming" end
  return "content"
end

--- Everyone on the shelf is asleep.
function M.mood_at(index)
  if index == M.active_index() then return M.mood() end
  return "asleep"
end

function M.mood_line()
  if not M.hatched() then return "An egg — feed it and it will hatch" end
  local mood = M.mood()
  if mood == "beaming" then return "Beaming" end
  if mood == "peckish" then return "Peckish — a snack is owed" end
  if mood == "lonely" then return "Lonely — nobody has played today" end
  if mood == "asleep" then return "Fast asleep" end
  return "Content"
end

function M.title_of(record)
  if not record or (record.hatchedAt or 0) <= 0 then return "Egg" end
  return record.name ~= "" and record.name or M.species_of(record).label
end

-- ---------------------------------------------------------------- watching --

-- Something drawing the pet (the panel, a desktop face) says so; the minute
-- clock runs while anything does or the pet is on the bar.
local watchers = morf.signal("impasto.pets.watchers", 0)

function M.subscribe() watchers:set(watchers:get() + 1) end
function M.release() watchers:set(math.max(0, watchers:get() - 1)) end

local function watched()
  if watchers:get() > 0 or M.on_bar() then return true end
  local panel = island_state.open_panel()
  return panel == "pet" or panel == "pet.detail"
end

local minute
morf.effect("impasto.pets.clock", function()
  if watched() then
    if not minute then
      -- Caught up at once (from a timer: an effect only reads), then kept
      -- up each minute.
      morf.timer(1, function() now:set(morf.time.now_ms()) end, false)
      minute = morf.timer(60000, function() now:set(morf.time.now_ms()) end, true)
    end
  elseif minute then
    minute:cancel()
    minute = nil
  end
end)

-- A trickle of experience for being out on the bar: only while hatched and
-- awake, so a machine left on overnight earns nothing and an egg never
-- hatches by itself. Keyed on the bar placement, i.e. whether a chip is
-- drawn. (The original also stops while the screen is locked; there is no
-- lock service here yet.)
M.company_every = 5 * 60000
M.company_xp = 1

local company
morf.effect("impasto.pets.company", function()
  local running = M.hatched() and M.on_bar() and M.mood() ~= "asleep"
  if running and not company then
    company = morf.timer(M.company_every, function() M.reward(M.company_xp, nil) end, true)
  elseif not running and company then
    company:cancel()
    company = nil
  end
end)

-- ---------------------------------------------------------------- loading --

-- Loads the saved family. Otherwise migrates a single-pet save (a flat
-- record), or lays the first egg.
local function adopt()
  if math.randomseed then pcall(math.randomseed, morf.time.now_ms()) end
  local text = fs.read(M.path)
  local previous
  if text and text ~= "" then
    local ok, decoded = pcall(json.decode, text)
    if ok and type(decoded) == "table" then previous = plain(decoded) end
  end
  if previous and type(previous.family) == "table" and #previous.family > 0 then
    for i, entry in ipairs(previous.family) do family[i] = record_from(entry) end
    active = math.max(1, math.min(#family, number(previous.active, 0) + 1))
    touch()
    return
  end
  if previous and type(previous.species) == "string" then
    local single = record_from(previous)
    single.restedAt = 0
    family = { single }
  else
    family = { newborn({}) }
  end
  active = 1
  touch()
  schedule_save()
end

adopt()

return M
