-- One module on the desk, one box of the grid.
--
-- Port of Widget.qml. Knows nothing about modules: the face registry picks
-- the content, the family sets the size and the grid the position. Built
-- once per key and reading its row from the service, so a move or a resize
-- does not rebuild it and a playing track survives a drag; only a change of
-- family or theme rebuilds the face inside it.
--
-- The same widget is built on two boards: at rest, on the wallpaper under
-- the windows, where only a right click on it is its own (the menu); and
-- while arranging, on the surface above the windows, where it is dragged
-- by the body, resized by the corner handle, removed by the badge, cycled
-- through its families by the wheel and selected by a click.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local deck = require("services.deck")
local face = require("desktop.face")

local C = theme.color
local M = {}

local count = 0

--- The widget for `key`. `arranging` builds the handles; `board` is the
--- board node, for the pointer's position.
function M.build(key, arranging)
  count = count + 1
  local id = "impasto.desk.widget." .. count
  local function row() return desk.entry_of(key) end
  local function box()
    return desk.geometry(key) or { x = 0, y = 0, width = 1, height = 1 }
  end
  local ink = desk.ink_for(row)
  local function style() return desk.style_of(row()) end
  local function on_picture() local s = style() return s == "bare" or s == "outline" end

  -- Where the pointer is over the widget, across the face's own areas.
  local region = require("desktop.faces.common").region()

  -- The face, rebuilt when the model's one row names another.
  local faces = desk.face_model(key)
  local face_holder = ui.Repeater {
    model = faces,
    delegate = function()
      local r = row()
      if not r then return ui.Item {} end
      local family = desk.family_of(r)
      local size = desk.size_for(family)
      return face.build {
        id = r.id, key = key, family = family, theme = desk.theme_of(r),
        ink = ink, row = row, width = size.width, height = size.height,
        arranging = arranging, region = region,
      }
    end,
  }

  local capsule = ui.Rect {
    anchors = { fill = true }, radius = theme.desktop_radius,
    visible = function() return not on_picture() end,
    color = function() return ink.ground():alpha(desk.opacity_of(row()) / 100) end,
    border_color = ink.border,
    border_width = function() return style() == "accent" and 0 or 1 end,
    -- What impasto asked of Hyprland with a layer rule: the wallpaper
    -- behind a translucent capsule blurred.
    backdrop_blur = function() return desk.opacity_of(row()) < 100 end,
    behavior = { color = theme.behave("medium") },
  }
  local outline = ui.Rect {
    anchors = { fill = true }, radius = theme.desktop_radius,
    visible = function() return style() == "outline" end,
    color = morf.color("transparent"),
    border_color = function() return ink.text():alpha(0.55) end,
    border_width = 1.5,
  }
  -- Without a capsule the contents get a drop shadow to stay readable on the
  -- wallpaper; not the spectrum, which is drawn as it is on an edge.
  local content = ui.Item {
    anchors = { fill = true },
    layer = function()
      local r = row()
      if on_picture() and r and r.id ~= "spectrum" then
        return { enabled = true, shadow_color = C.island:alpha(0.6), shadow_blur = 8, shadow_offset_y = 2 }
      end
      return { enabled = false }
    end,
    face_holder,
  }

  if not arranging then
    return ui.Item {
      x = function() return box().x end,
      y = function() return box().y end,
      width = function() return box().width end,
      height = function() return box().height end,
      behavior = {
        x = theme.behave("medium"), y = theme.behave("medium"),
        width = theme.behave("medium"), height = theme.behave("medium"),
      },
      -- Under the face, so its own buttons keep the left button; the right
      -- one opens the widget's menu at the pointer.
      ui.MouseArea {
        anchors = { fill = true }, accepted_buttons = "right",
        on_entered = region.enter, on_exited = region.leave,
        on_clicked = function(sx, sy)
          local b = desk.board()
          desk.open_menu(key, sx - b.x, sy - b.y)
        end,
      },
      capsule, outline, content,
    }
  end

  -- ------------------------------------------------------------ arranging --

  local held = function() return desk.dragging:get() == key end
  local selected = function() return desk.selected:get() == key end
  local hovered = morf.signal(id .. ".hovered", false)
  local resizing = morf.signal(id .. ".resizing", false)
  local dressed = function() return not held() and (hovered:get() or selected()) end

  local hand -- the part that follows the pointer
  local moved = false
  local press_x, press_y = 0, 0
  local wheel_spent, wheel_rest = 0, 0

  -- A note held against a screen edge is headed for that edge's deck, and
  -- a spectrum for the bars along it when it has none; the edge lights.
  local function edge_under(px, py)
    local r = row()
    if not r or (r.id ~= "notes" and r.id ~= "spectrum") then return "" end
    local board = desk.board()
    local edge = deck.edge_at(px, py, board.width, board.height)
    if r.id == "spectrum" and not desk.spectrum_takes(edge) then return "" end
    return edge
  end

  local function aim(dx, dy)
    local b = box()
    local board = desk.board()
    local x = math.max(0, math.min(board.width - b.width, b.x + dx))
    local y = math.max(0, math.min(board.height - b.height, b.y + dy))
    hand.translate_x, hand.translate_y = x - b.x, y - b.y
    local r = row()
    if not r then return end
    local px, py = press_x + dx - board.x, press_y + dy - board.y
    if desk.over_tray(px, py) then
      desk.set_landing(nil)
      deck.receiving:set("")
      return
    end
    local edge = edge_under(px, py)
    deck.receiving:set(edge)
    if edge ~= "" then
      desk.set_landing(nil)
      return
    end
    local family = desk.family_of(r)
    local spot = desk.nearest_free(desk.cell_x(x), desk.cell_y(y), family, key)
    desk.set_landing(spot, family)
  end

  local function drop(dx, dy)
    desk.dragging:set("")
    desk.set_landing(nil)
    deck.receiving:set("")
    local b = box()
    local board = desk.board()
    local tx, ty = hand.translate_x or 0, hand.translate_y or 0
    -- Glide from where it was let go to where it lands: the box animates to
    -- the new cell while the hand's offset animates back to nothing, with
    -- the same curve, so the sum is one straight tween.
    local ms = theme.duration_medium()
    morf.animation.play {
      { node = hand, property = "translate_x", duration = math.max(1, ms),
        keyframes = { { at = 0, value = tx }, { at = 1, value = 0, easing = theme.easing() } } },
      { node = hand, property = "translate_y", duration = math.max(1, ms),
        keyframes = { { at = 0, value = ty }, { at = 1, value = 0, easing = theme.easing() } } },
    }
    local r = row()
    if not r then return end
    if desk.over_tray(press_x + dx - board.x, press_y + dy - board.y) then
      desk.remove(key)
      return
    end
    -- A note dropped on a screen edge joins that edge's deck, and a
    -- spectrum becomes the bars along it.
    local edge = edge_under(press_x + dx - board.x, press_y + dy - board.y)
    if edge ~= "" and r.id == "spectrum" then
      desk.spectrum_to_edge(key, edge)
      return
    end
    if edge ~= "" then
      desk.note_to_edge(key, edge)
      return
    end
    desk.place(key, desk.cell_x(b.x + tx), desk.cell_y(b.y + ty))
  end

  local body = ui.MouseArea {
    anchors = { fill = true },
    accepted_buttons = { "left", "right" },
    cursor = function() return held() and "grabbing" or "grab" end,
    on_entered = function() hovered:set(true) end,
    on_exited = function() hovered:set(false) end,
    on_pressed = function(sx, sy)
      moved = false
      press_x, press_y = sx, sy
    end,
    on_dragged = function(_, _, dx, dy)
      if not moved and math.abs(dx) + math.abs(dy) < 4 then return end
      if not moved then
        moved = true
        desk.dragging:set(key)
        desk.select("")
      end
      aim(dx, dy)
    end,
    on_released = function(sx, sy)
      if moved then
        moved = false
        drop(sx - press_x, sy - press_y)
      end
    end,
    on_clicked = function(_, _, _, _, button)
      if moved then return end
      if button == "right" then desk.edit(false) return end
      desk.select(selected() and "" or key)
    end,
    on_wheel = function(_, _, _, py, _, steps)
      local now = morf.time.now_ms()
      if now < wheel_rest then return end
      local delta = steps ~= 0 and steps * 120 or py
      wheel_spent = wheel_spent + delta
      if math.abs(wheel_spent) < 120 then return end
      local step = wheel_spent > 0 and 1 or -1
      wheel_spent = 0
      wheel_rest = now + 250
      desk.cycle_family(key, step)
    end,
  }

  local function round_button(values)
    return ui.Rect {
      width = 24, height = 24, radius = 12,
      x = values.x, y = values.y,
      color = C.island,
      border_color = values.border_color or C.islandBorder,
      border_width = values.border_width or 1,
      opacity = values.opacity,
      behavior = { opacity = theme.behave("fast") },
      visible = function() return (values.opacity() or 0) > 0 end,
      table.unpack(values),
    }
  end

  local badge = round_button {
    x = -8, y = -8,
    opacity = function() return dressed() and 1 or 0 end,
    ui.Rect { x = 7, y = 11, width = 10, height = 2, radius = 1, color = C.scrimText },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_clicked = function() desk.remove(key) end,
    },
  }

  -- Pulling the handle picks the family whose box, in cells from the
  -- widget's top left, is closest to the pointer; the face changes only at
  -- those steps, and a widget never stretches.
  local handle_start_x, handle_start_y = 0, 0
  local handle = ui.Item {
    x = function() return box().width - 16 end,
    y = function() return box().height - 16 end,
    width = 24, height = 24,
    round_button {
      opacity = function() return (dressed() or resizing:get()) and 1 or 0 end,
      border_color = function() return resizing:get() and C.accent() or C.islandBorder end,
      border_width = function() return resizing:get() and 2 or 1 end,
      ui.Rect { x = 7, y = 15, width = 10, height = 2, radius = 1, color = C.scrimText },
      ui.Rect { x = 15, y = 7, width = 2, height = 10, radius = 1, color = C.scrimText },
    },
    ui.MouseArea {
      anchors = { fill = true }, cursor = "nwse_resize",
      on_entered = function() hovered:set(true) end,
      on_pressed = function(sx, sy)
        handle_start_x, handle_start_y = sx, sy
        resizing:set(true)
      end,
      on_dragged = function(_, _, dx, dy)
        local r = row()
        if not r then return end
        local g = desk.grid()
        local b = box()
        local board = desk.board()
        local px = handle_start_x - board.x + dx
        local py = handle_start_y - board.y + dy
        local cols = (px - b.x + theme.desktop_gutter) / g.stride
        local rows_ = (py - b.y + theme.desktop_gutter) / g.stride
        local next_family = desk.family_nearest(r.id, cols, rows_, desk.theme_of(r))
        if next_family ~= "" and next_family ~= desk.family_of(r) then desk.set_family(key, next_family) end
      end,
      on_released = function() resizing:set(false) end,
    },
  }

  local frame = ui.Rect {
    x = -3, y = -3,
    width = function() return box().width + 6 end,
    height = function() return box().height + 6 end,
    radius = theme.desktop_radius + 3,
    color = morf.color("transparent"),
    border_color = function() return (held() or selected()) and C.accent() or C.hairline end,
    border_width = function() return (held() or selected()) and 2 or 1 end,
  }

  hand = ui.Item {
    anchors = { fill = true },
    capsule, outline, content, frame, body, badge, handle,
  }

  return ui.Item {
    x = function() return box().x end,
    y = function() return box().y end,
    width = function() return box().width end,
    height = function() return box().height end,
    id = "desk-widget-" .. key,
    z = function() return held() and 2 or (selected() and 1 or 0) end,
    -- No room for it on this board even at its smallest: not drawn here.
    visible = function() return not desk.left_off(key) end,
    behavior = {
      x = theme.behave("medium"), y = theme.behave("medium"),
      width = theme.behave("medium"), height = theme.behave("medium"),
    },
    hand,
  }
end

return M
