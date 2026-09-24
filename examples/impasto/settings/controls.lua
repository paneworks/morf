-- Settings, Control Centre: arranging the grid, and the buttons along the
-- top in their order (ControlsSection). Blocks are arranged on the control
-- centre itself: Edit opens it in arranging mode and closes this window.

local ui = require("morf.ui")
local theme = require("theme")
local service = require("services.controls")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local tr = require("services.tr")

local C = theme.color

local M = {}

--- A door: its icon, name, arrows while it is on, and a switch.
local function order_row(W, door)
  local on = function() return service.shows_door(door.id) end
  local at = function()
    for index, id in ipairs(service.buttons()) do if id == door.id then return index end end
    return 0
  end
  local count = function() return #service.buttons() end
  return ui.Item {
    width = W, height = 48,
    setting.wheel_area(),
    kit.glyph {
      anchors = { left = true, left_margin = 14, vertical_center = true }, width = 20,
      glyph = door.icon, size = 13,
      color = function() return on() and C.accent() or C.textMuted() end,
    },
    kit.text {
      anchors = { left = true, left_margin = 46, vertical_center = true },
      text = door.label, size = theme.size.small, width = W - 46 - 170, elide = "right",
      color = function() return on() and C.text() or C.textMuted() end,
    },
    ui.Row {
      anchors = { right = true, right_margin = 14, vertical_center = true }, gap = 8, align = "center",
      ui.Row {
        gap = 2, align = "center", opacity = function() return on() and 1 or 0 end,
        controls.icon_button { icon = "󰅃", icon_size = 12, dim_opacity = 0.3,
          enabled = function() return on() and at() > 1 end,
          on_click = function() service.move_door(door.id, -1) end },
        controls.icon_button { icon = "󰅀", icon_size = 12, dim_opacity = 0.3,
          enabled = function() return on() and at() < count() end,
          on_click = function() service.move_door(door.id, 1) end },
      },
      controls.switch { checked = on, on_toggled = function(value) service.set_door(door.id, value) end },
    },
  }
end

function M.build(page)
  return setting.page(page, function(W)
    local doors = {
      width = W, title = tr("The top row"),
      note = "Session actions always sit on the left. These buttons, which open other panels and this window, fill the rest in this order.",
    }
    for _, door in ipairs(service.doors) do doors[#doors + 1] = order_row(W, door) end
    return {
      setting.group {
        width = W, title = tr("The panel"),
        note = tr("A six by eight grid, arranged on the panel itself."),
        hint = "Edit shows the grid with a card of every block: drag a block onto the grid to place it, pull its corner to resize, and press its badge to remove it.",
        setting.row { width = W, label = tr("Arrange the control centre"),
          control = ui.Row { gap = 8,
            controls.pill { text = tr("Default layout"), height = 30, on_click = function() service.restore() end },
            controls.pill { text = tr("Edit"), height = 30, active = true, on_click = function()
              service.edit(true)
              require("bar.island").open("controls")
              page.close()
            end },
          } },
      },
      setting.group(doors),
    }
  end)
end

return M
