-- The applications kept on the dock (KeptApplications): one row each, with
-- arrows to reorder and a button to let go, and a field that finds an
-- application to keep. The dock and the launcher's list both lead with
-- these.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local app_icon = require("components.app_icon")
local dock = require("services.dock")
local launcher = require("services.launcher")
local tr = require("services.tr")

local C = theme.color

local M = {}

local function app_of(id)
  for _, app in ipairs(launcher.applications()) do
    if app.id == id then return app end
  end
  return nil
end

--- A group's rows: `width`. Returns a list of nodes for `setting.group`.
function M.rows(width)
  local rows = {}
  local MAX = 12
  local count = function() return #dock.pinned() end
  for i = 1, MAX do
    local id = function() return dock.pinned()[i] or "" end
    rows[#rows + 1] = setting.row {
      width = width,
      visible = function() return i <= count() end,
      label = function() local app = app_of(id()) return app and app.name or id() end,
      reading = function() return id() ~= "" and (id() .. ".desktop") or "" end,
      control = ui.Row {
        gap = 4, align = "center",
        controls.icon_button { icon = "󰅃", icon_size = 12, dim_opacity = 0.3,
          enabled = function() return i > 1 end,
          on_click = function() dock.reorder(i, i - 1) end },
        controls.icon_button { icon = "󰅀", icon_size = 12, dim_opacity = 0.3,
          enabled = function() return i < count() end,
          on_click = function() dock.reorder(i, i + 1) end },
        controls.pill { text = "Let go", icon = "󰅖", height = 26,
          on_click = function() dock.unpin(id()) end },
      },
    }
  end
  rows[#rows + 1] = setting.row {
    width = width, label = "Nothing kept yet",
    reading = "Find an application below, or right-click one on the dock",
    visible = function() return count() == 0 end,
  }

  -- The finder: a field and the first few matches not kept yet.
  local term = controls.signal("kept.term", "")
  local matches = function()
    local wanted = term:get():lower():match("^%s*(.-)%s*$")
    if wanted == "" then return {} end
    local out = {}
    for _, app in ipairs(launcher.applications()) do
      if app.lower:find(wanted, 1, true) and not dock.is_pinned(app.id) then
        out[#out + 1] = app
        if #out >= 4 then break end
      end
    end
    return out
  end
  rows[#rows + 1] = setting.field {
    width = width, label = "Keep another",
    placeholder = "An application's name",
    on_edited = function(text) term:set(text) end,
  }
  for i = 1, 4 do
    local app = function() return matches()[i] end
    rows[#rows + 1] = setting.row {
      width = width, height = 40,
      visible = function() return app() ~= nil end,
      label = function() local a = app() return a and a.name or "" end,
      control = ui.Row {
        gap = 10, align = "center",
        app_icon.node { size = 20, name = function() local a = app() return a and a.icon or "" end },
        controls.pill { text = tr("Keep"), icon = "󰐃", height = 26, active = true,
          on_click = function() local a = app() if a then dock.pin(a.id) end end },
      },
    }
  end
  return rows
end

return M
