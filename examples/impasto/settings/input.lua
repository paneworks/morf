-- Settings, Input: the keyboard, the pointer and the cursor (InputSection).
--
-- Kept in impasto's settings (`keyboardLayouts`, `keyboardSwitch`,
-- `keyRepeatRate`, `pointerSensitivity`, `cursorColor`, `cursorSize`,
-- `shakeToFind`) and pushed to Hyprland by services/compositor.lua, on
-- change, at start and after every reload of its configuration; the
-- configuration files themselves are never written. Under another
-- compositor there is nothing to push to: the controls are locked and the
-- page says so.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local tr = require("services.tr")
local compositor = require("services.compositor")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

M.switches = {
  { id = "", label = tr("None") },
  { id = "grp:alt_shift_toggle", label = "Alt+Shift" },
  { id = "grp:win_space_toggle", label = "Super+Space" },
  { id = "grp:caps_toggle", label = tr("Caps Lock") },
}

local NOT_HERE = "Only under Hyprland"
local function away() return not compositor.available() end

--- A row saying why nothing here reaches the compositor, shown only then.
local function unavailable_group(W)
  return setting.group {
    width = W, visible = away,
    setting.row { width = W, label = tr("Not available here"),
      reading = "These are pushed to Hyprland, and this compositor is not Hyprland; they are kept for when it is.",
      control = kit.glyph { glyph = "󰅙", size = 13, color = C.textMuted } },
  }
end

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
  local box = setting.text_box { placeholder = "us", value = settings.keyboardLayouts,
    on_edited = function(text)
      field_text:set(text)
      -- Only what can be a list of xkb layouts ("us,de"); anything else
      -- stays in the field and is not kept.
      local clean = text:gsub("%s+", "")
      if clean:match("^[%w_,%-%(%)]*$") then settings.set("keyboardLayouts", clean ~= "" and clean or "us") end
    end }
  return {
    unavailable_group(W),
    setting.group {
      width = W, title = tr("Layouts"),
      note = tr("Loaded in this order; the first is active at login."),
      hint = "Pushed to Hyprland as input:kb_layout and input:kb_options; its configuration file keeps its own defaults, used again when impasto is not running.",
      setting.row { width = W, label = tr("Layouts"), control = box,
        locked = away, reason = NOT_HERE,
        reading = function()
          local n = #layouts()
          return n <= 1 and "One layout" or (n .. " layouts, comma-separated")
        end },
      setting.row { width = W, label = tr("Switch between them"),
        locked = function() return away() or #layouts() <= 1 end,
        reason = function() return away() and NOT_HERE or tr("Only one layout is loaded") end,
        control = controls.segmented { options = M.switches,
          current = function() return settings.keyboardSwitch end,
          on_selected = function(id) settings.set("keyboardSwitch", id) end } },
    },
    setting.group {
      width = W, title = tr("Typing"), note = tr("How fast a held key repeats, once it has started."),
      setting.slider { width = W, label = tr("Key repeat rate"), from = 10, to = 60, unit = "/s",
        locked = away, reason = NOT_HERE,
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
  chips[#chips + 1] = colour_chip { id = "palette", label = tr("Palette") }
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
    unavailable_group(W),
    setting.group {
      width = W, title = tr("Pointer"),
      note = "Zero is the device's native speed; either side adjusts the acceleration.",
      setting.slider { width = W, label = tr("Sensitivity"), from = -1, to = 1, step = 0.05, decimals = 2,
        locked = away, reason = NOT_HERE,
        value = function() return settings.pointerSensitivity end,
        on_moved = function(v) settings.set("pointerSensitivity", v) end },
    },
    setting.group {
      width = W, title = tr("The cursor"),
      note = "One shape, sharp at any size — Palette follows the wallpaper.",
      hint = "Colour and size are set with Hyprland's setcursor, so every application follows at once. The colour is a cursor theme compiled with hyprcursor-util from cursor-src; without either only the size changes. Shake to find needs the hypr-dynamic-cursors plugin.",
      setting.block { width = W,
        kit.text { text = tr("Cursor colour"), size = theme.size.small, weight = 500 },
        kit.text { text = function() return away() and NOT_HERE or compositor.cursor_note() end,
          size = theme.size.label, color = C.textMuted, width = W - 28, wrap = true,
          visible = function() return away() or compositor.cursor_note() ~= "" end },
        ui.Item { width = W - 28, height = function() return flow.layout_height or 0 end,
          enabled = function() return not away() end,
          opacity = function() return away() and 0.55 or 1 end, flow } },
      setting.slider { width = W, label = tr("Cursor size"), from = 16, to = 48, unit = " px",
        locked = away, reason = NOT_HERE,
        reading = function()
          return settings.cursorSize == 24 and tr("24 px — the default") or (settings.cursorSize .. " px")
        end,
        figure_width = 130,
        value = function() return settings.cursorSize end,
        on_moved = function(v) settings.set("cursorSize", v) end },
      setting.row { width = W, label = "At that size", control = cursor_preview },
      setting.switch_row { width = W, label = tr("Shake to find"),
        locked = function() return away() or not compositor.shake_available() end,
        reason = function()
          return away() and NOT_HERE or "Needs the hypr-dynamic-cursors plugin, which is not loaded"
        end,
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
