-- The battery, as UPower pushes it.
--
-- Port of BatteryService.qml over `lib.upower`. The display device gives
-- the charge, the direction and the estimates; the machine's own battery
-- row adds the energy and the design capacity for the detail's figures.

local upower = require("lib.upower")
local theme = require("theme")

local M = {}

local power = upower.connect()
M.lib = power
M.state = power.state
local d = power.state.display

function M.available()
  return power.state.available and d.present == true and d.kind == "battery"
end

function M.percent() return M.available() and math.floor((d.percentage or 0) + 0.5) or 0 end
function M.charging() return M.available() and d.charging == true end
function M.full() return M.available() and d.state == "fully_charged" end
function M.low() return M.available() and not M.charging() and not M.full() and M.percent() <= 20 end

-- The Material battery glyphs are not contiguous: empty, nine steps, full.
M.level_icons = { "󰂎", "󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹" }

--- Fixed indicator hues, so a colour means the same level on every palette.
function M.tint()
  if not M.available() then return theme.color.indicatorDim end
  if M.charging() or M.full() then return theme.color.indicatorGood end
  local p = M.percent()
  if p <= 15 then return theme.color.indicatorBad end
  if p <= 35 then return theme.color.indicatorWarn end
  return theme.color.indicatorGood
end

function M.seconds_to_empty() return M.available() and (d.time_to_empty or 0) or 0 end
function M.seconds_to_full() return M.available() and (d.time_to_full or 0) or 0 end

--- "4 h 8 min left", "35 min to full", or "" while UPower has no estimate.
function M.estimate()
  if not M.available() then return "" end
  if M.full() then return "Fully charged" end
  local seconds = M.charging() and M.seconds_to_full() or M.seconds_to_empty()
  if seconds <= 0 then return M.charging() and "Charging" or "" end
  local span = upower.format_time(seconds)
  return M.charging() and (span .. " to full") or (span .. " left")
end

function M.state_word()
  if not M.available() then return "" end
  if M.full() then return "Full" end
  if M.charging() then return "Charging" end
  return "On battery"
end

--- Watts, unsigned; the direction is in `state_word`.
function M.watts() return M.available() and math.abs(d.energy_rate or 0) or 0 end

-- The machine's own battery row, for energy and health.
local function cell()
  local _ = d.percentage, d.state
  for _, row in ipairs(power.devices() or {}) do
    if row.kind == "battery" and row.power_supply then return row end
  end
  return nil
end

function M.energy() local row = cell() return row and row.energy or 0 end
function M.energy_capacity() local row = cell() return row and row.energy_full or 0 end
function M.health_known() local row = cell() return row ~= nil and (row.capacity or 0) > 0 end
function M.health() local row = cell() return row and math.floor((row.capacity or 0) + 0.5) or 0 end

function M.icon()
  if not M.available() then return "󰂑" end
  if M.charging() then return "󰂄" end
  if M.low() then return "󰂃" end
  local step = math.max(0, math.min(10, math.floor(M.percent() / 10 + 0.5)))
  return M.level_icons[step + 1]
end

return M
