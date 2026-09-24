-- Which screen's shell does the work that must be done once.
--
-- Every output runs this configuration in a runtime of its own, and an IPC
-- verb, a timer or a setting reaches all of them. A screenshot, a recording,
-- the night light or one ddcutil must happen once, not once per screen, so
-- those ask `live.here()` first.
--
-- The island knows which screen is being worked on (`bar.island_state.live`
-- when it has it). Without that the focused monitor Hyprland reports is
-- used, and without Hyprland the screen with the lowest id: the same answer
-- in every runtime, so exactly one of them says yes.

local M = {}

local function own()
  return ((morf.screens or {})[1] or {})
end

local function hyprland_focus()
  local hyprland = package.loaded["lib.hyprland"]
  if type(hyprland) ~= "table" or not hyprland.available or not hyprland.available() then
    return ""
  end
  local ok, name = pcall(function() return hyprland.state.focused_monitor end)
  if ok and type(name) == "string" then return name end
  return ""
end

--- The screen that should act, by name ("" when nothing is known).
function M.name()
  local focus = hyprland_focus()
  if focus ~= "" then
    for _, screen in ipairs(morf.screens or {}) do
      if screen.name == focus then return focus end
    end
  end
  local first
  for _, screen in ipairs(morf.screens or {}) do
    if not first or (tonumber(screen.id) or 0) < (tonumber(first.id) or 0) then first = screen end
  end
  return first and first.name or ""
end

--- Whether this runtime is the one that acts.
function M.here()
  local ok, island_state = pcall(require, "bar.island_state")
  if ok and type(island_state) == "table" and type(island_state.live) == "function" then
    local asked, answer = pcall(island_state.live)
    if asked and type(answer) == "boolean" then return answer end
  end
  local name = M.name()
  return name == "" or name == own().name
end

return M
