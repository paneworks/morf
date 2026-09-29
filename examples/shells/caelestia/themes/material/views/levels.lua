-- Material OSD geometry and motion; service readings/timing are shared.
local morf=require("morf")
local ui=require("morf.ui")
local theme=require("theme")
local kit=require("kit")
local C=theme.color
local V={}
function V.build(model)
  local PILL_W=6
  local KINDS={"volume","brightness"}
  local shown,value,icon,muted=model.shown,model.value,model.icon,model.muted
  local function geometry()
    local r = model.rail_geometry()
    local track = 2 * r.item + r.gap
    local top = math.floor((r.h - track) / 2)
    local scale = math.min(r.w, r.h) / 2160
    return {
      w = r.w, h = r.h, gap = r.gap, pill_h = r.item, item = r.item,
      pill_x = r.w - theme.BORDER / 2 - PILL_W / 2,
      clear = math.max(8, math.floor(14 * scale)),
      tops = { volume = top, brightness = top + r.item + r.gap },
    }
  end

  local pills = {}
  for _, kind in ipairs(KINDS) do
    pills[#pills + 1] = kit.surface {
      id = "levels-" .. kind,
      x = function() return geometry().pill_x end,
      y = function() return geometry().tops[kind] end,
      width = PILL_W, height = function() return geometry().pill_h end,
      radius = PILL_W / 2,
      color = function() return shown:get() == kind and C.primary or C.outlineVariant end,
      opacity = function() return shown:get() == kind and 1 or 0.6 end,
      behavior = { color = { duration = 200 }, opacity = { duration = 200 } },
    }
  end

  -- The swell: a box in the frame's field (`M.shape`, which init.lua adds
  -- to it), the frame bulging out leftwards beside a pill, at its level.
  local g0 = geometry()
  local D = g0.item
  local PAD = math.max(5, math.floor(D * 0.12))
  local SW, SH = D + 2 * PAD + g0.clear + theme.BORDER - PAD, D + 2 * PAD
  local function out_x() return geometry().w - SW end
  local function tucked_x() return geometry().w + theme.SEAM + 2 end
  -- The value, and its icon above it, in a disc the pill's height.
  local bud = kit.surface {
    id = "levels-bud",
    x = PAD, y = PAD, width = D, height = D, radius = D / 2,
    color = function()
      if shown:get() == "volume" and muted() then return C.onSurfaceVariant end
      return C.primary
    end,
    behavior = { color = { duration = theme.duration.small } },
    ui.Column {
      anchors = { center_in = true }, gap = 0, align = "center",
      kit.icon(function()
        local k = shown:get()
        return k ~= "" and icon[k]() or "volume_up"
      end, math.max(16, math.floor(D * 0.3)), function() return C.onPrimary end),
      -- The value's digits morph from one reading to the next.
      kit.morph_number {
        id = "levels-value",
        value = function()
          local k = shown:get()
          if k == "" then return "" end
          return math.floor(value[k]() * 100 + 0.5)
        end,
        size = math.max(9, math.floor(D * 0.22)),
        color = function() return C.onPrimary end,
        duration = 220,
      },
    },
  }
  -- Nothing at rest (no size), so the sidebar carrying the pills out
  -- carries nothing that could show; it takes its size while it is out.
  local swell = ui.Item {
    id = "levels-swell",
    visible = false,
    x = tucked_x(), width = 0, height = 0,
    y = function()
      local k = shown:get()
      return geometry().tops[k ~= "" and k or "volume"] - PAD
    end,
    -- Eased, not sprung: a spring stepped by slow frames never settles.
    behavior = { y = { duration = 200, easing = theme.ease.standard } },
    bud,
  }
  local shape = ui.SdfShape {
    id = "levels-swell-background",
    shape = "box",
    operation = "smooth_union",
    blend_group = 1001,
    track = swell,
    opacity = 0,
    top_right_radius = 0, bottom_right_radius = 0,
    top_left_radius = 9999, bottom_left_radius = 9999,
  }

  local across, closing
  local function stop(h) if h then h:stop() end end

  local function sink(done)
    closing=true
    stop(across)
    across = morf.animation.play {
      {
        parallel = {
          { node = bud, property = "scale", to = 0.6, duration = 160, easing = "in_cubic" },
          { node = bud, property = "opacity", to = 0, duration = 140, easing = "in_cubic" },
          { node = swell, property = "x", to = tucked_x(), duration = theme.duration.drawer_close,
            easing = theme.ease.emphasized_accel, delay = 60 },
        },
      },
      on_finished = function(reason)
        if reason ~= "completed" then return end
        swell.width, swell.height = 0, 0
        swell.visible, shape.opacity = false, 0
        if done then done() end
      end,
    }
  end

  --- Shows `kind` ("volume" or "brightness") for a moment: the frame
  --- swells out at its level with the value, or, out already, slides to it.
  local function show(was)
    swell.visible, shape.opacity = true, 1
    swell.width, swell.height = SW, SH
    if was == "" or closing or (across and across:active() and swell.x > out_x() + 0.5) then
      stop(across)
      closing=false
      across = morf.animation.play {
        {
          parallel = {
            { node = swell, property = "x", to = out_x(), duration = 300, easing = theme.ease.spatial },
            { node = bud, property = "scale", from = 0.6, to = 1, duration = 300, easing = theme.ease.spatial, delay = 60 },
            { node = bud, property = "opacity", from = 0, to = 1, duration = 140, delay = 60 },
          },
        },
      }
    end
  end

  bud.opacity, bud.scale = 0, 0.6
  local root = ui.Item {
    id = "levels",
    anchors = { fill = true },
    ui.Item { anchors = { fill = true }, table.unpack(pills) },
    swell,
  }
  -- The sidebar opening carries the pills out with it, to the strip on its
  -- near side: between the desk and the panel.
  local sidebar = model.sidebar
  kit.ride("levels", root, sidebar.drawer,
    function() return -(theme.SIDE_W + theme.STRIP / 2 + theme.BORDER / 2) end)
  return {node=root,shape=shape,show=show,hide=sink,geometry=geometry}
end
return V
