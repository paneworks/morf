-- lib.kit.roving and lib.kit.overflow in the default kit: a toolbar is one
-- Tab stop the arrows walk, skipping a disabled member and remembering
-- where it was left; a menu bar opens with Down and follows the arrows;
-- an overflow toolbar narrowed hides its last items behind "more", whose
-- menu lists them and runs one; breadcrumbs keep their first and last.
--
--     morf test --no-dbus library/tests/kit_roving_overflow_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  morf.surface.width, morf.surface.height = 900, 600
  local kit = require("lib.kit.skins.default").make { variant = "light" }
  package.loaded.kit = kit
  local w = require("lib.kit.widgets")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local handles = {}

  local before = w.push { id = "before", label = "Before", width = 90, height = 34 }
  local bar
  bar, handles.bar = w.toolbar_group { id = "bar", accessible_name = "Format", items = {
    { id = "b1", icon = "format_bold", tooltip = "Bold", on_clicked = function() note("b1") end },
    { id = "b2", icon = "format_italic", tooltip = "Italic", on_clicked = function() note("b2") end },
    { id = "b3", icon = "format_underlined", tooltip = "Underline", enabled = false },
    { separator = true },
    { id = "b4", icon = "link", tooltip = "Link", on_clicked = function() note("b4") end },
  } }
  local after = w.push { id = "after", label = "After", width = 90, height = 34 }

  local menus
  menus, handles.menus = w.menubar { id = "mb", items = {
    { id = "file", label = "File", items = { { id = "file-new", label = "New", on_clicked = function() note("new") end },
      { id = "file-open", label = "Open" } } },
    { id = "edit", label = "Edit", items = { { id = "edit-undo", label = "Undo", on_clicked = function() note("undo") end } } },
    { id = "view", label = "View", items = { { id = "view-zoom", label = "Zoom" } } },
  } }

  local width = morf.signal("test.overflow.width", 600)
  local tools
  tools, handles.tools = w.overflow_toolbar { id = "ot", width = function() return width:get() end, items = {
    { id = "ot-cut", icon = "content_cut", label = "Cut", on_activated = function() note("cut") end },
    { id = "ot-copy", icon = "content_copy", label = "Copy", on_activated = function() note("copy") end },
    { id = "ot-paste", icon = "content_paste", label = "Paste", on_activated = function() note("paste") end },
    { id = "ot-share", icon = "share", label = "Share", on_activated = function() note("share") end },
    { id = "ot-print", icon = "print", label = "Print", on_activated = function() note("print") end },
  } }

  local crumbs
  crumbs, handles.crumbs = w.overflow_breadcrumbs { id = "bc", width = 300, items = {
    { label = "Home" }, { label = "Documents" }, { label = "Projects" }, { label = "morf" }, { label = "library" },
    { label = "tests" } } }

  local composite
  composite, handles.composite = require("lib.kit.composites").toolbar { id = "ct", width = function() return width:get() end,
    items = {
      { id = "ct-new", icon = "add", label = "New", on_clicked = function() note("new") end },
      { id = "ct-bold", icon = "format_bold", tooltip = "Bold", checked = false },
      { separator = true },
      { id = "ct-share", icon = "share", label = "Share" },
      { id = "ct-print", icon = "print", label = "Print" },
    } }

  ui.Item { width = 900, height = 600,
    ui.Item { x = 10, y = 480, width = 700, height = 40, composite },
    ui.Row { x = 10, y = 10, gap = 12, align = "center", before, bar, after },
    ui.Item { x = 10, y = 80, width = 400, height = 40, menus },
    ui.Item { x = 10, y = 300, width = 700, height = 48, tools },
    ui.Item { x = 10, y = 400, width = 300, height = 40, crumbs } }

  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.call = function(name, method, ...) return handles[name][method](...) end
  morf.ipc.width = function(v) width:set(v) end
  morf.ipc.shown = function(name) return handles[name].t.shown end
  morf.ipc.hidden = function(name) return handles[name].t.hidden end
]]

local function load() test.load { source = SOURCE, size = { 900, 600 } } test.settle(200) end

local function focused()
  for _, node in ipairs(test.nodes()) do if node.focused then return node.id end end
end

local function tab_to(id, limit)
  for _ = 1, limit or 12 do
    if focused() == id then return true end
    test.key("Tab") test.settle(20)
  end
  return focused() == id
end

test.it("Tab enters a toolbar once and the arrows walk it, past a disabled member", function()
  load()
  test.click("before") test.settle(20)
  test.truthy(tab_to("before"))
  test.key("Tab") test.settle(30)
  test.eq(focused(), "b1", "Tab did not enter at the first member")
  test.key("Right") test.settle(30)
  test.eq(focused(), "b2")
  test.key("Right") test.settle(30)
  test.eq(focused(), "b4", "the disabled member was not passed over")
  test.key("Right") test.settle(30)
  test.eq(focused(), "b1", "the arrows did not wrap")
  test.key("End") test.settle(30)
  test.eq(focused(), "b4")
  test.key("Return") test.settle(30)
  test.eq(test.ipc("log"), "b4")
  test.eq(#test.logs("error"), 0)
end)

test.it("Tab leaves the toolbar and Shift+Tab comes back to where it was", function()
  load()
  test.truthy(tab_to("before"))
  test.key("Tab") test.settle(30)
  test.key("Right") test.settle(30)
  test.eq(focused(), "b2")
  test.key("Tab") test.settle(30)
  test.eq(focused(), "after", "Tab stopped at another member")
  test.key("Tab", { "shift" }) test.settle(30)
  test.eq(focused(), "b2", "Shift+Tab did not land on the last current member")
  test.eq(test.ipc("call", "bar", "current"), 2)
end)

test.it("a menu bar opens with Down and follows Right", function()
  load()
  test.click("file") test.settle(150)
  test.truthy(test.ipc("call", "menus", "is_open"))
  test.key("Escape") test.settle(150)
  test.falsy(test.ipc("call", "menus", "is_open"))
  test.eq(focused(), "file")
  test.key("Down") test.settle(150)
  test.truthy(test.find { id = "file-new", visible = true }, "Down did not open the menu")
  test.key("Right") test.settle(150)
  test.truthy(test.find { id = "edit-undo", visible = true }, "Right did not open the next menu")
  test.falsy(test.find { id = "file-new", visible = true }, "the first menu stayed open")
  test.eq(test.ipc("call", "menus", "current"), 2)
  test.key("Escape") test.settle(150)
  test.falsy(test.ipc("call", "menus", "is_open"))
  test.eq(focused(), "edit")
  test.eq(#test.logs("error"), 0)
end)

test.it("an overflow toolbar narrowed hides its last items and more runs one", function()
  load()
  test.eq(test.ipc("hidden", "tools"), ",")
  test.ipc("width", 300) test.settle(300)
  test.truthy(test.ipc("hidden", "tools") ~= ",", "nothing went")
  test.falsy(test.find { id = "ot-print", visible = true }, "the last item is still shown")
  test.truthy(test.find { id = "ot-cut", visible = true }, "the first item went")
  test.click("ot-more") test.settle(150)
  test.truthy(test.find { id = "ot-menu-5", visible = true }, "the menu does not list the last item")
  test.falsy(test.find { id = "ot-menu-1", visible = true }, "the menu lists a shown item")
  test.click("ot-menu-5") test.settle(150)
  test.eq(test.ipc("log"), "print")
  test.ipc("width", 700) test.settle(300)
  test.eq(test.ipc("hidden", "tools"), ",")
  test.eq(#test.logs("error"), 0)
end)

test.it("the toolbar composite is one Tab stop the arrows walk, past what overflowed", function()
  load()
  test.click("ct-new") test.settle(30)
  test.eq(test.ipc("log"), "new")
  test.eq(focused(), "ct-new")
  test.key("Right") test.settle(30)
  test.eq(focused(), "ct-bold")
  test.key("End") test.settle(30)
  test.eq(focused(), "ct-print")
  test.key("Home") test.settle(30)
  test.ipc("width", 200) test.settle(200)
  test.truthy(test.ipc("call", "composite", "overflowed"))
  test.key("Right") test.settle(30)
  test.eq(focused(), "ct-bold")
  test.key("Right") test.settle(30)
  test.eq(focused(), "ct-new", "the arrows reached an item in the menu")
  test.eq(#test.logs("error"), 0)
end)

test.it("breadcrumbs keep their first and last", function()
  load()
  local shown = test.ipc("shown", "crumbs")
  test.truthy(shown:find(",1,", 1, true) and shown:find(",6,", 1, true), shown)
  test.truthy(test.ipc("hidden", "crumbs") ~= ",", "nothing went from 300 px")
  test.truthy(test.find { id = "bc-more", visible = true }, "no ellipsis")
  test.eq(#test.logs("error"), 0)
end)
