-- One countdown, started from the launcher and shown on the island.
--
-- Port of TimerService.qml. The end is a wall-clock time rather than a
-- count of ticks, so a busy machine does not make it drift; the ticker runs
-- only while a countdown does. When it runs out the island says so as a
-- critical notification, through the shell's own notification list.

local theme = require("theme")

local M = {}

local s = {
  running = morf.signal("impasto.timer.running", false),
  -- Keeps `remaining` and `duration`, so resume and restart both work.
  paused = morf.signal("impasto.timer.paused", false),
  label = morf.signal("impasto.timer.label", ""),
  remaining = morf.signal("impasto.timer.remaining", 0),
  duration = morf.signal("impasto.timer.duration", 0),
}
M.signals = s

local ends_at = 0
local ticker

local function now() return morf.time.now_ms() end

-- ------------------------------------------------------------- readings --

function M.running() return s.running:get() end
function M.paused() return s.paused:get() end
function M.label() return s.label:get() end

--- 0..1 of the countdown still to go.
function M.progress()
  local duration = s.duration:get()
  if duration <= 0 then return 0 end
  return math.max(0, math.min(1, s.remaining:get() / duration))
end

function M.seconds() return math.max(0, math.ceil(s.remaining:get() / 1000)) end

--- "4:05", or "1:02:03" past the hour.
function M.display()
  local total = M.seconds()
  local hours = total // 3600
  local minutes = (total % 3600) // 60
  local seconds = total % 60
  if hours > 0 then return ("%d:%02d:%02d"):format(hours, minutes, seconds) end
  return ("%d:%02d"):format(minutes, seconds)
end

--- Fixed indicator hues, not the palette, like the battery: calm, then
--- warning in the last two minutes, then bad in the last thirty seconds.
function M.tint()
  if not s.running:get() then return theme.color.indicatorDim end
  local left = M.seconds()
  if left <= 30 then return theme.color.indicatorBad end
  if left <= 120 then return theme.color.indicatorWarn end
  return theme.color.indicatorTimer
end

-- --------------------------------------------------------------- parsing --

local UNITS = { h = 3600, m = 60, s = 1 }

--- "25m", "1h30", "90s", "5" (minutes), "1:30". Milliseconds, or 0 when the
--- text is not a duration at all.
function M.parse(text)
  local clean = tostring(text or ""):lower():match("^%s*(.-)%s*$")
  if clean == "" then return 0 end

  -- Clock form first: 5:30 is five minutes thirty, not five hours.
  local a, b, c = clean:match("^(%d%d?):(%d%d?):(%d%d?)$")
  if a then return (tonumber(a) * 3600 + tonumber(b) * 60 + tonumber(c)) * 1000 end
  a, b = clean:match("^(%d%d?):(%d%d?)$")
  if a then return (tonumber(a) * 60 + tonumber(b)) * 1000 end

  -- Unit form: runs of "<number><unit>"; a bare trailing number takes the
  -- next unit down ("1h30" is an hour and a half). The whole string must be
  -- made of such runs, so arithmetic like "12*9" is not read as a time.
  local total, last, position = 0, nil, 1
  local matched = false
  while position <= #clean do
    local start, stop, amount, unit = clean:find("^%s*(%d+%.?%d*)%s*([hms]?)", position)
    if not start or stop < position then return 0 end
    local value = tonumber(amount)
    if not value then return 0 end
    if unit == "" then
      unit = last == "h" and "m" or (last == "m" and "s" or "m")
    end
    total = total + value * UNITS[unit]
    last = unit
    matched = true
    position = stop + 1
  end
  if not matched then return 0 end
  return math.floor(total * 1000 + 0.5)
end

--- "1h 5m", "90s" -> "1m 30s".
function M.spell(milliseconds)
  local total = math.floor(milliseconds / 1000 + 0.5)
  local parts = {}
  local hours, minutes, seconds = total // 3600, (total % 3600) // 60, total % 60
  if hours > 0 then parts[#parts + 1] = hours .. "h" end
  if minutes > 0 then parts[#parts + 1] = minutes .. "m" end
  if seconds > 0 then parts[#parts + 1] = seconds .. "s" end
  return #parts > 0 and table.concat(parts, " ") or "0s"
end

-- --------------------------------------------------------------- running --

local complete

local function tick()
  local left = ends_at - now()
  s.remaining:set(math.max(0, left))
  if left <= 0 then complete() end
end

-- The ticker runs only while a countdown is counting.
local function tend()
  local want = s.running:get() and not s.paused:get()
  if want and not ticker then
    ticker = morf.timer(250, tick, true)
  elseif not want and ticker then
    ticker:cancel()
    ticker = nil
  end
end

function M.start(milliseconds, label)
  if not milliseconds or milliseconds <= 0 then return end
  s.duration:set(milliseconds)
  ends_at = now() + milliseconds
  s.remaining:set(milliseconds)
  s.label:set(label or "")
  s.paused:set(false)
  s.running:set(true)
  tend()
end

function M.pause()
  if not s.running:get() or s.paused:get() then return end
  -- The last tick may be up to 250 ms old.
  s.remaining:set(math.max(0, ends_at - now()))
  s.paused:set(true)
  tend()
end

--- Pushes the end forward by the length of the pause.
function M.resume()
  if not s.running:get() or not s.paused:get() then return end
  ends_at = now() + s.remaining:get()
  s.paused:set(false)
  tend()
end

function M.toggle()
  if s.paused:get() then M.resume() else M.pause() end
end

function M.restart()
  if s.duration:get() > 0 then M.start(s.duration:get(), s.label:get()) end
end

function M.cancel()
  s.running:set(false)
  s.paused:set(false)
  s.remaining:set(0)
  s.label:set("")
  tend()
end

complete = function()
  local label = s.label:get()
  s.running:set(false)
  s.paused:set(false)
  s.remaining:set(0)
  s.label:set("")
  tend()
  -- Through the shell's own notification list, so it lands on the island
  -- like any other notification; critical, so it waits to be seen.
  local ok, notify = pcall(require, "services.notifications")
  if ok and notify.post then
    notify.post {
      app = "Timer",
      summary = label ~= "" and label or "Timer finished",
      body = "The countdown has run out.",
      urgency = 2,
    }
  end
  if M.on_finished then M.on_finished(label) end
end

return M
