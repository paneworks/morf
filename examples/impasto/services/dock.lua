-- The dock's state: pinned applications, running windows, and the geometry.
--
-- Port of DockService.qml. Pinned entries come first in their saved order,
-- then other running applications in the order they were opened. A pinned
-- application that is running is a single item, which means matching window
-- classes to desktop entries (below).
--
-- Windows come from the compositor-agnostic `morf.windows` (the foreign
-- toplevel list), and are acted on through `morf.toplevel`. Where Hyprland is
-- there and `lib.hyprland` can be required (`MORF_RUNTIME_PATH` naming
-- `examples/`), its clients add what the protocol does not say: each
-- window's workspace, for the menu, and which monitor a fullscreen window
-- covers. Under Hyprland without the protocol, its clients are the list.
--
-- -- MATCHING WINDOWS TO DESKTOP ENTRIES -----------------------------------
--
-- `StartupWMClass` is the intended field but many entries lack it, so the
-- entry's file id, its last dotted segment, its `Exec` binary and its name
-- are also scored, and the highest claim on a class wins. When several
-- entries share a `StartupWMClass` (a browser and its web apps), the one
-- whose id matches it wins. Unmatched windows are still shown under their
-- class.
--
-- Everything a binding reads goes through `M.items()`, `M.dragging()` and
-- the other readers here, each of which reads a signal, so a binding follows
-- the dock without anything subscribing by hand.

local settings = require("services.settings")
local theme = require("theme")

local M = {}

-- ------------------------------------------------------------- hyprland --

local hyprland
for _, name in ipairs { "lib.hyprland", "hyprland" } do
  local ok, lib = pcall(require, name)
  if ok and type(lib) == "table" and lib.available then
    hyprland = lib
    break
  end
end

local function hypr()
  return hyprland and hyprland.available() and hyprland or nil
end

-- -------------------------------------------------------------- signals --

local s = {
  windows = morf.signal("impasto.dock.windows", 0),
  items = morf.signal("impasto.dock.items", 0),
  dragging = morf.signal("impasto.dock.dragging", ""),
  drag_from = morf.signal("impasto.dock.drag_from", -1),
  drop_at = morf.signal("impasto.dock.drop_at", -1),
  hovered = morf.signal("impasto.dock.hovered", ""),
  menu = morf.signal("impasto.dock.menu", ""),
  peeked = morf.signal("impasto.dock.peeked", false),
  covered = morf.signal("impasto.dock.covered", false),
}
M.signals = s

-- The launcher button's key in `hovered`; not an item.
M.LAUNCHER = "@launcher"

-- ------------------------------------------------------------- settings --

function M.enabled() return settings.dockEnabled end
function M.edge()
  local edge = settings.dockEdge
  -- Never the top edge: that is the bar's.
  if edge ~= "left" and edge ~= "right" then return "bottom" end
  return edge
end
function M.alignment() return settings.dockAlignment end
function M.vertical() return M.edge() ~= "bottom" end
function M.shows_running() return settings.dockRunning end
function M.autohide() return settings.dockAutohide end
function M.has_launcher() return settings.dockLauncher end
function M.everywhere() return settings.dockEverywhere end
function M.opacity() return settings.dockOpacity / 100 end

-- ---------------------------------------------------------------- pinned --

local function id_of(id)
  return (tostring(id or ""):gsub("%.desktop$", ""))
end

--- The pinned ids in their order, without `.desktop` (the original saved
--- file names; both spellings are read).
function M.pinned()
  local out, seen = {}, {}
  for _, id in ipairs(settings.dockPinned or {}) do
    local clean = id_of(id)
    if clean ~= "" and not seen[clean] then
      seen[clean] = true
      out[#out + 1] = clean
    end
  end
  return out
end

function M.is_pinned(id)
  if not id or id == "" then return false end
  for _, entry in ipairs(M.pinned()) do
    if entry == id then return true end
  end
  return false
end

function M.pin(id)
  if not id or id == "" or M.is_pinned(id) then return end
  local next = M.pinned()
  next[#next + 1] = id
  settings.set("dockPinned", next)
end

function M.unpin(id)
  local next = {}
  for _, entry in ipairs(M.pinned()) do
    if entry ~= id then next[#next + 1] = entry end
  end
  settings.set("dockPinned", next)
end

function M.toggle_pin(id)
  if M.is_pinned(id) then M.unpin(id) else M.pin(id) end
end

--- `from` and `to` are 1-based positions in the pinned list.
function M.reorder(from, to)
  local next = M.pinned()
  if from < 1 or from > #next or to < 1 or to > #next or from == to then return end
  local moved = table.remove(next, from)
  table.insert(next, to, moved)
  settings.set("dockPinned", next)
end

-- ----------------------------------------------------- application index --

local source
local applications = {}
local by_id = {}
local class_index = {}

-- `/usr/bin/foo %U` -> `foo`.
local function binary_of(exec)
  local first = tostring(exec or ""):match("^%s*(%S+)") or ""
  first = first:gsub('"', "")
  return first:match("([^/]+)$") or first
end

local function claim(key, id, score)
  local normalised = tostring(key or ""):match("^%s*(.-)%s*$"):lower()
  if normalised == "" then return end
  local held = class_index[normalised]
  if not held or score > held.score then
    class_index[normalised] = { id = id, score = score }
  end
end

local function index_applications()
  local ok, entries = pcall(morf.desktop_entries)
  if not ok or not entries then
    morf.log("warn", "impasto: dock: no desktop entries: " .. tostring(entries))
    return
  end
  source = entries
  applications = entries:applications() or {}
  by_id, class_index = {}, {}
  for _, app in ipairs(applications) do
    by_id[app.id] = app
    local base = app.id
    local wmclass = app.startup_class or ""
    claim(app.name, app.id, 1)
    claim(binary_of(app.exec), app.id, 2)
    -- `org.kde.okular` often has the window class `okular`.
    claim(base:match("([^.]+)$"), app.id, 3)
    claim(base, app.id, 4)
    -- Bonus when the id matches too: breaks ties between a browser and its
    -- web apps.
    claim(wmclass, app.id, wmclass:lower() == base:lower() and 6 or 5)
  end
end

local indexed_at = -math.huge
local clock = morf.elapsed_timer()
local function now() return clock:elapsed_ms() / 1000 end

--- Reads the entries again, at most every half minute: a class nothing
--- claims may be an application installed since.
local function reindex(force)
  if not force and now() - indexed_at < 30 then return false end
  indexed_at = now()
  index_applications()
  return true
end

function M.entry(id) return by_id[id] end

function M.id_for(class)
  local key = tostring(class or ""):match("^%s*(.-)%s*$"):lower()
  if key == "" then return "" end
  local found = class_index[key]
  return found and found.id or ""
end

-- ---------------------------------------------------------------- running --

-- Plain rows: `{ handle, class, title, active, fullscreen, workspace,
-- monitor }`. `handle` is what acting on the window takes: a toplevel
-- identifier, or `hypr:<address>` when the list came from Hyprland.
local windows = {}
local window_signature = ""

--- The screen this instance draws on: `morf.screens[1]`.
local screen_name = ((morf.screens or {})[1] or {}).name or ""

local function hypr_clients()
  local h = hypr()
  if not h then return {} end
  local ok, snap = pcall(h.snapshot)
  return ok and snap and snap.clients or {}
end

local function hypr_active_address()
  local h = hypr()
  if not h then return "" end
  return h.state.active_window.address or ""
end

local function read_windows()
  local out = {}
  local clients = hypr_clients()
  local toplevels = morf.windows or {}
  if #toplevels > 0 then
    -- Workspace and monitor from Hyprland, matched by class and title; the
    -- first unused match wins, which is exact unless two windows of one
    -- application share a title.
    local used = {}
    for _, window in ipairs(toplevels) do
      local row = {
        handle = window.identifier,
        class = window.app_id or "",
        title = window.title or "",
        active = window.activated == true,
        fullscreen = window.fullscreen == true,
        workspace = 0,
        monitor = -1,
      }
      for index, client in ipairs(clients) do
        if not used[index] and client.class == row.class and client.title == row.title then
          used[index] = true
          row.workspace = client.workspace
          row.monitor = client.monitor
          row.fullscreen = (client.fullscreen or 0) >= 2
          break
        end
      end
      out[#out + 1] = row
    end
  else
    local active = hypr_active_address()
    for _, client in ipairs(clients) do
      if client.mapped ~= false and not client.hidden then
        out[#out + 1] = {
          handle = "hypr:" .. client.address,
          class = client.class or "",
          title = client.title or "",
          active = client.address == active,
          fullscreen = (client.fullscreen or 0) >= 2,
          workspace = client.workspace or 0,
          monitor = client.monitor or -1,
        }
      end
    end
    -- Hyprland lists by workspace; the dock wants the order they opened,
    -- which its addresses approximate.
    table.sort(out, function(a, b) return a.handle < b.handle end)
  end
  return out
end

local function signature_of(list)
  local parts = {}
  for _, w in ipairs(list) do
    parts[#parts + 1] = table.concat({
      w.handle, w.class, w.title, tostring(w.active), tostring(w.fullscreen),
      tostring(w.workspace), tostring(w.monitor),
    }, "\31")
  end
  return table.concat(parts, "\30")
end

--- Reads the window list and, when anything changed, tells the bindings.
function M.poll()
  if window_signature:sub(1, 9) == "injected:" then return end
  local list = read_windows()
  local h = hypr()
  local signature = signature_of(list) .. "\29" .. tostring(h and h.monitor_workspace(screen_name))
  if signature == window_signature then return end
  window_signature = signature
  windows = list
  -- A class nothing claims may be an application installed since start.
  for _, w in ipairs(list) do
    if w.class ~= "" and M.id_for(w.class) == "" then
      reindex(false)
      break
    end
  end
  s.windows:set(s.windows:get() + 1)
end

--- Test hook: replaces the window list, as the compositor would. Rows as
--- above; `handle` defaults to a made-up identifier.
function M.inject_windows(list)
  for index, w in ipairs(list) do
    w.handle = w.handle or ("fake:" .. index)
    w.title = w.title or w.class
    w.workspace = w.workspace or 1
    w.monitor = w.monitor or -1
  end
  windows = list
  window_signature = "injected:" .. signature_of(list)
  s.windows:set(s.windows:get() + 1)
end

-- ------------------------------------------------------------------ items --

local items = {}
local by_key = {}
local pinned_count = 0
local items_revision = 0

local function describe(id, class, group)
  local entry = id ~= "" and by_id[id] or nil
  local list = group and group.windows or {}
  local rows, active = {}, false
  for _, w in ipairs(list) do
    rows[#rows + 1] = {
      handle = w.handle,
      title = (w.title ~= "" and w.title) or w.class,
      workspace = w.workspace or 0,
      front = w.active,
    }
    active = active or w.active
  end
  return {
    key = id ~= "" and id or ("class:" .. class:lower()),
    id = id,
    name = entry and entry.name or (class ~= "" and class or id),
    icon = entry and entry.icon or class:lower(),
    pinned = M.is_pinned(id),
    windows = rows,
    running = #rows > 0,
    active = active,
  }
end

local function rebuild()
  local pinned = M.pinned()
  local running = M.shows_running()
  s.windows:get()

  local order, groups = {}, {}
  for _, w in ipairs(windows) do
    local class = (w.class or ""):match("^%s*(.-)%s*$")
    if class ~= "" then
      local id = M.id_for(class)
      local key = id ~= "" and id or ("class:" .. class:lower())
      if not groups[key] then
        groups[key] = { key = key, id = id, class = class, windows = {} }
        order[#order + 1] = groups[key]
      end
      table.insert(groups[key].windows, w)
    end
  end

  local list = {}
  for _, id in ipairs(pinned) do
    list[#list + 1] = describe(id, "", groups[id])
  end
  pinned_count = #pinned
  if running then
    local pinned_set = {}
    for _, id in ipairs(pinned) do pinned_set[id] = true end
    for _, group in ipairs(order) do
      if not (group.id ~= "" and pinned_set[group.id]) then
        list[#list + 1] = describe(group.id, group.class, group)
      end
    end
  end

  items, by_key = list, {}
  for index, item in ipairs(list) do
    item.index = index
    by_key[item.key] = item
  end
  M.model:replace(list, "key")

  -- Covered: a fullscreen window on the workspace this screen shows. Without
  -- Hyprland, which screen a window is on is not known, so any focused
  -- fullscreen window counts.
  local covered = false
  local h = hypr()
  local shown = h and h.monitor_workspace(screen_name) or nil
  for _, w in ipairs(windows) do
    if w.fullscreen then
      if shown then
        if w.workspace == shown then covered = true end
      elseif w.active then
        covered = true
      end
    end
  end
  s.covered:set(covered)
  items_revision = items_revision + 1
  s.items:set(items_revision)
end

--- One row per item, keyed by `key`, for the Repeater.
M.model = morf.list_model({})

--- Every item, in order; a binding that calls it follows the dock.
function M.items() s.items:get() return items end
function M.item(key) s.items:get() return by_key[key] end
function M.index_of(key)
  s.items:get()
  local item = by_key[key]
  return item and item.index or -1
end
function M.count() s.items:get() return #items end
function M.pinned_count() s.items:get() return pinned_count end
--- A divider is drawn only when there are both pinned and unpinned items.
function M.divides()
  s.items:get()
  return pinned_count > 0 and #items > pinned_count
end

--- Whether it is painted on this screen: away under a fullscreen window,
--- and, set to one dock, only on the screen being worked on (`live`).
function M.shown(live)
  if not M.enabled() then return false end
  if M.count() == 0 and not M.has_launcher() then return false end
  if s.covered:get() then return false end
  return M.everywhere() or live
end

--- The band windows keep clear of it: only when it does not hide.
function M.zone()
  if not M.enabled() or M.autohide() then return 0 end
  if M.count() == 0 and not M.has_launcher() then return 0 end
  return theme.dock_margin + theme.dock_thickness()
end

-- --------------------------------------------------------------- geometry --
--
-- Computed rather than measured, so the label, the menu and the drag all use
-- the same arithmetic as the icons.

function M.lead()
  return M.has_launcher() and (theme.dock_icon() + 2 * theme.dock_gap) or 0
end

--- Offset along the capsule of the item at 1-based `index`.
function M.offset_of(index)
  local i = index - 1
  local extra = (M.divides() and index > M.pinned_count()) and theme.dock_gap or 0
  return theme.dock_padding + M.lead() + i * (theme.dock_icon() + theme.dock_gap) + extra
end

M.launcher_offset = function() return theme.dock_padding end

--- The capsule's length along its edge.
function M.length()
  local count = M.count()
  if count == 0 then
    return M.has_launcher() and (2 * theme.dock_padding + theme.dock_icon()) or 0
  end
  return M.offset_of(count) + theme.dock_icon() + theme.dock_padding
end

-- ------------------------------------------------------------------- drag --
--
-- Only pinned items reorder. During a drag the list is untouched and items
-- shift by binding to these values, so no delegate is rebuilt and the grab
-- survives. The list is written once, on drop.

function M.dragging() return s.dragging:get() end

--- Display index of item `index` while a drag is in progress.
function M.shifted(index)
  local dragging, from, at = s.dragging:get(), s.drag_from:get(), s.drop_at:get()
  if dragging == "" or from < 1 or at < 1 or index == from then return index end
  if from < at then
    return (index > from and index <= at) and index - 1 or index
  end
  return (index >= at and index < from) and index + 1 or index
end

--- Drop slot for an offset along the capsule, clamped to the pinned run.
function M.slot_at(offset)
  local step = theme.dock_icon() + theme.dock_gap
  local raw = math.floor((offset - theme.dock_padding - M.lead()) / step + 0.5) + 1
  return math.max(1, math.min(pinned_count, raw))
end

function M.begin_drag(key, index)
  s.dragging:set(key)
  s.drag_from:set(index)
  s.drop_at:set(index)
end

function M.set_drop(slot)
  if s.dragging:get() ~= "" then s.drop_at:set(slot) end
end

function M.end_drag()
  if s.dragging:get() ~= "" then M.reorder(s.drag_from:get(), s.drop_at:get()) end
  s.dragging:set("")
  s.drag_from:set(-1)
  s.drop_at:set(-1)
end

-- ---------------------------------------------------------------- actions --

local function focus_window(handle)
  local address = handle:match("^hypr:(.+)$")
  if address then
    local h = hypr()
    if h then h.dispatch("focuswindow", "address:" .. address) end
    return
  end
  morf.toplevel.activate(handle)
end

local function close_window(handle)
  local address = handle:match("^hypr:(.+)$")
  if address then
    local h = hypr()
    if h then h.dispatch("closewindow", "address:" .. address) end
    return
  end
  morf.toplevel.close(handle)
end

M.focus_window = focus_window

--- New instance. Unmatched windows have no entry to launch from.
function M.launch(item)
  if not item or item.id == "" or not source then return end
  local ok, err = pcall(source.launch, source, item.id)
  if not ok then morf.log("warn", "impasto: dock: could not launch " .. item.id .. ": " .. tostring(err)) end
end

--- Click: launch if not running, focus if not focused, otherwise cycle to
--- the application's next window.
function M.activate(item)
  if not item then return end
  if not item.running then return M.launch(item) end
  if not item.active then return focus_window(item.windows[1].handle) end
  if #item.windows < 2 then return end
  local at = 1
  for index, w in ipairs(item.windows) do
    if w.front then at = index break end
  end
  focus_window(item.windows[at % #item.windows + 1].handle)
end

function M.close_all(item)
  if not item then return end
  for _, w in ipairs(item.windows) do close_window(w.handle) end
end

-- ------------------------------------------------------------------- menu --

--- The item whose menu is open, or nil. Tracked by key: `items` is rebuilt
--- whenever a window opens or closes, and a menu whose item went away closes.
function M.menu_item()
  local key = s.menu:get()
  if key == "" then return nil end
  return M.item(key)
end

function M.open_menu(key)
  s.menu:set(s.menu:get() == key and "" or key)
end

function M.close_menu() s.menu:set("") end

-- ------------------------------------------------------------------ start --

index_applications()
indexed_at = now()
M.poll()
morf.effect("impasto.dock.items", rebuild)

-- The window list is a plain table the engine refills in place, so it is
-- read on a timer; only while the dock is on.
local poller
morf.effect("impasto.dock.poll", function()
  local on = M.enabled()
  if on and not poller then
    poller = morf.timer(400, M.poll, true)
  elseif not on and poller then
    poller:cancel()
    poller = nil
  end
end)

return M
