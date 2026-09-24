-- Settings, Launcher: what it lists and how tall it gets, its sigils, and
-- the clipboard history, which is one of its modes (LauncherSection).
--
-- The sigils are shown, not edited: the launcher's modes are declared in
-- services/launcher.lua with their prefix, and nothing reads a changed one
-- from the settings yet.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local launcher = require("services.launcher")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local kept = require("settings.kept_applications")

local C = theme.color

local M = {}

--- The list at the chosen length, as a skeleton: names, not stripes.
local function skeleton(W)
  local inner = W - 28 - 24
  local widths = { 0.42, 0.3, 0.55, 0.36, 0.48, 0.26, 0.4, 0.33, 0.5, 0.29, 0.44, 0.38, 0.31, 0.47 }
  local rows = {
    direction = "column", gap = 5, align = "start", width = inner,
    ui.Row { gap = 8, align = "center",
      kit.glyph { glyph = "󰍉", size = 11, color = C.textMuted },
      ui.Rect { width = 90, height = 8, radius = 4, color = C.textMuted, opacity = 0.5 } },
    ui.Rect { width = inner, height = 1, color = C.hairline },
  }
  for i = 1, 14 do
    rows[#rows + 1] = ui.Rect {
      width = inner, height = 18, radius = 5,
      visible = function() return i <= settings.launcherResults end,
      color = i == 1 and C.islandSurfaceHover or "#00000000",
      ui.Rect { x = 6, y = 4, width = 10, height = 10, radius = 3, color = C.islandSurfaceHover },
      ui.Rect { x = 24, y = 6, width = math.floor(inner * widths[i]), height = 6, radius = 3,
        color = C.textMuted, opacity = i == 1 and 0.8 or 0.4 },
    }
  end
  local list = ui.Flex(rows)
  return ui.Rect {
    width = W - 28, height = function() return (list.layout_height or 0) + 20 end,
    radius = theme.radius_medium, color = C.island, border_width = 1, border_color = C.islandBorder,
    ui.Item { x = 12, y = 10, width = inner, height = function() return list.layout_height or 0 end, list },
  }
end

local function results_part(W)
  local kept_rows = kept.rows(W)
  kept_rows.width = W
  kept_rows.title = "Kept applications"
  kept_rows.note = "They lead the list, and the dock keeps them too."
  return {
    setting.group {
      width = W, title = "The list",
      note = "What the launcher lists with nothing typed, and how tall it gets.",
      hint = "By use ranks applications by launches, with older launches counting for less; those kept on the dock come first until something else is used more.",
      setting.row { width = W, label = "Order",
        control = controls.segmented {
          options = { { id = "recent", label = "By use" }, { id = "alphabetical", label = "Alphabetical" } },
          current = function() return settings.launcherOrder end,
          on_selected = function(id) settings.set("launcherOrder", id) end } },
      setting.slider { width = W, label = "Results shown", from = 4, to = 14,
        value = function() return settings.launcherResults end,
        reading = function() return settings.launcherResults .. " rows" end,
        on_moved = function(v) settings.set("launcherResults", v) end },
      setting.switch_row { width = W, label = "As tall as the answer",
        reading = function() return settings.launcherFits and "Only as tall as it needs" or "A fixed box" end,
        checked = function() return settings.launcherFits end,
        on_toggled = function(on) settings.set("launcherFits", on) end },
      setting.block { width = W, skeleton(W) },
    },
    setting.group(kept_rows),
  }
end

local function sigils_part(W)
  local group = {
    width = W, title = "Sigils",
    note = "Plain text searches applications, and a sigil in front switches mode.",
    hint = "Each mode's sigil is set where the launcher declares its modes.",
  }
  for _, mode in ipairs(launcher.modes) do
    group[#group + 1] = setting.row {
      width = W, label = mode.label, reading = mode.hint,
      control = ui.Rect {
        width = mode.prefix == "" and 44 or 34, height = 28, radius = theme.radius_small,
        color = C.island, border_width = 1, border_color = C.islandBorder,
        kit.text { anchors = { center_in = true }, text = mode.prefix == "" and "abc" or mode.prefix,
          mono = true, size = theme.size.small, color = C.accent },
      },
    }
  end
  return { setting.group(group) }
end

local function clipboard_part(W)
  local off = function() return not settings.clipboardHistory end
  local reason = "No history is being kept"
  return {
    setting.group {
      width = W, title = "Clipboard history",
      note = "Everything copied, searchable from the launcher.",
      hint = "Copies from password managers are never stored, and turning the history off stops the watcher entirely.",
      setting.switch_row { width = W, label = "Keep a history",
        reading = function() return settings.clipboardHistory and "Watching" or "Nothing kept" end,
        checked = function() return settings.clipboardHistory end,
        on_toggled = function(on) settings.set("clipboardHistory", on) end },
      setting.slider { width = W, label = "Entries kept", from = 20, to = 500, step = 10,
        locked = off, reason = reason,
        value = function() return settings.clipboardKeep end,
        on_moved = function(v) settings.set("clipboardKeep", v) end },
      setting.switch_row { width = W, label = "Keep images", locked = off, reason = reason,
        reading = function() return settings.clipboardImages and "Pictures as well as text" or "Text only" end,
        checked = function() return settings.clipboardImages end,
        on_toggled = function(on) settings.set("clipboardImages", on) end },
      setting.switch_row { width = W, label = "Empty it on lock", locked = off, reason = reason,
        reading = function() return settings.clipboardWipeOnLock and "Thrown away on lock" or "Kept across a lock" end,
        checked = function() return settings.clipboardWipeOnLock end,
        on_toggled = function(on) settings.set("clipboardWipeOnLock", on) end },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "results", build = results_part },
    { id = "sigils", build = sigils_part },
    { id = "clipboard", build = clipboard_part },
  })
end

return M
