-- The frame: the Tsugumori field (square opening, drawers and the rail
-- filleted into it) dressed as an instrument bezel. Tick rulers run along
-- the inner lip of every band, L brackets mark the opening's corners, and
-- the top and bottom bands carry tiny readouts: a frame code, the output,
-- the minute clock, workspace occupancy cells and the screen size.
--
-- Everything here is static at rest: the rulers are one cached path per
-- edge, the clock moves once a minute and the cells only when a workspace
-- fills or empties. (Brought over from the Futuristic theme.)
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local stripes = require("themes.tsugumori.stripes")
local common = require("themes.kit_common")
local C = theme.color
-- The few drawing helpers the frame uses.
local P = {}
local STRENGTH = { faint = .13, quiet = .24, idle = .4, mark = .72, hot = 1 }
function P.line(strength) local a = STRENGTH[strength] return function() return C.primary:alpha(a) end end
function P.signal() return function() return C.primary end end
function P.ink(kind)
  if kind == "lo" then return function() return C.onSurfaceVariant end end
  return function() return C.primary end
end
P.code = common.code
function P.label(props)
  local text = props.text
  props.micro = nil
  props.text = function() local t = type(text) == "function" and text() or text return tostring(t or ""):upper() end
  props.font_family = props.font_family or theme.font
  props.color = props.color or P.ink("lo")
  return ui.Text(props)
end
function P.hatch(spec)
  return stripes.box { x = spec.x, y = spec.y, width = spec.width, height = spec.height,
    gap = spec.spacing or 6, weight = spec.weight or 1.5, color = spec.color }
end
local V = {}

function V.insets(bar)
  bar = bar or { left = 0, top = 0, right = 0, bottom = 0 }
  return { left = theme.LEFT + bar.left, top = theme.BORDER + bar.top,
    right = theme.BORDER + bar.right, bottom = theme.BORDER + bar.bottom }
end

local PITCH, MAJOR = 8, 10        -- ruler: a tick every PITCH px, every MAJORth long
local CORNER = 26                 -- bracket arm

-- A ruler along one edge of the opening: ticks rise out of the band from
-- the opening's lip. `edge` is top/bottom/left/right; `from`/`to` are
-- fractions of the edge kept clear of ticks (the rail and the level marks
-- live mid-edge on the sides).
local function ruler(model, edge, gaps)
  local B, L = theme.BORDER, theme.LEFT
  local function size() local _, _, w, h = model.desk() return w, h end
  local horizontal = edge == "top" or edge == "bottom"
  local function d(major)
    local w, h = size()
    local a0, a1 = horizontal and L + CORNER + 4 or B + CORNER + 4,
      horizontal and w - B - CORNER - 4 or h - B - CORNER - 4
    local out, k = {}, 0
    for at = a0, a1, PITCH do
      local f = at / (horizontal and w or h)
      local clear = (k % MAJOR == 0) == major
      for _, g in ipairs(gaps or {}) do if f > g[1] and f < g[2] then clear = false end end
      if clear then
        local s = major and 5 or 2
        local p = math.floor(at) + .5
        if edge == "top" then out[#out + 1] = ("M%g %g V%g "):format(p, B, B - s)
        elseif edge == "bottom" then out[#out + 1] = ("M%g %g V%g "):format(p, h - B, h - B + s)
        elseif edge == "left" then out[#out + 1] = ("M%g %g H%g "):format(L, p, L - s)
        else out[#out + 1] = ("M%g %g H%g "):format(w - B, p, w - B + s) end
      end
      k = k + 1
    end
    return #out > 0 and table.concat(out) or "M0 0"
  end
  return ui.Item { id = "tsugumori-frame-ruler-" .. edge, anchors = { fill = true },
    ui.Path { anchors = { fill = true }, d = function() return d(false) end,
      fill_color = "transparent", stroke_color = P.line("idle"), stroke_width = 1 },
    ui.Path { anchors = { fill = true }, d = function() return d(true) end,
      fill_color = "transparent", stroke_color = P.line("mark"), stroke_width = 1 },
  }
end

-- A label set into a band: a patch of the band's own fill behind it so the
-- ruler stops where the text starts.
local function plate(spec)
  local text = spec.text
  local size = spec.font_size or 8
  local width = spec.width
  local node = { id = spec.id, x = spec.x, y = spec.y, anchors = spec.anchors,
    width = width, height = 10,
    ui.Rect { x = -3, width = width + 6, height = 10, color = function() return C.surface end },
  }
  if spec.filled then
    node[#node + 1] = ui.Rect { y = 0, width = width, height = 9, color = spec.color or P.signal("accent") }
  end
  node[#node + 1] = P.label { micro = true, text = text, x = spec.filled and 3 or 0, y = 0, font_size = size,
    width = width - (spec.filled and 3 or 0), height = 12, elide = "right",
    color = spec.filled and function() return C.surface end or (spec.color or P.ink("lo")) }
  return ui.Item(node)
end

-- Workspace occupancy: one cell per workspace of the current ten, lit when
-- it holds windows, the one on show a solid accent block.
local function cells(id)
  local ws = require("services").workspace
  if type(ws) ~= "table" or not ws.active then return ui.Item { id = id, width = 88, height = 10 } end
  local COUNT, CELL, GAP = 10, 7, 2
  local node = { id = id, width = COUNT * (CELL + GAP) - GAP, height = 10,
    anchors = { horizontal_center = true, bottom = true } }
  local function base() local a = ws.active() return a - ((a - 1) % COUNT) end
  for i = 1, COUNT do
    local function n() return base() + i - 1 end
    node[#node + 1] = ui.Rect { x = (i - 1) * (CELL + GAP), y = 3, width = CELL, height = 4,
      color = function()
        if ws.active() == n() then return C.primary end
        return ws.occupied(n()) and C.primary:alpha(.45) or C.primary:alpha(0)
      end,
      border_width = 1, border_color = P.line("mark"),
      behavior = { color = { duration = theme.duration.normal } } }
  end
  return ui.Item(node)
end

local function decorations(model)
  local B, L = theme.BORDER, theme.LEFT
  local function size() local _, _, w, h = model.desk() return w, h end
  local node = { id = "tsugumori-frame-marks", anchors = { fill = true } }
  -- Rulers. The side bands keep their middle third clear for the rail and
  -- the level marks.
  node[#node + 1] = ruler(model, "top", { { .40, .60 } })
  node[#node + 1] = ruler(model, "bottom", { { .44, .56 } })
  node[#node + 1] = ruler(model, "left", { { .30, .70 } })
  node[#node + 1] = ruler(model, "right", { { .30, .70 } })

  -- Corner brackets on the opening, each with a filled stud in the band.
  local function brackets()
    local w, h = size()
    local x0, y0, x1, y1 = L - .5, B - .5, w - B + .5, h - B + .5
    local c = CORNER
    return table.concat {
      ("M%g %g V%g H%g "):format(x0, y0 + c, y0, x0 + c),
      ("M%g %g H%g V%g "):format(x1 - c, y0, x1, y0 + c),
      ("M%g %g V%g H%g "):format(x1, y1 - c, y1, x1 - c),
      ("M%g %g H%g V%g"):format(x0 + c, y1, x0, y1 - c),
    }
  end
  node[#node + 1] = ui.Path { id = "tsugumori-frame-brackets", anchors = { fill = true }, d = brackets,
    fill_color = "transparent", stroke_color = P.line("hot"), stroke_width = 1.5, stroke_cap = "square" }
  for i, at in ipairs { { left = true, top = true }, { right = true, top = true },
      { left = true, bottom = true }, { right = true, bottom = true } } do
    node[#node + 1] = ui.Rect { id = "tsugumori-frame-stud-" .. i, anchors = {
        left = at.left, right = at.right, top = at.top, bottom = at.bottom,
        left_margin = 2, right_margin = 2, top_margin = 2, bottom_margin = 2 },
      width = 4, height = 4, color = P.line("mark") }
  end

  -- Top band: frame code and output on the left, a hatched notice in the
  -- middle, the clock on the right.
  -- Services a stripped-down host may not have: the frame shows what it
  -- can without them.
  local services = require("services")
  local output = type(services.output) == "function" and services.output or function() return "" end
  local top = { id = "tsugumori-frame-top", x = L + CORNER + 10, y = 0,
    width = function() local w = size() return w - L - B - 2 * CORNER - 20 end, height = B }
  top[#top + 1] = ui.Row { gap = 10,
    plate { text = "FRM-01", width = 40, filled = true },
    plate { text = P.code("tsugumori.frame", "SD. ###/##.##"), width = 72 },
    plate { text = function() return "OUT " .. (output() ~= "" and output() or "LOCAL") end, width = 110,
      color = P.ink("accent") },
  }
  top[#top + 1] = ui.Item { anchors = { horizontal_center = true }, width = 244, height = B,
    ui.Rect { x = -4, width = 252, height = B, color = function() return C.surface end },
    P.hatch { x = 0, y = 2, width = 36, height = 6, spacing = 4, weight = 1.5, color = P.line("mark") },
    plate { x = 46, text = P.code("tsugumori.frame.cp", "CP-##/CP-##"), width = 66 },
    plate { x = 122, text = "STAT. NOMINAL", width = 72, color = P.ink("accent") },
    P.hatch { x = 208, y = 2, width = 36, height = 6, spacing = 4, weight = 1.5, angle = "\\", color = P.line("mark") },
  }
  top[#top + 1] = ui.Row { anchors = { right = true }, gap = 8,
    plate { text = function() morf.minute_clock:get() return morf.time.format("%d.%m", morf.time.now()) end, width = 30 },
    plate { text = function() morf.minute_clock:get() return "T " .. morf.time.format("%H:%M", morf.time.now()) end, width = 44,
      filled = true },
  }
  node[#node + 1] = ui.Item(top)

  -- Bottom band: size and a part number on the left, occupancy cells in
  -- the middle, a system code on the right.
  local bottom = { id = "tsugumori-frame-bottom", x = L + CORNER + 10,
    y = function() local _, h = size() return h - B end,
    width = function() local w = size() return w - L - B - 2 * CORNER - 20 end, height = B }
  bottom[#bottom + 1] = ui.Row { gap = 10,
    plate { text = "PT-315", width = 36, filled = true },
    plate { text = function() local w, h = size() return ("%dx%d"):format(w, h) end, width = 70 },
  }
  bottom[#bottom + 1] = ui.Item { anchors = { horizontal_center = true }, width = 168, height = B,
    ui.Rect { x = -4, width = 176, height = B, color = function() return C.surface end },
    P.label { micro = true, text = "WS", x = 0, y = 0, font_size = 8, height = 12 },
    ui.Item { x = 18, width = 88, height = B, cells("tsugumori-frame-cells") },
    P.label { micro = true, x = 116, y = 0, font_size = 8, height = 12, color = P.ink("accent"),
      text = function()
        local ws = require("services").workspace
        if type(ws) ~= "table" or not ws.active then return "" end
        local base = ws.active() - ((ws.active() - 1) % 10)
        local n = 0
        for i = base, base + 9 do if ws.occupied(i) then n = n + 1 end end
        return ("OCC %02d/10"):format(n)
      end },
  }
  bottom[#bottom + 1] = ui.Row { anchors = { right = true }, gap = 8,
    plate { text = P.code("tsugumori.frame.sys", "SYS.SRP ##"), width = 56 },
    plate { text = "ACF", width = 22, filled = true },
  }
  node[#node + 1] = ui.Item(bottom)
  return ui.Item(node)
end

function V.build(model)
  local opening = ui.SdfShape { id = "frame-opening", shape = "box", operation = "subtract", radius = 0,
    x = function() local x = model.desk() return x + theme.LEFT end,
    y = function() local _, y = model.desk() return y + theme.BORDER end,
    width = function() local _, _, w = model.desk() return w - theme.LEFT - theme.BORDER end,
    height = function() local _, _, _, h = model.desk() return h - 2 * theme.BORDER end }
  local transition = require("themes.session").transition
  require("themes.switcher").morph(opening, "radius", 0, transition and transition.frame_rounding)
  local field = { id = "frame", anchors = { fill = true }, fill_color = function() return C.surface end, blend = 0,
    ui.SdfShape { shape = "box", anchors = { fill = true } },
    opening,
  }
  for _, drawer in ipairs(model.drawers) do field[#field + 1] = drawer.shape end
  field[#field + 1] = model.rail.shape
  -- The rail's lip swells the frame at the workspace on show.
  local rail = require("themes.tsugumori.views.rail")
  for _, shape in ipairs(rail.extra_shapes or {}) do field[#field + 1] = shape end
  field[#field + 1] = model.levels.shape
  return require("themes.frame_host")(model, ui.Sdf(field), V.insets(), decorations(model))
end
return V
