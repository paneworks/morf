-- The notes module: how many notes, and the latest three.
--
-- Port of NotesModule.qml. Its detail is the three most recently edited
-- notes, one line each with their tint, New and Open. A row opens its
-- note; editing needs the keyboard, so it happens in the notes panel.
--
-- On the bar "notes" is not this module but the notes panel's door button,
-- as in the original; the module is for the desktop and the settings.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local modules = require("services.modules")
local notes = require("services.notes")
local kit = require("components.kit")
local pill = require("components.pill")

local C = theme.color
local module = {}

module.width, module.height = 356, 150

local function open_on(key)
  if key and key ~= "" then notes.open(key) end
  island.open("notes")
end

local function row(i, width)
  local hovered = kit.hover_signal("notes.row")
  local note = function() return notes.live()[i] end
  local age = kit.text {
    anchors = { right = true, right_margin = 8, vertical_center = true },
    text = function() local n = note() return n and notes.age_of(n.edited) or "" end,
    mono = true, size = theme.size.label, color = C.textMuted,
  }
  return ui.Item {
    width = width, height = 24,
    visible = function() return note() ~= nil end,
    ui.Rect {
      anchors = { fill = true }, radius = theme.radius_small,
      color = function() return hovered:get() and C.islandSurfaceHover or "#00000000" end,
      behavior = { color = theme.behave("fast") },
    },
    ui.Rect {
      x = 6, anchors = { vertical_center = true }, width = 8, height = 8, radius = 4,
      color = function() local n = note() return notes.tint_color(n and n.tint or "yellow") end,
    },
    kit.text {
      x = 24, anchors = { vertical_center = true },
      width = function() return width - 24 - 16 - (age.layout_width or 0) end,
      elide = "right", size = theme.size.small,
      text = function() local n = note() return n and notes.title_of(n) or "" end,
      color = function() return notes.is_empty(note()) and C.textMuted() or C.text() end,
    },
    age,
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() local n = note() if n then open_on(n.key) end end,
    },
  }
end

--- The detail, at the module's declared size.
function module.detail()
  local W = module.width - 8 - 28
  local rows = {}
  for i = 1, 3 do rows[i] = row(i, W) end
  return ui.Item {
    anchors = { fill = true },
    ui.Column {
      x = 14, y = 12, gap = 8,
      ui.Item {
        width = W, height = 36,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
          kit.glyph { text = "󰎞", size = 20, color = C.accent },
          ui.Column {
            gap = 1,
            kit.text { text = "Notes", size = theme.size.medium, weight = 600 },
            kit.text {
              size = theme.size.small, color = C.textMuted,
              text = function()
                local n, a = notes.count(), #notes.archived()
                if n == 0 then return "Nothing written down yet" end
                local text = n .. (n == 1 and " note" or " notes")
                if a > 0 then text = text .. " · " .. a .. " archived" end
                return text
              end,
            },
          },
        },
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 6,
          pill.button { text = "New", icon = "󰐕", on_click = function()
            notes.create("yellow", false)
            island.open("notes")
          end },
          pill.button { text = "Open", icon = "󰏫", on_click = function() open_on("") end },
        },
      },
      table.unpack(rows),
    },
  }
end

-- On the bar "notes" is the notes panel's door button (ModuleService.qml
-- 77-83): an id is a button before it is a module. The module is what the
-- desktop and the settings know.
modules.define("notes", {
  glyph = function() return "󰎞" end,
  value = function() return tostring(notes.count()) end,
  has = function() return true end,
  detail = module.detail,
})

-- `morf ipc call module.notes` opens the detail.
morf.ipc["module.notes"] = function()
  modules.activate("notes")
  return modules.open_id:get()
end

return module

