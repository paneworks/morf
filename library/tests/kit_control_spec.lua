-- lib.kit.control and lib.kit.skin over morf.kit.native: a bare Control
-- drawn by a skin, slot inheritance between themes, lazy defaults, and a
-- theme switch that rebuilds in place with no node left behind.
--
--     morf test library/tests/kit_control_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local control = require("lib.kit.control")
  local native = require("morf.kit.native")
  local built = {}
  skin.define("base", {
    skins = { Control = function(t)
      built[#built + 1] = "base"
      return {
        background = ui.Rect { id = "bg-base", anchors = { fill = true },
          color = function() return t.hovered and "#ff0000" or "#202020" end },
        content = ui.Text { id = "content-base", text = "base" },
      }
    end },
    defaults = { Control = { content = function() return ui.Text { id = "content-default", text = "default" } end } },
  })
  -- Replaces only the background; the content comes from base.
  skin.define("square", { skins = { Control = {
    background = function(t) built[#built + 1] = "square"
      return ui.Rect { id = "bg-square", anchors = { fill = true }, color = "#00ff00" } end,
  } } }, { extends = "base" })
  -- Fills nothing: every slot from base, its content from base's defaults
  -- for a widget base has no skin for.
  skin.define("bare", {}, { extends = "base" })
  skin.use("base")
  local clicks = 0
  local root = ui.Item { width = 400, height = 200 }
  local node = control.make("Control", "Control", { id = "the-control", x = 10, y = 10, padding = 8,
    on_clicked = function() clicks = clicks + 1 end })
  ui.reparent(node, root)
  local other = control.make("Control", "Card", { id = "the-card", x = 200, y = 10, width = 120, height = 60 })
  ui.reparent(other, root)
  morf.ipc.use = function(name) skin.use(name) end
  morf.ipc.clicks = function() return clicks end
  morf.ipc.built = function() return table.concat(built, ",") end
  morf.ipc.controls = function() return native.count() end
  morf.ipc.destroy = function() ui.destroy(other, true) end
]]

local function load() test.load { source = HOST } test.settle(200) end

local function count()
  local n = 0
  for _ in ipairs(test.nodes()) do n = n + 1 end
  return n
end

test.it("draws a bare Control with its skin and sizes it from its slots", function()
  load()
  test.truthy(test.get("bg-base").visible)
  local control, content = test.get("the-control"), test.get("content-base")
  -- Implicit size: the content with 8 px of padding all round.
  test.near(control.width, content.width + 16, 1)
  test.near(control.height, content.height + 16, 1)
  test.click("the-control") test.settle(50)
  test.eq(test.ipc("clicks"), 1)
end)

test.it("takes a slot from the theme it extends and rebuilds on a switch", function()
  load()
  local before = count()
  test.ipc("use", "square") test.settle(50)
  test.truthy(test.get("bg-square").visible)
  test.truthy(test.get("content-base").visible, "the content was not inherited")
  test.falsy(test.find { id = "bg-base" }, "the replaced background was left behind")
  test.eq(count(), before, "a theme switch leaked or lost nodes")
  test.ipc("use", "bare") test.settle(50)
  test.truthy(test.get("bg-base").visible)
  test.ipc("use", "base") test.settle(50)
  test.eq(count(), before)
end)

test.it("lets go of a control's behaviour when its node goes", function()
  load()
  test.eq(test.ipc("controls"), 2)
  test.ipc("destroy") test.settle(50)
  test.eq(test.ipc("controls"), 1)
  test.eq(#test.logs("error"), 0)
end)
