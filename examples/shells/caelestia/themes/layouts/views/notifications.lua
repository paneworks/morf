-- Original Material notification cards; data/actions come from the controller.
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
function V.build(M)
  local WIDTH, CARD_W, CARD_H, GAP = 430, 408, 58, 8
  local TOP, BOTTOM, LEFT, MAX = 6, 16, 15, 5
  local function shown() return M.shown(MAX) end
  local is_open = M.expanded
  local function card_height(n)
    if n and is_open(n.id) and (n.body or "") ~= "" then return CARD_H + 40 end
    return CARD_H
  end

  local function height()
    local list = shown()
    if #list == 0 then return CARD_H + TOP + BOTTOM end
    local h = TOP + BOTTOM + (#list - 1) * GAP
    for _, n in ipairs(list) do h = h + card_height(n) end
    return h
  end

  local function plain(text)
    -- Bodies may carry a little markup; the popup shows the words.
    return (tostring(text or ""):gsub("<[^>]->", ""):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("&amp;", "&"))
  end

  local dripping = morf.signal("caelestia.notifications.dripping", false)
  local cards = {}
  local shapes = {}
  local layers = {
    id = "notifications-field", anchors = { fill = true },
    blend = function() return dripping:get() and 8 or 0 end,
    behavior = { blend = { duration = 260 } },
  }
  local dry
  for i = 1, MAX do
    local function n() return shown()[i] end
    local motion = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
    local card
    card = kit.action {
      id = "notification-" .. i,
      width = CARD_W, cursor = "pointer",
      height = function() return card_height(n()) end,
      visible = function() return n() ~= nil end,
      behavior = { height = motion },
      clip = true,
      on_clicked = function() M.dismiss_at(i) end,
      kit.surface {
        x = 12, y = 8, width = 42, height = 42, radius = 21,
        color = function()
          local x = n()
          return (x and x.urgency == 2) and C.error or C.surfaceContainerHighest
        end,
        kit.icon(function()
          local x = n()
          if x and x.urgency == 2 then return "release_alert" end
          return "chat"
        end, 22, function()
          local x = n()
          return (x and x.urgency == 2) and C.onError or C.onSurfaceVariant
        end, { anchors = { center_in = true } }),
      },
      ui.Row {
        x = 66, y = 9, gap = 0, align = "center",
        kit.heading { active = function() return M.opened:get() and n() ~= nil end, level = "caption", reveal_delay = 240,
          id = "notification-summary-" .. i,
          ink = function()
            local x = n()
            return (x and x.urgency == 2) and C.onErrorContainer or C.primary
          end,
          text = function() local x = n() return x and plain(x.summary) or "" end,
          font_size = theme.size.normal + 1, font_weight = 500,
          color = function()
            local x = n()
            return (x and x.urgency == 2) and C.onErrorContainer or C.onSurface
          end,
        },
        kit.text {
          text = "  •  now", font_size = theme.size.normal,
          color = function() return C.onSurfaceVariant end,
        },
      },
      kit.text {
        x = 66, y = 29, width = 300,
        id = "notification-body-" .. i,
        text = function() local x = n() return x and plain(x.body) or "" end,
        font_size = theme.size.normal, elide = "right",
        visible = function() local x = n() return not (x and is_open(x.id)) end,
        color = function()
          local x = n()
          return (x and x.urgency == 2) and C.onErrorContainer or C.onSurfaceVariant
        end,
      },
      kit.text {
        x = 66, y = 29, width = 300,
        text = function() local x = n() return x and plain(x.body) or "" end,
        font_size = theme.size.normal, wrap = true, max_lines = 3,
        visible = function() local x = n() return x ~= nil and is_open(x.id) end,
        color = function()
          local x = n()
          return (x and x.urgency == 2) and C.onErrorContainer or C.onSurfaceVariant
        end,
      },
      kit.hover(kit.action {
        id = "notification-expand-" .. i,
        anchors = { right = true, right_margin = 12 }, y = 13, width = 32, height = 32, cursor = "pointer",
        on_clicked = function() local x=n() if x then M.toggle(x.id) end end,
        kit.icon(function() local x = n() return (x and is_open(x.id)) and "expand_less" or "expand_more" end,
          22, function() return C.onSurface end, { anchors = { center_in = true } }),
      }, function(hovered) return hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end, 16),
    }
    -- The card's background is a layer of the popups' field (built after
    -- the card, so its colour can read the card's hover).
    shapes[i] = ui.SdfShape {
      id = "notification-" .. i .. "-shape",
      shape = "box", radius = 16, track = card,
      operation = i == 1 and "union" or "smooth_union",
      fill_color = function()
        local x = n()
        if x and x.urgency == 2 then return C.errorContainer end
        return card.hovered and C.surfaceContainerHigh or C.surfaceContainer
      end,
      behavior = { fill_color = { duration = theme.duration.small } },
    }
    layers[#layers + 1] = shapes[i]
    cards[i] = card
  end

  -- A new popup drops in from the frame's edge above: it slides down a
  -- little and grows evenly from just smaller, fading in, while the field's
  -- seams soften so it touches the cards below like liquid; dismissed, it
  -- lifts a little, shrinks a touch and fades. At rest the cards are crisp.
  local function drip(i, coming, done)
    local node, shape = cards[i], shapes[i]
    dripping:set(true)
    if dry then dry:cancel() end
    dry = morf.timer(coming and 520 or 260, function() dry = nil dripping:set(false) end, false)
    local steps
    if coming then
      steps = {
        { node = node, property = "translate_y", from = -24, to = 0, duration = 460, easing = theme.ease.spatial },
        { node = node, property = "scale", from = 0.92, to = 1, duration = 460, easing = theme.ease.spatial },
        { node = node, property = "opacity", from = 0, to = 1, duration = 200 },
        { node = shape, property = "opacity", from = 0, to = 1, duration = 200 },
      }
    else
      steps = {
        { node = node, property = "translate_y", to = -16, duration = 200, easing = theme.ease.emphasized_accel },
        { node = node, property = "scale", to = 0.94, duration = 200, easing = theme.ease.emphasized_accel },
        { node = node, property = "opacity", to = 0, duration = 160 },
        { node = shape, property = "opacity", to = 0, duration = 160 },
      }
    end
    morf.animation.play {
      { parallel = steps },
      on_finished = function(reason)
        if not coming then
          node.translate_y, node.scale, node.opacity, shape.opacity = 0, 1, 1, 1
        end
        if done and reason == "completed" then done() end
      end,
    }
  end

  local count = #M.list:get()
  morf.effect("caelestia.notifications.drip", function()
    local now = #M.list:get()
    if now > count and cards[1] then drip(1, true) end
    count = now
  end)

  local function dismiss(i)
    if cards[i] then drip(i,false) return 200 end
    return 0
  end

  local content = ui.Item {
    anchors = { fill = true },
    ui.Sdf(layers),
    ui.Column { x = LEFT, y = TOP, gap = GAP, table.unpack(cards) },
  }

  return {content=content,width=WIDTH,height=height,edge="top",dismiss=dismiss,
    props={anchors={top=true,right=true}}}
end
return V
