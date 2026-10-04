-- The input composites (library/lib/kit/composites: combo_box, menu_button,
-- picker, search_bar, tag_input, input_group, rows, media_controls) at
-- work in each caelestia theme: each opens and chooses by the pointer and
-- by the keys, hands what was chosen to its callback, closes on Escape and
-- gives focus back to what opened it.
--
--     morf test --no-dbus examples/shells/caelestia/tests/composites_inputs_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local composites = require("lib.kit.composites")
  local rows = require("lib.kit.composites.rows")
  morf.surface.height = 900
  local log = {}
  local function note(s) log[#log + 1] = s end
  local handles, nodes = {}, {}

  nodes.combo, handles.combo = composites.combo_box { id = "combo", x = 20, y = 20, width = 220,
    items = { "Small", "Medium", "Large" }, current = 1,
    on_changed = function(i, item) note("combo:" .. i .. ":" .. item) end }
  nodes.search, handles.search = composites.combo_box { id = "fruit", x = 260, y = 20, width = 220, search = true,
    items = { "Apple", "Banana", "Cherry", "Date" }, current = 2,
    on_changed = function(i, item) note("fruit:" .. i .. ":" .. item) end }
  nodes.menu, handles.menu = composites.menu_button { id = "file", x = 500, y = 20, width = 140, label = "File",
    items = { { label = "New", on_clicked = function() note("new") end },
              { label = "Open", on_clicked = function() note("open") end } } }
  nodes.split, handles.split = composites.menu_button { id = "save", x = 660, y = 20, width = 160, label = "Save",
    split = true, on_clicked = function() note("save") end,
    items = { { label = "Save as", on_clicked = function() note("save-as") end } } }
  nodes.picker, handles.picker = composites.picker { id = "icon", x = 840, y = 20, width = 200, columns = 4,
    items = { { icon = "home", label = "Home" }, { icon = "star", label = "Star" }, { icon = "bolt", label = "Bolt" },
              { icon = "cloud", label = "Cloud" }, { icon = "pets", label = "Pets" } },
    current = 1, on_changed = function(i, item) note("icon:" .. i .. ":" .. item.label) end }
  nodes.bar, handles.bar = composites.search_bar { id = "find", x = 20, y = 400, width = 400, title = "Files",
    on_search = function(text) note("search:" .. text) end, on_revealed = function(on) note("revealed:" .. tostring(on)) end }
  nodes.tags, handles.tags = composites.tag_input { id = "tags", x = 20, y = 520, width = 460, tags = { "lua" },
    on_changed = function(list) note("tags:" .. table.concat(list, "|")) end }
  nodes.group = composites.input_group { id = "url", x = 500, y = 400, width = 360, prefix = "https://",
    suffix = { id = "url-go", label = "Go", on_clicked = function() note("go") end },
    on_accepted = function(text) note("url:" .. text) end }
  nodes.page = rows.preferences_page { id = "prefs", x = 500, y = 460, width = 440, height = 420, groups = {
    { title = "General", rows = {
      { kind = "switch_row", id = "dark", title = "Dark mode", on_toggled = function(on) note("dark:" .. tostring(on)) end },
      { kind = "check_row", id = "blur", title = "Blur", on_toggled = function(on) note("blur:" .. tostring(on)) end },
      { kind = "combo_row", id = "scale", title = "Scale", items = { "100%", "125%", "150%" }, current = 1,
        on_changed = function(i) note("scale:" .. i) end },
      { kind = "spin_row", id = "size", title = "Font size", value = 11, from = 6, to = 12,
        on_changed = function(v) note("size:" .. math.floor(v)) end },
      { kind = "action_row", id = "about", title = "About", on_activated = function() note("about") end },
      { kind = "entry_row", id = "name", title = "Name", on_accepted = function(t) note("name:" .. t) end },
    } } } }
  local media = morf.signal("test.media", { playing = false, volume = 0.5 })
  nodes.media = composites.media_controls { id = "player", x = 1060, y = 400, width = 400,
    playing = function() return media:get().playing end, position = 30, length = 200,
    volume = function() return media:get().volume end,
    on_play_pause = function(on) note("play:" .. tostring(on)) media:set { playing = on, volume = media:get().volume } end,
    on_previous = function() note("prev") end, on_next = function() note("next") end,
    on_seek = function(s) note("seek:" .. math.floor(s + 0.5)) end,
    on_volume = function(v) note("volume:" .. ("%.1f"):format(v)) end }
  local root = ui.Item { width = 1500, height = 900 }
  for _, node in pairs(nodes) do ui.reparent(node, root) end

  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.open = function(name) return handles[name].is_open() end
  morf.ipc.focus = function(name)
    local node = name == "fruit" and handles.search.input or name == "tags" and handles.tags.input or nodes[name]
    morf.focus.set(node, true)
  end
  morf.ipc.revealed = function() return handles.bar.revealed() end
  morf.ipc.tags = function() return table.concat(handles.tags.list(), "|") end
  morf.ipc.fruit_text = function() return handles.search.input.text end
  morf.ipc.fruit_shown = function() return table.concat(handles.search.filtered(), ",") end
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
  test.it(style .. ": a combo box opens and picks by the pointer and by the keys", function()
    load(style)
    test.click("combo") test.settle(60)
    test.truthy(test.ipc("open", "combo"), "a press did not open it")
    local field, item = test.get("combo"), test.get("combo-item-1")
    test.truthy(item.y >= field.y + field.height, "the list is not under its field")
    test.click("combo-item-3") test.settle(60)
    test.eq(test.ipc("log"), "combo:3:Large")
    test.falsy(test.ipc("open", "combo"))
    test.truthy(test.find { text = "Large", visible = true })
    -- The keys: Space opens on the current item, Up moves, Return picks.
    test.ipc("focus", "combo") test.settle(30)
    test.key("space") test.settle(60)
    test.truthy(test.ipc("open", "combo"))
    test.eq(focused(), "combo-list")
    test.key("Up") test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "combo:2:Medium")
    test.falsy(test.ipc("open", "combo"))
    test.eq(focused(), "combo")
    -- Alt+Down opens; Escape closes and leaves the current item.
    test.key("Down", "alt") test.settle(60)
    test.truthy(test.ipc("open", "combo"))
    test.key("Down") test.key("Escape") test.settle(60)
    test.falsy(test.ipc("open", "combo"))
    test.eq(test.ipc("log"), "")
    test.eq(focused(), "combo")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a searchable combo filters as it is typed into", function()
    load(style)
    test.click("fruit") test.settle(30) test.advance(20)
    test.type("ch") test.settle(60)
    test.truthy(test.ipc("open", "search"), "typing did not open the list")
    test.eq(test.ipc("fruit_shown"), "3")
    test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "fruit:3:Cherry")
    test.eq(test.ipc("fruit_text"), "Cherry")
    test.falsy(test.ipc("open", "search"))
    -- Down opens the whole list on the current item; Down again, Return.
    test.key("Down") test.settle(60)
    test.truthy(test.ipc("open", "search"))
    test.key("Down") test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "fruit:4:Date")
    -- A press on a row picks it; Escape closes and puts the text back.
    test.key("Down") test.settle(60)
    local x, y = centre("fruit-list")
    local list = test.get("fruit-list")
    test.click(x, list.y + 36 * 0.5) test.settle(60)
    test.eq(test.ipc("log"), "fruit:1:Apple")
    test.type("zz") test.settle(30)
    test.key("Escape") test.settle(60)
    test.falsy(test.ipc("open", "search"))
    test.eq(test.ipc("fruit_text"), "Apple")
    test.eq(focused(), "fruit")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a menu button and a split button", function()
    load(style)
    test.click("file-button") test.settle(60)
    test.truthy(test.ipc("open", "menu"))
    test.click("file-item-2") test.settle(60)
    test.eq(test.ipc("log"), "open")
    test.falsy(test.ipc("open", "menu"))
    test.ipc("focus", "menu") test.settle(30)
    test.key("Return") test.settle(60)
    test.truthy(test.ipc("open", "menu"))
    test.eq(focused(), "file-item-1")
    test.key("Escape") test.settle(60)
    test.falsy(test.ipc("open", "menu"))
    test.eq(focused(), "file-button")
    test.key("Down", "alt") test.settle(60)
    test.truthy(test.ipc("open", "menu"))
    test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "new")
    -- The split button: its face runs the action, its arrow opens the menu.
    test.click("save") test.settle(30)
    test.eq(test.ipc("log"), "save")
    test.falsy(test.ipc("open", "split"))
    test.click("save-arrow") test.settle(60)
    test.truthy(test.ipc("open", "split"))
    test.click("save-item-1") test.settle(60)
    test.eq(test.ipc("log"), "save-as")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a picker chooses from its grid", function()
    load(style)
    test.click("icon") test.settle(60)
    test.truthy(test.ipc("open", "picker"))
    test.click("icon-item-3") test.settle(60)
    test.eq(test.ipc("log"), "icon:3:Bolt")
    test.falsy(test.ipc("open", "picker"))
    test.ipc("focus", "picker") test.settle(30)
    test.key("Return") test.settle(60)
    test.truthy(test.ipc("open", "picker"))
    -- From the third: back two, down a row of four, Return.
    test.key("Left") test.key("Left") test.key("Down") test.key("Return") test.settle(60)
    test.eq(test.ipc("log"), "icon:5:Pets")
    test.eq(focused(), "icon")
    test.key("Return") test.settle(60)
    test.key("Escape") test.settle(60)
    test.falsy(test.ipc("open", "picker"))
    test.eq(focused(), "icon")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a search bar comes with Ctrl+F and goes with Escape", function()
    load(style)
    test.ipc("focus", "combo") test.settle(30)
    test.falsy(test.ipc("revealed"))
    test.falsy(test.get("find").visible, "the folded field is still shown")
    test.key("f", "ctrl") test.settle(300)
    test.truthy(test.ipc("revealed"))
    test.eq(focused(), "find")
    test.type("doc") test.settle(30)
    test.eq(test.ipc("log"), "revealed:true,search:d,search:do,search:doc")
    test.key("Escape") test.settle(300)
    test.falsy(test.ipc("revealed"))
    test.eq(test.ipc("log"), "search:,revealed:false")
    test.eq(focused(), "combo")
    -- Its toggle too.
    test.click("find-toggle") test.settle(300)
    test.truthy(test.ipc("revealed"))
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": a tag input adds and removes chips", function()
    load(style)
    test.click("tags") test.settle(30)
    test.type("rust") test.key("Return") test.settle(30)
    test.eq(test.ipc("tags"), "lua|rust")
    test.type("ui,morf,") test.settle(30)
    test.eq(test.ipc("tags"), "lua|rust|ui|morf")
    test.key("BackSpace") test.settle(30)
    test.eq(test.ipc("tags"), "lua|rust|ui")
    test.click("tags-tag-1") test.settle(30)
    test.eq(test.ipc("tags"), "rust|ui")
    test.truthy(test.find { id = "tags-tag-2", visible = true })
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": an input group's button and field", function()
    load(style)
    test.click("url-go") test.settle(30)
    test.eq(test.ipc("log"), "go")
    test.click("url") test.settle(30)
    test.type("morf.dev") test.key("Return") test.settle(30)
    test.eq(test.ipc("log"), "url:morf.dev")
  end)

  test.it(style .. ": preference rows", function()
    load(style)
    test.click("dark") test.settle(30)
    test.click("dark-switch") test.settle(30)
    test.click("blur") test.settle(30)
    test.eq(test.ipc("log"), "dark:true,dark:false,blur:true")
    test.click("scale-combo") test.settle(60)
    test.click("scale-combo-item-2") test.settle(60)
    test.eq(test.ipc("log"), "scale:2")
    test.click("size-up") test.settle(30)
    test.click("size-down") test.click("size-down") test.settle(30)
    test.eq(test.ipc("log"), "size:12,size:11,size:10")
    test.click("size-value") test.settle(20)
    test.key("Up") test.settle(20)
    test.eq(test.ipc("log"), "size:11")
    test.click("about") test.settle(30)
    test.eq(test.ipc("log"), "about")
    test.click("name-entry") test.settle(20)
    test.type("Ada") test.key("Return") test.settle(20)
    test.eq(test.ipc("log"), "name:Ada")
    test.eq(#test.logs("error"), 0)
  end)

  test.it(style .. ": media controls", function()
    load(style)
    test.click("player-play") test.settle(30)
    test.click("player-next") test.click("player-previous") test.settle(30)
    test.eq(test.ipc("log"), "play:true,next,prev")
    local seek = test.get("player-seek")
    test.click(seek.x + seek.width * 0.5, seek.y + seek.height / 2) test.settle(30)
    test.eq(test.ipc("log"), "seek:100")
    local volume = test.get("player-volume")
    test.click(volume.x + volume.width - 1, volume.y + volume.height / 2) test.settle(30)
    test.matches(test.ipc("log"), "^volume:1%.0")
    test.eq(#test.logs("error"), 0)
  end)
end
