-- Settings, Input: the keyboard, the pointer and the cursor (InputSection).
--
-- The original stored these and pushed them to Hyprland at login and after
-- every reload. Here they are kept in impasto's own settings only
-- (`keyboardLayouts`, `keyboardSwitch`, `keyRepeatRate`,
-- `pointerSensitivity`, `cursorColor`, `cursorSize`, `shakeToFind`) and
-- nothing is written to the compositor: the page says so, and whoever runs
-- the compositor can read them from the settings file.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

M.switches = {
  { id = "", label = "None" },
  { id = "grp:alt_shift_toggle", label = "Alt+Shift" },
  { id = "grp:win_space_toggle", label = "Super+Space" },
  { id = "grp:caps_toggle", label = "Caps Lock" },
}

local function layouts()
  local out = {}
  for entry in tostring(settings.keyboardLayouts or ""):gmatch("[^,]+") do
    local clean = entry:match("^%s*(.-)%s*$")
    if clean ~= "" then out[#out + 1] = clean end
  end
  return out
end

local function keyboard_part(W)
  local field_text = controls.signal("input.layouts", settings.keyboardLayouts)
  return {
    setting.group {
      width = W, title = "Layouts",
      note = "Loaded in this order; the first is active at login.",
      hint = "Kept in the shell's settings; the compositor's own keyboard configuration is left as it is.",
      setting.field { width = W, label = "Layouts", placeholder = "us",
        reading = function()
          local n = #layouts()
          return n <= 1 and "One layout" or (n .. " layouts, comma-separated")
        end,
        value = settings.keyboardLayouts,
        on_edited = function(text)
          field_text:set(text)
          local clean = text:gsub("%s+", "")
          settings.set("keyboardLayouts", clean ~= "" and clean or "us")
        end },
      setting.row { width = W, label = "Switch between them",
        locked = function() return #layouts() <= 1 end, reason = "Only one layout is loaded",
        control = controls.segmented { options = M.switches,
          current = function() return settings.keyboardSwitch end,
          on_selected = function(id) settings.set("keyboardSwitch", id) end } },
    },
    setting.group {
      width = W, title = "Typing", note = "How fast a held key repeats, once it has started.",
      setting.slider { width = W, label = "Key repeat rate", from = 10, to = 60, unit = "/s",
        value = function() return settings.keyRepeatRate end,
        on_moved = function(v) settings.set("keyRepeatRate", v) end },
    },
  }
end

--- A colour chip: a swatch and a word, ringed when chosen.
local function colour_chip(entry)
  local hovered = controls.signal("input.colour", false)
  local taken = function() return settings.cursorColor == entry.id end
  local label = kit.text { text = entry.label, size = theme.size.small,
    color = function() return taken() and C.accent() or C.text() end }
  local row = ui.Row {
    gap = 7, align = "center",
    ui.Rect { width = 14, height = 14, radius = 7,
      color = entry.id == "palette" and C.accent or entry.id,
      border_width = entry.id == "palette" and 3 or 1,
      border_color = entry.id == "palette" and C.accentText or C.hairline },
    label,
  }
  return ui.Rect {
    width = function() return (row.layout_width or 0) + 24 end, height = 30, radius = 15,
    color = function() return hovered:get() and C.islandSurfaceHover or C.island end,
    border_width = 1,
    border_color = function() return taken() and C.accent() or C.islandBorder end,
    behavior = { color = fast(), border_color = fast() },
    ui.Item { x = 12, anchors = { vertical_center = true },
      width = function() return row.layout_width or 0 end, height = 16, row },
    setting.hit { hovered = hovered, on_click = function() settings.set("cursorColor", entry.id) end },
  }
end

local function pointer_part(W)
  local chips = { direction = "row", wrap = true, gap = 6, align = "start", width = W - 28 }
  chips[#chips + 1] = colour_chip { id = "palette", label = "Palette" }
  for _, entry in ipairs(theme.fixed_colours) do chips[#chips + 1] = colour_chip(entry) end
  local flow = ui.Flex(chips)
  local cursor_preview = ui.Item {
    width = 60, height = 48,
    ui.Path {
      anchors = { center_in = true },
      width = function() return settings.cursorSize end,
      height = function() return settings.cursorSize end,
      view_box = { 0, 0, 24, 24 },
      d = "M4 2 L4 20 L9 15.5 L12.5 22 L15.5 20.5 L12 14 L19 14 Z",
      fill_color = function()
        local id = settings.cursorColor
        if id == "palette" then return C.accent() end
        return id
      end,
      stroke_color = C.island, stroke_width = 1.2, stroke_join = "round",
    },
  }
  return {
    setting.group {
      width = W, title = "Pointer",
      note = "Zero is the device's native speed; either side adjusts the acceleration.",
      setting.slider { width = W, label = "Sensitivity", from = -1, to = 1, step = 0.05, decimals = 2,
        value = function() return settings.pointerSensitivity end,
        on_moved = function(v) settings.set("pointerSensitivity", v) end },
    },
    setting.group {
      width = W, title = "The cursor",
      note = "One shape, sharp at any size — Palette follows the wallpaper.",
      hint = "Kept in the shell's settings for whoever sets the cursor; nothing is pushed to the compositor.",
      setting.block { width = W,
        kit.text { text = "Cursor colour", size = theme.size.small, weight = 500 },
        ui.Item { width = W - 28, height = function() return flow.layout_height or 0 end, flow } },
      setting.slider { width = W, label = "Cursor size", from = 16, to = 48, unit = " px",
        reading = function()
          return settings.cursorSize == 24 and "24 px — the default" or (settings.cursorSize .. " px")
        end,
        figure_width = 130,
        value = function() return settings.cursorSize end,
        on_moved = function(v) settings.set("cursorSize", v) end },
      setting.row { width = W, label = "At that size", control = cursor_preview },
      setting.switch_row { width = W, label = "Shake to find",
        reading = function() return settings.shakeToFind and "Grows while it is shaken" or "Stays its size" end,
        checked = function() return settings.shakeToFind end,
        on_toggled = function(on) settings.set("shakeToFind", on) end },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "keyboard", build = keyboard_part },
    { id = "pointer", build = pointer_part },
  })
end

return M
