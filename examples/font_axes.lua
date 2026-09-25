-- Variable font axes: `axes = { FILL = 1, wght = 600 }` on a text node, and
-- a behavior that moves them like any other number.
--
-- A strip of Material Symbols icons: the selected one fills in (FILL 0 -> 1)
-- and grows bolder, the way a navigation rail marks where you are. Below it,
-- a line set at every weight of whichever variable sans is installed.
--
--     morf render examples/font_axes.lua -o axes.png
--     morf test examples/tests/font_axes_spec.lua
--
-- IPC: `select N` picks the Nth icon; `selected` says which it is.

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "font-axes"
morf.surface.anchors = {}
morf.surface.width = 560
morf.surface.height = 260

local ICONS = { "home", "search", "favorite", "notifications", "settings" }
local ICON_FAMILY = "Material Symbols Rounded"

-- What the icon font can move, asked of the machine rather than assumed.
local function has_axis(family, tag)
  for _, axis in ipairs(morf.font_axes(family)) do
    if axis.tag == tag then return true end
  end
  return false
end

local icons_fill = has_axis(ICON_FAMILY, "FILL")

local sans = "sans-serif"
for _, family in ipairs { "Inter Variable", "Inter", "Roboto Flex", "Adwaita Sans", "Noto Sans" } do
  if has_axis(family, "wght") then
    sans = family
    break
  end
end

local selected = morf.signal("font_axes.selected", 1)

morf.ipc.select = function(n)
  n = tonumber(n)
  if n and n >= 1 and n <= #ICONS then selected:set(math.floor(n)) end
  return selected:get()
end
morf.ipc.selected = function() return selected:get() end

local function icon(index, name)
  local on = function() return selected:get() == index end
  local glyph = ui.Text {
    id = "icon-" .. name,
    anchors = { center_in = true },
    text = name,
    font_family = ICON_FAMILY,
    font_size = 48,
    color = function() return on() and "#e8eaf6" or "#9aa0ae" end,
    -- The same keys in every state, so the map moves as one number each.
    axes = function()
      return on() and { FILL = 1, wght = 600, GRAD = 0, opsz = 48 }
        or { FILL = 0, wght = 300, GRAD = 0, opsz = 48 }
    end,
    behavior = {
      axes = { duration = 260, easing = "out_cubic" },
      color = { duration = 220 },
    },
  }
  return ui.Rect {
    id = "tab-" .. name,
    width = 88, height = 88, radius = 44,
    color = function() return on() and "#3a3f4b" or "#00000000" end,
    behavior = { color = { duration = 220, easing = "out_cubic" } },
    glyph,
    ui.MouseArea {
      anchors = { fill = true },
      on_clicked = function() selected:set(index) end,
    },
  }, glyph
end

local tabs, glyphs = {}, {}
for index, name in ipairs(ICONS) do
  tabs[#tabs + 1], glyphs[#glyphs + 1] = icon(index, name)
end

-- Where an icon's fill is right now, part way through its animation or not.
morf.ipc.fill = function(n)
  local glyph = glyphs[tonumber(n) or 0]
  return glyph and glyph.axes.FILL
end

local weights = {}
for weight = 100, 900, 200 do
  weights[#weights + 1] = ui.Text {
    id = "weight-" .. weight,
    text = tostring(weight),
    font_family = sans,
    font_size = 30,
    color = "#e8eaf6",
    axes = { wght = weight },
  }
end

ui.Rect {
  id = "panel",
  anchors = { fill = true },
  color = "#1b1e24",
  radius = 20,
  ui.Column {
    x = 24, y = 20, gap = 18,
    ui.Row { id = "tabs", gap = 14, table.unpack(tabs) },
    ui.Row { id = "weights", gap = 22, table.unpack(weights) },
    ui.Text {
      id = "caption",
      text = (icons_fill and "" or "(no " .. ICON_FAMILY .. " with a FILL axis here) ")
        .. "icons: " .. ICON_FAMILY .. " · weights: " .. sans,
      font_size = 13,
      color = "#9aa0ae",
    },
  },
}
