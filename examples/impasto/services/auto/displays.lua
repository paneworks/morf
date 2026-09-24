-- Keeps the screens arranged as the Displays page left them, per set of
-- connected monitors, and lights one again should none be
-- (services/displays.lua).
--
-- `morf ipc call displays` lists the screens as JSON; `morf ipc call
-- display ...` changes the arrangement as the page does, for a keybind or
-- a script:
--   display <name> on|off                 display <name> scale 1.25
--   display <name> position 1920x0        display <name> transform 1
--   display <name> mode 2560x1440@144     display <name> vrr 0|1
--   display mirror on|off                 display primary <name>
--   display forget                        display lid closed|open
local displays = require("services.displays")
displays.start()

morf.ipc.displays = function()
  local out = {}
  for _, m in ipairs(displays.monitors()) do
    out[#out + 1] = { name = m.name, description = m.description, disabled = m.disabled, mode = m.mode,
      position = m.position, scale = m.scale, transform = m.transform, vrr = m.vrr, mirror = m.mirror }
  end
  return morf.json.encode(out)
end

morf.ipc.display = function(what, field, value)
  if not displays.available() then return "not available here: this compositor is not Hyprland" end
  if what == "mirror" then displays.remember_mirror(field == "on") return "ok" end
  if what == "forget" then displays.forget() return "ok" end
  if what == "lid" then displays.lid(field == "closed") return "ok" end
  local target = what == "primary" and field or what
  local m = displays.by_name(target or "")
  if not m then return "no screen named " .. tostring(target) end
  local key = displays.key(m)
  if what == "primary" then displays.remember_primary(key) return "ok" end
  if field == "on" or field == "off" then displays.remember(key, { disabled = field == "off" }) return "ok" end
  local numbers = { scale = true, transform = true, vrr = true }
  if numbers[field] then
    local n = tonumber(value)
    if not n then return "not a number: " .. tostring(value) end
    return displays.remember(key, { [field] = n }) == false and "refused" or "ok"
  end
  if field == "position" or field == "mode" then
    return displays.remember(key, { [field] = tostring(value) }) == false and "refused" or "ok"
  end
  return "unknown: " .. tostring(field)
end
