-- The bar drawn as it will look, with a catalogue of every piece under it
-- (LayoutEditor).
--
-- Drag a piece onto the bar, along it, across the island, or off it to
-- remove it; clicking a catalogue piece puts it at the end of the right
-- side. Clicking a piece on the bar picks it and opens a card under the
-- bar to give it its own shape, figure and, for the modules that run,
-- when it shows. The side a piece lands on is whichever half of the bar it
-- is let go over. The catalogue always lists everything; taking from it
-- copies, so a piece can be on the bar twice.
--
-- The pieces on the picture are the bar's own chip faces, so the picture
-- cannot drift from the bar. The original rebuilt the bar with a gap under
-- the pointer on every move; here the picture stays still during a drag,
-- a copy of the piece follows the pointer and an accent mark shows where it
-- would land, and the lists are written once, when it is let go. Where a
-- drop lands is found by asking the window where each piece is
-- (`window:item_rect`), since the picture may be scaled to fit.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local modules = require("services.modules")
local bar = require("bar.bar")
local chip = require("bar.modules.chip")
local clock = require("bar.modules.clock")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

local function window() return require("settings.window") end

-- ----------------------------------------------------------------- lists --

--- A side's items, less any piece no longer in the catalogue and the loose
--- splits that leaves, so the next write drops both.
function M.items(side)
  local out = {}
  for _, item in ipairs(bar.items(side)) do
    if modules.placeable(item.id) then out[#out + 1] = item end
  end
  return M.tidy(out)
end

--- No split at either end, and never two in a row.
function M.tidy(list)
  local out = {}
  for _, item in ipairs(list) do
    if item.id == "split" then
      if #out > 0 and out[#out].id ~= "split" then out[#out + 1] = item end
    else
      out[#out + 1] = item
    end
  end
  while #out > 0 and out[#out].id == "split" do table.remove(out) end
  return out
end

--- Saves a side: a piece with no look of its own is saved as its id.
function M.set_zone(side, list)
  local out = {}
  for _, item in ipairs(M.tidy(list)) do
    local own = (item.shape or "") ~= "" or (item.figure or "") ~= "" or (item.when or "") ~= ""
    if own then
      local entry = { id = item.id }
      if (item.shape or "") ~= "" then entry.shape = item.shape end
      if (item.figure or "") ~= "" then entry.figure = item.figure end
      if (item.when or "") ~= "" then entry.when = item.when end
      out[#out + 1] = entry
    else
      out[#out + 1] = item.id
    end
  end
  settings.set(side == "left" and "barLeft" or "barRight", out)
end

--- Four groups: modules that measure (a ring or a symbol), modules that
--- read out a state or a count, the buttons, then the strip and the split.
function M.catalogue()
  local gauges, readings, buttons = {}, {}, {}
  for _, entry in ipairs(modules.catalogue) do
    if entry.bar then
      if modules.ringed[entry.id] then gauges[#gauges + 1] = entry.id
      else readings[#readings + 1] = entry.id end
    end
  end
  local doors = modules.buttons()
  for _, id in ipairs(modules.button_ids) do
    if doors[id] then buttons[#buttons + 1] = id end
  end
  return { gauges, readings, buttons, { "workspaces", "split" } }
end

function M.name_of(id)
  if id == "workspaces" then return "Workspaces" end
  if id == "split" then return "Split" end
  local door = modules.buttons()[id]
  if door then return door.name end
  return modules.entry(id).name
end

local function glyph_of(id)
  if id == "workspaces" then return "󰍹" end
  if id == "split" then return "󰇙" end
  local door = modules.buttons()[id]
  if door then return door.glyph end
  local g = modules.glyph_of(id)
  return g ~= "" and g or "󰕮"
end

local function is_module(id)
  return id ~= "workspaces" and id ~= "split" and not modules.is_button(id)
end

--- Lets a carried piece go: `drag` is `{ from, index, item, over, at }`,
--- `from` and `over` each "left", "right", "tray" or "" (nowhere), `index`
--- and `at` 1-based places on those sides. Let go nowhere, nothing moves;
--- let go on the catalogue, a piece from the bar is taken off it.
function M.drop(drag)
  local left, right = M.items("left"), M.items("right")
  if drag.over == "" then return end
  if drag.from == "tray" and drag.over == "tray" then return end
  if drag.from == "left" then table.remove(left, drag.index) end
  if drag.from == "right" then table.remove(right, drag.index) end
  local item = { id = drag.item.id, shape = drag.item.shape or "", figure = drag.item.figure or "",
    when = drag.item.when or "" }
  if drag.over == "left" then table.insert(left, math.max(1, math.min(drag.at, #left + 1)), item) end
  if drag.over == "right" then table.insert(right, math.max(1, math.min(drag.at, #right + 1)), item) end
  M.set_zone("left", left)
  M.set_zone("right", right)
end

--- TESTING ONLY: `morf ipc call layout_drop <from> <index-or-id> <over>
--- <at>` makes the drop a drag would, since a headless compositor has no
--- pointer to drag with. From the catalogue, the second word is the id.
morf.ipc.layout_drop = function(from, which, over, at)
  local item
  if from == "tray" then
    if not modules.placeable(which or "") then return "no piece " .. tostring(which) end
    item = { id = which }
  else
    item = M.items(from)[tonumber(which) or 0]
    if not item then return "nothing there" end
  end
  M.drop { from = from, index = tonumber(which) or 0, item = item, over = over or "", at = tonumber(at) or 1 }
  local function ids(side)
    local out = {}
    for _, each in ipairs(M.items(side)) do out[#out + 1] = each.id end
    return table.concat(out, ",")
  end
  return ids("left") .. " | " .. ids("right")
end

-- ----------------------------------------------------------------- build --

--- `width`: the editor, as wide as a settings page.
function M.new(values)
  local W = values.width
  local H = theme.capsule_height
  local STAGE_PAD = 20
  local MARGIN = 12

  local revision = controls.signal("layout.revision", 0)
  local picked_side = controls.signal("layout.picked.side", "")
  local picked_index = controls.signal("layout.picked.index", 0)
  local held = controls.signal("layout.held", false)
  local over_side = controls.signal("layout.over", "")
  local pointer_x = controls.signal("layout.pointer.x", 0)
  local pointer_y = controls.signal("layout.pointer.y", 0)
  local marker_x = controls.signal("layout.marker.x", -1)
  local marker_y = controls.signal("layout.marker.y", 0)
  local drag = nil          -- { from, index, item, over, at }
  local tiles = { left = {}, right = {} }

  local style = function() return settings.barStyle end
  local chromeless = function() return style() == "capsule" end

  local function list_of(side)
    if side == "left" or side == "right" then return M.items(side) end
    return {}
  end

  local function picked()
    local side = picked_side:get()
    if side == "" then return nil end
    return list_of(side)[picked_index:get()]
  end

  local function unpick() picked_side:set("") picked_index:set(0) end
  local function pick(side, index)
    if picked_side:get() == side and picked_index:get() == index then unpick() return end
    picked_side:set(side)
    picked_index:set(index)
  end

  local function set_look(changes)
    local side = picked_side:get()
    local list = list_of(side)
    local item = list[picked_index:get()]
    if not item then return end
    for k, v in pairs(changes) do item[k] = v end
    M.set_zone(side, list)
  end

  local function remove_picked()
    local side = picked_side:get()
    local list = list_of(side)
    table.remove(list, picked_index:get())
    unpick()
    M.set_zone(side, list)
  end

  -- ------------------------------------------------------------ pieces --

  local function reveal_of(item)
    return function()
      return modules.figure_of(item.figure) == "on" and 1 or 0
    end
  end

  -- What a piece looks like on the bar, without its own clicks.
  local function face(item, alone)
    local id = item.id
    if id == "split" then
      return ui.Item {
        width = 12, height = H,
        ui.Rect { anchors = { center_in = true }, width = 2, height = function() return H() * 0.6 end,
          radius = 1, color = C.textMuted },
      }
    end
    if id == "workspaces" then
      local dots = { gap = 8, align = "center" }
      for i = 1, math.max(1, settings.workspaceCount) do
        dots[#dots + 1] = ui.Rect { width = i == 1 and 22 or 6, height = 6, radius = 3,
          color = i == 1 and C.accent or C.indicatorDim }
      end
      local row = ui.Row(dots)
      return kit.capsule {
        width = function() return (row.layout_width or 0) + 20 end,
        ui.Item { anchors = { center_in = true }, width = function() return row.layout_width or 0 end,
          height = 6, row },
      }
    end
    if modules.is_button(id) then
      return ui.Item {
        width = H, height = H,
        kit.glyph { anchors = { center_in = true }, glyph = glyph_of(id),
          size = function() return math.floor(H() * 0.44 + 0.5) end },
      }
    end
    return chip.face(id, { reveal = reveal_of(item), shape = item.shape, alone = alone })
  end

  -- A piece on the picture: its face, lit when picked, and the drag.
  local press_handlers
  local function bar_tile(side, item, alone)
    local node
    local is_picked = function()
      return picked_side:get() == side and picked_index:get() == item.index
    end
    local shows = item.id == "workspaces" or item.id == "split" or modules.is_button(item.id)
      or modules.shows(item.id, item.when ~= "" and item.when or nil)
    local body = face(item, alone)
    node = ui.Item {
      width = function() return math.max(1, body.layout_width or 1) end,
      height = H,
      opacity = shows and 1 or 0.45,
      ui.Rect {
        anchors = { fill = true, margins = 1 },
        radius = function() return (H() - 2) / 2 end,
        visible = is_picked,
        color = C.islandSurfaceHover, border_width = 1, border_color = C.accent,
      },
      body,
      press_handlers(side, item.index, item),
    }
    tiles[side][item.index] = node
    return node
  end

  local function side_node(side)
    local groups = bar.groups_of(list_of(side))
    local children = {
      gap = function() return chromeless() and 14 or theme.capsule_spacing end,
      height = H, align = "center",
    }
    tiles[side] = {}
    if #groups == 0 then
      children[#children + 1] = ui.Rect {
        width = 64, height = H, radius = function() return H() / 2 end,
        color = "#00000000", border_width = 1, border_color = C.islandBorder,
        kit.text { anchors = { center_in = true }, text = "Empty", size = theme.size.label,
          color = C.textMuted },
      }
    end
    for _, group in ipairs(groups) do
      if group.kind == "chips" then
        local alone = #group.items == 1
        local row = { height = H, align = "center" }
        for _, item in ipairs(group.items) do row[#row + 1] = bar_tile(side, item, alone) end
        local chips = ui.Row(row)
        local pad = (chromeless() or alone) and 0 or 4
        local bare = alone and is_module(group.items[1].id)
          and modules.shape_of(group.items[1].id, group.items[1].shape) == "ring"
          and modules.figure_of(group.items[1].figure) ~= "on"
        children[#children + 1] = ui.Rect {
          width = function() return (chips.layout_width or 0) + 2 * pad end,
          height = H, radius = function() return H() / 2 end,
          color = chromeless() and "#00000000" or C.island,
          border_width = (chromeless() or bare) and 0 or 1, border_color = C.islandBorder,
          ui.Item { x = pad, width = function() return chips.layout_width or 0 end, height = H, chips },
        }
      else
        children[#children + 1] = bar_tile(side, group.items[1], false)
      end
    end
    return ui.Row(children)
  end

  -- ------------------------------------------------------------- stage --

  local stage, tray, editor
  local island_w = function() return modules.entry("clock").width end

  -- The picture is built again whenever the lists or the style change.
  local model = morf.list_model({ { key = 0 } })
  local built = 0
  local watcher = setting.watch(function()
    settings.get("barLeft")
    settings.get("barRight")
    local _ = settings.barStyle
    built = built + 1
    if built > 1 then
      local key = built
      morf.timer(1, function() model:replace({ { key = key } }, "key") end, false)
    end
  end)

  local function scene()
    local left = side_node("left")
    local right = side_node("right")
    local reach = function() return math.max(left.layout_width or 0, right.layout_width or 0) end
    local stage_w = W
    local natural = function()
      return 2 * (island_w() / 2 + theme.capsule_spacing + reach() + MARGIN + (chromeless() and 10 or 0))
    end
    local scene_w = function() return math.max(stage_w, natural()) end
    local middle = function() return scene_w() / 2 end
    return ui.Item {
      width = scene_w, height = H,
      x = function() return (stage_w - scene_w()) / 2 end,
      y = STAGE_PAD,
      scale = function() return math.min(1, stage_w / math.max(1, natural())) end,
      -- One band behind everything in the single-capsule style.
      ui.Rect {
        visible = chromeless,
        x = function() return middle() - (island_w() / 2 + theme.capsule_spacing + reach() + 10) end,
        width = function() return 2 * (island_w() / 2 + theme.capsule_spacing + reach() + 10) end,
        height = H, radius = function() return H() / 2 end,
        color = C.island, border_width = 1, border_color = C.islandBorder,
      },
      ui.Item {
        x = function()
          if style() == "spread" then return MARGIN end
          if chromeless() then return middle() - island_w() / 2 - theme.capsule_spacing - reach() end
          return middle() - island_w() / 2 - theme.capsule_spacing - (left.layout_width or 0)
        end,
        width = function() return left.layout_width or 1 end, height = H,
        left,
      },
      ui.Rect {
        x = function() return middle() - island_w() / 2 end,
        width = island_w, height = H, radius = function() return H() / 2 end,
        color = C.island, border_width = function() return chromeless() and 0 or 1 end,
        border_color = C.islandBorder,
        clock.build(),
      },
      ui.Item {
        x = function()
          if style() == "spread" then return scene_w() - MARGIN - (right.layout_width or 0) end
          if chromeless() then
            return middle() + island_w() / 2 + theme.capsule_spacing + reach() - (right.layout_width or 0)
          end
          return middle() + island_w() / 2 + theme.capsule_spacing
        end,
        width = function() return right.layout_width or 1 end, height = H,
        right,
      },
    }
  end

  -- -------------------------------------------------------------- drag --

  local function inside(rect, x, y)
    return rect and x >= rect.x and y >= rect.y and x < rect.x + rect.width and y < rect.y + rect.height
  end

  -- Where the pointer is: a half of the bar, the catalogue, or nowhere;
  -- and on the bar, the index it would be inserted at.
  local function aim(x, y)
    local stage_rect = window().rect_of(stage)
    local tray_rect = window().rect_of(tray)
    local editor_rect = window().rect_of(editor)
    if editor_rect then
      pointer_x:set(x - editor_rect.x)
      pointer_y:set(y - editor_rect.y)
    end
    if inside(stage_rect, x, y) then
      local side = x < stage_rect.x + stage_rect.width / 2 and "left" or "right"
      local count, edge = 0, nil
      local list = tiles[side]
      for index = 1, #list do
        local r = window().rect_of(list[index])
        if r then
          if r.x + r.width / 2 < x then
            count = index
            edge = r.x + r.width + 2
          elseif not edge then
            edge = r.x - 2
          end
        end
      end
      if not edge then edge = side == "left" and stage_rect.x + stage_rect.width / 2 - 60
        or stage_rect.x + stage_rect.width / 2 + 60 end
      if drag and drag.from == side and count >= drag.index then
        -- Its own place counts once: it leaves before it lands.
        count = count - 1
      end
      over_side:set(side)
      drag.over, drag.at = side, count + 1
      if editor_rect then
        marker_x:set(edge - editor_rect.x)
        marker_y:set(stage_rect.y - editor_rect.y + STAGE_PAD - 4)
      end
      return
    end
    marker_x:set(-1)
    if inside(tray_rect, x, y) then
      over_side:set("tray")
      drag.over, drag.at = "tray", 0
      return
    end
    over_side:set("")
    drag.over, drag.at = "", 0
  end

  local function commit() M.drop(drag) end

  local carried = controls.signal("layout.carried", "")

  press_handlers = function(side, index, item)
    local moved = false
    return ui.MouseArea {
      anchors = { fill = true }, z = 2,
      cursor = function() return held:get() and "grabbing" or "grab" end,
      on_pressed = function()
        moved = false
        drag = { from = side, index = index, item = item, over = side, at = index }
      end,
      on_drag_started = function(sx, sy)
        if not drag then return end
        moved = true
        held:set(true)
        carried:set(item.id)
        unpick()
        aim(sx, sy)
      end,
      on_dragged = function(sx, sy)
        if drag and moved then aim(sx, sy) end
      end,
      on_released = function()
        if not drag then return end
        if moved then
          commit()
        elseif side == "tray" then
          local right = M.items("right")
          right[#right + 1] = { id = item.id, shape = "", figure = "", when = "" }
          M.set_zone("right", right)
        else
          pick(side, index)
        end
        drag = nil
        moved = false
        held:set(false)
        carried:set("")
        over_side:set("")
        marker_x:set(-1)
      end,
      on_wheel = setting.wheel,
    }
  end

  -- ---------------------------------------------------------- catalogue --

  local function entry(id)
    local hovered = controls.signal("layout.entry", false)
    local label = ui.Row {
      gap = 8, align = "center",
      kit.glyph { glyph = glyph_of(id), size = 12, color = C.text },
      kit.text { text = M.name_of(id), size = theme.size.small },
    }
    return ui.Rect {
      width = function() return (label.layout_width or 0) + 28 end,
      height = function() return H() + 4 end,
      radius = function() return (H() + 4) / 2 end,
      color = function() return hovered:get() and C.islandSurfaceHover or C.island end,
      border_width = 1, border_color = C.islandBorder,
      behavior = { color = fast() },
      ui.Item { x = 14, anchors = { vertical_center = true },
        width = function() return label.layout_width or 0 end, height = 16, label },
      ui.MouseArea {
        anchors = { fill = true }, z = -1,
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
      },
      press_handlers("tray", 0, { id = id, shape = "", figure = "", when = "" }),
    }
  end

  local groups = { direction = "column", gap = 16, align = "start", width = W - 16 }
  for _, ids in ipairs(M.catalogue()) do
    local flow = { direction = "row", wrap = true, gap = 8, align = "start", width = W - 16 }
    for _, id in ipairs(ids) do flow[#flow + 1] = entry(id) end
    groups[#groups + 1] = ui.Flex(flow)
  end
  local catalogue = ui.Flex(groups)
  tray = ui.Rect {
    width = W, height = function() return (catalogue.layout_height or 0) + 16 end,
    radius = theme.radius_medium, color = C.islandSurface, border_width = 1,
    border_color = function()
      return (held:get() and over_side:get() == "tray") and C.accent() or C.islandBorder
    end,
    behavior = { border_color = fast() },
    setting.wheel_area(),
    ui.Item { x = 8, y = 8, width = W - 16, height = function() return catalogue.layout_height or 0 end,
      catalogue },
  }

  -- ---------------------------------------------------------- inspector --

  local function look_control(title, options, current, on_selected, enabled)
    return ui.Column {
      gap = 6,
      opacity = function() return (enabled == nil or enabled()) and 1 or 0.55 end,
      kit.text { text = title, size = theme.size.label, weight = 600, color = C.textMuted },
      ui.Item {
        enabled = function() return enabled == nil or enabled() end,
        width = 10, height = 28,
        controls.segmented { options = options, current = current, on_selected = on_selected },
      },
    }
  end

  local picked_module = function()
    local p = picked()
    return p ~= nil and is_module(p.id)
  end
  local inspector = ui.Rect {
    width = W, radius = theme.radius_medium, color = C.island,
    border_width = 1, border_color = C.accent,
    visible = function() return picked() ~= nil end,
    height = function() return picked_module() and 128 or 76 end,
    setting.wheel_area(),
    kit.text {
      x = 14, y = 16, width = W - 220, elide = "right",
      text = function() local p = picked() return p and M.name_of(p.id) or "" end,
      size = theme.size.medium, weight = 600,
    },
    ui.Row {
      anchors = { right = true, right_margin = 14, top = true, top_margin = 12 }, gap = 6, align = "center",
      controls.pill { text = "Remove", height = 26, on_click = remove_picked },
      controls.icon_button { icon = "󰅖", icon_size = 12, on_click = unpick },
    },
    kit.text {
      x = 14, y = 46, text = "Nothing to set: it is drawn one way.", size = theme.size.label,
      color = C.textMuted, visible = function() return not picked_module() end,
    },
    ui.Row {
      x = 14, y = 50, gap = 24,
      visible = picked_module,
      look_control("Shape",
        { { id = "", label = "Like the bar" }, { id = "icon", label = "Icon" }, { id = "ring", label = "Ring" } },
        function() local p = picked() return p and p.shape or "" end,
        function(id) set_look { shape = id } end,
        function() local p = picked() return p ~= nil and modules.ringed[p.id] == true end),
      look_control("Figure",
        { { id = "", label = "Like the bar" }, { id = "off", label = "No" },
          { id = "hover", label = "On hover" }, { id = "on", label = "Always" } },
        function() local p = picked() return p and p.figure or "" end,
        function(id) set_look { figure = id } end),
      ui.Item {
        width = 1, height = 1,
        visible = function() local p = picked() return p ~= nil and modules.runners[p.id] == true end,
        ui.Item { x = 0, y = 0, width = 200, height = 60,
          look_control("When",
            { { id = "", label = "Always" }, { id = "running", label = "While it runs" } },
            function() local p = picked() return p and p.when or "" end,
            function(id) set_look { when = id } end) },
      },
    },
  }

  -- -------------------------------------------------------------- whole --

  stage = ui.ClipRect {
    width = W, height = function() return H() + 2 * STAGE_PAD end,
    radius = theme.radius_medium, color = C.islandSurface, border_width = 1,
    border_color = function()
      local over = over_side:get()
      return (held:get() and (over == "left" or over == "right")) and C.accent() or C.islandBorder
    end,
    behavior = { border_color = fast() },
    -- The half a carried piece would land in.
    ui.Rect {
      x = function() return over_side:get() == "right" and W / 2 or 0 end,
      width = W / 2, height = function() return H() + 2 * STAGE_PAD end,
      visible = function()
        local over = over_side:get()
        return held:get() and (over == "left" or over == "right")
      end,
      color = C.accent, opacity = 0.06,
    },
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_clicked = unpick, on_wheel = setting.wheel,
    },
    ui.Repeater {
      anchors = { fill = true },
      model = model,
      delegate = function()
        local ok, node = pcall(scene)
        if ok then return node end
        morf.log("warn", "impasto: the bar's picture: " .. tostring(node))
        return kit.text { x = 14, y = STAGE_PAD, text = "The picture of the bar failed to draw",
          size = theme.size.small, color = C.red }
      end,
    },
  }

  local ghost_face = ui.Rect {
    width = function() return 40 end, height = function() return H() + 4 end,
    radius = function() return (H() + 4) / 2 end,
    color = C.islandSurfaceHover, border_width = 1, border_color = C.accent,
    kit.glyph {
      anchors = { center_in = true },
      glyph = function() local id = carried:get() return id ~= "" and glyph_of(id) or "" end,
      size = 14, color = C.text,
    },
  }

  editor = ui.Item {
    width = W,
    height = function()
      return (stage.layout_height or 0) + 10 + (picked() and ((inspector.layout_height or 0) + 10) or 0)
        + (tray.layout_height or 0)
    end,
    watcher,
    ui.Flex {
      direction = "column", gap = 10, align = "start", width = W,
      stage, inspector, tray,
    },
    -- Where the piece would land, and the copy under the pointer.
    ui.Rect {
      x = function() return marker_x:get() - 1 end, y = function() return marker_y:get() end,
      width = 2, height = function() return H() + 8 end, radius = 1, color = C.accent,
      visible = function() return held:get() and marker_x:get() >= 0 end,
      z = 10,
    },
    ui.Item {
      z = 11, width = 40, height = function() return H() + 4 end,
      x = function() return pointer_x:get() - 20 end,
      y = function() return pointer_y:get() - (H() + 4) / 2 end,
      visible = function() return held:get() end,
      opacity = 0.92,
      ghost_face,
    },
  }
  return editor
end

return M
