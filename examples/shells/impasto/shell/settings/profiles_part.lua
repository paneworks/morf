-- The profiles, one row each in one card, the one in use marked
-- (ProfilesPart). Each row shows the profile's wallpaper and a line on its
-- bar, dock and widgets. Rename, duplicate, export and delete are in the
-- row's menu; delete is confirmed on the row, since it cannot be undone.
--
-- The original's file dialogs have no counterpart in the engine, so export
-- and import take a path in a field, starting in the documents folder.

local ui = require("morf.ui")
local theme = require("theme")
local profiles = require("services.profiles")
local thumbnails = require("services.thumbnails")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local tr = require("services.tr")

local C = theme.color

local M = {}

local function documents()
  local fs = morf.fs
  return fs.dir("documents") or fs.home() or ""
end

--- A row's menu: rename, duplicate, export, delete.
local function menu(W, id, s)
  local items = {
    { id = "rename", label = tr("Rename"), icon = "󰑕" },
    { id = "duplicate", label = tr("Duplicate"), icon = "󰆏" },
    { id = "export", label = tr("Export to a file…"), icon = "󰈝" },
    { id = "delete", label = tr("Delete"), icon = "󰆴", warn = true },
  }
  local row = { gap = 6, align = "center" }
  for _, item in ipairs(items) do
    if not (item.id == "delete" and profiles.active() == id) then
      row[#row + 1] = controls.pill { text = item.label, icon = item.icon, height = 26,
        on_click = function()
          s.menu:set("")
          if item.id == "rename" then s.renaming:set(id)
          elseif item.id == "duplicate" then s.renaming:set(profiles.duplicate(id))
          elseif item.id == "export" then
            s.path:set("")
            s.importing:set(false)
            s.exporting:set(id)
          elseif item.id == "delete" then s.confirming:set(id) end
        end }
    end
  end
  return ui.Row(row)
end

local function profile_row(W, id, s)
  local entry = function() return profiles.entry(id) or { name = "" } end
  local in_use = function() return profiles.active() == id end
  local naming = function() return s.renaming:get() == id end
  local asking = function() return s.confirming:get() == id end
  local menu_open = function() return s.menu:get() == id end
  local picture = function() return profiles.wallpaper_of(id) end

  local text_w = W - 14 - 64 - 12 - 260
  return ui.Item {
    width = W, height = function() return menu_open() and 96 or 62 end,
    setting.wheel_area(),
    ui.ClipRect {
      x = 14, y = 11, width = 64, height = 40, radius = 40 * theme.picture_corner, color = C.island,
      ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source = function() return thumbnails.of(picture(), 128, 80) end },
      kit.glyph { anchors = { center_in = true }, glyph = "󰸉", size = 13, color = C.textMuted,
        visible = function() return picture() == "" end },
    },
    ui.Column {
      x = 14 + 64 + 12, y = 13, gap = 2,
      ui.Item { width = text_w, height = 20,
        kit.text { anchors = { vertical_center = true }, text = function() return entry().name end,
          size = theme.size.small, weight = 600, width = text_w, elide = "right",
          visible = function() return not naming() end },
        ui.Item { visible = naming, width = 220, height = 24, y = -2,
          ui.Loader { active = naming, source = function()
            -- Built each time, with the name in it and the keyboard.
            local box = setting.text_box {
              field_width = 220, field_height = 24, value = entry().name, focus = true,
              max_length = profiles.NAME_LENGTH,
              on_accepted = function(text) profiles.rename(id, text) s.renaming:set("") end,
              on_escape = function() s.renaming:set("") end,
            }
            return box
          end } },
      },
      kit.text {
        width = text_w, elide = "right", size = theme.size.label,
        text = function()
          if asking() then return tr("Deleted for good — there is no undo") end
          if naming() then return tr("Enter to keep the name · Esc to leave it") end
          return profiles.summary(id)
        end,
        color = function() return asking() and C.red() or C.textMuted() end,
      },
    },
    ui.Flex {
      direction = "row",
      anchors = { right = true, right_margin = 10, top = true, top_margin = 17 }, gap = 6, align = "center",
      ui.Rect {
        width = 64, height = 26, radius = 13, color = "#00000000", border_width = 1, border_color = C.accent,
        visible = function() return in_use() and not asking() end,
        kit.text { anchors = { center_in = true }, text = tr("In use"), size = theme.size.small, color = C.accent },
      },
      ui.Item { width = 92, height = 26, visible = function() return not in_use() and not asking() end,
        controls.pill { text = tr("Switch"), icon = "󰁔", height = 26, width = 92,
          on_click = function() s.menu:set("") profiles.switch_to(id) end } },
      ui.Item { width = 84, height = 26, visible = asking,
        controls.pill { text = tr("Delete"), icon = "󰆴", height = 26, width = 84, active = true,
          on_click = function() s.confirming:set("") profiles.remove(id) end } },
      ui.Item { width = 64, height = 26, visible = asking,
        controls.pill { text = tr("Keep"), height = 26, width = 64, on_click = function() s.confirming:set("") end } },
      ui.Item { width = 32, height = 28, visible = function() return not asking() end,
        controls.icon_button { icon = "󰇘", icon_size = 13, active = menu_open,
          on_click = function() s.menu:set(menu_open() and "" or id) end } },
    },
    ui.Item {
      x = 14 + 64 + 12, y = 60, width = W - 90, height = 28, visible = menu_open,
      ui.Loader { active = menu_open, source = function() return menu(W, id, s) end },
    },
  }
end

--- The group, for System's Profiles part: `width`.
function M.build(W)
  local s = {
    menu = controls.signal("profiles.menu", ""),
    renaming = controls.signal("profiles.renaming", ""),
    confirming = controls.signal("profiles.confirming", ""),
    exporting = controls.signal("profiles.exporting", ""),
    importing = controls.signal("profiles.importing", false),
    path = controls.signal("profiles.path", ""),
    said = controls.signal("profiles.said", ""),
  }
  local rows = morf.list_model({})
  local function refresh()
    local out = {}
    for _, item in ipairs(profiles.list()) do out[#out + 1] = { key = item.id } end
    return out
  end
  local watcher = setting.watch(function()
    local next = refresh()
    morf.timer(1, function() rows:replace(next, "key") end, false)
  end)
  rows:replace(refresh(), "key")

  local list = ui.Repeater {
    as = "flex", direction = "column", align = "start", width = W, gap = 0,
    model = rows,
    delegate = function(row)
      return ui.Item {
        width = W,
        profile_row(W, row.key, s),
        ui.Rect { x = 0, y = 0, width = W, height = 1, color = C.islandBorder,
          visible = function() return (profiles.list()[1] or {}).id ~= row.key end },
      }
    end,
  }

  -- Export and import take a path; the field opens under the list, on the
  -- documents folder.
  local function default_path()
    local e = profiles.entry(s.exporting:get())
    if e then return morf.fs.join(documents(), (e.name:gsub("[/%c]", "-")) .. ".json") end
    return documents() .. "/"
  end
  local path_holder = ui.Item {
    width = 300, height = 30,
    ui.Loader {
      active = function() return s.exporting:get() ~= "" or s.importing:get() end,
      source = function()
        local first = default_path()
        return (setting.text_box {
          field_width = 300, value = first, focus = true,
          placeholder = "A path to a .json file",
          on_edited = function(text) s.path:set(text) end,
          on_escape = function() s.exporting:set("") s.importing:set(false) end,
        })
      end,
    },
  }
  local function act()
    local path = s.path:get() ~= "" and s.path:get() or default_path()
    if s.exporting:get() ~= "" then
      local ok, where = profiles.export_to(s.exporting:get(), path)
      s.said:set(ok and ("Written to " .. tostring(where)) or ("Not written: " .. tostring(where)))
      s.exporting:set("")
    elseif s.importing:get() then
      local id, why = profiles.import_from(path)
      s.said:set(id and "Imported" or ("Not imported: " .. tostring(why)))
      s.importing:set(false)
    end
  end
  local asking_path = function() return s.exporting:get() ~= "" or s.importing:get() end

  return setting.group {
    width = W, title = tr("Profiles"),
    note = tr("Changes are saved to the profile in use as you make them."),
    hint = "A profile holds the bar, the desktop widgets and the notes on the edges, the dock, the launcher, the control centre, the look and the wallpaper with its palette. Your name, picture, screens and keyboard stay the same across all of them.",
    ui.Item { width = W, height = function() return list.layout_height or 0 end, watcher, list },
    setting.block { width = W, padding = 10,
      ui.Row { gap = 8,
        controls.pill { text = tr("New profile"), icon = "󰐕", height = 28,
          on_click = function() s.menu:set("") s.renaming:set(profiles.create()) end },
        controls.pill { text = tr("Import…"), icon = "󰋺", height = 28,
          on_click = function()
            s.menu:set("")
            s.exporting:set("")
            s.path:set("")
            s.importing:set(true)
          end },
      },
      kit.text { text = function() return s.said:get() end, size = theme.size.label, color = C.textMuted,
        visible = function() return s.said:get() ~= "" end },
    },
    setting.row {
      width = W, visible = asking_path,
      label = function()
        if s.importing:get() then return "Import from" end
        local e = profiles.entry(s.exporting:get())
        return "Export " .. (e and e.name or "") .. " to"
      end,
      control = ui.Row { gap = 8, align = "center",
        path_holder,
        controls.pill { text = "Go", height = 30, active = true, on_click = act },
        controls.pill { text = tr("Cancel"), height = 30,
          on_click = function() s.exporting:set("") s.importing:set(false) end },
      },
    },
  }
end

return M
