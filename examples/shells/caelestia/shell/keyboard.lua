-- The on-screen keyboard: a drawer of its own along the bottom edge --
-- beside the capture drawer, not in it -- carrying lib.board's keys, which
-- type into whatever has the focus through a virtual keyboard. The drawer
-- never takes the focus itself, so the text goes where it was going.
--
-- It comes up by itself when a program asks for text (a text field is
-- focused: the input-method protocol says so) and no real keyboard is
-- attached (lib.keyboards), and goes when the field does -- unless it was
-- opened by hand. By hand: `keyboard [toggle|open|close]` over IPC, which a
-- key can be bound to. `keyboard.auto = false` in the settings stops the
-- coming up by itself.

local morf = require("morf")
local theme = require("theme")
local config = require("config")
local drawer = require("drawer")
local board = require("lib.board")
local keyboards = require("lib.keyboards")

local C = theme.color
local M = {}

M.WIDTH = 1020

local SHIFT_MASK = 1

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

--- One key struck: pressed and released on the virtual keyboard, with shift
--- held around it when the board was shifted.
local function press(code, shift, label)
  if dry_run() then
    morf.log("info", "caelestia: keyboard (dry run): " .. tostring(label or code))
    return
  end
  local vk = morf.virtual_keyboard
  if not vk then return end
  if shift then vk.modifiers(SHIFT_MASK, 0, 0, 0) end
  vk.key(code, true)
  vk.key(code, false)
  if shift then vk.modifiers(0, 0, 0, 0) end
end

local PAD = 18
local keys, board_h = board.build {
  prefix = "caelestia.keyboard",
  width = M.WIDTH - 2 * PAD,
  x = PAD, y = PAD,
  key = press,
  -- The shell's own colours, so the board re-themes with the desk.
  look = {
    panel = function() return C.surfaceContainer:alpha(0) end,
    keyface = function() return C.surfaceContainerHighest end,
    live = function() return C.primary end,
    label = function() return C.onSurface end,
    dim = function() return C.onSurfaceVariant end,
    down = function() return C.primary end,
    font = theme.font,
  },
}

M.HEIGHT = board_h + 2 * PAD

local d = drawer.new {
  name = "keyboard",
  edge = "bottom",
  width = M.WIDTH,
  height = M.HEIGHT,
  content = keys,
}
M.drawer = d

-- ------------------------------------------------------------ by itself --

-- Opened because a field asked (and so shut when it goes), or by hand.
local asked = false
if morf.input_method and morf.input_method.subscribe then
  pcall(morf.input_method.subscribe, function(active)
    if config.get("keyboard.auto") == false then return end
    if active then
      if not d.open:get() and not keyboards.attached() then
        asked = true
        d.set(true)
      end
    elseif asked then
      asked = false
      d.set(false)
    end
  end)
end
-- Opened or shut by hand, it is the hand's until the next field.
morf.effect("caelestia.keyboard.hand", function()
  if not d.open:get() then asked = false end
end)

return M
