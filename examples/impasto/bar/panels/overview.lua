-- The overview: every workspace side by side, each a scale model of its
-- screen with its windows in place.
--
-- Port of OverviewPanel.qml, at the size DynamicIsland.qml gives it: 1560
-- wide, 72 plus 190 for every row of five workspaces up to `workspaceMax`.
-- Every window is drawn at its real position and size divided by one
-- factor, so a layout is recognisable at a glance. The picture in a window
-- is a capture of it (`morf.screencopy.capture_window`, matched to
-- Hyprland's window by class and title, since the two protocols name
-- windows differently); a window with no capture shows its application's
-- icon.
--
-- Click a workspace to go there; click a window to focus it, right-click
-- to close it; drag a window onto another workspace to move it there. The
-- arrows move a ring over the workspaces and Enter goes to the ringed one.
-- Outside Hyprland the grid is drawn empty and says why.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
local kit = require("components.kit")
local workspaces = require("services.workspaces")
local app_icon = require("components.app_icon")

local C = theme.color

-- impasto's 1560, less on a screen narrower than that (the island leaves
-- a margin either side, as DynamicIsland's `roomForPanel` does).
local SCREEN = (morf.screens or {})[1] or {}
local WIDTH = math.min(1560, (tonumber(SCREEN.width) or 1920) - 2 * 24)
local GAP = 10
local INSET = 3
local CAPTION = 18

local function rows_for(max) return math.ceil(max / 5) end
-- 190 a row at full width; a narrower panel has narrower cells, so its
-- rows shrink with it rather than leaving a band of nothing below them.
local function panel_height()
  local row = math.floor(190 * math.min(1, (WIDTH - 40) / 1520) + 0.5)
  return 72 + rows_for(settings.workspaceMax) * row
end

local KEY = { LEFT = 0xff51, UP = 0xff52, RIGHT = 0xff53, DOWN = 0xff54,
  RETURN = 0xff0d, KP_ENTER = 0xff8d, ESCAPE = 0xff1b }

-- ------------------------------------------------------------- captures --

-- One picture per window address, refreshed round-robin while the overview
-- is open and let go when it closes.
local shots = {}           -- address -> signal of the published source
local shot_names = {}      -- address -> capture name, to release
local capture_timer

local function shot_signal(address)
  local signal = shots[address]
  if not signal then
    signal = morf.signal("impasto.overview.shot." .. address, "")
    shots[address] = signal
  end
  return signal
end

-- The toplevel a Hyprland window is: same class and title, else same title.
local function toplevel_for(client)
  local windows = morf.windows or {}
  for _, window in ipairs(windows) do
    if window.app_id == client.class and window.title == client.title then return window end
  end
  for _, window in ipairs(windows) do
    if window.title == client.title and client.title ~= "" then return window end
  end
end

local next_capture = 1
local function capture_one()
  local clients = workspaces.clients()
  if #clients == 0 or not morf.screencopy then return end
  if next_capture > #clients then next_capture = 1 end
  local client = clients[next_capture]
  next_capture = next_capture + 1
  local toplevel = toplevel_for(client)
  if not toplevel then return end
  local address = client.address
  local name = "impasto-overview-" .. address
  local ok = pcall(morf.screencopy.capture_window, toplevel.identifier, function(frame, err)
    if err or not frame or island.state.open_panel() ~= "overview" then return end
    shot_names[address] = name
    shot_signal(address):set(frame.source or "")
  end, { gpu = true, name = name })
  if not ok then return end
end

local function start_captures()
  if capture_timer then return end
  -- A few at once on open, then one at a time: nine full-size copies in a
  -- frame is a stutter for a grid nobody reads that closely.
  for _ = 1, 3 do capture_one() end
  capture_timer = morf.timer(500, capture_one, true)
end

local function stop_captures()
  if capture_timer then capture_timer:cancel() capture_timer = nil end
  for address, name in pairs(shot_names) do
    pcall(morf.screencopy.release, "gpu:capture/" .. name)
    shot_signal(address):set("")
  end
  shot_names = {}
end

-- ---------------------------------------------------------------- state --

local selected = morf.signal("impasto.overview.selected", 1)
local drop_target = morf.signal("impasto.overview.drop", 0)
local ghost = morf.state { address = "", class = "", x = 0, y = 0, width = 0, height = 0, dx = 0, dy = 0 }

morf.effect("impasto.overview.lifecycle", function()
  local open = island.state.open_panel() == "overview"
  if open then
    morf.timer(1, function()
      workspaces.reload()
      local active = workspaces.active_id()
      selected:set(math.min(math.max(1, active), settings.workspaceMax))
      start_captures()
    end, false)
  else
    morf.timer(1, stop_captures, false)
  end
end)

-- The windows of the open overview's cells follow the compositor. One
-- effect for the file, pointed at whichever overview was built last.
local current_fill
morf.effect("impasto.overview.windows", function()
  workspaces.revision:get()
  if current_fill and island.state.open_panel() == "overview" then current_fill() end
end)

-- ------------------------------------------------------------- geometry --

-- The area windows can occupy on a monitor, in the layout every screen
-- shares: its place and its size in logical pixels, less the bar's band
-- (hyprctl's per-monitor reservation is not in the library's rows).
local function area_for(monitor)
  if not monitor then return { x = 0, y = 0, width = 1920, height = 1200 } end
  local scale = (monitor.scale and monitor.scale > 0) and monitor.scale or 1
  local width = (monitor.width > 0 and monitor.width or 1920) / scale
  local height = (monitor.height > 0 and monitor.height or 1200) / scale
  local top = theme.bar_reserve()
  return { x = monitor.x or 0, y = (monitor.y or 0) + top, width = width, height = height - top }
end

local function build()
  local max = math.max(1, settings.workspaceMax)
  local columns = math.min(5, max)
  local rows = rows_for(max)
  local inner_w = WIDTH - 2 * theme.panel_padding
  local inner_h = panel_height() - 2 * theme.panel_padding
  local board_h = inner_h - CAPTION - GAP

  -- The screen this panel is on decides the shape of a cell.
  local here = area_for(workspaces.monitor_of(workspaces.active_id()))
  local aspect = here.height / here.width

  -- Sized by whichever dimension runs out first, then centred. A cell is
  -- the model plus a margin of constant thickness.
  local cell_w = math.min(
    (inner_w - (columns - 1) * GAP) / columns,
    ((board_h - (rows - 1) * GAP) / rows - 2 * INSET) / aspect + 2 * INSET)
  local cell_h = (cell_w - 2 * INSET) * aspect + 2 * INSET
  local model_w, model_h = cell_w - 2 * INSET, cell_h - 2 * INSET
  local grid_w = columns * cell_w + (columns - 1) * GAP
  local grid_h = rows * cell_h + (rows - 1) * GAP
  local x0 = (inner_w - grid_w) / 2
  local y0 = (board_h - grid_h) / 2

  local function cell_origin(id)
    local column = (id - 1) % columns
    local row = (id - 1) // columns
    return x0 + column * (cell_w + GAP), y0 + row * (cell_h + GAP)
  end

  -- Which cell a point on the board is over, or 0.
  local function cell_at(x, y)
    for id = 1, max do
      local cx, cy = cell_origin(id)
      if x >= cx and x <= cx + cell_w and y >= cy and y <= cy + cell_h then return id end
    end
    return 0
  end

  local function activate(id)
    workspaces.focus(id)
    island.close()
  end

  local function select_by(delta)
    selected:set((selected:get() - 1 + delta) % max + 1)
  end

  -- ------------------------------------------------------------ a window --

  local function thumb(id, factor_of, client_row)
    -- A signal holds only scalars; a state table holds the row's fields,
    -- each followed on its own.
    local function fields(row)
      return {
        x = row.x or 0, y = row.y or 0, width = row.width or 0, height = row.height or 0,
        floating = row.floating == true, class = row.class or "", workspace = row.workspace or 0,
      }
    end
    local data = morf.state(fields(client_row))
    local hovered = kit.hover_signal("thumb")
    local address = client_row.address
    local shot = shot_signal(address)
    local function geometry()
      local area, factor = factor_of()
      local c = data
      return INSET + (c.x - area.x) * factor, INSET + (c.y - area.y) * factor,
        math.max(8, c.width * factor), math.max(8, c.height * factor)
    end
    local node = ui.Item {
      x = function() local x = geometry() return x end,
      y = function() local _, y = geometry() return y end,
      width = function() local _, _, w = geometry() return w end,
      height = function() local _, _, _, h = geometry() return h end,
      -- Floating above tiled: the only stacking hyprctl reports.
      z = function() return data.floating and 2 or 1 end,
      opacity = function() return ghost.address == address and 0.35 or 1 end,
      behavior = {
        x = theme.behave("morph"), y = theme.behave("morph"),
        width = theme.behave("morph"), height = theme.behave("morph"),
        opacity = theme.behave("fast"),
      },
      ui.ClipRect {
        anchors = { fill = true },
        radius = theme.radius_small,
        color = C.island,
        border_width = 2,
        border_color = function() return hovered:get() and C.blue() or C.islandBorder end,
        behavior = { border_color = theme.behave("fast") },
        ui.Image {
          anchors = { fill = true, margins = 2 },
          fill_mode = "preserve_aspect_crop",
          source = function() return shot:get() end,
          visible = function() return shot:get() ~= "" end,
        },
        -- The app icon, for a window with no capture.
        app_icon.node {
          anchors = { center_in = true }, size = 24,
          name = function() return data.class end,
          fallback = "󰖯",
          visible = function() return shot:get() == "" end,
        },
      },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = "pointer",
        on_entered = function() hovered:set(true) selected:set(id) end,
        on_exited = function() hovered:set(false) end,
        -- Left focuses, right closes (the panel stays open for that).
        on_clicked = function(button)
          if button == "right" then
            workspaces.close_window(address)
            return
          end
          if button ~= "left" then return end
          workspaces.focus_window(address)
          island.close()
        end,
        on_drag_started = function()
          local cx, cy = cell_origin(id)
          local tx, ty, tw, th = geometry()
          ghost.class = data.class
          ghost.x, ghost.y, ghost.width, ghost.height = cx + tx, cy + ty, tw, th
          ghost.dx, ghost.dy = 0, 0
          ghost.address = address
        end,
        on_dragged = function(_, _, dx, dy)
          ghost.dx, ghost.dy = dx, dy
          drop_target:set(cell_at(ghost.x + ghost.width / 2 + dx, ghost.y + ghost.height / 2 + dy))
        end,
        on_drag_finished = function(_, _, dx, dy)
          local target = cell_at(ghost.x + ghost.width / 2 + dx, ghost.y + ghost.height / 2 + dy)
          local moving = ghost.address
          ghost.address = ""
          drop_target:set(0)
          -- Another workspace: move it there, tiling decides where. The
          -- same one: nothing to do.
          if moving ~= "" and target ~= 0 and target ~= data.workspace then
            workspaces.move_window(moving, target)
          end
        end,
      },
    }
    return node, function(next_row)
      for key, value in pairs(fields(next_row)) do
        if data[key] ~= value then data[key] = value end
      end
    end
  end

  -- -------------------------------------------------------- a workspace --

  local models = {}
  local function cell(id)
    local cx, cy = cell_origin(id)
    local model = morf.list_model({})
    models[id] = model
    local hovered = kit.hover_signal("cell")
    local windows = function() return workspaces.clients_on(id) end
    local empty = function() return #windows() == 0 end
    local focused = function() return workspaces.active_id() == id end
    local is_selected = function() return selected:get() == id end
    local is_target = function() return drop_target:get() == id end
    -- The screen this workspace models, and one scale that fits it.
    local function factor_of()
      local area = area_for(workspaces.monitor_of(id))
      return area, math.min(model_w / area.width, model_h / area.height)
    end
    local wallpaper = function() return settings.wallpaper or "" end

    return ui.Item {
      x = cx, y = cy, width = cell_w, height = cell_h,
      ui.ClipRect {
        anchors = { fill = true },
        radius = theme.radius_large,
        color = C.island,
        ui.Image {
          anchors = { fill = true },
          fill_mode = "preserve_aspect_crop",
          source_width = 480,
          source = function()
            local path = wallpaper()
            if path == "" then return "" end
            return morf.fs.expand and morf.fs.expand(path) or path
          end,
          visible = function() return wallpaper() ~= "" end,
          opacity = function() return empty() and 0.62 or 0.3 end,
          behavior = { opacity = theme.behave("fast") },
        },
        ui.Repeater {
          model = model,
          delegate = function(client_row) return thumb(id, factor_of, client_row) end,
        },
      },
      -- Both rings in the accent, told apart by weight: thin for the
      -- current workspace, thick for Enter's target and a drop.
      ui.Rect {
        anchors = { fill = true },
        radius = theme.radius_large,
        color = "#00000000",
        z = 3,
        border_width = function() return (is_selected() or is_target()) and 2 or 1 end,
        border_color = function()
          if is_selected() or focused() or is_target() then return C.accent() end
          return C.islandBorder
        end,
        behavior = { border_color = theme.behave("fast") },
      },
      -- The number, on empty cells only; occupied ones are known by their
      -- windows.
      kit.text {
        anchors = { center_in = true },
        text = tostring(id),
        size = math.floor(cell_h * 0.44 + 0.5), weight = 600,
        z = 4,
        opacity = function()
          if not empty() then return 0 end
          return hovered:get() and 0.2 or 0.3
        end,
        behavior = { opacity = theme.behave("medium") },
      },
      -- A click on the cell goes there; windows on top take their own
      -- clicks first.
      ui.MouseArea {
        anchors = { fill = true },
        z = -1,
        cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_position_changed = function()
          if not is_selected() then selected:set(id) end
        end,
        on_clicked = function() activate(id) end,
      },
    }
  end

  local cells = {}
  for id = 1, max do cells[#cells + 1] = cell(id) end

  -- The windows of every cell follow the compositor, keyed by address so
  -- a window that moved within a workspace is the same node, slid.
  local fill = function()
    for id = 1, max do
      local list = {}
      for _, client in ipairs(workspaces.clients_on(id)) do list[#list + 1] = client end
      models[id]:replace(list, "address")
    end
  end
  current_fill = fill
  fill()

  -- One ghost for the whole grid, above every cell, while a window is
  -- dragged.
  local ghost_node = ui.Rect {
    x = function() return ghost.x + ghost.dx end,
    y = function() return ghost.y + ghost.dy end,
    width = function() return math.max(8, ghost.width) end,
    height = function() return math.max(8, ghost.height) end,
    visible = function() return ghost.address ~= "" end,
    radius = theme.radius_small,
    color = C.island,
    border_width = 2,
    border_color = C.blue,
    opacity = 0.92,
    z = 100,
    ui.Image {
      anchors = { fill = true, margins = 2 },
      fill_mode = "preserve_aspect_crop",
      source = function()
        if ghost.address == "" then return "" end
        return shot_signal(ghost.address):get()
      end,
    },
    app_icon.node {
      anchors = { center_in = true }, size = 26,
      name = function() return ghost.class end,
      fallback = "󰖯",
      visible = function()
        return ghost.address ~= "" and shot_signal(ghost.address):get() == ""
      end,
    },
  }

  cells[#cells + 1] = ghost_node

  -- The keyboard: the island's surface has it while a panel is open. The
  -- ring opens on the current workspace, so Enter alone stays put.
  local keys = ui.MouseArea {
    anchors = { fill = true },
    z = -2,
    on_key_pressed = function(keysym)
      if keysym == KEY.LEFT then select_by(-1)
      elseif keysym == KEY.RIGHT then select_by(1)
      elseif keysym == KEY.UP then select_by(-columns)
      elseif keysym == KEY.DOWN then select_by(columns)
      elseif keysym == KEY.RETURN or keysym == KEY.KP_ENTER then activate(selected:get())
      elseif keysym == KEY.ESCAPE then island.close()
      end
    end,
  }

  return ui.Item {
    width = inner_w, height = inner_h,
    keys,
    ui.Item {
      x = 0, y = 0, width = inner_w, height = board_h,
      table.unpack(cells),
    },
    kit.text {
      anchors = { bottom = true, horizontal_center = true },
      size = theme.size.small, color = C.textMuted,
      text = function()
        if not workspaces.available() then
          return "Hyprland is not running here, so there are no workspaces to show"
        end
        return "Click a workspace to go there  ·  drag a window onto another to move it  ·  right-click a window to close it"
      end,
    },
  }
end

island.register("overview", {
  size = function() return WIDTH, panel_height() end,
  build = build,
})
