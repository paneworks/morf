-- Sound bars, for the spectrum on a square and along a screen edge.
--
-- Port of SpectrumBars.qml. The original ran cava; here the bands come from
-- `morf.audio.monitor` on the default output, running only while some bars
-- are listening. `build { x, y, width, height, looks, listening, edge }`:
-- `looks()` is `services.desktop.spectrum_of(row)`, `listening()` whether the
-- bars draw at all, `edge` where they rise from.

local ui = require("morf.ui")

local M = {}

M.BANDS = 48
local bands = {}
for i = 1, M.BANDS do bands[i] = morf.signal("impasto.desk.band." .. i, 0) end

local listeners = 0
local monitor = nil

local function start()
  if monitor or not morf.audio or not morf.audio.monitor then return end
  local ok, handle = pcall(morf.audio.monitor, {
    rate_hz = 30, bands = M.BANDS,
    on_level = function(_, _, levels)
      for i = 1, M.BANDS do
        local v = levels and levels[i] or 0
        bands[i]:set(math.max(v, bands[i]:get() * 0.8))
      end
    end,
  })
  monitor = ok and handle or nil
end

local function stop()
  if monitor and monitor.stop then pcall(monitor.stop, monitor) end
  monitor = nil
  for i = 1, M.BANDS do bands[i]:set(0) end
end

--- The level of band `i` (1..BANDS), 0..1; a binding follows it.
function M.level(i) return bands[math.max(1, math.min(M.BANDS, i))]:get() end

--- Counts a listener in or out; the monitor runs while there is one.
function M.listen(on)
  listeners = math.max(0, listeners + (on and 1 or -1))
  if listeners > 0 then start() else stop() end
end

function M.build(values)
  local w, h = values.width, values.height
  local looks = values.looks
  local count = math.max(1, math.floor((w + 6) / 16))
  local nodes = {}
  for i = 1, count do
    local band = math.floor((i - 1) / count * M.BANDS) + 1
    nodes[i] = ui.Rect {
      x = (i - 1) * (w / count), width = math.max(2, w / count - 6),
      y = function() return h - math.max(3, M.level(band) * h) end,
      height = function() return math.max(3, M.level(band) * h) end,
      radius = 3,
      color = function() return looks().color end,
    }
  end
  local listening = false
  return ui.Item {
    x = values.x, y = values.y, width = w, height = h,
    visible = values.listening,
    ui.Timer {
      interval = 1, running = function()
        local want = values.listening() and true or false
        return want ~= listening
      end,
      on_triggered = function()
        listening = not listening
        M.listen(listening)
      end,
    },
    table.unpack(nodes),
  }
end

return M
