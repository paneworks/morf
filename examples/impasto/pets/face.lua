-- One pet, drawn in the style the setting names.
--
-- Port of PetFace.qml. The record and mood are arguments rather than read
-- from the service because the shelf and the family draw every pet at once.
-- An egg until it hatches, a star at level fifteen, a "z" beside a sleeper.
-- The blink is timed here, so every style shares one clock and one set of
-- moods.
--
-- A face is two pictures: the creature (an SVG document its style writes),
-- and its open eyes in a band of their own, which the blink squashes
-- vertically about the band's middle -- the Scale the original puts on each
-- eye. Both documents are cached by what they depend on, so a repaint that
-- changes nothing costs a table lookup.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local pets = require("services.pets")
local draw = require("pets.draw")
local kit = require("components.kit")

local face = {}

local styles = {
  creature = require("pets.creature"),
  plush = require("pets.plush"),
  paper = require("pets.paper"),
  pixel = require("pets.pixel"),
}

-- The documents are framed a fifth of the size wider than the pet on every
-- side: crowns, tails and coronas reach past the square, as they do in the
-- original, where nothing clips them.
local PAD = 20

local cache, cached = {}, 0

--- The pet's documents: `{ body, eyes }`, eyes nil when shut.
function face.drawing(style_id, record, mood, size)
  local kind = pets.species_of(record)
  local egg = not (record and (record.hatchedAt or 0) > 0)
  local coat = draw.coat(kind)
  local style = styles[style_id] or styles.creature
  -- The cell of a sprite depends on the drawn size; nothing else does.
  local cell = style_id == "pixel" and math.max(1, math.floor(size / 16 + 0.5)) .. "/" .. size or ""
  local key = table.concat({ style_id, kind.id, coat, mood, tostring(egg), cell }, "|")
  local hit = cache[key]
  if hit then return hit end
  local body, defs, eyes = style.draw { kind = kind, coat = coat, mood = mood, egg = egg, size = size }
  local out = {
    body = draw.document({ -PAD, -PAD, 100 + 2 * PAD, 100 + 2 * PAD }, defs, body),
  }
  if eyes and not egg then
    out.eyes = draw.document({ -PAD, 0, 100 + 2 * PAD, eyes.h }, "", eyes.body)
    out.eye_y, out.eye_h = eyes.y, eyes.h
  end
  -- A palette change mints new keys; now and then start over rather than
  -- keep every coat the wallpaper ever had.
  if cached > 400 then cache, cached = {}, 0 end
  cache[key] = out
  cached = cached + 1
  return out
end

local function value(v)
  if type(v) == "function" then return v() end
  return v
end

--- A pet. `record` and `mood` are functions (default: the pet that is out
--- and its mood), `size` a number or function, `lively` a boolean: only a
--- lively face blinks and shows the sleeper's "z". `style` a function
--- returning a style id (default: the setting).
function face.new(options)
  options = options or {}
  local record = options.record or pets.pet
  local mood = options.mood or pets.mood
  local size = function() return value(options.size or 40) end
  local style = options.style or function() return settings.petStyle end
  local lively = options.lively or false

  local function drawing()
    return face.drawing(style(), record(), mood(), size())
  end
  local function egg()
    local r = record()
    return not (r and (r.hatchedAt or 0) > 0)
  end

  local eyes = ui.Item {
    x = function() return -size() * PAD / 100 end,
    y = function() local d = drawing() return size() * (d.eye_y or 0) / 100 end,
    width = function() return size() * (100 + 2 * PAD) / 100 end,
    height = function() local d = drawing() return math.max(1, size() * (d.eye_h or 1) / 100) end,
    visible = function() return drawing().eyes ~= nil end,
    ui.Image {
      anchors = { fill = true },
      source = function() return drawing().eyes or "" end,
    },
  }

  local children = {
    ui.Image {
      x = function() return -size() * PAD / 100 end,
      y = function() return -size() * PAD / 100 end,
      width = function() return size() * (100 + 2 * PAD) / 100 end,
      height = function() return size() * (100 + 2 * PAD) / 100 end,
      source = function() return drawing().body end,
    },
    eyes,
    -- Level-fifteen star, in a fixed indicator colour.
    kit.text {
      text = "★",
      visible = function() local r = record() return not egg() and (r.level or 1) >= 15 end,
      width = size,
      horizontal_alignment = "center",
      y = function() return -size() * 0.06 end,
      font_size = function() return math.floor(size() * 0.26 + 0.5) end,
      color = theme.color.indicatorWarn,
    },
  }
  if lively then
    children[#children + 1] = kit.text {
      text = "z",
      mono = true,
      visible = function() return mood() == "asleep" and not egg() end,
      x = function() return size() - size() * 0.14 end,
      font_size = function() return math.floor(size() * 0.24 + 0.5) end,
      color = theme.color.textMuted,
    }
    -- Blink timing is part of the character, not a motion token: 2.8 s
    -- open, 70 ms to shut, 110 ms to open again.
    children[#children + 1] = ui.Timer {
      interval = 2980,
      ["repeat"] = true,
      running = function() return mood() ~= "asleep" and not egg() end,
      on_triggered = function()
        morf.animation.play {
          { node = eyes, property = "scale_y", duration = 180, keyframes = {
            { at = 0, value = 1 },
            { at = 70 / 180, value = 0.15, easing = "linear" },
            { at = 1, value = 1, easing = "linear" },
          } },
        }
      end,
    }
  end

  return ui.Item {
    width = size,
    height = size,
    table.unpack(children),
  }
end

local holders = 0

--- The face in its idle bob, hopping when the pet is played with, levels
--- up or is brought out: what the panel and the module's detail wrap their
--- big face in. `rise` is the bob's height, `hop` the hop's.
function face.lively(node, rise, hop)
  holders = holders + 1
  local seen = morf.signal("impasto.pets.hop.seen." .. holders, pets.hops())
  local hopping = false
  local holder
  local function bob()
    if hopping then return end
    morf.animation.play {
      { node = holder, property = "translate_y", duration = 2800, keyframes = {
        { at = 0, value = 0 },
        { at = 0.5, value = -rise, easing = "in_out_sine" },
        { at = 1, value = 0, easing = "in_out_sine" },
      } },
    }
  end
  holder = ui.Item {
    width = function() return node.width end,
    height = function() return node.height end,
    node,
    -- The bob runs while the pet is awake, one pass per tick.
    ui.Timer {
      interval = 2800, ["repeat"] = true,
      running = function() return pets.mood() ~= "asleep" end,
      on_triggered = bob,
    },
    -- A hop pauses the bob for its own length.
    ui.Timer {
      interval = 1,
      running = function() return pets.hops() ~= seen:get() end,
      on_triggered = function()
        seen:set(pets.hops())
        hopping = true
        morf.animation.play {
          on_finished = function() hopping = false end,
          { node = holder, property = "translate_y", duration = 320, keyframes = {
            { at = 0, value = 0 },
            { at = 130 / 320, value = -hop, easing = "out_quad" },
            { at = 1, value = 0, easing = "out_bounce" },
          } },
        }
      end,
    },
  }
  return holder
end

return face
