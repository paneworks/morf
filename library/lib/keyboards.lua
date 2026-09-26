-- Whether a real keyboard is attached: what an on-screen keyboard asks
-- before it offers itself.
--
--   local keyboards = require("lib.keyboards")
--   keyboards.attached()        -- true while one is plugged in (or built in)
--   keyboards.list()            -- { { name, handlers } }, the ones that count
--
-- Read from /proc/bus/input/devices. Plenty of things there call themselves
-- keyboards and are not: a power button, a laptop's hotkeys, a dock's audio
-- controls, the extra interface on a gaming mouse, a virtual keyboard (an
-- on-screen keyboard's own). A keyboard here is a device with letter keys
-- and LEDs -- the caps lock light every real one has -- and a name that is not
-- one of those.

local morf = require("morf")

local keyboards = {}

-- Names of things that report keys but are not keyboards.
local NOT = {
  "power button", "sleep button", "lid switch", "video bus", "hid events", "button array",
  "consumer control", "system control", "pc speaker", "wmi hotkeys", "headset", "audio",
  "mouse", "virtual", "ydotool", "morf", "wtype", "uinput",
}

local function counts(device)
  local name = (device.name or ""):lower()
  for _, word in ipairs(NOT) do
    if name:find(word, 1, true) then return false end
  end
  local handlers = device.handlers or ""
  -- Letter keys and the lights for caps and num lock.
  return handlers:find("kbd", 1, true) ~= nil and handlers:find("leds", 1, true) ~= nil
end

--- The keyboards that count, each `{ name, handlers }`.
function keyboards.list(path)
  local ok, text = pcall(morf.fs.read, path or "/proc/bus/input/devices")
  if not ok or type(text) ~= "string" then return {} end
  local found = {}
  for block in (text .. "\n\n"):gmatch("(.-)\n\n") do
    local device = {
      name = block:match('N: Name="([^"]*)"'),
      handlers = block:match("H: Handlers=([^\n]*)"),
    }
    if device.name and counts(device) then found[#found + 1] = device end
  end
  return found
end

--- Whether one is attached now.
function keyboards.attached(path)
  return #keyboards.list(path) > 0
end

return keyboards
