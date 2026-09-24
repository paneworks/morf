-- What every game shares: the keys, a few colour sums, the board's ground,
-- and the two ways a hit is drawn (a burst and the points it paid).
--
-- Port of GameBoard's contract, Burst.qml and Pop.qml. A game is a file in
-- this folder returning `build(ctx)`; `ctx` carries
--
--   width, height   the board's size from the catalogue
--   tint()          the game's colour (a binding follows the palette)
--   score, over     signals the frame reads: set them, never replace them
--   finished(n)     call once, when the round ends
--   on_screen()     whether this game is still the one showing
--   live()          whether a tick should run (the round is on and the
--                   game still on screen); every game timer asks first
--
-- and `build` returns `{ node = ..., key = function(keysym, text) ... end,
-- restart = function() ... end }`. `key` returns true when it used the key.
-- A game draws only its board; the frame draws the score strip, the best,
-- the game-over card and the restart button, and Escape and R are the
-- panel's.

local ui = require("morf.ui")
local theme = require("theme")

local C = theme.color
local common = {}

-- ------------------------------------------------------------------ keys --

common.K = {
  ESCAPE = 0xff1b, RETURN = 0xff0d, KP_ENTER = 0xff8d, SPACE = 0x20,
  LEFT = 0xff51, UP = 0xff52, RIGHT = 0xff53, DOWN = 0xff54,
  HOME = 0xff50, END = 0xff57, BACKSPACE = 0xff08, TAB = 0xff09,
}
local K = common.K

--- The digit a key stands for (top row or keypad), or nil.
function common.digit(keysym)
  if keysym >= 0x30 and keysym <= 0x39 then return keysym - 0x30 end
  if keysym >= 0xffb0 and keysym <= 0xffb9 then return keysym - 0xffb0 end
  return nil
end

--- The key as a lower-case letter, or nil: `W` and `w` are one key here,
--- as Qt's key codes make them.
function common.letter(keysym)
  if keysym >= 0x61 and keysym <= 0x7a then return string.char(keysym) end
  if keysym >= 0x41 and keysym <= 0x5a then return string.char(keysym + 32) end
  return nil
end

--- Arrows and WASD as a direction: dx, dy, or nil.
function common.direction(keysym, wasd)
  if keysym == K.UP then return 0, -1 end
  if keysym == K.DOWN then return 0, 1 end
  if keysym == K.LEFT then return -1, 0 end
  if keysym == K.RIGHT then return 1, 0 end
  if wasd then
    local l = common.letter(keysym)
    if l == "w" then return 0, -1 end
    if l == "s" then return 0, 1 end
    if l == "a" then return -1, 0 end
    if l == "d" then return 1, 0 end
  end
  return nil
end

function common.confirm(keysym)
  return keysym == K.RETURN or keysym == K.KP_ENTER
end

-- ---------------------------------------------------------------- colour --

local function clamp01(v) return v < 0 and 0 or v > 1 and 1 or v end

--- Qt.rgba(c.r, c.g, c.b, a).
function common.alpha(c, a)
  c = morf.color(c)
  return morf.color.rgb(c.r, c.g, c.b, a)
end

--- Qt.darker: the HSV value divided by `factor`.
function common.darker(c, factor)
  c = morf.color(c)
  return morf.color.rgb(c.r / factor, c.g / factor, c.b / factor, c.a)
end

--- Qt.lighter: the value times `factor`; what overflows white takes
--- saturation out instead, as Qt's does.
function common.lighter(c, factor)
  c = morf.color(c)
  local r, g, b = c.r * factor, c.g * factor, c.b * factor
  local over = math.max(r, g, b) - 1
  if over > 0 then r, g, b = r + over, g + over, b + over end
  return morf.color.rgb(clamp01(r), clamp01(g), clamp01(b), c.a)
end

--- `a` moved towards `b` by `t`, in plain RGB as the Canvas code did it.
function common.mix(a, b, t)
  a, b = morf.color(a), morf.color(b)
  return morf.color.rgb(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t,
    a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
end

--- `c` at `alpha` laid over `ground`, as one opaque colour.
---
--- morf blends in linear light and Qt in sRGB, so a faint colour over a
--- dark ground comes out several times brighter here than in the
--- original: white at 3.5% over the island's grey is a clear line instead
--- of a hint of one. Where the ground under a translucent colour is known,
--- the two are mixed here, in sRGB, the way the original saw them.
function common.over(c, alpha, ground)
  return common.mix(morf.color(ground or C.islandSurface), common.alpha(c, 1), alpha)
end

--- A face in colour `c` with the light on it, top to bottom: white at
--- `top`, white at `middle` (at `at`), black at `bottom`.
---
--- The original laid a translucent gradient over the face; blended in
--- linear light that reads as a grey film on a dark tile, so here the
--- light is mixed into the face's own gradient, in sRGB (see
--- `common.over`). A behaviour on `gradient` moves it like a colour.
function common.lit(c, top, middle, bottom, at)
  return {
    angle = 180,
    stops = {
      { common.mix(c, "#ffffff", top), 0 },
      { common.mix(c, "#ffffff", middle), at or 0.5 },
      { common.mix(c, "#000000", bottom), 1 },
    },
  }
end

-- ---------------------------------------------------------------- random --

--- An integer in [0, n).
function common.random(n) return math.floor(math.random() * n) end

-- ---------------------------------------------------------------- ground --

--- The board's ground: the island's surface, bordered, rounded.
function common.ground(values)
  values.radius = values.radius or theme.radius_medium
  values.color = values.color or C.islandSurface
  values.border_color = values.border_color or C.islandBorder
  values.border_width = values.border_width or 1
  return ui.Rect(values)
end

--- `count` nodes from `build(i)`, each built on a budget of its own.
---
--- A handler, a Loader's source and a delegate each get a fixed number of
--- Lua instructions, and a board of two hundred blocks with their bindings
--- is more than one of them. A Repeater builds each row in a call of its
--- own, so a board built through one costs one cell at a time. The cells
--- are built a moment after the board, so whatever writes to them must
--- allow for one that is not there yet. Children are placed by their own
--- `x` and `y`.
function common.cells(count, build, values)
  local rows = {}
  for i = 1, count do rows[i] = { id = i } end
  values = values or {}
  values.model = morf.list_model(rows)
  values.delegate = function(row) return build(row.id) end
  return ui.Repeater(values)
end

--- Plays one property from `from` to `to`.
function common.kick(node, property, from, to, duration, easing)
  morf.animation.play {
    { node = node, property = property, from = from, to = to,
      duration = duration, easing = easing or "out_back" },
  }
end

-- ----------------------------------------------------------------- burst --

--- A hit, drawn: a ring opening out of the point and a handful of sparks
--- thrown out of it, both fading as they go. One per place a hit can
--- happen, replayed rather than created, so a round builds nothing.
---
--- `tint` (a colour or a binding), `sparks`, `spread`, `span`. Returns the
--- node (an Item `2 * spread` square, placed by its owner) and `play()`.
function common.burst(values)
  local spread = values.spread or 26
  local count = values.sparks or 7
  local span = values.span or 320
  local tint = values.tint or C.accent
  local size = spread * 2
  local ring = ui.Rect {
    x = 0, y = 0, width = size, height = size, radius = spread,
    color = "#00000000",
    border_color = tint,
    border_width = math.max(1, spread * 0.14),
    opacity = 0, scale = 0,
  }
  local dot = math.max(1, spread * 0.18)
  local sparks = {}
  for i = 1, count do
    sparks[i] = ui.Rect {
      x = spread - dot / 2, y = spread - dot / 2,
      width = dot, height = dot, radius = dot / 2,
      color = tint, opacity = 0,
    }
  end
  local node = ui.Item {
    x = values.x, y = values.y,
    width = size, height = size,
    ring, table.unpack(sparks),
  }
  local function play()
    local tracks = {
      { node = ring, property = "scale", from = 0, to = 1, duration = span, easing = "out_cubic" },
      { node = ring, property = "opacity", from = 1, to = 0, duration = span, easing = "out_cubic" },
    }
    for i, spark in ipairs(sparks) do
      local angle = (i - 1) * (math.pi * 2 / count)
      local cx, cy = math.cos(angle) * spread, math.sin(angle) * spread
      tracks[#tracks + 1] = { node = spark, property = "translate_x", from = cx * 0.3, to = cx * 1.3, duration = span, easing = "out_cubic" }
      tracks[#tracks + 1] = { node = spark, property = "translate_y", from = cy * 0.3, to = cy * 1.3, duration = span, easing = "out_cubic" }
      tracks[#tracks + 1] = { node = spark, property = "scale", from = 1, to = 0.5, duration = span, easing = "out_cubic" }
      tracks[#tracks + 1] = { node = spark, property = "opacity", from = 1, to = 0, duration = span, easing = "out_cubic" }
    end
    morf.animation.play { { parallel = tracks } }
  end
  return node, play
end

-- ------------------------------------------------------------------- pop --

--- What a hit was worth: a figure that rises off it and fades. Returns the
--- text node (placed by its owner; `x`, `y` are its top left) and
--- `play(figure)`.
function common.pop(values)
  local span = values.span or 640
  local rise = values.rise or 26
  local width = values.width or 60
  local node = ui.Text {
    x = values.x, y = values.y, width = width,
    horizontal_alignment = "center",
    text = "",
    font_family = function() return theme.font_mono() end,
    font_size = theme.size.small,
    font_weight = 600,
    color = values.tint or C.accent,
    opacity = 0,
  }
  local function play(figure)
    node.text = figure
    morf.animation.play { { parallel = {
      { node = node, property = "translate_y", from = 0, to = -rise, duration = span, easing = "out_cubic" },
      -- Full for the first part of the rise, then fading: 1 - phase, sped
      -- up, as the original's min(1, (1 - phase) * 2.4).
      { sequential = {
        { node = node, property = "opacity", from = 1, to = 1, duration = span * 0.45 },
        { node = node, property = "opacity", from = 1, to = 0, duration = span * 0.55, easing = "out_quad" },
      } },
      { sequential = {
        { node = node, property = "scale", from = 1.35, to = 1, duration = span * 0.2, easing = "out_cubic" },
      } },
    } } }
  end
  return node, play
end

return common
