-- The account pill: the picture and the name, then the password field.
--
-- Port of LockAccount.qml. One black pill: the picture, the name and how to
-- get in; from the first character, the same pill stretched into the field
-- with the dots and a button to send them. Only the count of characters
-- reaches the screen -- the password stays a local in the lock service.

local ui = require("morf.ui")
local theme = require("theme")
local lock = require("services.lock")
local account = require("services.account")
local kit = require("components.kit")

local C = theme.color
local st = lock.state

local PILL_HEIGHT = 60
local FACE = 48
local INSET = 6
local FIELD_WIDTH = 380

--- A round picture, or initials when there is none: impasto's Avatar.
local function avatar(size)
  local source = account.avatar()
  return ui.ClipRect {
    x = INSET, y = (PILL_HEIGHT - size) / 2,
    width = size, height = size, radius = size / 2,
    color = C.islandSurface,
    border_width = 2, border_color = C.hairline,
    content_under_border = true,
    source ~= "" and ui.Image {
      anchors = { fill = true },
      source = source,
      fill_mode = "preserve_aspect_crop",
      source_width = size * 2, source_height = size * 2,
    } or kit.text {
      anchors = { center_in = true },
      text = account.initials,
      size = math.floor(size * 0.36 + 0.5), weight = 300,
    },
  }
end

--- The pill, and the message under it.
return function(values)
  local typing = function() return st.typed > 0 or st.authenticating or st.failed end

  local resting = ui.Column {
    gap = 1,
    kit.text { text = account.name, size = theme.size.large, weight = 600 },
    kit.text {
      text = function() return st.face_ready and "Face unlock or password" or "Enter your password" end,
      size = theme.size.regular, color = C.textMuted,
    },
  }

  local dots = kit.text {
    text = function() return string.rep("●", st.typed) end,
    size = theme.size.small, letter_spacing = 3,
    elide = "left",
    width = FIELD_WIDTH - (INSET + FACE + 18) - 44 - 8 - 12,
  }

  -- The spinner: a quarter of a ring, turning while PAM checks. The ring
  -- is whole and a square clips all but one quadrant of it.
  local spinner = ui.Item {
    anchors = { center_in = true },
    width = 22, height = 22,
    visible = function() return st.authenticating end,
    ui.ClipRect {
      x = 0, y = 0, width = 11, height = 11,
      color = "#00000000",
      ui.Rect {
        x = 0, y = 0, width = 22, height = 22, radius = 11,
        color = "#00000000",
        border_width = 2, border_color = C.text,
      },
    },
  }
  local spinning
  morf.effect("impasto.lock.account.spinner", function()
    local on = st.authenticating
    if on and not spinning then
      spinning = morf.animation.play {
        loops = "forever",
        { node = spinner, property = "rotation", from = 0, to = 360, duration = 900, easing = "linear" },
      }
    elseif not on and spinning then
      spinning:stop()
      spinning = nil
    end
  end)

  local send_hover = kit.hover_signal("lock.send")
  local send = ui.Rect {
    anchors = { right = true, top = true, right_margin = 8, top_margin = (PILL_HEIGHT - 44) / 2 },
    width = 44, height = 44, radius = 22,
    color = function()
      if st.authenticating then return "#00000000" end
      return send_hover:get() and C.textMuted() or C.text()
    end,
    opacity = function() return typing() and 1 or 0 end,
    scale = function() return typing() and 1 or 0.6 end,
    behavior = {
      opacity = theme.behave("medium"),
      scale = { duration = math.max(1, theme.duration_medium()), easing = "out_back" },
      color = theme.behave("fast"),
    },
    kit.glyph {
      anchors = { center_in = true },
      glyph = "󰁔", size = 20, color = C.island,
      visible = function() return not st.authenticating end,
    },
    spinner,
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      enabled = function() return not st.authenticating end,
      on_entered = function() send_hover:set(true) end,
      on_exited = function() send_hover:set(false) end,
      on_clicked = function() lock.submit() end,
    },
  }

  local pill_width = function()
    if typing() then return FIELD_WIDTH end
    return INSET + FACE + 14 + (resting.layout_width or 0) + 26
  end
  -- Centred on the width it is heading for, and moved on the same curve as
  -- that width, so it stays centred through the whole stretch.
  local pill = ui.Rect {
    x = function() return (FIELD_WIDTH - pill_width()) / 2 end,
    width = pill_width,
    height = PILL_HEIGHT,
    radius = PILL_HEIGHT / 2,
    -- Pure black, like the island: over a photograph only an opaque capsule
    -- reads as a surface.
    color = C.island,
    border_width = 1,
    border_color = function()
      if st.failed then return C.indicatorBad end
      return typing() and C.accent() or C.islandBorder
    end,
    shadow_color = "#00000080", shadow_blur = 16, shadow_offset_y = 4,
    behavior = { x = theme.behave("morph"), width = theme.behave("morph"), border_color = theme.behave("fast") },
    -- Anywhere on the pill wakes the screen.
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "text",
      on_clicked = function() lock.rouse() end,
    },
    avatar(FACE),
    ui.Item {
      x = INSET + FACE + 14,
      y = function() return (PILL_HEIGHT - (resting.layout_height or 0)) / 2 end,
      opacity = function() return typing() and 0 or 1 end,
      behavior = { opacity = theme.behave("fast") },
      resting,
    },
    ui.Item {
      x = INSET + FACE + 18,
      y = function() return (PILL_HEIGHT - (dots.layout_height or 0)) / 2 end,
      opacity = function() return typing() and 1 or 0 end,
      behavior = { opacity = theme.behave("fast") },
      dots,
    },
    send,
  }

  -- The shake is seen before the text below it.
  local holder = ui.Item {
    width = FIELD_WIDTH,
    height = PILL_HEIGHT,
    pill,
  }
  local seen = st.refusals
  morf.effect("impasto.lock.account.refusal", function()
    local count = st.refusals
    if count == seen then return end
    seen = count
    morf.animation.play {
      loops = 2,
      { node = pill, property = "translate_x", to = -9, duration = 55, easing = "out_cubic" },
      { node = pill, property = "translate_x", to = 9, duration = 55, easing = "out_cubic" },
      { node = pill, property = "translate_x", to = 0, duration = 55, easing = "out_cubic" },
    }
  end)

  local message = kit.text {
    text = function() return st.message end,
    size = theme.size.small, weight = 600,
    color = C.indicatorBad,
    layer = { enabled = true, shadow_color = morf.color("#000000b3"), shadow_blur = 6 },
  }

  local out = {
    width = FIELD_WIDTH,
    height = PILL_HEIGHT + 40,
    holder,
    ui.Item {
      x = function() return (FIELD_WIDTH - (message.layout_width or 0)) / 2 end,
      y = PILL_HEIGHT + 14,
      visible = function() return st.message ~= "" end,
      message,
    },
  }
  for key, value in pairs(values or {}) do out[key] = value end
  return ui.Item(out)
end
