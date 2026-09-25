-- A drawer: a panel that grows out of the frame.
--
-- The panel is an ordinary node inside the frame's opening, tucked past the
-- edge it hangs from while shut. Its background is not drawn by the panel:
-- it is one layer of the frame's own distance field (`shape`), a box that
-- follows wherever the panel is drawn (`track`) and joins the frame with a
-- circular seam, so the frame itself seems to bulge out into the drawer,
-- with a concave fillet where the drawer's sides meet the frame.
--
-- Opening and closing each run their own curve and duration, measured off
-- films of the reference: a separate play per direction rather than one
-- behavior, which can only have one.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")

local M = {}

M.all = {}

local groups = 0

--- `spec`: `name`, `edge` ("top" or "bottom"), `width`, `height` (numbers
--- or bindings), `content` (a node, laid out in the panel), `props` (more
--- properties for the panel).
function M.new(spec)
  groups = groups + 1
  local d = { name = spec.name, edge = spec.edge }
  d.open = morf.signal("caelestia.drawer." .. spec.name, false)
  local sign = spec.edge == "top" and -1 or 1

  local props = spec.props or {}
  props.id = "drawer-" .. spec.name
  props.width = spec.width
  props.height = spec.height
  props.anchors = spec.edge == "top" and { top = true, horizontal_center = true }
    or { bottom = true, horizontal_center = true }
  props.visible = false
  props.behavior = props.behavior or {}
  -- The results of a search change the launcher's height: it follows at
  -- the reference's pace.
  props.behavior.height = props.behavior.height
    or { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
  props[#props + 1] = spec.content
  local panel = ui.Item(props)
  d.panel = panel

  --- How far the panel moves to be out of sight: its size and the seam, so
  --- not even the fillet of its far edge dents the frame.
  local function tucked()
    local h = panel.height_target or panel.height or 0
    return sign * (h + theme.SEAM + theme.BORDER + 2)
  end
  panel.translate_y = tucked()

  local running
  local function move(opening)
    if running then running:stop() end
    if opening then panel.visible = true end
    local slide = {
      node = panel, property = "translate_y", to = opening and 0 or tucked(),
      duration = opening and theme.duration.drawer_open or theme.duration.drawer_close,
      easing = opening and theme.ease.spatial or theme.ease.emphasized_accel,
    }
    -- The reference's contents fade in over the first hundred-odd
    -- milliseconds of the slide; closing only slides.
    if opening then spec.content.opacity = 0 end
    running = morf.animation.play {
      {
        parallel = {
          slide,
          { node = spec.content, property = "opacity", to = 1, duration = opening and 150 or 1,
            easing = theme.ease.standard_decel },
        },
      },
      on_finished = function(reason)
        if reason == "completed" and not d.open:get() then panel.visible = false end
      end,
    }
  end

  local was = false
  morf.effect("caelestia.drawer." .. spec.name, function()
    local now = d.open:get()
    if now == was then return end
    was = now
    move(now)
  end, { owner = panel })

  -- Its background in the frame's field: square on the frame's side (the
  -- seam rounds that join), the reference's rounding on the far side.
  local near, far = 0, theme.ROUNDING
  d.shape = ui.SdfShape {
    shape = "box",
    operation = "smooth_union",
    blend_group = groups,
    track = panel,
    top_left_radius = spec.edge == "top" and near or far,
    top_right_radius = spec.edge == "top" and near or far,
    bottom_left_radius = spec.edge == "bottom" and near or far,
    bottom_right_radius = spec.edge == "bottom" and near or far,
  }

  function d.set(on) d.open:set(on and true or false) end
  function d.toggle() d.open:set(not d.open:get()) end
  function d.is_open() return d.open:get() end

  M.all[#M.all + 1] = d
  M[spec.name] = d
  return d
end

--- Shuts every drawer.
function M.close_all()
  for _, d in ipairs(M.all) do d.set(false) end
end

return M
