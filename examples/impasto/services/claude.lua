-- Claude Code usage for the current five-hour block and the last seven days.
--
-- Port of ClaudeService.qml over `lib.claude_usage`. Token and message
-- counts come from the transcripts on disk, where every assistant turn
-- records its usage. Percentages come only from the account: the API's
-- `anthropic-ratelimit-unified-*` headers, the figures `/usage` shows,
-- asked for with `claude_usage.limits` every ten minutes while something
-- watches. Local peaks are never used as a denominator: with no account
-- figures the gauge is the time elapsed in the block.
--
-- A dry run (IMPASTO_DRY_RUN) never sends the limits request, which is a
-- real request against the account; `M.sample_limits` sets figures for a
-- test bench instead.

local theme = require("theme")
local act = require("services.act")

local M = {}

M.POLL_MS = 120000
M.LIMITS_MS = 600000

local ok, lib = pcall(require, "lib.claude_usage")
if not ok then lib = nil end

local handle = nil
local EMPTY = { available = false }

-- The account's figures, from the last limits answer.
local limits = {
  known = morf.signal("impasto.claude.limits.known", false),
  session_used = morf.signal("impasto.claude.limits.session_used", 0),
  week_used = morf.signal("impasto.claude.limits.week_used", 0),
  session_resets = morf.signal("impasto.claude.limits.session_resets", 0),
  week_resets = morf.signal("impasto.claude.limits.week_resets", 0),
  claim = morf.signal("impasto.claude.limits.claim", ""),
  -- The headers report a rate limit before requests start failing.
  limited = morf.signal("impasto.claude.limits.limited", false),
}
M.limits = limits

local function reader()
  if not lib then return nil end
  if handle == nil then
    local okn, made = pcall(lib.new, { interval = M.POLL_MS })
    handle = okn and made or false
  end
  return handle or nil
end

--- The transcripts' reading, as `lib.claude_usage` gives it.
function M.now()
  local r = reader()
  if not r then return EMPTY end
  local okr, value = pcall(function() return r:get() end)
  if not okr or type(value) ~= "table" then return EMPTY end
  return value
end

local function num(field) return tonumber(M.now()[field]) or 0 end

-- ----------------------------------------------------------------- counts --

--- Whether any transcript was found.
function M.available() return M.now().available == true end
function M.block_tokens() return num("block_tokens") end
function M.block_messages() return num("block_messages") end
function M.week_tokens() return num("week_tokens") end
function M.week_messages() return num("week_messages") end
function M.peak_block_tokens() return num("peak_block_tokens") end
function M.peak_week_tokens() return num("peak_week_tokens") end
function M.block_start() return num("block_start") end
function M.block_end() return num("block_end") end

-- ------------------------------------------------------------ percentages --

--- Only the account's own figures are shown as percentages.
function M.measured() return limits.known:get() end
M.session_measured = M.measured
M.weekly_measured = M.measured
function M.limited() return limits.limited:get() end
function M.claim() return limits.claim:get() end

local function clamp(v) return math.max(0, math.min(1, tonumber(v) or 0)) end
function M.session_fraction() return M.measured() and clamp(limits.session_used:get()) or 0 end
function M.weekly_fraction() return M.measured() and clamp(limits.week_used:get()) or 0 end

local function nothing() return not M.available() and not M.measured() end

--- When the block ends, epoch seconds. The account's reset time wins: the
--- transcripts only know when this machine first wrote into the block,
--- which may be after it started.
function M.block_ends()
  local resets = limits.session_resets:get()
  if M.measured() and resets > 0 then return resets end
  return M.block_end()
end

--- Milliseconds left in the block, on the wall clock so it advances
--- between polls.
function M.remaining()
  if nothing() then return 0 end
  morf.minute_clock:get()
  return math.max(0, M.block_ends() * 1000 - morf.time.now_ms())
end

--- 0..1 of the block gone. The window is always five hours; only its end
--- moves.
function M.elapsed()
  local span
  if M.measured() and limits.session_resets:get() > 0 then span = 5 * 3600 * 1000
  else span = (M.block_end() - M.block_start()) * 1000 end
  if nothing() or span <= 0 then return 0 end
  return clamp(1 - M.remaining() / span)
end

--- "resets in 3 h 53 min"
function M.resets_in()
  if nothing() then return "" end
  local minutes = math.ceil(M.remaining() / 60000)
  if minutes <= 0 then return "resets now" end
  local hours = minutes // 60
  if hours > 0 then return ("resets in %d h %d min"):format(hours, minutes % 60) end
  return ("resets in %d min"):format(minutes)
end

--- Coloured only past the warning thresholds, and only against a real
--- ceiling; fixed indicator hues, neutral at rest like the other rings.
function M.tint()
  local C = theme.color
  if nothing() then return C.indicatorDim end
  if M.limited() then return C.indicatorBad end
  if not M.measured() then return C.indicator end
  local worst = math.max(M.session_fraction(), M.weekly_fraction())
  if worst >= 0.85 then return C.indicatorBad end
  if worst >= 0.6 then return C.indicatorWarn end
  return C.indicator
end

--- The fraction spent when the account's figures are known, otherwise the
--- block's elapsed time.
function M.gauge()
  if M.measured() then return math.max(M.session_fraction(), M.weekly_fraction()) end
  return M.elapsed()
end

-- ---------------------------------------------------------------- figures --

--- 1.2k, 34k, 8.7M.
function M.compact(tokens)
  tokens = math.floor(tonumber(tokens) or 0)
  if tokens >= 1e6 then return ("%.1fM"):format(tokens / 1e6) end
  if tokens >= 1e3 then return math.floor(tokens / 1e3 + 0.5) .. "k" end
  return tostring(tokens)
end
function M.percent(f) return math.floor((tonumber(f) or 0) * 100 + 0.5) .. "%" end
--- "84 messages", "1.2k messages".
function M.messages(n)
  n = math.floor(tonumber(n) or 0)
  return M.compact(n) .. (n == 1 and " message" or " messages")
end

-- --------------------------------------------------------------- watching --

local function take(report)
  if type(report) ~= "table" or report.available ~= true then
    -- No credentials, no network, or an expired token. The counts still
    -- work; percentages are hidden, not stale.
    limits.known:set(false)
    return
  end
  if report.session then
    limits.session_used:set(tonumber(report.session.used) or 0)
    limits.session_resets:set(tonumber(report.session.resets) or 0)
  end
  if report.week then
    limits.week_used:set(tonumber(report.week.used) or 0)
    limits.week_resets:set(tonumber(report.week.resets) or 0)
  end
  limits.claim:set(report.claim or "")
  limits.limited:set(report.status == "rate_limited")
  limits.known:set(true)
end

local asking = false
function M.ask_limits()
  if not lib or not lib.limits or asking then return end
  if act.dry then
    morf.log("info", "impasto: dry run, not asking the Claude API for the account's limits")
    return
  end
  asking = true
  lib.limits({}, function(report)
    asking = false
    take(report)
  end)
end

--- A test bench's figures: `session` and `week` 0..1, resets in `minutes`.
function M.sample_limits(session, week, minutes, status)
  local resets = morf.time.now() + (minutes or 150) * 60
  take {
    available = true, status = status or "allowed", claim = "five_hour",
    session = { used = session or 0.42, resets = resets },
    week = { used = week or 0.18, resets = morf.time.now() + 3 * 86400 },
  }
end

function M.refresh()
  local r = reader()
  if r then r:refresh() end
end

local watchers, limits_poller = 0, nil
function M.subscribe()
  watchers = watchers + 1
  local r = reader()
  if r and r.source then r.source:pin(true) end
  if watchers == 1 then
    M.refresh()
    M.ask_limits()
    limits_poller = morf.timer(M.LIMITS_MS, M.ask_limits, true)
  end
end
function M.release()
  watchers = math.max(0, watchers - 1)
  if watchers > 0 then return end
  local r = reader()
  if r and r.source then r.source:pin(false) end
  if limits_poller then limits_poller:cancel() limits_poller = nil end
end

return M
