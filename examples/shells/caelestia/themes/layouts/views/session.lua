-- Original Material session geometry and expressive motion.
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
function V.build(M)
  local BUTTON, GAP, PAD = 80, 16, 16
  local WIDTH = PAD + BUTTON + 6
  local ITEMS = { M.actions[1], M.actions[2], {id="picture"}, M.actions[3], M.actions[4] }
  local HEIGHT = 2 * PAD + #ITEMS * BUTTON + (#ITEMS - 1) * GAP - 1
  local function picture()
    local face = M.picture
    local has_face = face ~= nil
    return ui.Item {
      id = "session-picture",
      width = BUTTON, height = BUTTON,
      has_face and ui.Image {
        anchors = { fill = true }, source = face, fill_mode = "preserve_aspect_crop",
        mask = kit.surface { radius = 22, color = "#ffffff" },
      } or ui.Item {
        anchors = { fill = true },
        ui.Path {
          anchors = { fill = true, margins = 4 }, view_box = { 0, 0, 100, 100 },
          d = kit.shape_path("cookie9", { segments = false }),
          fill_color = function() return C.primaryContainer end,
        },
        kit.icon("person", 40, function() return C.onPrimaryContainer end, { anchors = { center_in = true }, fill = true }),
      },
    }
  end

  local buttons = {}
  local order = {}
  -- The buttons' backgrounds are layers of one field under them: drops that
  -- bud out of the frame one after another as the menu opens, swell under
  -- the pointer until they fuse with a neighbour, and morph their shape with
  -- their state (M3 expressive): a rounded square at rest, a nine-point
  -- cookie with the focus (turning slowly), a sunburst while pressed.
  -- Soft seams only while the drops bud; at rest the buttons are crisp.
  local budding = morf.signal("caelestia.session.budding", false)
  local layers = {
    id = "session-field", anchors = { fill = true },
    blend = function() return theme.motion.liquid_cards ~= false and budding:get() and 20 or 0 end,
    behavior = { blend = { duration = 300, easing = theme.ease.standard } },
  }
  local swell = kit.spring(420, 16)
  for _, item in ipairs(ITEMS) do
    if item.id == "picture" then
      buttons[#buttons + 1] = picture()
    else
      order[#order + 1] = item.id
      local index = #order
      local on = function() return M.focus:get() == index end
      local area
      area = kit.action {
        id = "session-" .. item.id,
        width = BUTTON, height = BUTTON, cursor = "pointer",
        on_entered = function() M.focus:set(index) end,
        on_clicked = function() M.run(item.id) end,
        scale = function()
          if area and area.pressed then return 0.94 end
          return (area and area.hovered) and 1.14 or 1
        end,
        behavior = { scale = swell },
        stretch = kit.STRETCH,
        kit.icon(item.icon, 36, function() return on() and C.onSecondaryContainer or C.onSurface end, {
          anchors = { center_in = true },
          fill = on,
        }),
      }
      layers[#layers + 1] = kit.sdf_shape {
        id = "session-" .. item.id .. "-shape",
        track = area,
        operation = #layers == 0 and "union" or "smooth_union",
        shape = function()
          if area.pressed then return "sunny" end
          if on() then return "cookie9" end
          return "square"
        end,
        fill_color = function()
          if on() then return C.secondaryContainer end
          return area.hovered and C.surfaceContainerHigh or C.surfaceContainer
        end,
        behavior = { fill_color = { duration = theme.duration.small } },
        loop = function()
          if not (on() and M.opened:get()) then return nil end
          return { rotation = { to = 360, duration = 12000, hold = true } }
        end,
      }
      buttons[#buttons + 1] = area
    end
  end

  -- Opening, each button drops out of the frame's edge in turn: from beyond
  -- it, small, on the expressive spatial curve.
  local settle
  local function bud()
    budding:set(true)
    if settle then settle:cancel() end
    settle = morf.timer(60 + #buttons * 55 + 420, function() settle = nil budding:set(false) end, false)
    for k, node in ipairs(buttons) do
      morf.animation.play {
        {
          parallel = {
            { node = node, property = "translate_x", from = 110, to = 0, duration = 560,
              easing = theme.ease.spatial, delay = 60 + (k - 1) * 55 },
            { node = node, property = "scale", from = 0.35, to = 1, duration = 560,
              easing = theme.ease.spatial, delay = 60 + (k - 1) * 55 },
          },
        },
      }
    end
  end

  local keys = ui.TextInput {
    id = "session-keys",
    width = 1, height = 1, opacity = 0, tab_navigation = false,
    on_escape = M.close,
    on_accepted = M.accept,
    on_key_pressed = M.key,
  }

  local content = ui.Item {
    anchors = { fill = true },
    ui.Sdf(layers),
    ui.Column { x = PAD, y = PAD, gap = GAP, table.unpack(buttons) },
    keys,
  }

  local function dim()
    return kit.surface {
      id = "session-dim",
      anchors = { fill = true },
      color = function() return M.opened:get() and "#00000080" or "#00000000" end,
      behavior = { color = { duration = theme.duration.normal, easing = theme.ease.standard } },
      ui.MouseArea {
        anchors = { fill = true },
        visible = function() return M.opened:get() end,
        on_clicked = function() M.close() end,
      },
    }
  end

  return { content=content, width=WIDTH, height=HEIGHT, edge="right", dim=dim,
    shown=function(on) keys.focus=on if on then bud() end end }
end
return V
