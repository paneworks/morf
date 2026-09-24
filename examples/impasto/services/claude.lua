-- Claude usage, for the desk's Claude faces.
--
-- Port of ClaudeService.qml over `lib.claude_usage`, which lands in
-- lua-stdlib separately; until it does the faces read "no usage found".
-- The reading is taken as the library gives it, with these fields when it
-- has them: `block_tokens`, `block_messages`, `block_end` (epoch seconds),
-- `week_tokens`, `peak_block_tokens`, `peak_week_tokens`, and, when the
-- account's limits are known, `session_fraction` and `weekly_fraction`.

local theme = require("theme")

local M = {}

local ok, lib = pcall(require, "lib.claude_usage")
if not ok then lib = nil end

local handle = nil
local EMPTY = { available = false }

function M.now()
  if not lib then return EMPTY end
  if handle == nil then
    local okn, made = pcall(function()
      if lib.new then return lib.new {} end
      return lib
    end)
    handle = okn and made or false
  end
  if not handle then return EMPTY end
  local okr, value = pcall(function()
    if handle.get then return handle:get() end
    if handle.read then return handle.read() end
    return nil
  end)
  if not okr or type(value) ~= "table" then return EMPTY end
  if value.available == nil then
    value.available = (tonumber(value.block_tokens) or 0) > 0 or value.session_fraction ~= nil
  end
  return value
end

local function num(field) return tonumber(M.now()[field]) or 0 end

function M.available() return M.now().available == true end
function M.session_measured() return M.now().session_fraction ~= nil end
function M.weekly_measured() return M.now().weekly_fraction ~= nil end
function M.session_fraction() return num("session_fraction") end
function M.weekly_fraction() return num("weekly_fraction") end
function M.block_tokens() return num("block_tokens") end
function M.block_messages() return num("block_messages") end
function M.week_tokens() return num("week_tokens") end
function M.peak_block_tokens() return num("peak_block_tokens") end
function M.peak_week_tokens() return num("peak_week_tokens") end

--- 0..1: the session's share when known, else the block against the busiest.
function M.gauge()
  if M.session_measured() then return math.min(1, M.session_fraction()) end
  local peak = M.peak_block_tokens()
  return peak > 0 and math.min(1, M.block_tokens() / peak) or 0
end

--- The accent, yellow past 60%, red past 85%.
function M.tint()
  local g = M.gauge()
  if g >= 0.85 then return theme.color.indicatorBad end
  if g >= 0.6 then return theme.color.indicatorWarn end
  return theme.color.accent()
end

--- "resets in 2 h 10 min"
function M.resets_in()
  local ends = num("block_end")
  if ends <= 0 then return "" end
  morf.clock:get()
  local left = math.max(0, ends - morf.time.now())
  local h, m = math.floor(left / 3600), math.floor(left % 3600 / 60)
  if h > 0 then return string.format("resets in %d h %d min", h, m) end
  return string.format("resets in %d min", m)
end

function M.compact(tokens)
  tokens = tonumber(tokens) or 0
  if tokens >= 1e6 then return string.format("%.1fM", tokens / 1e6) end
  if tokens >= 1e3 then return string.format("%.0fk", tokens / 1e3) end
  return tostring(math.floor(tokens))
end
function M.percent(f) return string.format("%d%%", math.floor((tonumber(f) or 0) * 100 + 0.5)) end
function M.messages(n)
  n = math.floor(tonumber(n) or 0)
  return n == 1 and "1 message" or (n .. " messages")
end

return M
