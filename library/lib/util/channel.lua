-- Data channels for what draws them (`ui.Path { series = ch.id, plot = ... }`).
--
--     local ch, start = channel.from(values, { size = 8192 })
--     local node = ui.Path { series = ch.id, plot = { kind = "bars" } }
--     start(node)    -- fed while the node lives
--
-- `values` is a channel already (`morf.channel`, `sysinfo.channel`, an audio
-- monitor's), which is drawn as it is, or a list or a function returning
-- one, copied into a channel of the drawer's own by an effect owned by the
-- node -- so a chart reads a channel whatever its data comes from, and a
-- source that only samples while something reads it still knows it is read.
local morf = require("morf")

local M = {}

--- Whether `v` is a channel handle.
function M.is(v)
  return type(v) == "table" and type(v.id) == "number" and type(v.push) == "function"
end

--- A channel for `values` (see the head), and the function that starts
--- feeding it once its owner node exists. `options`: `size` (8192),
--- `mode` ("frame").
function M.from(values, options)
  options = options or {}
  if M.is(values) then return values, function() end end
  local ch = morf.channel { size = options.size or 8192, mode = options.mode or "frame" }
  local function start(owner)
    morf.effect("lib.channel." .. ch.id, function()
      local list = values
      if type(values) == "function" then list = values() end
      ch:set(type(list) == "table" and list or {})
    end, owner and { owner = owner } or nil)
  end
  return ch, start
end

return M
