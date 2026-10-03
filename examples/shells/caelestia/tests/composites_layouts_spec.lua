-- The layout composites (library/lib/kit/composites/: tab_view, sidebar,
-- carousel, wizard, transfer_list, kanban, dashboard, file_chooser)
-- worked by the pointer and by the keyboard, in each theme.
--
--     morf test --no-dbus examples/shells/caelestia/tests/composites_layouts_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local widgets = require("lib.kit.widgets")
  local composites = require("lib.kit.composites")
  morf.surface.height = 1000
  local got = {}
  local function keep(name) return function(...)
    local parts = {}
    for _, v in ipairs({ ... }) do
      if type(v) ~= "table" then parts[#parts + 1] = tostring(v)
      elseif #v > 0 then parts[#parts + 1] = table.concat(v, ",") end
    end
    got[name] = table.concat(parts, " ")
  end end

  -- A folder to choose from.
  local dir = (morf.env("XDG_RUNTIME_DIR") or "/tmp") .. "/morf-layouts-" .. tostring(morf.process_id)
  pcall(morf.fs.remove, dir, { recursive = true })
  for _, d in ipairs { dir, dir .. "/alpha", dir .. "/alpha/deep", dir .. "/beta" } do morf.fs.mkdir(d) end
  for _, f in ipairs { "photo.png", "notes.txt", "zeta.jpg", "alpha/inner.txt", "alpha/shot.png" } do
    morf.fs.write(dir .. "/" .. f, "x")
  end

  local tv_node, tv = composites.tab_view { id = "tv", width = 520, height = 300, on_changed = keep("tab"),
    tabs = { { title = "Notes", icon = "description" }, { title = "Mail", icon = "mail" },
      { title = "Music", icon = "music_note" } } }
  local sb_node, sb = composites.sidebar { id = "sb", width = 240, height = 340, on_changed = keep("side"),
    on_collapsed = keep("collapsed"),
    sections = {
      { title = "Library", items = { { key = "inbox", label = "Inbox", icon = "inbox", badge = 4 },
        { key = "sent", label = "Sent", icon = "send" }, { key = "drafts", label = "Drafts", icon = "draft" } } },
      { title = "Labels", items = { { key = "work", label = "Work", icon = "work" },
        { key = "home", label = "Home", icon = "home", badge = "new" } } } } }
  local car_node, car = composites.carousel { id = "car", width = 520, height = 300, on_changed = keep("slide"),
    slides = { { title = "One", icon = "looks_one" }, { title = "Two", icon = "looks_two" },
      { title = "Three", icon = "looks_3" } } }
  local name = morf.state { value = "" }
  local wiz_node, wiz = composites.wizard { id = "wiz", width = 520, height = 320, on_finish = keep("finished"),
    steps = {
      { title = "Name", content = function(w)
          return ui.Item { width = w, height = 120,
            (widgets.entry { id = "wiz-name", x = 20, y = 30, width = 260, height = 40, inset = { 10, 0, 10, 0 },
              placeholder = "Your name", on_edited = function(t) name.value = t end }) }
        end,
        validate = function() if name.value == "" then return false, "Enter a name" end return true end },
      { title = "Theme" }, { title = "Done" } } }
  local tr_node, tr = composites.transfer_list { id = "tr", width = 520, height = 260, on_changed = keep("chosen"),
    items = { "Name", "Size", "Type", "Modified", "Owner" }, chosen = { "Name" } }
  local kb_node, kb = composites.kanban { id = "kb", width = 480, height = 300, on_moved = keep("moved"),
    columns = {
      { key = "todo", title = "To do", cards = { { key = "a", title = "Write docs", tag = "docs" },
        { key = "b", title = "Fix bug", tag = "core" } } },
      { key = "doing", title = "Doing", cards = { { key = "c", title = "Review" } } },
      { key = "done", title = "Done", cards = {} } } }
  local db_node, db = composites.dashboard { id = "db", width = 520, height = 280, on_reordered = keep("order"),
    tiles = { { key = "cpu", title = "CPU", icon = "memory", value = "12%", level = 0.12 },
      { key = "ram", title = "Memory", icon = "memory_alt", value = "4.1 GB", level = 0.5 },
      { key = "net", title = "Network", icon = "wifi", value = "48 Mb/s" },
      { key = "disk", title = "Disk", icon = "hard_drive", value = "61%", level = 0.61 },
      { key = "bat", title = "Battery", icon = "battery_full", value = "88%", level = 0.88 } } }
  local width = morf.state { w = 640 }
  local fc_node, fc = composites.file_chooser { id = "fc", width = function() return width.w end, height = 320,
    root = dir, mode = "open", on_accepted = keep("file"), on_cancelled = keep("cancelled"),
    places = { { label = "Fixture", path = dir, icon = "folder" }, { label = "Alpha", path = dir .. "/alpha" } },
    filters = { { name = "All files" }, { name = "Images", patterns = { "png", "jpg" } } } }
  kit.card { width = 1800, height = 1000,
    ui.Item { x = 20, y = 20, tv_node }, ui.Item { x = 560, y = 20, sb_node },
    ui.Item { x = 820, y = 20, car_node }, ui.Item { x = 1360, y = 20, width = 1, height = 1 },
    ui.Item { x = 20, y = 350, wiz_node }, ui.Item { x = 820, y = 360, tr_node },
    ui.Item { x = 20, y = 690, db_node }, ui.Item { x = 1240, y = 650, kb_node },
    ui.Item { x = 560, y = 660, fc_node } }
  morf.ipc.got = function(k) return got[k] or "" end
  morf.ipc.tabs = function() return tv.titles() .. "|" .. tv.current() .. "|" .. tostring(tv.overview()) end
  morf.ipc.side = function() return sb.current() .. "|" .. tostring(sb.collapsed()) .. "|" .. tostring(sb.expanded(2)) end
  morf.ipc.slide = function() return car.current() end
  morf.ipc.wizard = function() return wiz.current() .. "|" .. wiz.error() .. "|" .. tostring(wiz.finished()) end
  morf.ipc.chosen = function() return table.concat(tr.chosen(), ",") end
  morf.ipc.cards = function(col) return table.concat(kb.cards(col), ",") end
  morf.ipc.order = function() return table.concat(db.order(), ",") .. "|" .. db.size() end
  morf.ipc.fc = function() return fc.path():sub(#dir + 1) .. "|" .. table.concat(fc.entries(), ",") end
  morf.ipc.fc_shell = function() return tostring(fc.shell.t.collapsed) .. "|" .. tostring(fc.shell.t.sidebar_open) end
  morf.ipc.narrow = function(w) width.w = tonumber(w) end
  morf.ipc.dir = function() return dir end
  morf.ipc.cleanup = function() morf.fs.remove(dir, { recursive = true }) end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 1800, 1000 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
  test.settle(800)
end
local function got(name) return test.ipc("got", name) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS") == "1" then test.snapshot(name .. ".png") end end
-- The node of that id that shows (a kept page's twin may be hidden).
local function shown(id)
  for _, node in ipairs(test.find_all(id)) do if node.visible then return node end end
  test.truthy(false, "no visible " .. id)
end
local function centre(node) return node.x + node.width / 2, node.y + node.height / 2 end
local function clean()
  test.ipc("cleanup")
  test.eq(#test.logs("error"), 0, "errors were logged")
  test.eq(#test.logs("warn"), 0, "warnings were logged")
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " tab view switches, reorders, closes and adds tabs, and shows the overview", function()
    load(style)
    test.eq(test.ipc("tabs"), "Notes,Mail,Music|1|false")
    test.click("tv-tab-2") test.settle(400)
    test.eq(test.ipc("tabs"), "Notes,Mail,Music|2|false")
    test.truthy(test.get("tv-page-2").visible)
    -- The arrows walk the strip; Alt and an arrow moves the tab.
    test.key("Right") test.settle(400)
    test.eq(test.ipc("tabs"), "Notes,Mail,Music|3|false")
    test.key("Left", "alt") test.settle(300)
    test.eq(test.ipc("tabs"), "Notes,Music,Mail|2|false")
    -- A drag along the strip moves it too.
    local first = test.get("tv-tab-1")
    test.drag({ first.x + 40, first.y + 20 }, { first.x + 190, first.y + 20 }, { steps = 8 }) test.settle(300)
    test.eq(test.ipc("tabs"), "Music,Notes,Mail|2|false")
    shot(style .. "-layouts-tabs")
    test.click("tv-close-3") test.settle(300)
    test.eq(test.ipc("tabs"), "Music,Notes|2|false")
    test.click("tv-add") test.settle(400)
    test.eq(test.ipc("tabs"), "Music,Notes,New tab|3|false")
    test.key("w", "ctrl") test.settle(300)
    test.eq(test.ipc("tabs"), "Music,Notes|2|false")
    -- The overview: a press on a card opens its tab.
    test.click("tv-overview") test.settle(300)
    test.eq(test.ipc("tabs"), "Music,Notes|2|true")
    test.truthy(test.get("tv-card-1").visible)
    test.click("tv-card-1") test.settle(400)
    test.eq(test.ipc("tabs"), "Music,Notes|1|false")
    clean()
  end)

  test.it(style .. " sidebar folds its sections, picks by pointer and keys, and collapses to icons", function()
    load(style)
    test.eq(test.ipc("side"), "inbox|false|true")
    test.click("sb-item-sent") test.settle(200)
    test.eq(got("side"), "sent")
    test.key("Down") test.settle(200)
    test.eq(test.ipc("side"), "drafts|false|true")
    local section = test.get("sb-section-2")
    test.click(section.x + 60, section.y + 16) test.settle(400)
    test.eq(test.ipc("side"), "drafts|false|false")
    test.eq(test.get("sb-section-2").height, 32)
    test.click(section.x + 60, section.y + 16) test.settle(400)
    test.click("sb-item-work") test.settle(200)
    test.eq(got("side"), "work")
    test.click("sb-toggle") test.settle(500)
    test.eq(got("collapsed"), "true")
    test.eq(test.get("sb").width, 56)
    test.truthy(test.get("sb-rail-home").visible)
    test.click("sb-rail-inbox") test.settle(200)
    test.eq(test.ipc("side"), "inbox|true|true")
    shot(style .. "-layouts-sidebar")
    test.click("sb-toggle") test.settle(500)
    test.eq(test.get("sb").width, 240)
    clean()
  end)

  test.it(style .. " carousel turns by arrows, keys, swipes and dots", function()
    load(style)
    test.eq(test.ipc("slide"), 1)
    test.click("car-next") test.settle(400)
    test.eq(test.ipc("slide"), 2)
    test.truthy(test.get("car-slide-2").visible)
    -- A press on the slide gives it the keys.
    local swipe = test.get("car-swipe")
    local x, y = centre(swipe)
    test.click(x, y) test.key("Right") test.settle(400)
    test.eq(test.ipc("slide"), 3)
    test.key("Right") test.settle(400)
    test.eq(test.ipc("slide"), 3)
    test.drag({ x - 100, y }, { x + 100, y }, { steps = 8 }) test.settle(400)
    test.eq(test.ipc("slide"), 2)
    test.click("car-dot-1") test.settle(400)
    test.eq(test.ipc("slide"), 1)
    test.click("car-previous") test.settle(300)
    test.eq(test.ipc("slide"), 1)
    shot(style .. "-layouts-carousel")
    clean()
  end)

  test.it(style .. " wizard validates each step and finishes", function()
    load(style)
    test.click("wiz-next") test.settle(300)
    test.eq(test.ipc("wizard"), "1|Enter a name|false")
    -- Nor past it by the header.
    test.click("wiz-step-3") test.settle(300)
    test.eq(test.ipc("wizard"), "1|Enter a name|false")
    test.click("wiz-name-field") test.type("Ada") test.settle(100)
    test.click("wiz-next") test.settle(400)
    test.eq(test.ipc("wizard"), "2||false")
    test.click("wiz-back") test.settle(400)
    test.eq(test.ipc("wizard"), "1||false")
    test.click("wiz-step-2") test.settle(400)
    test.eq(test.ipc("wizard"), "2||false")
    test.key("Right", "alt") test.settle(400)
    test.eq(test.ipc("wizard"), "3||false")
    test.key("Left", "alt") test.settle(400)
    test.eq(test.ipc("wizard"), "2||false")
    test.click("wiz-next") test.settle(400)
    test.truthy(test.get("wiz-finish").visible)
    shot(style .. "-layouts-wizard")
    test.click("wiz-finish") test.settle(200)
    test.eq(test.ipc("wizard"), "3||true")
    test.eq(got("finished"), "")
    clean()
  end)

  test.it(style .. " transfer list moves selected items both ways", function()
    load(style)
    test.eq(test.ipc("chosen"), "Name")
    test.click("tr-left-Size") test.click("tr-left-Owner") test.settle(100)
    test.click("tr-add") test.settle(200)
    test.eq(test.ipc("chosen"), "Name,Size,Owner")
    test.eq(got("chosen"), "Name,Size,Owner")
    test.click("tr-right-Name") test.settle(100)
    test.click("tr-remove") test.settle(200)
    test.eq(test.ipc("chosen"), "Size,Owner")
    -- The keys: the arrows walk, Return moves the current one across.
    test.click("tr-left-Type") test.settle(100)
    test.key("Down") test.key("Return") test.settle(200)
    test.eq(test.ipc("chosen"), "Size,Modified,Owner")
    shot(style .. "-layouts-transfer")
    test.click("tr-add-all") test.settle(200)
    test.eq(test.ipc("chosen"), "Name,Size,Type,Modified,Owner")
    test.click("tr-remove-all") test.settle(200)
    test.eq(test.ipc("chosen"), "")
    clean()
  end)

  test.it(style .. " kanban moves a card by drag and by keys", function()
    load(style)
    test.eq(test.ipc("cards", "todo"), "a,b")
    local card = test.get("kb-card-a")
    local done = test.get("kb-column-done")
    test.drag({ card.x + 40, card.y + 20 }, { done.x + 60, done.y + 60 }, { steps = 10 }) test.settle(300)
    test.eq(test.ipc("cards", "todo"), "b")
    test.eq(test.ipc("cards", "done"), "a")
    test.eq(got("moved"), "a todo done 1")
    -- Alt and the arrows move the current card.
    test.click("kb-card-a") test.settle(100)
    test.key("Left", "alt") test.settle(300)
    test.eq(test.ipc("cards", "doing"), "a,c")
    test.key("Down", "alt") test.settle(300)
    test.eq(test.ipc("cards", "doing"), "c,a")
    test.key("Left", "alt") test.settle(300)
    test.eq(test.ipc("cards", "todo"), "b,a")
    test.eq(test.ipc("cards", "doing"), "c")
    -- Within a column by drag.
    local a = test.get("kb-card-a")
    test.drag({ a.x + 40, a.y + 20 }, { a.x + 40, a.y - 50 }, { steps = 8 }) test.settle(300)
    test.eq(test.ipc("cards", "todo"), "a,b")
    shot(style .. "-layouts-kanban")
    clean()
  end)

  test.it(style .. " dashboard rearranges tiles by drag and keys, and resizes them", function()
    load(style)
    test.eq(test.ipc("order"), "cpu,ram,net,disk,bat|medium")
    local cpu, net = test.get("db-tile-cpu"), test.get("db-tile-net")
    test.drag({ cpu.x + 30, cpu.y + 30 }, { net.x + 30, net.y + 30 }, { steps = 10 }) test.settle(300)
    test.eq(test.ipc("order"), "ram,net,cpu,disk,bat|medium")
    test.eq(got("order"), "ram,net,cpu,disk,bat")
    test.click("db-tile-cpu") test.key("Right", "alt") test.settle(300)
    test.eq(test.ipc("order"), "ram,net,disk,cpu,bat|medium")
    test.key("Left", "alt") test.key("Left", "alt") test.settle(300)
    test.eq(test.ipc("order"), "ram,cpu,net,disk,bat|medium")
    test.click("db-size-large") test.settle(300)
    test.eq(test.ipc("order"), "ram,cpu,net,disk,bat|large")
    test.truthy(test.get("db-tile-cpu").width > cpu.width)
    shot(style .. "-layouts-dashboard")
    test.click("db-size-small") test.settle(300)
    test.truthy(test.get("db-tile-cpu").width < cpu.width)
    clean()
  end)

  test.it(style .. " file chooser walks folders, filters, picks a file and collapses its places", function()
    load(style)
    test.eq(test.ipc("fc"), "|alpha,beta,notes.txt,photo.png,zeta.jpg")
    -- Into a folder: a double press, then Return on one inside.
    local alpha = shown("fc-entry-alpha")
    test.click(alpha.x + 60, alpha.y + 10) test.settle(50) test.click(alpha.x + 60, alpha.y + 10) test.settle(500)
    test.eq(test.ipc("fc"), "/alpha|deep,inner.txt,shot.png")
    test.key("Home") test.key("Return") test.settle(500)
    test.eq(test.ipc("fc"), "/alpha/deep|")
    -- Back, then up by the breadcrumbs and the up press.
    test.click("fc-back") test.settle(500)
    test.eq(test.ipc("fc"), "/alpha|deep,inner.txt,shot.png")
    test.click("fc-crumb-1") test.settle(500)
    test.eq(test.ipc("fc"), "|alpha,beta,notes.txt,photo.png,zeta.jpg")
    test.click("fc-place-2") test.settle(500)
    test.eq(test.ipc("fc"), "/alpha|deep,inner.txt,shot.png")
    test.click("fc-up") test.settle(500)
    test.eq(test.ipc("fc"), "|alpha,beta,notes.txt,photo.png,zeta.jpg")
    -- The filter.
    test.click("fc-filter") test.settle(300)
    test.click("fc-filter-item-2") test.settle(300)
    test.eq(test.ipc("fc"), "|alpha,beta,photo.png,zeta.jpg")
    -- A file: chosen by a press, accepted by Open.
    test.click(shown("fc-entry-photo.png")) test.settle(100)
    test.click("fc-accept") test.settle(100)
    test.eq(got("file"), test.ipc("dir") .. "/photo.png")
    -- The path field goes where it is told.
    test.click("fc-path-field") test.key("a", "ctrl") test.type(test.ipc("dir") .. "/beta") test.key("Return")
    test.settle(500)
    test.eq(test.ipc("fc"), "/beta|")
    shot(style .. "-layouts-files")
    -- Narrow: the places become a drawer, F9 opens it, Escape shuts it.
    test.eq(test.ipc("fc_shell"), "false|true")
    test.ipc("narrow", 420) test.settle(500)
    test.eq(test.ipc("fc_shell"), "true|false")
    test.falsy(test.get("fc-place-1").visible and test.get("fc-place-1").x >= test.get("fc").x)
    test.click("fc-sidebar-toggle") test.settle(400)
    test.eq(test.ipc("fc_shell"), "true|true")
    shot(style .. "-layouts-files-narrow")
    test.click("fc-place-2") test.settle(500)
    test.eq(test.ipc("fc"), "/alpha|deep,shot.png")
    test.key("F9") test.settle(400)
    test.eq(test.ipc("fc_shell"), "true|false")
    clean()
  end)
end
