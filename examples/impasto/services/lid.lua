-- The laptop lid, from logind's LidClosed, and the `lidPolicy` setting.
--
-- Port of shell.qml's lid binds (277-292) and MonitorService.lid. Upstream
-- the compositor's switch binds called into the shell; here the shell hears
-- logind itself, which knows the lid whatever the compositor. With another
-- screen connected and the policy "off", closing the lid turns the laptop's
-- panel off, and opening it always lights it again; "keep" leaves it on and
-- "system" leaves the lid to logind. With nothing else connected nothing is
-- done: logind suspends.
--
-- The panel is turned off by its own screen's runtime asking the compositor
-- for output power (wlr-output-power-management), as the idle blank does,
-- rather than by rewriting the compositor's monitor rules. The lock hears
-- the lid too: opening it wakes the lock, which looks for a face.

local settings = require("services.settings")

local M = {}

local function is_panel(name)
  name = tostring(name or "")
  return name:match("^eDP") ~= nil or name:match("^LVDS") ~= nil or name:match("^DSI") ~= nil
end
M.is_panel = is_panel

--- What this screen does about the lid: its own panel off or on.
function M.apply(closed)
  if settings.lidPolicy == "system" then return end
  local screens = morf.screens or {}
  local own = screens[1]
  if not own or not is_panel(own.name) then return end
  local others = 0
  for _, screen in ipairs(screens) do
    if screen.name ~= own.name then others = others + 1 end
  end
  if others == 0 then return end
  if closed and settings.lidPolicy ~= "off" then return end
  -- Opening always lights the panel, whatever the policy.
  local act = require("services.act")
  act.run(closed and "turn the laptop panel off" or "turn the laptop panel on", function()
    return pcall(morf.output_power.set, closed and "off" or "on")
  end)
end

local started = false
--- Starts listening. `options.login` is a `lib.logind` connection to share;
--- `options.on_change(closed)` hears each change.
function M.start(options)
  if started then return end
  started = true
  options = options or {}
  local login = options.login
  if not login then
    local ok, logind = pcall(require, "lib.logind")
    if not ok then return end
    login = logind.connect { udev = false }
  end
  M.login = login
  local last = nil
  morf.effect("impasto.lid", function()
    local closed = login.state.lid_closed == true
    if last == nil then last = closed return end
    if closed == last then return end
    last = closed
    local handler = options.on_change
    if handler then
      -- Out of the effect, which only reads.
      morf.timer(1, function() handler(closed) end, false)
    end
  end)
end

--- The lid, now.
function M.closed()
  return M.login ~= nil and M.login.state.lid_closed == true
end

return M
