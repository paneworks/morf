-- Tsugumori motion, adapted from its MIT-licensed Menu and ControlCenterView.
-- See REFERENCE.md and LICENSE-Tsugumori. Reversals retain the current pose.
local morf = require("morf")
local ui = require("morf.ui")
return function(theme)
  local M = { liquid_cards = false }
  -- Keep the cover/swap/reveal order, with a shorter, consistent cadence.
  local PAGE_COVER, REVEAL = 140, 220
  local ENTER, COVER, WITHDRAW, OVERLAP = 280, 270, 330, 170
  local ENTRY_DELAY, ENTRY_MOVE, ENTRY_LEAVE, STAGGER = 180, 240, 120, 50
  function M.page_wipe(host, width, id)
    local edge = ui.Rect { id=id.."-page-flash",width=4,
      anchors={right=true,top=true,bottom=true},
      color=function() return theme.color.onPrimary end,opacity=0 }
    local cover = ui.Rect { id = id .. "-page-curtain", z = 90,
      anchors = { top = true, bottom = true }, x = 0, width = 0, visible = false,clip=true,
      color = function() return theme.color.primary end,
      ui.MouseArea { anchors = { fill = true } },
      edge,
    }
    ui.reparent(cover, host)
    local running, generation = nil, 0
    return function(swap, immediate)
      generation = generation + 1
      local own = generation
      if running then running:stop() end
      if immediate then cover.visible = false cover.width = 0 edge.opacity=0 swap() return end
      cover.visible = true
      running = morf.animation.play { { parallel = {
        { node = cover, property = "x", to = 0, duration = PAGE_COVER, easing = "in_out_quint" },
        { node = cover, property = "width", to = width(), duration = PAGE_COVER, easing = "in_out_quint" },
        { node = edge, property = "opacity", from = 0, to = .85, duration = PAGE_COVER, easing = "out_cubic" },
      } }, on_finished = function(reason)
        if reason ~= "completed" or own ~= generation then return end
        swap()
        running = morf.animation.play { { parallel = {
          { node = cover, property = "x", to = width(), duration = REVEAL, easing = "out_expo" },
          { node = cover, property = "width", to = 0, duration = REVEAL, easing = "out_expo" },
          { node = edge, property = "opacity", to = 0, duration = REVEAL, easing = "out_cubic" },
        } }, on_finished = function(why)
          if why == "completed" and own == generation then cover.visible = false end
        end }
      end }
    end
  end
  function M.entries(entries, coming, opts)
    if require("themes.session").restoring then
      for _, entry in ipairs(entries) do
        entry.node.opacity = coming and 1 or 0
        entry.node.scale, entry.node.translate_x, entry.node.translate_y = 1, 0, 0
        if entry.shape then entry.shape.opacity = coming and 1 or 0 end
      end
      return {}, 0
    end
    opts = opts or {}
    local handles = {}
    for k, entry in ipairs(entries) do
      local node = entry.node
      local fresh = not (node.opacity > 0 and node.opacity < 1)
      local delay = coming and ((opts.delay or ENTRY_DELAY) + (k - 1) * (opts.stagger or STAGGER)) or 0
      local steps = {
        { node = node, property = "translate_x", from = coming and fresh and -12 or nil, to = coming and 0 or -12,
          delay = delay, duration = coming and ENTRY_MOVE or ENTRY_LEAVE, easing = "out_cubic" },
        { node = node, property = "translate_y", from = coming and fresh and -8 or nil, to = coming and 0 or -6,
          delay = delay, duration = coming and ENTRY_MOVE or ENTRY_LEAVE, easing = "out_cubic" },
        { node = node, property = "opacity", from = coming and fresh and 0 or nil, to = coming and 1 or 0,
          delay = delay, duration = coming and ENTRY_MOVE or ENTRY_LEAVE, easing = "out_cubic" },
      }
      if entry.shape then
        steps[#steps + 1] = { node = entry.shape, property = "opacity", from = coming and fresh and 0 or nil,
          to = coming and 1 or 0, delay = delay, duration = coming and ENTRY_MOVE or ENTRY_LEAVE }
      end
      handles[#handles + 1] = morf.animation.play { { parallel = steps } }
    end
    return handles, coming and ((opts.delay or ENTRY_DELAY) + ENTRY_MOVE + math.max(0, #entries - 1) * (opts.stagger or STAGGER)) or ENTRY_LEAVE
  end
  -- Menu.qml's covered entrance and uncover at a quicker cadence, with
  -- overlapping cover/withdrawal on exit. Each channel resumes its current
  -- pose when interrupted; an old completion cannot hide a reopened panel.
  function M.drawer(ctx)
    local panel, content, d = ctx.panel, ctx.spec.content, ctx.drawer
    local curtain, edge = require("themes.tsugumori.curtain")(theme, panel, "drawer-" .. (d.name or "auth"))
    local axis = ctx.floating and "translate_x" or ctx.axis
    local function tucked()
      return ctx.floating and (panel.width + 2) or ctx.tucked()
    end
    local running, generation = nil, 0
    return function(opening)
      generation = generation + 1
      local own = generation
      local hidden = not panel.visible
      if running then running:stop() end
      if hidden then
        panel[axis] = tucked()
        curtain.width, curtain.x = math.max(panel.width, panel.width_target or 0), 0
      end
      panel.visible, curtain.visible, content.opacity, d.shape.opacity = true, true, 1, 1
      local travel = math.min(1, math.abs((opening and 0 or tucked()) - panel[axis]) / math.max(1, math.abs(tucked())))
      local width = panel.width_target or panel.width
      local cover = math.min(1, curtain.width / math.max(1, width))
      local enter_ms = math.max(1, ENTER * travel)
      local easing = { x1 = 0.76, y1 = 0, x2 = 0.24, y2 = 1 }
      local steps = {
        { node = panel, property = axis, to = opening and 0 or tucked(),
          delay = opening and 0 or OVERLAP * (1 - cover),
          duration = opening and enter_ms or math.max(1, WITHDRAW * travel),
          easing = opening and "out_expo" or easing },
        { node = curtain, property = "width", to = opening and 0 or width,
          delay = opening and enter_ms or 0,
          duration = math.max(1, opening and REVEAL * cover or COVER * (1 - cover)),
          easing = opening and "out_expo" or easing },
        { node = edge, property = "opacity", delay = opening and enter_ms or 0,
          duration = opening and math.max(1,REVEAL*cover) or COVER,
          keyframes = {{at=0,value=0},{at=.18,value=.82},{at=.48,value=.58},{at=1,value=0}} },
      }
      steps[#steps + 1] = { node = curtain, property = "x", to = opening and width or 0,
        delay = opening and enter_ms or 0,
        duration = math.max(1, opening and REVEAL * cover or COVER * (1 - cover)),
        easing = opening and "out_expo" or easing }
      running = morf.animation.play { { parallel = steps }, on_finished = function(reason)
        if generation ~= own then return end
        running = nil
        if reason == "completed" then
          panel.visible = d.open:get()
          curtain.visible = false
        end
      end }
    end
  end
  return M
end
