-- The session actions at the left of the control centre's top row:
-- PowerRow.qml. A destructive one (log out, restart, shut down) arms on the
-- first click -- it turns red and says what it will do -- and runs on the
-- second; it disarms by itself after three seconds.
--
-- The actions are the session service's when one is ported
-- (`services.session`, with `actions` and `run(id)`); until then they go to
-- logind directly. Either way through `services.act`.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local act = require("services.act")

local C = theme.color
local M = {}

M.actions = {
  { id = "lock", icon = "󰌾", label = "Lock", destructive = false },
  { id = "suspend", icon = "󰤄", label = "Suspend", destructive = false },
  { id = "logout", icon = "󰗽", label = "Log out", destructive = true },
  { id = "reboot", icon = "󰜉", label = "Restart", destructive = true },
  { id = "shutdown", icon = "󰐥", label = "Shut down", destructive = true },
}

local function session()
  local ok, service = pcall(require, "services.session")
  if ok and type(service) == "table" and service.run then return service end
  return nil
end

function M.run(id)
  local service = session()
  if service then return service.run(id) end
  local login = require("services.brightness").lib
  if id == "lock" then return act.run("locking the session", login.lock) end
  if id == "suspend" then return act.run("suspending", login.suspend) end
  if id == "reboot" then return act.run("restarting", login.reboot) end
  if id == "shutdown" then return act.run("shutting down", login.power_off) end
  if id == "logout" then
    local ok, hyprland = pcall(require, "lib.hyprland")
    if ok and hyprland.available and hyprland.available() then
      return act.run("logging out", hyprland.dispatch, "exit", "")
    end
  end
end

--- `on_ran(id)` closes the panel: lock in particular captures the screen,
--- which would otherwise include it.
function M.build(on_ran)
  local armed = controls.signal("power.armed", "")
  local generation = 0
  local row = { gap = 4, align = "center" }
  for _, action in ipairs(M.actions) do
    local hovered = controls.signal("power.hover", false)
    local is_armed = function() return armed:get() == action.id end
    local label = kit.text { text = action.label, size = theme.size.small, weight = 600,
      color = C.accentText, visible = is_armed }
    row[#row + 1] = ui.Rect {
      height = 28,
      width = function() return is_armed() and ((label.layout_width or 0) + 34) or 32 end,
      radius = theme.radius_small,
      color = function()
        if is_armed() then return C.red() end
        return hovered:get() and C.islandSurfaceHover or "#00000000"
      end,
      border_width = 1,
      border_color = function()
        if is_armed() then return C.red() end
        return hovered:get() and C.islandBorder or "#00000000"
      end,
      behavior = { width = theme.behave("fast"), color = theme.behave("fast") },
      ui.Row {
        anchors = { center_in = true }, gap = 6, align = "center",
        kit.glyph { glyph = action.icon, size = 14,
          color = function()
            if is_armed() then return C.accentText() end
            return hovered:get() and C.accent() or C.text()
          end },
        label,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function()
          if not action.destructive or is_armed() then
            armed:set("")
            if on_ran then on_ran(action.id) end
            M.run(action.id)
            return
          end
          armed:set(action.id)
          generation = generation + 1
          local mine = generation
          morf.timer(3000, function() if mine == generation then armed:set("") end end, false)
        end,
      },
    }
  end
  return ui.Row(row)
end

return M
