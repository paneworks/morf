-- Small copies of pictures, for strips and rows of wallpapers.
--
-- Decoding forty full-size wallpapers to draw them at 160 pixels leaves the
-- strip black for a second each time it opens. The original let Qt decode
-- at a source size and cache the result; here each picture is shrunk once
-- with `morf.image.process` into the cache directory, keyed by its path,
-- size and modification time, and the small copy is what is drawn. Two are
-- made at a time, nearest first as they are asked for.

local fs = morf.fs

local M = {}

M.dir = fs.join(fs.dir("cache") or (fs.home() .. "/.cache"), "impasto-morf", "thumbs")

local ready = {}       -- key -> output path, once written
local queued = {}      -- key -> true while waiting or working
local waiting = {}     -- { key, source, output, w, h }
local running = 0
local MAX_RUNNING = 2

-- One signal per copy, so a copy being finished wakes only what shows it.
local signals = {}
local function signal_of(key)
  local found = signals[key]
  if not found then
    found = morf.signal("impasto.thumbnails." .. key, false)
    signals[key] = found
  end
  return found
end

local function hash(text)
  local h = 5381
  for i = 1, #text do h = (h * 33 + text:byte(i)) % 4294967296 end
  return string.format("%08x", h)
end

-- The picture is looked at once per size in a session: a binding asks
-- again every time any copy is finished.
local keys = {}
local function key_of(path, w, h, blur)
  local asked = path .. "|" .. w .. "x" .. h .. "|" .. tostring(blur or 0)
  if keys[asked] then return keys[asked] end
  local info = fs.stat(path)
  local stamp = info and (info.modified or info.size) or 0
  keys[asked] = hash(asked .. "|" .. tostring(stamp))
  return keys[asked]
end

local pump
pump = function()
  while running < MAX_RUNNING and #waiting > 0 do
    -- Newest first: what was asked for last is what is on screen now.
    local job = table.remove(waiting)
    running = running + 1
    local ok = morf.image.process {
      source = job.source, output = job.output, quality = 82,
      ops = job.blur and job.blur > 0
        and { { "resize", job.w, job.h, "fill" }, { "blur", job.blur } }
        or { { "resize", job.w, job.h, "fill" } },
      on_done = function(success)
        running = running - 1
        queued[job.key] = nil
        if success then
          ready[job.key] = job.output
          signal_of(job.key):set(true)
        end
        pump()
      end,
    }
    if not ok then
      running = running - 1
      queued[job.key] = nil
    end
  end
end

--- The small copy of `path` at `w` by `h` (a cover crop), blurred by
--- `blur` (a Gaussian sigma) when given, or "" while it is being made; a
--- binding that calls this follows it being made.
function M.of(path, w, h, blur)
  if not path or path == "" then return "" end
  w, h = math.floor(w or 320), math.floor(h or 200)
  blur = blur and math.max(0, math.min(100, blur)) or nil
  local key = key_of(path, w, h, blur)
  signal_of(key):get()
  if ready[key] then return ready[key] end
  local output = fs.join(M.dir, key .. ".jpg")
  if fs.is_file(output) then
    ready[key] = output
    return output
  end
  if not queued[key] then
    queued[key] = true
    fs.mkdir(M.dir, { parents = true })
    waiting[#waiting + 1] = { key = key, source = path, output = output, w = w, h = h, blur = blur }
    morf.timer(1, pump, false)
  end
  return ""
end

return M
