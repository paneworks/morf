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
local osk = require("lib.osk")
local keyboards = require("lib.keyboards")

local C = theme.color
local M = {}

M.WIDTH = 1060
local PAD = 12

local function dry_run()
  local v = morf.env and morf.env("CAELESTIA_DRY_RUN")
  return v ~= nil and v ~= "" and v ~= "0"
end

-- Whether a text field is asking (the input method is active): text is
-- committed to it then, in any language, not pressed as keys.
local field = false
local send = osk.sender { ime = function() return field end }

local d
local kb = osk.new {
  prefix = "caelestia.osk",
  width = M.WIDTH - 2 * PAD,
  mode = "full",
  numbers = true,
  send = function(e)
    if dry_run() then
      morf.log("warn", "caelestia: keyboard (dry run): " .. tostring(e.text or e.key) .. ((e.mods and e.mods.shift) and " +shift" or ""))
      return
    end
    send(e)
  end,
  look = {
    panel = function() return C.surfaceContainer:alpha(0) end,
    key = function() return C.surfaceContainerHighest end,
    key_dim = function() return C.surfaceContainerHigh end,
    accent = function() return C.primary end,
    on_accent = function() return C.onPrimary end,
    text = function() return C.onSurface end,
    dim = function() return C.onSurfaceVariant end,
    press = function() return C.secondaryContainer end,
    font = theme.font,
    icons = theme.icon_font,
  },
}
M.keys = kb

local ui = require("morf.ui")
d = drawer.new {
  name = "keyboard",
  edge = "bottom",
  width = M.WIDTH,
  height = function() return kb.height() + 2 * PAD end,
  content = ui.Item { x = PAD, y = PAD, width = M.WIDTH - 2 * PAD, height = kb.height, kb.node },
}
M.drawer = d

--- Shows it as `mode` ("full", "dev", "letters", "numbers", "phone",
--- "pattern").
function M.show(mode)
  if mode then kb.mode:set(mode) end
  kb.reset()
  d.set(true)
end

-- ------------------------------------------------------------ by itself --

-- Opened because a field asked (and so shut when it goes), or by hand.
local asked = false
if morf.input_method and morf.input_method.subscribe then
  pcall(morf.input_method.subscribe, function(active)
    field = active == true
    if config.get("keyboard.auto") == false then return end
    if active then
      if not d.open:get() and not keyboards.attached() then
        asked = true
        kb.reset()
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
