-- The spectrum under a playing track, and its loudness.
--
-- The original ran cava as a process while something subscribed. Here it is
-- `morf.audio.monitor` on the default output's monitor, running only while a
-- player is playing -- the island shows the bars beside the time whenever
-- one is -- and stopped the moment it pauses. It follows the actual audio,
-- so it stops on silence.

local media = require("services.media")

local M = {}

M.BANDS = 8
M.bands = {}
for index = 1, M.BANDS do M.bands[index] = morf.signal("impasto.spectrum.band." .. index, 0) end
M.level = morf.signal("impasto.spectrum.level", 0)


local meter
local function stop()
  if meter then pcall(meter.stop) meter = nil end
  for _, band in ipairs(M.bands) do band:set(0) end
  M.level:set(0)
end

morf.effect("impasto.spectrum.run", function()
  local want = media.playing() and morf.audio.available()
  if want and not meter then
    local ok, handle = pcall(morf.audio.monitor, {
      rate_hz = 30, bands = M.BANDS,
      on_level = function(left, right, bands)
        M.level:set(math.min(1, math.max(tonumber(left) or 0, tonumber(right) or 0)))
        if type(bands) == "table" then
          for index = 1, M.BANDS do M.bands[index]:set(math.min(1, tonumber(bands[index]) or 0)) end
        end
      end,
    })
    if ok then meter = handle end
  elseif not want and meter then
    stop()
  end
end)

return M
