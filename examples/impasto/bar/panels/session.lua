-- The session panel: lock, suspend, log out, restart, shut down.
--
-- Port of SessionPanel.qml. The same actions and the same two-press guard
-- as the control centre's power row, in a larger, keyboard-first layout:
-- arrows move, Enter commits, and the pointer drives the same selection.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local session = require("services.session")
local kit = require("components.kit")

local C = theme.color

local KEY = { LEFT = 0xff51, RIGHT = 0xff53, RETURN = 0xff0d, KP_ENTER = 0xff8d, ESCAPE = 0xff1b }

local selected = morf.signal("impasto.session.selected", 1)
local armed = morf.signal("impasto.session.armed", "")
local disarm_generation = 0

local function disarm()
  disarm_generation = disarm_generation + 1
  armed:set("")
end

local function move(delta)
  local count = #session.actions
  selected:set((selected:get() - 1 + delta) % count + 1)
  -- Moving away from an armed action disarms it: the confirmation is for
  -- that button, not for wherever the cursor ends up next.
  disarm()
end

-- Destructive actions arm on the first press and run on the second, from
-- the keyboard as from the pointer.
local function activate(action)
  if not action then return end
  if not action.destructive or armed:get() == action.id then
    disarm()
    -- Close first, then act: `lock` photographs the screen before covering
    -- it, and anything still open would end up in the picture.
    island.close()
    session.run(action.id)
    return
  end
  disarm_generation = disarm_generation + 1
  local mine = disarm_generation
  armed:set(action.id)
  morf.timer(3000, function() if mine == disarm_generation then armed:set("") end end, false)
end

local function tile(index, action)
  local is_selected = function() return selected:get() == index end
  local is_armed = function() return armed:get() == action.id end
  return ui.Rect {
    layout = { grow = 1, basis = 0 },
    radius = theme.radius_large,
    -- Armed is red, selected is the accent. Only the selected tile can be
    -- armed, so they never conflict.
    color = function()
      if is_armed() then return C.red() end
      return is_selected() and C.islandSurfaceHover or C.islandSurface
    end,
    border_color = function()
      if is_armed() then return C.red() end
      return is_selected() and C.accent() or C.islandBorder
    end,
    border_width = function() return (is_selected() or is_armed()) and 2 or 1 end,
    behavior = { color = theme.behave("fast"), border_color = theme.behave("fast") },
    ui.Column {
      anchors = { center_in = true },
      gap = 10, align = "center",
      kit.glyph {
        glyph = action.icon, size = 30,
        color = function()
          if is_armed() then return C.accentText() end
          return is_selected() and C.accent() or C.text()
        end,
        behavior = { color = theme.behave("fast") },
      },
      kit.text {
        text = function() return is_armed() and "Confirm" or action.label end,
        size = theme.size.small,
        weight = 400,
        font_weight = function() return is_selected() and 600 or 400 end,
        color = function() return is_armed() and C.accentText() or C.textMuted() end,
        behavior = { color = theme.behave("fast") },
      },
    },
    -- The pointer drives the same selection as the arrows.
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function()
        if selected:get() ~= index then selected:set(index) disarm() end
      end,
      on_clicked = function()
        selected:set(index)
        activate(action)
      end,
    },
  }
end

island.register("session", {
  size = function() return 720, 180 end,
  build = function()
    -- A fresh panel starts on the first action, unarmed.
    selected:set(1)
    disarm()
    local tiles = {}
    for index, action in ipairs(session.actions) do tiles[#tiles + 1] = tile(index, action) end
    -- Sized by the island's inset, which keeps the panel's padding.
    return ui.Item {
      -- The keyboard is the panel's while it is open.
      ui.MouseArea {
        anchors = { fill = true },
        z = -1,
        -- Asked for, so it wins over the island's own Escape catcher.
        focus = true,
        on_key_pressed = function(keysym)
          if keysym == KEY.LEFT then move(-1)
          elseif keysym == KEY.RIGHT then move(1)
          elseif keysym == KEY.RETURN or keysym == KEY.KP_ENTER then activate(session.actions[selected:get()])
          elseif keysym == KEY.ESCAPE then island.close()
          end
        end,
      },
      ui.Flex {
        anchors = { fill = true },
        direction = "row", gap = 12, align = "stretch",
        table.unpack(tiles),
      },
    }
  end,
})
