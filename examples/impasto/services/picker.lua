-- The colour under the pointer, onto the clipboard.
--
-- Port of PickerService.qml, done natively instead of through hyprpicker:
-- the screen is photographed (`morf.screencopy.save`), the picker surface
-- (capture/picker.lua) shows that still picture with a magnifier under the
-- pointer, and a click reads the pixel from the same picture with
-- `morf.image.pixel`, copies its hex with `morf.clipboard.set` (no
-- trailing newline) and says so. Escape or a right click lets it go.

local fs = morf.fs
local M = {}

M.icon = "󰈊"

local s = {
  active = morf.signal("impasto.picker.active", false),
  busy = morf.signal("impasto.picker.busy", false),
  photo = morf.signal("impasto.picker.photo", ""),
  photo_w = morf.signal("impasto.picker.photo_w", 0),
  photo_h = morf.signal("impasto.picker.photo_h", 0),
  -- The colour under the pointer, as the magnifier's label reads it.
  hover = morf.signal("impasto.picker.hover", ""),
  last = morf.signal("impasto.picker.last", ""),
}
M.signals = s

function M.active() return s.active:get() end
function M.photo() return s.photo:get() end
function M.hover() return s.hover:get() end
function M.last() return s.last:get() end
function M.picking() return s.active:get() or s.busy:get() end
function M.available() return morf.screencopy ~= nil end

local function screen()
  local entry = (morf.screens or {})[1] or {}
  return tonumber(entry.width) or 1920, tonumber(entry.height) or 1080, entry.name
end

function M.ratio()
  local w = screen()
  local pw = s.photo_w:get()
  if pw <= 0 or w <= 0 then return 1 end
  return pw / w
end

--- Photographs the screen and shows the lens. Calling it again while the
--- lens is up lets it go, as the original's second press did.
function M.pick(after)
  if s.active:get() then M.cancel() return end
  if s.busy:get() then return end
  s.busy:set(true)
  local _, _, name = screen()
  local path = fs.join(fs.dir("runtime") or "/tmp", "impasto-pick-" .. morf.time.now_ms() .. ".png")
  morf.timer(math.max(1, after or 1), function()
    local queued = pcall(morf.screencopy.save, {
      path = path, output = name,
      on_done = function(done, info)
        s.busy:set(false)
        if not done then return end
        s.photo:set(path)
        s.photo_w:set(tonumber(info and info.width) or 0)
        s.photo_h:set(tonumber(info and info.height) or 0)
        s.hover:set("")
        s.active:set(true)
      end,
    })
    if not queued then s.busy:set(false) end
  end, false)
end

function M.cancel()
  if not s.active:get() then return end
  s.active:set(false)
  local photo = s.photo:get()
  s.photo:set("")
  if photo ~= "" then fs.remove(photo) end
end

local function hex(colour)
  if not colour then return nil end
  local text = tostring(colour.hex and colour:hex() or colour)
  return text:sub(1, 7):lower()
end

-- One read in flight at a time: the pointer moves faster than a worker
-- decodes, and only the latest position matters.
local reading, wanted = false, nil
local function read_next()
  if reading or not wanted then return end
  local x, y = wanted[1], wanted[2]
  wanted = nil
  reading = true
  local photo = s.photo:get()
  local queued = morf.image.pixel(photo, x, y, function(ok, colour)
    reading = false
    if ok and s.active:get() then s.hover:set(hex(colour) or "") end
    read_next()
  end)
  if not queued then reading = false end
end

local function physical(x, y)
  local r = M.ratio()
  local px = math.max(0, math.min(s.photo_w:get() - 1, math.floor(x * r)))
  local py = math.max(0, math.min(s.photo_h:get() - 1, math.floor(y * r)))
  return px, py
end

--- The pointer moved to (x, y) in layout units: read what is under it.
function M.look(x, y)
  if not s.active:get() then return end
  local px, py = physical(x, y)
  wanted = { px, py }
  read_next()
end

--- Takes the pixel at (x, y) and puts its hex on the clipboard.
function M.take(x, y)
  if not s.active:get() then return end
  local photo = s.photo:get()
  local px, py = physical(x, y)
  s.active:set(false)
  s.photo:set("")
  morf.image.pixel(photo, px, py, function(ok, colour)
    fs.remove(photo)
    local value = ok and hex(colour)
    if not value or not value:match("^#%x%x%x%x%x%x$") then return end
    s.last:set(value)
    pcall(morf.clipboard.set, value)
    local okn, notify = pcall(require, "services.notifications")
    if okn and notify.post then
      notify.post { app = "Colour picker", summary = value, body = "Copied to the clipboard", icon = M.icon }
    end
  end)
end

return M
