-- The right panel's Bar page, behind the Bar tile's ">": whether the bar is
-- up (on, off, or up only on a narrow screen), and which edge it is on.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local bar = require("bar")

local C = theme.color
local M = {}

local function choice(id, icon, name, on, pick, width)
  local area
  area = ui.MouseArea {
    id = id, width = width, height = 70, cursor = "pointer",
    on_clicked = pick,
    ui.Rect {
      anchors = { fill = true },
      radius = function() return on() and 14 or 22 end,
      color = function()
        local base = on() and C.primary or C.surfaceContainerHighest
        if area and area.hovered then return base:mix(on() and C.onPrimary or C.onSurface, 0.08) end
        return base
      end,
      behavior = { color = { duration = theme.duration.small }, radius = kit.spring(260, 16) },
    },
    ui.Column {
      anchors = { center_in = true }, gap = 4, align = "center",
      kit.icon(icon, 22, function() return on() and C.onPrimary or C.onSurfaceVariant end),
      kit.text { text = name, font_size = theme.size.small, font_weight = 600,
        color = function() return on() and C.onPrimary or C.onSurface end },
    },
  }
  return area
end

function M.page(w, h)
  local function card(height, children)
    children.width, children.height, children.radius = w, height, 22
    return kit.card(children)
  end
  local function label(text) return kit.text { x = 18, y = 14, text = text, font_size = theme.size.small,
    color = function() return C.onSurfaceVariant end } end

  local shows = {
    { "on", "visibility", "On" },
    { "off", "visibility_off", "Off" },
    { "auto", "smartphone", "Auto" },
  }
  local show_buttons = {}
  local sw = (w - 36 - 16) / 3
  for _, s in ipairs(shows) do
    show_buttons[#show_buttons + 1] = choice("bar-show-" .. s[1], s[2], s[3], function()
      local v = config.get("bar.enabled")
      if s[1] == "auto" then return v ~= "on" and v ~= "off" end
      return v == s[1]
    end, function() config.set("bar.enabled", s[1]) end, sw)
  end

  local sides = {
    { "top", "vertical_align_top", "Top" },
    { "bottom", "vertical_align_bottom", "Bottom" },
    { "left", "align_horizontal_left", "Left" },
    { "right", "align_horizontal_right", "Right" },
  }
  local side_buttons = {}
  local bw = (w - 36 - 24) / 4
  for _, s in ipairs(sides) do
    side_buttons[#side_buttons + 1] = choice("bar-side-" .. s[1], s[2], s[3],
      function() return bar.side() == s[1] end, function() bar.set_side(s[1]) end, bw)
  end

  return ui.Flickable {
    width = w, height = h, clip = true,
    ui.Column {
      gap = 12, width = w,
      card(128, {
        id = "bar-show",
        label("Show the bar"),
        ui.Row { x = 18, y = 42, gap = 8, table.unpack(show_buttons) },
      }),
      card(128, {
        id = "bar-position",
        label("Position"),
        ui.Row { x = 18, y = 42, gap = 8, table.unpack(side_buttons) },
      }),
      kit.text {
        width = w, wrap = true, font_size = theme.size.small,
        color = function() return C.onSurfaceVariant end,
        text = "Auto puts the bar up on a narrow screen -- a phone -- and keeps it down on a desk.",
      },
    },
  }
end

return M
