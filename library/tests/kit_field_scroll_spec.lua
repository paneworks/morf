-- lib.kit.text_field and lib.kit.scroll on the TextField and Scroll
-- archetypes, with a plain skin: validation, Return only when acceptable,
-- revert on Escape, a clear button, and a page the keys scroll with a
-- scroll bar that is a kit Range.
--
--     morf test library/tests/kit_field_scroll_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  morf.surface.height = 400
  skin.define("plain", { skins = {
    Press = function() return { background = ui.Rect { anchors = { fill = true }, color = "#444444" } } end,
    TextField = function(t, spec, _, send)
      return { error = ui.Rect { id = (spec.id or "f") .. "-error", anchors = { left = true, right = true, bottom = true },
          height = 2, color = "#ff0000", visible = function() return not t.acceptable end },
        trailing = spec.clear and w.push { id = (spec.id or "f") .. "-clear", width = 20, height = 20,
          anchors = { right = true }, on_clicked = function() send("clear") end } or nil }
    end,
    Range = function(t, spec) return { track = ui.Item { anchors = { fill = true } },
      handle = ui.Rect { id = "bar-handle", width = 6, height = 20, color = "#ffffff",
        y = function() return t.visual_position * (t.height - 20) end } } end,
    Scroll = function(t, spec)
      local flick = spec.flick
      return { scroll_bar_y = w.scroll_bar { id = "bar", width = 8, orientation = "vertical", inverted = true,
        anchors = { right = true, top = true, bottom = true }, visible = function() return t.bar_y end,
        value = function() return t.position_y end,
        on_moved = function(v) flick.content_y = v * math.max(0, t.content_height - t.viewport_height) end } }
    end,
  } })
  skin.use("plain")
  local log = {}
  local rows = {}
  for i = 1, 40 do rows[i] = ui.Text { text = "Row " .. i, height = 20 } end
  local port_node, port = w.numeric_entry { id = "port", x = 10, y = 10, width = 200, height = 30,
    minimum = 1, maximum = 1024, revert_on_escape = true, text = "80", clear = true,
    on_accepted = function(text) log[#log + 1] = "accepted:" .. text end,
    on_invalid = function(text) log[#log + 1] = "invalid:" .. text end }
  local page_node, page = w.scroll_view { id = "page", x = 10, y = 60, width = 300, height = 200, clip = true, focus_policy = "tab",
    ui.Column(rows) }
  ui.Item { width = 600, height = 400, port_node, page_node }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.text = function() return port.text end
  morf.ipc.scrolled = function() return page.content_y end
  morf.ipc.focus_page = function() morf.focus.set(page_node, true) end
]]

local function load() test.load { source = HOST, size = { 600, 400 } } test.settle(200) end

test.it("a numeric entry accepts only a number in range and reverts on Escape", function()
  load()
  test.click("port") test.settle(20)
  test.key("End") test.type("80") test.settle(20)
  test.key("Return") test.settle(20)
  test.eq(test.ipc("log"), "invalid:8080")
  test.truthy(test.get("port-field-error").visible)
  test.key("BackSpace") test.key("BackSpace") test.key("BackSpace") test.settle(20)
  test.key("Return") test.settle(20)
  test.eq(test.ipc("log"), "accepted:8")
  test.type("1") test.settle(20)
  test.key("Escape") test.settle(20)
  test.eq(test.ipc("text"), "8")
end)

test.it("a clear button empties it", function()
  load()
  test.click("port-field-clear") test.settle(20)
  test.eq(test.ipc("text"), "")
end)

test.it("a page scrolls by keys and its scroll bar follows and drags", function()
  load()
  test.ipc("focus_page") test.settle(20)
  test.key("Page_Down") test.settle(50)
  test.truthy(test.ipc("scrolled") > 100)
  test.key("End") test.settle(50)
  test.near(test.ipc("scrolled"), 600, 1)
  local handle = test.get("bar-handle")
  test.near(handle.y + handle.height, 60 + 200, 2)
  local bar = test.get("bar")
  test.drag({ bar.x + 4, bar.y + bar.height - 10 }, { bar.x + 4, bar.y + 10 }, { steps = 4 })
  test.settle(50)
  test.truthy(test.ipc("scrolled") < 100, tostring(test.ipc("scrolled")))
end)
