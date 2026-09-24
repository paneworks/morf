-- Restart and shut down, on the lock screen.
--
-- Port of LockPower.qml, matching the login screen. No suspend: it already
-- has a shortcut and a button in the session panel. Armed on the first click
-- and run on the second, the session's rule for anything that ends it:
-- there is unsaved work behind this surface.

local ui = require("morf.ui")
local theme = require("theme")
local session = require("services.session")
local kit = require("components.kit")

local C = theme.color

return function(values)
  local armed = morf.signal("impasto.lock.power.armed", "")
  local disarm_generation = 0

  local function chip(action, glyph, caption)
    local hovered = kit.hover_signal("lock.power." .. action)
    local is_armed = function() return armed:get() == action end
    local name = kit.text {
      text = caption, size = theme.size.small, weight = 600,
      visible = is_armed,
    }
    local icon = kit.glyph {
      glyph = glyph, size = 15,
      opacity = function() return (is_armed() or hovered:get()) and 1 or 0.75 end,
      behavior = { opacity = theme.behave("fast") },
    }
    -- A Row keeps room for a hidden child, so the holder below is sized
    -- to the glyph alone until the caption shows.
    local content = ui.Row { gap = 7, align = "center", icon, name }
    return ui.Rect {
      width = function()
        local cap = theme.capsule_height()
        if is_armed() then return (name.layout_width or 0) + cap + 16 end
        return cap
      end,
      height = function() return theme.capsule_height() end,
      radius = function() return theme.capsule_height() / 2 end,
      -- Pure black, like the island. Hover lifts the fill; the outline stays.
      color = function()
        if is_armed() then return C.indicatorBad end
        return hovered:get() and C.islandSurfaceHover or C.island
      end,
      border_width = 1,
      border_color = function() return is_armed() and C.indicatorBad or C.islandBorder end,
      behavior = {
        width = theme.behave("fast"), color = theme.behave("fast"), border_color = theme.behave("fast"),
      },
      ui.Item { anchors = { center_in = true },
        width = function() return is_armed() and (content.layout_width or 0) or (icon.layout_width or 0) end,
        height = function() return content.layout_height or 0 end,
        content },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function()
          disarm_generation = disarm_generation + 1
          if is_armed() then
            armed:set("")
            session.run(action)
            return
          end
          armed:set(action)
          local mine = disarm_generation
          morf.timer(3000, function()
            if mine == disarm_generation then armed:set("") end
          end, false)
        end,
      },
    }
  end

  local out = {
    gap = 8,
    chip("reboot", "󰜉", "Restart"),
    chip("shutdown", "󰐥", "Shut down"),
  }
  for key, value in pairs(values or {}) do out[key] = value end
  out.armed = nil
  return ui.Row(out), armed
end
