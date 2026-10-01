-- UI/animation policy for the engine's native rounded-shape geometry.
local ui=require("morf.ui")
local geometry=assert(require("morf").geometry,"native shape geometry requires the updated Morf engine")
local shapes={SEGMENTS=geometry.shape_segments,NAMES=geometry.shape_names}
shapes.polygon=geometry.polygon
shapes.star=geometry.star
shapes.regular=geometry.regular
shapes.lobes=geometry.lobes
shapes.curves=geometry.shape_curves
shapes.path=geometry.shape_path

--- A `ui.Path` showing a shape, morphing when it changes. Props: `shape`
--- (a name, or a function returning one), `color`, `duration` (350),
--- `easing` ("out_cubic"); anything else goes to the `ui.Path` (size,
--- anchors, rotation, ...).
function shapes.Shape(props)
  local source = props.shape
  local current = type(source) == "function" and source() or source
  local path_props = {
    view_box = { 0, 0, 100, 100 },
    d = shapes.path(current),
    morph_to = shapes.path(current),
    morph_progress = 0,
    fill_color = props.color or "#000000",
    behavior = {
      morph_progress = { duration = props.duration or 350, easing = props.easing or "out_cubic" },
    },
  }
  for k, v in pairs(props) do
    if k ~= "shape" and k ~= "color" and k ~= "duration" and k ~= "easing" then
      path_props[k] = v
    end
  end
  local node = ui.Path(path_props)
  if type(source) == "function" then
    -- The two ends take turns: the one on show stays put and the other
    -- becomes the new shape, so a change never jumps back to a start.
    local at_end = false
    morf.effect("m3shapes.morph", function()
      local name = source()
      if name == current then return end
      current = name
      if at_end then
        node.d = shapes.path(name)
        node.morph_progress = 0
      else
        node.morph_to = shapes.path(name)
        node.morph_progress = 1
      end
      at_end = not at_end
    end, { owner = node })
  end
  return node
end

return shapes
