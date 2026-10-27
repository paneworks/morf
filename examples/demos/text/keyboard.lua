-- An on-screen keyboard, as its own process.
--
-- It types into *other* programs. Not through a socket, a pipe or an agreement
-- with the thing being typed into — through `zwp_virtual_keyboard_v1`, which
-- hands key events to the compositor, which routes them to whatever holds
-- focus. So this keyboard works with every Wayland client on the machine, and
-- nothing has to be written to accept it. Run it and type:
--
--     morf examples/demos/text/keyboard.lua
--
-- The keyboard itself is `library/lib/shell/osk.lua`, because it is also wanted
-- inside a greeter — a kiosk compositor shows one window, so a login screen
-- cannot put its keyboard in a second surface and see it. This file is the
-- half that is about having a surface of one's own; that file is the keyboard.
--
-- Two consequences worth being explicit about.
--
-- It must never take keyboard focus. A keyboard that holds focus types into
-- itself and the program the person is looking at receives nothing, so the
-- surface asks for `none` and means it.
--
-- And it emits *key presses*, not text, wherever it can: `osk.sender()` turns
-- `a` into evdev 30 and `A` into evdev 30 with shift held. The receiving
-- program sees a key press, so its own shortcuts, repeat, and input method all
-- behave exactly as they do for real hardware. Only a character no key makes
-- goes through the input method as text.

local morf = require("morf")
local ui = require("morf.ui")
local osk = require("lib.shell.osk")

local screen = morf.screens[1]
local W = (screen and screen.width) or 1920

local kb = osk.new { width = W, send = osk.sender() }
local height = kb.height()

morf.surface.width = W
morf.surface.height = height
morf.surface.anchors = { left = true, right = true, bottom = true }
morf.surface.layer = "overlay"
-- The whole point. A keyboard that holds focus types into itself.
morf.surface.keyboard_focus = "none"
-- No exclusive zone: an on-screen keyboard floats over what it is typing into
-- rather than resizing it, because the thing being typed into is usually a
-- password field that has already been laid out.
morf.surface.exclusive_zone = 0

ui.Item { width = W, height = height, kb.node }
