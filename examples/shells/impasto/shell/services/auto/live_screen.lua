-- Which screen is live: the one being worked on.
--
-- Port of shell.qml's `islandName` / `wantedIslandName` / `claimIsland`.
-- Every output runs this file in a runtime of its own, and each decides for
-- itself whether it is the live one (`island_state.set_active`):
--
--   1. the compositor's focused output, when there is a compositor that
--      says (lib.hyprland's `focused_monitor`);
--   2. otherwise the screen the pointer last came to: a bar that is hovered
--      or clicked claims the island (`island_state.claim`), and the claim is
--      told to every screen through the shell's own IPC socket, since the
--      runtimes share nothing else;
--   3. otherwise the primary: the output whose name sorts first, as
--      services/session.lua's `leader` has it, so every screen agrees.
--
-- A claim also covers the case the compositor's focus does not: a pointer
-- warped onto a bar. The island waits while a panel is open on it, since
-- moving then would be the panel closing itself; it settles once the panel
-- closes.

local state = require("bar.island_state")

local ok_lib, hyprland = pcall(require, "lib.integrations.hyprland")
if not ok_lib then hyprland = nil end

local M = {}

-- The screen asked for last, by the compositor's focus or by a claim; "" is
-- none yet.
local wanted = morf.signal("impasto.live.wanted", "")

local function names()
  local out = {}
  for _, screen in ipairs(morf.screens or {}) do
    if screen.name and screen.name ~= "" then out[#out + 1] = screen.name end
  end
  return out
end

local function plugged(name)
  for _, each in ipairs(names()) do if each == name then return true end end
  return false
end

--- The output every screen falls back to: the first by name.
function M.primary()
  local best
  for _, name in ipairs(names()) do
    if not best or name < best then best = name end
  end
  return best or state.own_screen()
end

--- The screen that should be live now, by the rules above. Pure: given the
--- wanted name and the plugged screens, the same answer on every screen.
function M.resolve(want, screens, primary)
  for _, name in ipairs(screens) do
    if want ~= "" and name == want then return want end
  end
  return primary
end

--- The live screen's name.
function M.name()
  return M.resolve(wanted:get(), names(), M.primary())
end

-- The compositor's focus, whenever it moves.
if hyprland then
  morf.effect("impasto.live.focus", function()
    local focused = hyprland.state.focused_monitor
    if focused and focused ~= "" and plugged(focused) then wanted:set(focused) end
  end)
end

-- This screen's own decision. A live island holding a panel keeps it until
-- the panel closes (the effect reads `expanded`, so it settles then).
morf.effect("impasto.live.settle", function()
  local own = state.own_screen()
  local live = own == "" or M.name() == own
  if not live and state.signals.active:get() and state.expanded() then return end
  state.set_active(live)
end)

-- -------------------------------------------------------------- claims --

-- The shell's IPC socket, as `morf ipc` finds it.
local function socket_path()
  local runtime = morf.env("XDG_RUNTIME_DIR") or ""
  local display = morf.env("WAYLAND_DISPLAY") or ""
  if runtime == "" or display == "" or display:find("/", 1, true) then return nil end
  local path = runtime .. "/morf/" .. display .. ".sock"
  if morf.fs.exists(path) then return path end
  return nil
end

local last_claim = 0

--- This screen says the pointer is on it. Told to every screen, each of
--- which takes it as the wanted one; ignored while the compositor's focus
--- already says the same.
function M.claim()
  local own = state.own_screen()
  if own == "" or wanted:get() == own then return end
  wanted:set(own)
  if #names() < 2 then return end
  -- A hand resting on the bar hovers it many times; one word is enough.
  local now = morf.time.now_ms()
  if now - last_claim < 250 then return end
  last_claim = now
  local path = socket_path()
  if not path then return end
  local request = morf.json.encode { op = "call", target = "live_claim", args = { own } } .. "\n"
  local connection
  connection = morf.connect {
    path = path,
    connect_timeout_ms = 1000,
    on_connect = function() connection:send(request) end,
    on_line = function() connection:close() end,
    on_close = function() end,
  }
end

state.claim = M.claim

-- Every screen hears a claim, its own included.
morf.ipc.live_claim = function(name)
  if type(name) == "string" and name ~= "" and plugged(name) then wanted:set(name) end
  return M.name()
end

-- `morf ipc call live`: which screen is live, as this one sees it.
morf.ipc.live = function()
  return state.own_screen() .. (state.live() and " live" or " at rest") .. " (" .. M.name() .. ")"
end

return M
