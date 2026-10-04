-- Selections (the Selection archetype): tabs, segmented choices, list and
-- grid selection, swatch grids. The archetype keeps the current item and
-- the selected set and answers the keys; this lays out an item per entry
-- of `spec.items` with the skin's `item` delegate and keeps the current
-- item's box in the live state, so a skin's `indicator` can travel to it.
--
--     selection.make("tabs", {
--       items = { "Overview", "Media", "Weather" },     -- or a function
--       current = function() return tab:get() end,
--       on_current_changed = function(i) tab:set(i) end,
--       orientation = "horizontal",                     -- vertical, grid
--       columns = 4, gap = 0, item_width = 80, item_height = 36,
--     })
--
-- A skin's `item(index, value, s)` builds one entry; `s` reads its state:
-- `s.current()`, `s.selected()`, `s.hovered()`, `s.down()`, and `s.area`
-- (the entry's MouseArea). A skin may also give `place(index, value)`, the
-- entry area's own properties (`x`, `width`, `layout`, ...), and
-- `container()`, the node the entries go in (a Row, a Column or a Grid by
-- `orientation` otherwise). `spec.item_id(index, value)` names each
-- entry's area; `spec.delegate(index, value, s)`, when given, draws the
-- entries instead of the skin (a layout's own swatches); and
-- `spec.press_activates` makes one press activate an entry, not only
-- choose it. The live state adds `current_x`, `current_y`,
-- `current_width`, `current_height`: the current entry's box within the
-- control.
--
-- `geometry` lays the entries out other than in a row:
--
--   "radial" (a radial menu, a pie menu): item 1 at twelve o'clock, the
--   rest clockwise on a circle of `radius` (half way between the hub and
--   the rim) in a `size` px square. The pointer over it points from the
--   centre (the archetype's "point": the entry in that sector is current,
--   none within `dead_radius` of the centre) and a release past the dead
--   radius activates -- a marking menu's flick. The live state adds
--   `outer`, `inner` and `radius` for the skin's sectors.
--
--   "tumbler" (a spinning wheel picker): the entries on a drum, `rows`
--   (5) tall. A vertical drag turns it continuously and a release
--   snaps it to the nearest entry, with a little of the flick's speed
--   carried on; a press on an entry off the centre turns it there; the
--   wheel and the arrows step. Entries away from the centre fold over the
--   drum (their area's `translate_y`, `scale_y`, `opacity`). The live
--   state adds `rows` (visible) and `row` (an entry's height).
--
-- `selection.pie(spec)` makes a pie menu that opens over everything else:
-- see there.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local clock = morf.elapsed_timer()
local function ease_out(u) return 1 - (1 - u) ^ 3 end

local function label_of(item)
  if type(item) == "table" then return tostring(item.label or item.name or item.text or item.caption or item.title or "") end
  return tostring(item)
end

function M.make(widget, spec)
  spec = spec or {}
  local function items()
    local v = spec.items
    if type(v) == "function" then v = v() end
    return v or {}
  end
  local full = {}
  for k, v in pairs(spec) do full[k] = v end
  full.widget = widget
  full.count = function() return #items() end
  full.labels = function()
    local out = {}
    for i, item in ipairs(items()) do out[i] = label_of(item) end
    return out
  end
  local orientation = spec.orientation or "horizontal"
  local geometry = spec.geometry
  -- A radial menu is a square; a tumbler `visible` rows tall.
  local SIZE = tonumber(spec.size) or tonumber(spec.width) or 220
  local ROW = tonumber(spec.item_height) or 36
  local ROWS = tonumber(spec.rows) or 5
  if geometry == "radial" then
    if full.width == nil then full.width = SIZE end
    if full.height == nil then full.height = SIZE end
    SIZE = math.min(tonumber(full.width) or SIZE, tonumber(full.height) or SIZE)
  elseif geometry == "tumbler" then
    if full.width == nil then full.width = spec.item_width or 72 end
    if full.height == nil then full.height = ROW * ROWS end
  end
  local OUTER = SIZE / 2
  local INNER = tonumber(spec.dead_radius) or math.floor(SIZE * 0.17)
  local RADIUS = tonumber(spec.radius) or (OUTER + INNER) / 2
  local function default_container()
    if geometry then return ui.Item { anchors = { fill = true }, clip = geometry == "tumbler" } end
    if orientation == "vertical" then return ui.Column { gap = spec.gap or 0 } end
    if orientation == "grid" then return ui.Grid { columns = spec.columns or 4, gap = spec.gap or 0 } end
    return ui.Row { gap = spec.gap or 0 }
  end
  local container
  local areas, root = {}, nil
  local live, deliver = nil, nil
  local function build_items(builders, t, send, fresh)
    live, deliver = t, send
    for _, area in ipairs(areas) do ui.destroy(area, true) end
    areas = {}
    -- On a theme switch the entries' container is the new skin's too.
    if fresh or not container then
      if container then ui.destroy(container, true) end
      container = builders.container and builders.container() or default_container()
      if root then ui.reparent(container, root) end
    end
    local make_item = spec.delegate or builders.item
    -- Each entry is a tab, an option, a cell to a screen reader.
    local _, item_role = control.roles("Selection", widget)
    for index, value in ipairs(items()) do
      local area
      local s = {
        current = function() return live.current == index end,
        selected = function() return control.has(live.selected, index) end,
        hovered = function() return area and area.hovered or false end,
        down = function() return area and area.pressed or false end,
      }
      -- A reorderable entry drags along the row (a headless Drag): past a
      -- neighbour, it asks to swap with it.
      local dragging
      if spec.reorderable then
        dragging = control.headless("Drag", { mode = "reorder",
          axis = (spec.orientation == "vertical") and "y" or "x",
          extent = (spec.orientation == "vertical") and (spec.item_height or 36) or (spec.item_width or 80),
          on_reorder = function(step) if spec.on_reorder then spec.on_reorder(index, step) end end })
      end
      local props = { width = spec.item_width, height = spec.item_height, cursor = "pointer",
        id = spec.item_id and spec.item_id(index, value) or nil,
        on_pressed = function(sx, sy)
          deliver("item_pressed", index, "")
          if spec.press_activates then deliver("item_activated", index) end
          if dragging then dragging.send("pressed", sx, sy) end
        end,
        on_dragged = dragging and function(sx, sy) dragging.send("dragged", sx, sy) end or nil,
        on_released = dragging and function() dragging.send("released", 0, 0) end or nil,
        on_double_clicked = function() deliver("item_activated", index) end,
        accessible_role = item_role or "list_box_option",
        accessible_name = label_of(value),
        accessible = function() return { selected = s.current() or s.selected() } end,
        -- A screen reader's press is the pointer's.
        on_accessible_action = function(action)
          if action ~= "click" and action ~= "focus" then return false end
          if root then morf.focus.set(root, true) end
          deliver("item_pressed", index, "")
          if action == "click" then deliver("item_activated", index) end
          return true
        end,
      }
      if geometry == "radial" then
        local n = math.max(1, #items())
        local a = math.rad((index - 1) * 360 / n)
        local w, h = spec.item_width or 48, spec.item_height or 48
        props.x = OUTER + RADIUS * math.sin(a) - w / 2
        props.y = OUTER - RADIUS * math.cos(a) - h / 2
        props.width, props.height = w, h
      elseif geometry == "tumbler" then
        props.anchors = { left = true, right = true, vertical_center = true }
        props.width, props.height = nil, ROW
      end
      for k, v in pairs(builders.place and builders.place(index, value) or {}) do props[k] = v end
      area = ui.MouseArea(props)
      s.area = area
      local look = make_item and make_item(index, value, s)
      if look then ui.reparent(look, area) end
      areas[index] = area
      ui.reparent(area, container)
    end
  end
  local state = { current_x = 0, current_y = 0, current_width = 0, current_height = 0 }
  if geometry == "radial" then state.outer, state.inner, state.radius, state.open = OUTER, INNER, RADIUS, false end
  if geometry == "tumbler" then state.rows, state.row = ROWS, ROW end
  local on_rebuilt
  local node, t, ctl = control.make("Selection", widget, full, {
    builders = { item = true, place = true, container = true },
    state = state,
    on_rebuild = function(builders, t, send)
      build_items(builders, t, send, true)
      if on_rebuilt then on_rebuilt() end
    end,
  })
  root = node
  ui.reparent(container, root)
  if geometry == "radial" then M.radial(root, t, ctl, INNER) end
  if geometry == "tumbler" then on_rebuilt = M.tumbler(root, t, ctl, function() return areas end, spec, ROW, ROWS) end
  -- A new list of entries: new delegates.
  local seen
  morf.effect("kit.selection.items." .. ctl.id, function()
    local list = items()
    local key = #list .. ":" .. table.concat(full.labels(), "\0")
    if seen ~= nil and key ~= seen then
      build_items(ctl.builders(), t, ctl.send)
      if on_rebuilt then on_rebuilt() end
    end
    seen = key
  end, { owner = root })
  -- The current entry's box, for the indicator.
  morf.effect("kit.selection.current." .. ctl.id, function()
    local area = areas[t.current]
    if not area then return end
    t.current_x = (area.layout_x or 0) - (root.layout_x or 0)
    t.current_y = (area.layout_y or 0) - (root.layout_y or 0)
    t.current_width = area.layout_width or 0
    t.current_height = area.layout_height or 0
  end, { owner = root })
  return node, t, ctl
end

--- A radial geometry's pointer: a catcher over the entries hears it and
--- points from the centre -- hovering, pressing and dragging -- and lets go
--- as a flick. Returns the catcher, and `point(sx, sy, release)` for a
--- pointer someone else holds (a pie menu opened by a press elsewhere),
--- in surface coordinates.
function M.radial(root, t, ctl, dead)
  local function centre()
    return (root.layout_x or 0) + (root.layout_width or 0) / 2, (root.layout_y or 0) + (root.layout_height or 0) / 2
  end
  local function point(sx, sy, release)
    local cx, cy = centre()
    ctl.send(release and "point_release" or "point", sx - cx, sy - cy, dead)
  end
  local catcher = ui.MouseArea { anchors = { fill = true }, z = 100, cursor = "pointer",
    on_position_changed = function(sx, sy) point(sx, sy, false) end,
    on_pressed = function(sx, sy)
      morf.focus.set(root, false)
      point(sx, sy, false)
    end,
    on_dragged = function(sx, sy) point(sx, sy, false) end,
    on_released = function(sx, sy) point(sx, sy, true) end,
  }
  ui.reparent(catcher, root)
  return catcher, point
end

--- A tumbler's drum: the entries' areas folded round it by where the
--- wheel stands (`pos`, the entry at the centre, continuous), a drag that
--- turns it, a release that snaps it, and the current entry it follows.
--- Returns what to call when the entries are rebuilt.
function M.tumbler(root, t, ctl, areas, spec, ROW, ROWS)
  local wrap = spec.wrap ~= false
  local H = ROW * ROWS
  -- The angle between neighbours: the next one sits a row from the centre.
  local STEP = math.asin(math.min(1, 2 / ROWS))
  local R = H / 2
  local function count() return #areas() end
  local function wrapped(off, n)
    if not wrap or n == 0 then return off end
    return (off + n / 2) % n - n / 2
  end
  -- An entry `off` rows from the centre: where it is drawn on the drum.
  local function pose(off)
    local a = off * STEP
    if math.abs(a) >= math.pi / 2 then return R + ROW, 0.001, 0 end
    local c = math.cos(a)
    -- (Past about sixty degrees it has folded out of sight.)
    return R * math.sin(a), math.max(0.001, c), math.max(0, (c - 0.4) / 0.6) ^ 1.2
  end
  local pos = (t.current and t.current > 0) and t.current or 1
  local anim, running
  local function now_pos()
    if not anim then return pos end
    local u = math.min(1, (clock:elapsed_ms() - anim.t0) / anim.duration)
    return anim.from + (anim.to - anim.from) * ease_out(u)
  end
  local function write(p)
    local list = areas()
    local n = #list
    for i, area in ipairs(list) do
      local y, sy, op = pose(wrapped(i - p, n))
      area.translate_y, area.scale_y, area.opacity = y, sy, op
    end
  end
  local function stop()
    if running then running:stop() running = nil end
    pos = now_pos()
    anim = nil
  end
  local function turn(to, duration)
    local from = now_pos()
    stop()
    pos = to
    if math.abs(to - from) < 1e-3 then write(to) return end
    duration = duration or math.floor(math.min(620, 240 + 60 * math.abs(to - from)))
    local list = areas()
    local n = #list
    local tracks = {}
    local N = 12
    for i, area in ipairs(list) do
      local ty, sy, op = {}, {}, {}
      local seen = false
      for k = 0, N do
        local u = k / N
        local y, s, o = pose(wrapped(i - (from + (to - from) * ease_out(u)), n))
        if o > 0 then seen = true end
        ty[#ty + 1] = { at = u, value = y }
        sy[#sy + 1] = { at = u, value = s }
        op[#op + 1] = { at = u, value = o }
      end
      if seen then
        tracks[#tracks + 1] = { node = area, property = "translate_y", duration = duration, keyframes = ty }
        tracks[#tracks + 1] = { node = area, property = "scale_y", duration = duration, keyframes = sy }
        tracks[#tracks + 1] = { node = area, property = "opacity", duration = duration, keyframes = op }
      else
        local y, s, o = pose(wrapped(i - to, n))
        area.translate_y, area.scale_y, area.opacity = y, s, o
      end
    end
    anim = { from = from, to = to, t0 = clock:elapsed_ms(), duration = duration }
    if #tracks > 0 then
      running = morf.animation.play { { parallel = tracks },
        on_finished = function() running = nil anim = nil end }
    else
      anim = nil
    end
  end
  -- The entry a wheel position stands on.
  local function index_of(p)
    local n = count()
    if n == 0 then return 0 end
    local i = math.floor(p + 0.5)
    if wrap then return (i - 1) % n + 1 end
    return math.max(1, math.min(n, i))
  end
  local function settle(target)
    local n = count()
    if n == 0 then return end
    if not wrap then target = math.max(1, math.min(n, target)) end
    turn(target)
    local index = index_of(target)
    if index ~= t.current then ctl.send("item_pressed", index, "") end
  end
  -- The pointer: a drag turns, a release snaps, a tap turns to the entry.
  local press_y, press_pos, dragging, trail
  local catcher = ui.MouseArea { anchors = { fill = true }, z = 100, cursor = "grab",
    on_pressed = function(_, sy)
      morf.focus.set(root, false)
      stop()
      write(pos)
      press_y, press_pos, dragging = sy, pos, false
      trail = { { clock:elapsed_ms(), sy } }
    end,
    on_dragged = function(_, sy)
      if not press_y then return end
      if not dragging and math.abs(sy - press_y) < 4 then return end
      dragging = true
      local p = press_pos - (sy - press_y) / ROW
      local n = count()
      if not wrap and n > 0 then
        -- Past either end it gives, a rubber band.
        local function band(d) return 0.6 * d / (d + 1) end
        if p < 1 then p = 1 - band(1 - p) end
        if p > n then p = n + band(p - n) end
      end
      pos = p
      write(p)
      trail[#trail + 1] = { clock:elapsed_ms(), sy }
      if #trail > 6 then table.remove(trail, 1) end
    end,
    on_released = function(_, sy, _, ly)
      if not press_y then return end
      local start = press_pos
      press_y = nil
      if not dragging then
        -- A tap: the entry under it, by its angle on the drum.
        local d = math.max(-1, math.min(1, ((ly or H / 2) - H / 2) / R))
        local rows = math.asin(d) / STEP
        settle(math.floor(start + rows + 0.5))
        return
      end
      -- A little of the flick's speed carries on.
      local first, last = trail[1], trail[#trail]
      local v = 0
      if last[1] - first[1] >= 12 and clock:elapsed_ms() - last[1] < 120 then
        v = (last[2] - first[2]) / (last[1] - first[1])
      end
      local carry = math.max(-ROWS, math.min(ROWS, -v * 140 / ROW))
      settle(math.floor(pos + carry + 0.5))
    end,
    on_wheel = function(_, _, _, _, _, step_y)
      if (step_y or 0) == 0 then return end
      ctl.send("key", step_y > 0 and "Down" or "Up", "", "", clock:elapsed_ms())
    end,
  }
  ui.reparent(catcher, root)
  -- The current entry, however it changed (the arrows, a binding): the
  -- wheel turns the shorter way to it.
  morf.effect("kit.selection.tumbler." .. ctl.id, function()
    local current = t.current
    local n = count()
    if current < 1 or n == 0 or index_of(pos) == current then return end
    local target = math.floor(pos + 0.5) + wrapped(current - index_of(pos), n)
    if not wrap then target = current end
    turn(target)
  end, { owner = root })
  write(pos)
  return function() stop() write(pos) end
end

--- A pie menu: a radial menu that opens over everything else, round a
--- point. Returns its node and a handle: `open_at(node)` (centred on it)
--- or `open_at(x, y)` (surface coordinates, over `spec.root` or the node
--- attached), `close()`, `is_open()`, and `attach(node)` -- the node
--- then waits hidden inside it -- which opens it round the pointer on a
--- right press or a press held there; the press
--- goes on into the menu, so a flick towards an entry and a release picks
--- it (a marking menu), and a release in the hub leaves it open to click.
--- `on_activated(index)` hears the pick; it closes after it.
function M.pie(widget, spec)
  local handle = {}
  local given = spec.on_activated
  spec.on_activated = function(index)
    if given then given(index) end
    handle.close()
  end
  local node, t, ctl = M.make(widget, spec)
  local open = false
  local anchor
  local host = spec.root
  -- (A press held elsewhere points from the menu's centre as well.)
  local function relative(sx, sy, release)
    local cx = (node.layout_x or 0) + (node.layout_width or 0) / 2
    local cy = (node.layout_y or 0) + (node.layout_height or 0) / 2
    ctl.send(release and "point_release" or "point", sx - cx, sy - cy, t.inner)
  end
  function handle.open_at(x, y)
    local target
    if type(x) == "table" then target = x
    else
      local base = host
      if not base then error("pie_menu.open_at(x, y) wants spec.root or an attached node", 2) end
      if anchor then ui.destroy(anchor, true) end
      anchor = ui.Item { width = 1, height = 1,
        x = (x or 0) - (base.layout_x or 0), y = (y or 0) - (base.layout_y or 0) }
      ui.reparent(anchor, base)
      target = anchor
    end
    open = true
    t.open = true
    node.visible = true
    morf.overlay.open(node, { anchor = target, placement = "center", gap = 0, focus = true,
      on_close = function() open = false t.open = false end })
  end
  function handle.close()
    if not open then return end
    open = false
    t.open = false
    morf.overlay.close(node)
  end
  function handle.is_open() return open end
  function handle.attach(target)
    host = host or target
    -- Until it opens it waits, hidden, in what it is attached to (the
    -- overlay takes it from there).
    if not open then
      ui.reparent(node, target)
      node.visible = false
    end
    target.accepted_buttons = { "left", "right" }
    local pressed_at
    -- (Handlers it had already go on after the menu's; an unset one reads
    -- as an error, so it is asked carefully.)
    local function own(name) local ok, v = pcall(function() return target[name] end) return ok and v or nil end
    local own_pressed, own_dragged, own_released = own("on_pressed"), own("on_dragged"), own("on_released")
    target.on_pressed = function(sx, sy, lx, ly, button, ...)
      pressed_at = { sx, sy }
      if button == "right" then handle.open_at(sx, sy) end
      if own_pressed then return own_pressed(sx, sy, lx, ly, button, ...) end
    end
    target.on_long_pressed = function() if pressed_at then handle.open_at(pressed_at[1], pressed_at[2]) end end
    target.on_dragged = function(sx, sy, ...)
      if open then relative(sx, sy, false) end
      if own_dragged then return own_dragged(sx, sy, ...) end
    end
    target.on_released = function(sx, sy, ...)
      if open and pressed_at then relative(sx, sy, true) end
      pressed_at = nil
      if own_released then return own_released(sx, sy, ...) end
    end
  end
  handle.node, handle.t = node, t
  return node, handle
end

return M
