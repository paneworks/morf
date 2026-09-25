-- lib/hyprland_config in a configuration, for hyprland_config_spec.lua:
-- IPC verbs that use it and say what came back.
local ui = require("morf.ui")
local config = require("lib.hyprland_config")
local answers = {}
morf.ipc.flavour = function() return tostring(config.known_flavour()) end
morf.ipc.push = function()
  config.apply(function(how)
    return config.options_plan({ { "input:kb_layout", "str", "us,de" }, { "input:repeat_rate", "int", 30 } }, how)
  end, function(ok) answers.push = tostring(ok) end)
  return "sent"
end
morf.ipc.answer = function(name) return answers[name] or "" end
morf.ipc.outputs = function()
  local out = {}
  for _, o in ipairs(config.outputs()) do out[#out + 1] = o.name .. ":" .. tostring(o.disabled) .. ":" .. o.mode end
  table.sort(out)
  return table.concat(out, " ")
end
morf.ipc.keyboard = function()
  local state = require("lib.hyprland").state
  return tostring(state.keyboard) .. ":" .. tostring(state.keyboard_layout)
end
ui.Rect { width = 10, height = 10 }
