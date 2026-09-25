-- The wallpaper, drawn by the shell.
--
-- impasto handed the picture to awww, which drew it with a transition; the
-- shell draws it itself here, on a background layer of its own, so the
-- change and the palette that follows it are one motion. The picture that
-- was up stays underneath while the new one is revealed over it: faded in,
-- wiped across, or opened as a circle from the middle; `outer` closes the
-- old one into the middle instead.

local ui = require("morf.ui")
local theme = require("theme")
local wallpaper = require("services.wallpaper")

local M = {}

M.DURATION = 900

local function play(tracks) morf.animation.play(tracks) end

--- The wallpaper's root, `width` by `height`.
function M.build(width, height)
  local diagonal = math.ceil(math.sqrt(width * width + height * height))
  local under_path = morf.signal("impasto.wallpaper.under", wallpaper.current:get())
  local over_path = morf.signal("impasto.wallpaper.over", wallpaper.current:get())

  local under = ui.Image {
    width = width, height = height, fill_mode = "preserve_aspect_crop",
    source = function() return under_path:get() end,
  }
  local over_image = ui.Image {
    width = width, height = height, fill_mode = "preserve_aspect_crop",
    source = function() return over_path:get() end,
  }
  local reveal = ui.ClipRect {
    x = 0, y = 0, width = width, height = height, radius = 0,
    over_image,
  }

  local function finish()
    under_path:set(over_path:get())
  end

  local function transition(kind)
    local ms = math.max(1, math.floor(M.DURATION * theme.motion()))
    local ease = "in_out_cubic"
    if kind == "none" or theme.motion() == 0 then
      reveal.opacity = 1
      finish()
      return
    end
    if kind == "fade" then
      play { { node = reveal, property = "opacity", duration = ms,
        keyframes = { { at = 0, value = 0 }, { at = 1, value = 1, easing = ease } } } }
    elseif kind == "wipe" or kind == "wave" then
      play { { node = reveal, property = "width", duration = ms,
        keyframes = { { at = 0, value = 0 }, { at = 1, value = width, easing = kind == "wave" and "in_out_sine" or ease } } } }
    elseif kind == "circle" or kind == "outer" then
      -- A circle growing from the middle: the clip grows about the centre
      -- while the picture inside is moved the other way, so the picture
      -- itself never moves on screen.
      local r0, r1 = 0, diagonal / 2
      local track = function(property, from, to)
        return { node = property[1], property = property[2], duration = ms,
          keyframes = { { at = 0, value = from }, { at = 1, value = to, easing = ease } } }
      end
      play {
        track({ reveal, "x" }, width / 2 - r0, width / 2 - r1),
        track({ reveal, "y" }, height / 2 - r0, height / 2 - r1),
        track({ reveal, "width" }, 2 * r0, 2 * r1),
        track({ reveal, "height" }, 2 * r0, 2 * r1),
        track({ reveal, "radius" }, r0, r1),
        track({ over_image, "x" }, r0 - width / 2, r1 - width / 2),
        track({ over_image, "y" }, r0 - height / 2, r1 - height / 2),
      }
    end
    morf.timer(ms + 60, finish, false)
  end

  -- Each new picture: the old one goes under, the new one is revealed.
  local seen = wallpaper.generation:get()
  morf.effect("impasto.wallpaper.transition", function()
    local generation = wallpaper.generation:get()
    if generation == seen then return end
    seen = generation
    local path = wallpaper.current:get()
    local kind = wallpaper.transition:get()
    morf.timer(1, function()
      under_path:set(over_path:get())
      over_path:set(path)
      -- The clip back to where the reveal starts before it runs.
      reveal.x, reveal.y, reveal.width, reveal.height, reveal.radius = 0, 0, width, height, 0
      over_image.x, over_image.y = 0, 0
      transition(kind)
    end, false)
  end)

  return ui.Item {
    width = width, height = height,
    ui.Rect { width = width, height = height, color = theme.color.background },
    under,
    reveal,
    -- The desk's widgets, on the wallpaper and under every window.
    require("desktop.desk").rest(width, height),
  }
end

--- Puts the wallpaper on a background layer of its own, one per screen.
function M.open_layer(width, height)
  return morf.window.layer {
    namespace = "impasto-wallpaper",
    layer = "background",
    anchors = { top = true, bottom = true, left = true, right = true },
    exclusive_zone = -1,
    keyboard_focus = "none",
    width = width, height = height,
    -- A layer surface starts closed: without this the wallpaper was built
    -- and never shown under a compositor with layer shell.
    visible = true,
    root = M.build(width, height),
  }
end

return M
