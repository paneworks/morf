-- One screen of the lock: the desk blurred behind, the island where the bar
-- has it, the clock, the account and the power buttons.
--
-- Port of LockSurface.qml. A key (or, where the lock gets the pointer, a
-- click) wakes it: the clock rises and the account, the field and the power
-- buttons come in underneath; Escape or thirty seconds untouched sends them
-- away again. Blurred enough that text on the desk cannot be read.
--
-- The blur is a picture the shell made before locking (a quarter-size copy
-- of the desk, blurred on an image worker), laid over the sharp one. The
-- original relaxed a live blur as the lock let go; here the blurred copy
-- fades off the sharp one, which is the same picture at every step but the
-- middle ones, for none of the per-frame cost.

local ui = require("morf.ui")
local theme = require("theme")
local lock = require("services.lock")
local kit = require("components.kit")
local lock_island = require("lock.island")
local lock_clock = require("lock.clock")
local lock_account = require("lock.account")
local lock_power = require("lock.power")

local C = theme.color
local st = lock.state

local KEY = {
  RETURN = 0xff0d, KP_ENTER = 0xff8d, BACKSPACE = 0xff08, ESCAPE = 0xff1b,
}

--- Shift, Control, Caps Lock, Alt, Super and friends on their own are
--- nobody typing.
local function modifier(keysym)
  return (keysym >= 0xffe1 and keysym <= 0xffee) or keysym == 0xfe03 or keysym == 0xff7f
end

local function on_key(keysym, text)
  if keysym == KEY.ESCAPE then
    -- The password off the screen and the screen back to its clock.
    lock.rest()
    return
  end
  if modifier(keysym) then return end
  -- The key that wakes the screen only opens it and types nothing
  -- (LockAccount.qml); once awake, keys reach the field.
  local was_awake = st.awake
  lock.rouse()
  if not was_awake then return end
  if keysym == KEY.RETURN or keysym == KEY.KP_ENTER then
    lock.submit()
  elseif keysym == KEY.BACKSPACE then
    lock.backspace()
  elseif text and text ~= "" and text:byte(1) >= 32 and text:byte(1) ~= 127 then
    lock.type(text)
  end
end

-- The battery, read once a minute for every screen's chip.
local battery_level
local function battery_signal()
  if battery_level ~= nil then return battery_level or nil end
  local base
  for _, entry in ipairs(morf.fs.list("/sys/class/power_supply") or {}) do
    if entry.name:match("^BAT") then base = "/sys/class/power_supply/" .. entry.name break end
  end
  if not base then
    battery_level = false
    return nil
  end
  battery_level = morf.signal("impasto.lock.battery", "")
  local function read()
    local capacity = (morf.fs.read(base .. "/capacity") or ""):match("%d+")
    local status = (morf.fs.read(base .. "/status") or ""):match("%a+") or ""
    battery_level:set(capacity and ((status == "Charging" and "󰂄 " or "󰁹 ") .. capacity .. "%") or "")
  end
  read()
  morf.timer(60000, read, true)
  return battery_level
end

--- A ring-less battery chip in the bar's corner, where the machine has one.
local function battery()
  local level = battery_signal()
  if not level then return nil end
  local label = kit.text {
    text = function() return level:get() end,
    size = theme.size.small, weight = 600,
    font_family = function() return theme.font_mono() end,
  }
  return kit.capsule {
    width = function() return (label.layout_width or 0) + 20 end,
    border_width = 0,
    ui.Item { anchors = { center_in = true },
      width = function() return label.layout_width or 0 end,
      height = function() return label.layout_height or 0 end,
      label },
  }
end

--- The whole lock, as one opaque root the size of `width()` x `height()`,
--- over the desk of `output` (each output was photographed on its own).
return function(values)
  local width, height = values.width, values.height

  local desk, blurred = lock.pictures(values.output)

  -- 1 while the lock holds, 0 once it is answered.
  local held = function() return st.leaving and 0 or 1 end
  -- The blur and the wash lift off as the lock lets go, where the sharp
  -- desk is there to be seen under them.
  local clearing = function() return desk ~= "" and held() or 1 end
  local awake = function() return st.awake and 1 or 0 end
  local morph = { duration = math.max(1, theme.duration_morph()), easing = "in_out_cubic" }
  local rise = theme.behave("morph")

  local tree = {
    width = width, height = height,
    color = C.island,
  }
  local function add(node) if node then tree[#tree + 1] = node end end

  -- Under everything: the keyboard's handler, and a click anywhere wakes.
  add(ui.MouseArea {
    anchors = { fill = true },
    on_clicked = function() lock.rouse() end,
    on_key_pressed = on_key,
  })

  -- ---------------------------------------------------------- background --
  if desk ~= "" then
    add(ui.Image { anchors = { fill = true }, source = desk, fill_mode = "preserve_aspect_crop" })
  end
  if blurred ~= "" then
    add(ui.Image {
      anchors = { fill = true }, source = blurred, fill_mode = "preserve_aspect_crop",
      -- Without the sharp one under it there is nothing to fade to.
      opacity = function() return desk ~= "" and clearing() or 1 end,
      behavior = { opacity = morph },
    })
  end
  -- Barely darkened, then the lightest of washes, for separation rather
  -- than contrast: a dark desk dimmed further looks broken.
  add(ui.Rect {
    anchors = { fill = true },
    color = "#000000",
    opacity = function() return 0.185 * clearing() end,
    behavior = { opacity = morph },
  })

  -- -------------------------------------------------------------- status --
  local chip = battery()
  if chip then
    add(ui.Item {
      anchors = { right = true, top = true, right_margin = 10, top_margin = theme.bar_top_margin() },
      width = function() return chip.layout_width or 0 end,
      height = function() return theme.capsule_height() end,
      opacity = held, behavior = { opacity = morph },
      chip,
    })
  end


  -- --------------------------------------------------------------- clock --
  -- Just above the middle at rest; awake, it rises under the island and
  -- steps back a little for the account. Shadowed rather than dimming the
  -- background, which would hide the desk.
  local clock = lock_clock {}
  -- The layer that draws the shadow is as large as its node, and the
  -- figures' tight tracking puts ink past their boxes: room on every side.
  local PAD = 48
  local rest_y = function() return math.floor((height() - (clock.layout_height or 0)) / 2 - 40 + 0.5) end
  local awake_y = function() return math.min(rest_y(), 170) end
  add(ui.Item {
    x = function() return (width() - (clock.layout_width or 0)) / 2 - PAD end,
    y = function() return rest_y() - PAD end,
    width = function() return (clock.layout_width or 0) + 2 * PAD end,
    height = function() return (clock.layout_height or 0) + 2 * PAD end,
    translate_y = function() return (awake_y() - rest_y()) * awake() end,
    scale = function() return 1 - 0.1 * awake() end,
    -- Scaled about the clock's top edge, as the original's Item.Top.
    transform_origin_y = function()
      local h = (clock.layout_height or 0) + 2 * PAD
      return h > 0 and PAD / h or 0
    end,
    opacity = held,
    layer = { enabled = true, shadow_color = morf.color("#00000073"), shadow_blur = 18, shadow_offset_y = 3 },
    behavior = { translate_y = rise, scale = rise, opacity = morph },
    ui.Item { x = PAD, y = PAD, clock },
  })

  -- After the clock, so on a short screen the island grown round the face
  -- scan passes over the date rather than under it.
  add(lock_island { screen_width = width, held = held })

  -- ------------------------------------------------------------- account --
  -- Invisible at rest by opacity, never `visible`: the field is there from
  -- the start, so the first key is its first character.
  local account = lock_account {}
  add(ui.Item {
    x = function() return (width() - 380) / 2 end,
    y = function() return height() - 60 - (account.layout_height or 100) end,
    width = 380,
    height = function() return account.layout_height or 100 end,
    translate_y = function() return 24 * (1 - awake()) end,
    opacity = function() return awake() * held() end,
    behavior = { translate_y = rise, opacity = rise },
    account,
  })

  -- --------------------------------------------------------------- power --
  -- Bottom left at the bar's margin, where the login screen has the same
  -- buttons, and only while awake.
  local power = lock_power {}
  add(ui.Item {
    x = function() return theme.bar_top_margin() + 6 end,
    y = function() return height() - theme.bar_top_margin() - 6 - theme.capsule_height() end,
    width = function() return power.layout_width or 0 end,
    height = function() return theme.capsule_height() end,
    opacity = function() return awake() * held() end,
    visible = function() return st.awake and not st.leaving end,
    behavior = { opacity = rise },
    power,
  })

  return ui.Rect(tree)
end
