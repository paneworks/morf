-- Session actions: lock, suspend, log out, restart, shut down.
--
-- Port of SessionService.qml. The actions are data, rendered from one list by
-- the session panel and the control centre; `destructive` marks the ones that
-- end the session, and asking twice is the panel's business, not this one's.
--
-- The original ran `systemctl`; here the same requests go to logind itself
-- over the system bus, which is all `systemctl suspend` does. `interactive`
-- is false: nothing here can answer a polkit prompt, and an interactive
-- check would wait for one.

local M = {}

M.actions = {
  { id = "lock",     icon = "󰌾", label = "Lock",      destructive = false },
  { id = "suspend",  icon = "󰤄", label = "Suspend",   destructive = false },
  { id = "logout",   icon = "󰗽", label = "Log out",   destructive = true },
  { id = "reboot",   icon = "󰜉", label = "Restart",   destructive = true },
  { id = "shutdown", icon = "󰐥", label = "Shut down", destructive = true },
}

local LOGIND = "org.freedesktop.login1"
local MANAGER = "org.freedesktop.login1.Manager"

local manager
local function logind()
  if manager then return manager end
  local ok, proxy = pcall(morf.dbus.proxy, "system", LOGIND, "/org/freedesktop/login1", MANAGER, 5000)
  if ok then manager = proxy end
  return manager
end

--- One logind method with its one argument; logged, never raised.
local function call(method, argument)
  local proxy = logind()
  if not proxy then
    morf.log("warn", "impasto: logind is not reachable; not running " .. method)
    return false
  end
  local ok, err = pcall(proxy.call_with, proxy, method, argument)
  if not ok then morf.log("warn", "impasto: logind " .. method .. ": " .. tostring(err)) end
  return ok
end

-- The lock service registers itself here, so this file needs nothing of it
-- and the lock screen's own process can use `run` for its power buttons.
local lock

--- Called by the lock service with itself.
function M.attach_lock(service) lock = service end

-- --------------------------------------------------------------- sleep --

-- Suspend only once the lock is up, so the machine never wakes on the desk.
-- No lock within five seconds means no suspend.
local suspend_generation = 0

local function suspend_when_locked()
  suspend_generation = suspend_generation + 1
  local mine = suspend_generation
  local waited = 0
  local timer
  timer = morf.timer(100, function()
    if mine ~= suspend_generation then timer:cancel() return end
    waited = waited + 100
    if lock and lock.secure() then
      timer:cancel()
      call("Suspend", false)
    elseif waited >= 5000 then
      timer:cancel()
      morf.log("warn", "impasto: the session did not lock; not suspending")
    end
  end, true)
end

-- Every sleep locks first, the lid and `systemctl suspend` included: logind
-- says PrepareForSleep(true) before sleeping and (false) after waking. The
-- original also held a delay inhibitor so logind waits for the lock; that
-- needs a file descriptor back from `Inhibit`, which morf.dbus cannot hold
-- yet, so here the lock races the sleep.
local watching = false
function M.watch_sleep()
  if watching or not M.leader() then return end
  watching = true
  local proxy = logind()
  if not proxy then return end
  pcall(proxy.subscribe, proxy, "PrepareForSleep", function(going)
    if type(going) == "table" then going = going[1] end
    if going and lock and not lock.secure() then lock.lock() end
  end)
end

--- Every screen runs the shell; what must happen once a machine (a suspend,
--- listening for sleep) is done by the screen whose output name sorts
--- first.
function M.leader()
  local screens = morf.screens or {}
  local own = screens[1] and screens[1].name
  if not own then return true end
  for _, screen in ipairs(screens) do
    if screen.name and screen.name < own then return false end
  end
  return true
end

-- ------------------------------------------------------------- actions --

local runners = {
  lock = function() if lock then lock.lock() end end,
  suspend = function()
    -- Already locked (the idle service got there): `lock()` would do
    -- nothing and the wait would never end.
    if lock and lock.secure() then
      call("Suspend", false)
      return
    end
    if lock then lock.lock() end
    suspend_when_locked()
  end,
  logout = function()
    local ok, hyprland = pcall(require, "lib.hyprland")
    if ok and hyprland and hyprland.dispatch then
      hyprland.dispatch("exit")
      return
    end
    local session = morf.env("XDG_SESSION_ID")
    if session then call("TerminateSession", session) end
  end,
  reboot = function() call("Reboot", false) end,
  shutdown = function() call("PowerOff", false) end,
}

--- Runs one action by id.
function M.run(id)
  local runner = runners[id]
  if not runner then
    morf.log("warn", "impasto: unknown session action " .. tostring(id))
    return
  end
  runner()
end

return M
