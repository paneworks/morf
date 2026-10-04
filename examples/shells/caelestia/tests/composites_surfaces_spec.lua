-- The surface composites (library/lib/kit/composites: command_palette,
-- notification_stack, tour, about_dialog, shortcuts_window, toolbar,
-- status_bar) at work in each caelestia theme, by the pointer and by the
-- keys: the palette filters and runs, a notification swipes away, the
-- tour steps, the about dialog's pages switch, the shortcuts filter, the
-- toolbar overflows and its menu runs an item, the status bar's items
-- press.
--
--     morf test --no-dbus examples/shells/caelestia/tests/composites_surfaces_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local composites = require("lib.kit.composites")
  morf.surface.height = 900
  local log = {}
  local function note(s) log[#log + 1] = s end
  local handles, nodes = {}, {}
  local root = ui.Item { width = 1500, height = 900 }

  -- What the tour points at, and what opens the dialogs.
  local first = kit.card { id = "target-1", x = 40, y = 700, width = 160, height = 44 }
  local second = kit.card { id = "target-2", x = 600, y = 700, width = 160, height = 44 }
  ui.reparent(first, root) ui.reparent(second, root)

  nodes.palette, handles.palette = composites.command_palette { id = "palette", root = root, commands = {
    { title = "Open file", subtitle = "Browse the disk", icon = "folder_open", shortcut = "Ctrl+O", group = "File",
      action = function() note("run:open") end },
    { title = "Save", icon = "save", shortcut = "Ctrl+S", group = "File", action = function() note("run:save") end },
    { title = "Toggle sidebar", icon = "side_navigation", group = "View", action = function() note("run:sidebar") end },
    { title = "Zoom in", icon = "zoom_in", group = "View", action = function() note("run:zoom") end },
  } }

  nodes.notes, handles.notes = composites.notification_stack { id = "notes", root = root, timeout = 0,
    on_dismissed = function(n, reason) note("dismissed:" .. n.title .. ":" .. reason) end,
    on_activated = function(n) note("activated:" .. n.title) end }

  nodes.toasts, handles.toasts = composites.notification_stack { id = "toasts", root = root, mode = "toast",
    timeout = 1000, on_dismissed = function(n, reason) note("toast:" .. n.title .. ":" .. reason) end }

  nodes.tour, handles.tour = composites.tour { id = "tour", root = root, steps = {
    { target = first, title = "First", body = "The first thing." },
    { target = second, title = "Second", body = "The second thing." },
    { title = "Done", body = "Nothing to point at." },
  }, on_finished = function() note("tour:finished") end, on_skipped = function(n) note("tour:skipped:" .. n) end }

  nodes.about, handles.about = composites.about_dialog { id = "about", root = root, app_name = "Morf",
    version = "0.15", comments = "A UI engine.", links = { { label = "Website", url = "https://morf.dev" } },
    developers = { "Ada" }, license = "MIT", legal = "No warranty.",
    on_link = function(url) note("link:" .. url) end }

  nodes.keys, handles.keys = composites.shortcuts_window { id = "keys", root = root, groups = {
    { title = "General", shortcuts = { { keys = "Ctrl+K", description = "Command palette" },
      { keys = "Ctrl+,", description = "Preferences" } } },
    { title = "Editing", shortcuts = { { keys = "Ctrl+Z", description = "Undo" },
      { keys = "Ctrl+Shift+Z", description = "Redo" } } },
  } }

  nodes.toolbar, handles.toolbar = composites.toolbar { id = "tools", x = 20, y = 20, width = 230, items = {
    { id = "new", icon = "add", label = "New", on_clicked = function() note("new") end },
    { id = "bold", icon = "format_bold", tooltip = "Bold", checked = false,
      on_toggled = function(on) note("bold:" .. tostring(on)) end },
    { separator = true },
    { id = "share", icon = "share", label = "Share", on_clicked = function() note("share") end },
    { id = "print", icon = "print", label = "Print", on_clicked = function() note("print") end },
  } }
  ui.reparent(nodes.toolbar, root)

  nodes.status = composites.status_bar { id = "status", x = 300, y = 20, width = 600,
    left = { { id = "branch", icon = "commit", text = "main", on_clicked = function() note("branch") end } },
    center = { { kind = "progress", value = 0.5 } },
    right = { { id = "line", text = "Ln 1, Col 1", on_clicked = function() note("line") end },
      { kind = "badge", count = 2 } } }
  ui.reparent(nodes.status, root)

  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.open = function(name) return handles[name].is_open() end
  morf.ipc.call = function(name, method, arg) return handles[name][method](arg) end
  morf.ipc.shown = function() return table.concat(handles.palette.shown(), ",") end
  morf.ipc.keys_shown = function() return table.concat(handles.keys.shown(), "|") end
  morf.ipc.toast = function(title) return handles.toasts.notify { title = title, icon = "check" } end
  morf.ipc.notify = function(title, body)
    return handles.notes.notify { title = title, body = body, icon = "mail", app = "Mail",
      actions = { { label = "Open", on_clicked = function() note("action:" .. title) end } } }
  end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 1500, 900 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
  test.settle(400)
end

local function focused()
  for _, node in ipairs(test.nodes()) do if node.focused then return node.id end end
end

local function centre(id)
  local n = test.get(id)
  return n.x + n.width / 2, n.y + n.height / 2
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. ": a command palette opens on Ctrl+K, filters and runs", function()
    load(style)
    test.key("k", "ctrl") test.settle(100)
    test.truthy(test.ipc("open", "palette"), "Ctrl+K did not open it")
    test.eq(focused(), "palette-search")
    test.eq(test.ipc("shown"), "1,2,3,4")
    test.type("zo") test.settle(60)
    test.eq(test.ipc("shown"), "4")
    test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "run:zoom")
    test.falsy(test.ipc("open", "palette"))
    -- The arrows walk the commands past the headers; Escape closes.
    test.key("p", { "ctrl", "shift" }) test.settle(100)
    test.truthy(test.ipc("open", "palette"))
    test.eq(test.ipc("shown"), "1,2,3,4", "it did not open afresh")
    test.key("Down") test.key("Down") test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "run:sidebar")
    test.key("k", "ctrl") test.settle(100)
    test.key("Escape") test.settle(60)
    test.falsy(test.ipc("open", "palette"))
    test.eq(test.ipc("log"), "")
    -- A press on a command runs it.
    test.key("k", "ctrl") test.settle(100)
    test.click("palette-command-2") test.settle(60)
    test.eq(test.ipc("log"), "run:save")
    test.falsy(test.ipc("open", "palette"))
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a notification swipes away, closes and expands", function()
    load(style)
    local a = test.ipc("notify", "One", "The first note")
    local b = test.ipc("notify", "Two", "The second note, with a body long enough to wrap onto a second line when expanded.")
    test.settle(100)
    test.truthy(test.ipc("open", "notes"), "the stack did not open")
    test.eq(test.ipc("call", "notes", "count"), 2)
    -- Newest first: Two is on top.
    test.truthy(test.get("notes-note-" .. b).y < test.get("notes-note-" .. a).y)
    -- A short drag springs back; a long one dismisses.
    local x, y = centre("notes-note-" .. b)
    test.drag({ x, y }, { x + 20, y }, { steps = 3 }) test.settle(100)
    test.eq(test.ipc("call", "notes", "count"), 2)
    test.drag({ x, y }, { x + 200, y }, { steps = 6 }) test.settle(100)
    test.eq(test.ipc("log"), "dismissed:Two:swiped")
    test.eq(test.ipc("call", "notes", "count"), 1)
    -- The expand button shows its actions; an action runs and dismisses it.
    test.click("notes-expand-" .. a) test.settle(300)
    test.truthy(test.find { id = "notes-action-" .. a .. "-1", visible = true }, "expanding showed no actions")
    test.click("notes-action-" .. a .. "-1") test.settle(100)
    test.eq(test.ipc("log"), "action:One,dismissed:One:action")
    test.falsy(test.ipc("open", "notes"), "the empty stack is still open")
    -- The close button, and a press on the card.
    local c = test.ipc("notify", "Three", "x")
    test.settle(100)
    test.click("notes-note-" .. c) test.settle(60)
    test.eq(test.ipc("log"), "activated:Three")
    test.click("notes-close-" .. c) test.settle(100)
    test.eq(test.ipc("log"), "dismissed:Three:closed")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": toasts time out at the foot of the surface, or swipe away", function()
    load(style)
    local a = test.ipc("toast", "Copied")
    test.settle(100)
    test.truthy(test.ipc("open", "toasts"))
    local toast = test.get("toasts-note-" .. a)
    test.truthy(toast.y > 600 and math.abs(toast.x + toast.width / 2 - 750) < 4, "a toast is not at the bottom centre")
    test.advance(1200) test.settle(100)
    test.eq(test.ipc("log"), "toast:Copied:timeout")
    test.falsy(test.ipc("open", "toasts"))
    local b = test.ipc("toast", "Saved")
    test.settle(100)
    local x, y = centre("toasts-note-" .. b)
    test.drag({ x, y }, { x - 200, y }, { steps = 6 }) test.settle(100)
    test.eq(test.ipc("log"), "toast:Saved:swiped")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a tour steps between its targets", function()
    load(style)
    test.ipc("call", "tour", "start") test.settle(200)
    test.truthy(test.ipc("open", "tour"))
    local ring, target = test.get("tour-ring"), test.get("target-1")
    test.truthy(ring.visible and ring.x < target.x and ring.x + ring.width > target.x + target.width,
      "the ring is not round the first target")
    local card = test.get("tour-card")
    test.truthy(card.y >= target.y + target.height or card.y + card.height <= target.y, "the card covers its target")
    test.click("tour-next") test.settle(300)
    test.eq(test.ipc("call", "tour", "step"), 2)
    ring, target = test.get("tour-ring"), test.get("target-2")
    test.truthy(ring.x < target.x and ring.x + ring.width > target.x + target.width, "the ring did not move")
    test.click("tour-back") test.settle(300)
    test.eq(test.ipc("call", "tour", "step"), 1)
    -- The keys: Right walks on; Next on the last step finishes.
    test.click("tour-card") test.settle(30)
    test.key("Right") test.settle(300)
    test.eq(test.ipc("call", "tour", "step"), 2)
    test.key("Right") test.settle(300)
    test.eq(test.ipc("call", "tour", "step"), 3)
    test.click("tour-next") test.settle(300)
    test.eq(test.ipc("log"), "tour:finished")
    test.falsy(test.ipc("open", "tour"))
    test.falsy(test.find { id = "tour-ring", visible = true }, "the ring stayed")
    -- Escape skips.
    test.ipc("call", "tour", "start") test.settle(200)
    test.key("Escape") test.settle(100)
    test.eq(test.ipc("log"), "tour:skipped:1")
    test.falsy(test.ipc("open", "tour"))
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": an about dialog switches its pages", function()
    load(style)
    test.ipc("call", "about", "open") test.settle(200)
    test.truthy(test.ipc("open", "about"))
    test.eq(test.ipc("call", "about", "page"), "about")
    test.click("about-link-1") test.settle(30)
    test.eq(test.ipc("log"), "link:https://morf.dev")
    test.click("about-tab-2") test.settle(400)
    test.eq(test.ipc("call", "about", "page"), "credits")
    test.truthy(test.find { text = "Ada", visible = true }, "the credits page shows no names")
    -- The tabs' arrows.
    test.key("Right") test.settle(400)
    test.eq(test.ipc("call", "about", "page"), "legal")
    test.truthy(test.find { text = "No warranty.", visible = true })
    test.key("Escape") test.settle(100)
    test.falsy(test.ipc("open", "about"))
    -- It opens on its first page again; the close button closes it.
    test.ipc("call", "about", "open") test.settle(200)
    test.eq(test.ipc("call", "about", "page"), "about")
    test.click("about-close") test.settle(100)
    test.falsy(test.ipc("open", "about"))
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a shortcuts window filters its keys", function()
    load(style)
    test.ipc("call", "keys", "open") test.settle(200)
    test.truthy(test.ipc("open", "keys"))
    test.eq(focused(), "keys-search")
    test.eq(test.ipc("keys_shown"), "Command palette|Preferences|Undo|Redo")
    test.truthy(test.find { id = "keys-row-2-1", visible = true })
    test.type("redo") test.settle(60)
    test.eq(test.ipc("keys_shown"), "Redo")
    test.type("zzz") test.settle(60)
    test.eq(test.ipc("keys_shown"), "")
    test.truthy(test.find { id = "keys-empty", visible = true }, "an empty list says nothing")
    test.key("Escape") test.settle(100)
    test.falsy(test.ipc("open", "keys"))
    test.ipc("call", "keys", "open") test.settle(200)
    test.eq(test.ipc("keys_shown"), "Command palette|Preferences|Undo|Redo", "it did not open afresh")
    test.click("keys-close") test.settle(100)
    test.falsy(test.ipc("open", "keys"))
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a toolbar overflows into a menu", function()
    load(style)
    test.truthy(test.ipc("call", "toolbar", "overflowed"), "five items fit 230 px")
    test.eq(test.ipc("call", "toolbar", "shown"), 2)
    test.falsy(test.find { id = "share", visible = true }, "an overflowed item is still shown")
    test.click("new") test.settle(30)
    test.click("bold") test.settle(30)
    test.click("bold") test.settle(30)
    test.eq(test.ipc("log"), "new,bold:true,bold:false")
    test.click("tools-more") test.settle(100)
    test.truthy(test.ipc("call", "toolbar", "menu_open"))
    test.falsy(test.find { id = "tools-menu-1", visible = true }, "a shown item is in the menu")
    test.click("tools-menu-5") test.settle(100)
    test.eq(test.ipc("log"), "print")
    test.falsy(test.ipc("call", "toolbar", "menu_open"))
    -- The keys: Return on the more button opens it, Return runs the item.
    test.key("Tab") test.settle(20)
    for _ = 1, 8 do
      if focused() == "tools-more" then break end
      test.key("Tab") test.settle(20)
    end
    test.eq(focused(), "tools-more")
    test.key("Return") test.settle(100)
    test.truthy(test.ipc("call", "toolbar", "menu_open"))
    test.eq(focused(), "tools-menu-4")
    test.key("Return") test.settle(100)
    test.eq(test.ipc("log"), "share")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a status bar's items press", function()
    load(style)
    test.click("branch") test.settle(30)
    test.click("line") test.settle(30)
    test.eq(test.ipc("log"), "branch,line")
    local bar, line = test.get("status"), test.get("line")
    test.truthy(line.x + line.width <= bar.x + bar.width and line.x > bar.x + bar.width / 2, "the right zone is not at the right")
    for _ = 1, 12 do
      test.key("Tab") test.settle(20)
      if focused() == "line" then break end
    end
    test.eq(focused(), "line")
    test.key("space") test.settle(30)
    test.eq(test.ipc("log"), "line")
    test.eq(#test.logs("error"), 0)
  end)
end
