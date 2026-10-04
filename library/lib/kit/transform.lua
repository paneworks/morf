-- Transforms (the Transform archetype): a box the user moves by its body,
-- resizes from its edges and corners, and turns -- a floating panel, an
-- image cropper's frame, the handles round a canvas item, a
-- picture-in-picture window, a calendar's event.
--
--     local node, box = lib.kit.transform.make("floating_panel", {
--       title = "Inspector", x = 40, y = 40, width = 320, height = 240,
--       container = { 1280, 800 },          -- what it floats in (the area it fills, by default)
--       content = inspector,                -- or the spec's array part
--       on_committed = function(x, y, w, h, angle) save(x, y, w, h) end,
--       on_close = function() end,
--     })
--     box.maximize()  box.minimize()  box.restore()  box.set(x, y, w, h)  box.t.box_width
--
-- The node returned is the area the box floats in (filling its parent, or
-- `container` px when given); the box itself (`box.node`) sits in it at
-- `t.x`, `t.y`, `t.box_width` x `t.box_height` and carries the content.
-- Its body moves it (a floating panel's only by its title bar, the top
-- `title_height` px, whose double click maximizes); the skin's `handle`
-- builder draws each grip (told `s.name` -- n, ne, e, se, s, sw, w, nw --
-- `s.hovered()`, `s.held()`, `s.corner`), and a turned box (`rotatable`)
-- has a knob `stalk` px over its top (the skin's `rotate_handle`). The
-- skin's other slots: `background` (under the content: a window's ground,
-- a cropper's dimming of what is outside), `frame` (over it: an outline,
-- a title bar), `guide` (thirds while dragging) and `content`; a skin
-- that sets `t.content_radius` has the content cut to those corners.
--
-- Every press tells the archetype the box's centre on the surface, a drag
-- the pointer on the surface (the box moves under it) and what is held:
-- Shift keeps the aspect, Alt resizes about the centre. The arrows, Ctrl
-- with them, Alt with them, Return and Escape are the archetype's.
local ui = require("morf.ui")
local morf = require("morf")
local control = require("lib.kit.control")

local M = {}

local function get(v) if type(v) == "function" then return v() end return v end

-- What each widget is unless its spec says otherwise. Fields the glue
-- reads (not the archetype's): `region` ("title": only the title bar
-- moves it), `title_height`, `grips` (which handles, over `handles`),
-- `glide` (x, y and size follow on a spring: a maximize, a snap),
-- `corner_snap`, `clip` (the area cuts what leaves it), `guides`, `dim`.
local DEFAULTS = {
  floating_panel = { region = "title", title_height = 40, min_width = 200, min_height = 120, bounds = "container",
    glide = true, width = 320, height = 220 },
  image_cropper = { handles = "all", guides = true, dim = true, bounds = "container", clip = true, min_width = 32,
    min_height = 32 },
  resize_box = { handles = "all", rotatable = true, min_width = 24, min_height = 24 },
  pip_window = { aspect = 16 / 9, corner_snap = true, glide = true, handles = "corners", bounds = "container",
    min_width = 160, min_height = 90, width = 256, height = 144 },
  event_block = { handles = "edges", grips = { "n", "s" }, min_height = 24 },
}
M.DEFAULTS = DEFAULTS

local GRIPS = { all = { "nw", "n", "ne", "e", "se", "s", "sw", "w" }, corners = { "nw", "ne", "se", "sw" },
  edges = { "n", "e", "s", "w" }, none = {} }
local CURSOR = { n = "n_resize", ne = "ne_resize", e = "e_resize", se = "se_resize", s = "s_resize",
  sw = "sw_resize", w = "w_resize", nw = "nw_resize" }
-- The spring a glide follows: stiff and critically damped, so a drag
-- keeps up and a maximize or a snap lands without wobbling.
local GLIDE = { stiffness = 700, damping = 53 }
-- Fields the area or the glue keeps, not the control.
local OWN = { x = true, y = true, width = true, height = true, content = true, container = true, anchors = true,
  z = true, image = true, behavior = true }

function M.make(widget, given)
  given = given or {}
  local spec = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do spec[k] = v end
  for k, v in pairs(given) do spec[k] = v end
  -- An event block snaps to the grid it is given.
  if spec.grid and spec.snap == nil then spec.snap = spec.grid end
  local children = {}
  if spec.content ~= nil then children[1] = spec.content end
  for _, child in ipairs(spec) do children[#children + 1] = child end
  local full = {}
  for k, v in pairs(spec) do
    if type(k) == "string" and not OWN[k] then full[k] = v end
  end
  full.widget = widget
  local by_container = spec.bounds == "container"
  if by_container then full.bounds = nil end
  local title_h = spec.region == "title" and (spec.title_height or 40) or 0
  full.title_height = title_h

  -- The area: what it floats in, maximizes to and is kept inside.
  local container = spec.container
  local area_props = { anchors = spec.anchors, z = spec.z, clip = spec.clip == true }
  if container ~= nil then
    area_props.width = function() local c = get(container) return c and c[1] or 0 end
    area_props.height = function() local c = get(container) return c and c[2] or 0 end
  elseif spec.anchors == nil then
    area_props.anchors = { fill = true }
  end
  local area = ui.Item(area_props)
  -- A picture to crop: a node, or an image's source.
  if spec.image ~= nil then
    local picture = spec.image
    if type(picture) == "string" then picture = ui.Image { anchors = { fill = true }, source = picture, fill_mode = "cover" } end
    ui.reparent(picture, area)
  end

  local t, ctl, root, send
  local glide = spec.glide and { x = ui.spring(GLIDE), y = ui.spring(GLIDE), width = ui.spring(GLIDE),
    height = ui.spring(GLIDE) } or nil
  if glide and spec.behavior then for k, v in pairs(spec.behavior) do glide[k] = v end end
  local grips_layer
  local rebuild_grips
  root, t, ctl = control.make("Transform", widget, full, {
    props = {
      width = 0, height = 0,
      behavior = glide or spec.behavior,
      stretch = spec.stretch,
    },
    builders = { handle = true },
    state = { container_width = 0, container_height = 0, content_radius = 0 },
    on_rebuild = function() if rebuild_grips then rebuild_grips() end end,
  })
  send = ctl.send
  -- (Bound once `t` is known.)
  root.x = function() return t.x end
  root.y = function() return t.y end
  root.width = function() return t.box_width end
  root.height = function() return (t.minimized and title_h > 0) and title_h or t.box_height end
  if spec.rotatable then root.rotation = function() return t.angle end end

  -- The box's place.
  for _, field in ipairs { "width", "height", "x", "y" } do
    local v = spec[field]
    if type(v) == "function" then
      morf.effect("kit.transform." .. field .. "." .. ctl.id, function() ctl.configure(field, v()) end, { owner = root })
    elseif v ~= nil then
      ctl.configure(field, v)
    end
  end
  -- What it floats in: told to the archetype (maximized fills it) and, by
  -- default, what it is kept inside.
  morf.effect("kit.transform.container." .. ctl.id, function()
    local w, h = area.layout_width or 0, area.layout_height or 0
    if w <= 0 or h <= 0 then return end
    t.container_width, t.container_height = w, h
    send("container", w, h)
    if by_container then ctl.configure("bounds", { 0, 0, w, h }) end
  end, { owner = root })

  -- The box's centre on the surface, where a turn turns about.
  local function centre()
    return (root.layout_x or 0) + (root.layout_width or 0) / 2, (root.layout_y or 0) + (root.layout_height or 0) / 2
  end
  local held = false
  local function press(handle, sx, sy, modifiers)
    held = true
    if root.focus_policy ~= "none" then morf.focus.set(root, true) end
    local cx, cy = centre()
    send("pressed", sx, sy, handle, modifiers or "", cx, cy)
  end
  local function drag(sx, sy, modifiers) if held then send("dragged", sx, sy, modifiers or "") end end
  -- A picture-in-picture goes to the nearest corner of what it floats in.
  local function snap_corner()
    local cw, ch = t.container_width, t.container_height
    if cw <= 0 or ch <= 0 then return end
    local m = spec.snap_margin or 12
    local w, h = t.box_width, t.box_height
    local x = (t.x + w / 2 < cw / 2) and m or (cw - w - m)
    local y = (t.y + h / 2 < ch / 2) and m or (ch - h - m)
    if x ~= t.x or y ~= t.y then send("set", x, y, w, h) end
  end
  local function release()
    if not held then return end
    held = false
    send("released")
    if spec.corner_snap then snap_corner() end
  end

  -- The body moves it (a floating panel only by its title bar).
  local function on_body(y) return title_h <= 0 or y <= title_h end
  root.on_pressed = function(sx, sy, x, y, button, modifiers, ...)
    if (button == nil or button == "left") and on_body(y) then press("body", sx, sy, modifiers) end
    if spec.on_pressed then spec.on_pressed(sx, sy, x, y, button, modifiers, ...) end
  end
  root.on_dragged = function(sx, sy, dx, dy, x, y, modifiers, ...)
    drag(sx, sy, modifiers)
    if spec.on_dragged then spec.on_dragged(sx, sy, dx, dy, x, y, modifiers, ...) end
  end
  root.on_released = function(...)
    release()
    if spec.on_released then spec.on_released(...) end
  end
  local cursor = spec.cursor or ((title_h <= 0 and spec.movable ~= false) and "move" or nil)
  if cursor then root.cursor = cursor end
  -- A double click on the title bar maximizes (and restores).
  root.on_double_clicked = function(sx, sy, x, y)
    if title_h > 0 and y <= title_h then send("maximize") end
    if spec.on_double_clicked then spec.on_double_clicked(sx, sy, x, y) end
  end

  -- The grips: a pressable area per handle, the skin's look inside it.
  local GRIP = spec.grip or 14
  local function grip_names()
    if type(spec.grips) == "table" then return spec.grips end
    return GRIPS[get(spec.handles) or "all"] or GRIPS.all
  end
  rebuild_grips = function()
    if grips_layer then ui.destroy(grips_layer, true) end
    grips_layer = ui.Item { anchors = { fill = true }, z = 20,
      visible = function() return not t.maximized and not t.minimized and get(spec.resizable) ~= false end }
    local build = (ctl.builders() or {}).handle
    for _, name in ipairs(grip_names()) do
      local corner = #name == 2
      local north, south = name:sub(1, 1) == "n", name:sub(1, 1) == "s"
      local west, east = name:find("w", 1, true) ~= nil, name:find("e", 1, true) ~= nil
      local grip
      local props = { cursor = CURSOR[name], accessible_role = "grip", accessible_name = "Resize " .. name,
        on_pressed = function(sx, sy, _, _, _, modifiers) press(name, sx, sy, modifiers) end,
        on_dragged = function(sx, sy, _, _, _, _, modifiers) drag(sx, sy, modifiers) end,
        on_released = function() release() end }
      if corner then
        props.width, props.height = GRIP, GRIP
        props.anchors = { left = west or nil, right = east or nil, top = north or nil, bottom = south or nil,
          left_margin = -GRIP / 2, right_margin = -GRIP / 2, top_margin = -GRIP / 2, bottom_margin = -GRIP / 2 }
      elseif north or south then
        props.height = GRIP
        props.anchors = { left = true, right = true, top = north or nil, bottom = south or nil,
          left_margin = GRIP / 2, right_margin = GRIP / 2, top_margin = -GRIP / 2, bottom_margin = -GRIP / 2 }
      else
        props.width = GRIP
        props.anchors = { top = true, bottom = true, left = west or nil, right = east or nil,
          top_margin = GRIP / 2, bottom_margin = GRIP / 2, left_margin = -GRIP / 2, right_margin = -GRIP / 2 }
      end
      grip = ui.MouseArea(props)
      local s = { name = name, corner = corner,
        hovered = function() return grip.hovered or t.handle == name end,
        held = function() return t.handle == name end }
      local look = build and build(s)
      if look then ui.reparent(look, grip) end
      ui.reparent(grip, grips_layer)
    end
    -- The knob a turn is made by, `stalk` px over the top.
    if get(spec.rotatable) then
      local KNOB, STALK = spec.knob or 22, spec.stalk or 28
      ui.reparent(ui.MouseArea { width = KNOB, height = KNOB, cursor = "grab", accessible_role = "grip",
        accessible_name = "Rotate",
        anchors = { horizontal_center = true, top = true, top_margin = -STALK - KNOB / 2 },
        on_pressed = function(sx, sy, _, _, _, modifiers) press("rotate", sx, sy, modifiers) end,
        on_dragged = function(sx, sy, _, _, _, _, modifiers) drag(sx, sy, modifiers) end,
        on_released = function() release() end }, grips_layer)
    end
    ui.reparent(grips_layer, root)
  end

  -- The configuration's content, under the frame (a window's below its
  -- title bar).
  if #children > 0 then
    -- (Cut to the corners the skin rounds its content to: a video tile's.)
    local props = { anchors = { fill = true, top_margin = title_h }, clip = true,
      visible = function() return not t.minimized end }
    local body
    if (t.content_radius or 0) > 0 then
      props.clip, props.color = nil, "transparent"
      props.radius = function() return t.content_radius or 0 end
      body = ui.ClipRect(props)
    else
      body = ui.Item(props)
    end
    for _, child in ipairs(children) do ui.reparent(child, body) end
    ui.reparent(body, root)
    -- (The skin's frame and grips go over it.)
    local s = ctl.slots() or {}
    for _, name in ipairs { "frame", "guide", "rotate_handle" } do
      if s[name] then ui.reparent(s[name], root) end
    end
  end
  rebuild_grips()
  ui.reparent(root, area)

  local box = { node = root, area = area, t = t, send = send }
  function box.maximize() if not t.maximized then send("maximize") end end
  function box.minimize() if not t.minimized then send("minimize") end end
  function box.restore() send("restore") end
  function box.toggle_maximized() send("maximize") end
  function box.set(x, y, w, h) send("set", x, y, w or t.box_width, h or t.box_height) end
  return area, box
end

return M
