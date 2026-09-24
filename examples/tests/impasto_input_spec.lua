-- impasto driven by input the way a person drives it: clicks, drags, the
-- wheel and keys go through the shell's own pointer and keyboard paths.
-- The scenarios first driven with a real pointer and keyboard in a nested
-- compositor, kept here as regressions.
--
--     morf test --private-bus examples/tests/impasto_input_spec.lua

local test = morf.test

local function load(env)
  local vars = { IMPASTO_DRY_RUN = "1", IMPASTO_INLINE_WALLPAPER = "1" }
  for name, value in pairs(env or {}) do vars[name] = value end
  test.load("../impasto/init.lua", { size = { 1280, 720 }, env = vars })
  test.settle(3000)
end

test.describe("impasto input", function()
  test.it("a click beside an open island closes it", function()
    load()
    test.eq(test.ipc("wifi"), "wifi")
    test.settle(2000)
    test.click(1100, 400)
    test.settle(2000)
    test.ne(test.ipc("wifi"), "", "the island stayed open")
  end)

  test.it("the launcher closes on Escape after the actions mode was typed and erased", function()
    load()
    test.eq(test.ipc("launcher"), "launcher")
    test.settle(2000)
    test.type(">")
    test.settle(500)
    test.key("BackSpace")
    test.settle(500)
    test.key("Escape")
    test.settle(2000)
    test.eq(test.ipc("launcher"), "launcher", "the launcher stayed open")
  end)

  test.it("arranging the control centre: a block dragged onto the tray is removed", function()
    load()
    test.eq(test.ipc("controls"), "controls")
    test.settle(2000)
    test.eq(test.ipc("controls_edit"), "editing")
    test.settle(2000)
    local airplane = test.get { text = "Airplane", visible = true }
    local tray = test.get { text_contains = "drop here to remove", visible = true }
    test.drag({ airplane.x + 10, airplane.y + 5 }, { tray.x + 40, tray.y - 300 }, { steps = 20 })
    test.settle(2000)
    test.falsy(test.find(function(n) return n.text == "Airplane" and n.visible and n.x < tray.x end), "the toggles block is still on the grid")
  end)

  test.it("the arcade: Escape leaves a game for the shelf, and the shelf for nothing", function()
    load()
    test.eq(test.ipc("games"), "games")
    test.settle(2000)
    for _ = 1, 8 do test.key("Right") test.settle(100) end
    test.key("Return")
    test.settle(1500)
    test.key("space")
    test.settle(300)
    test.key("Escape")
    test.settle(1000)
    test.key("Escape")
    test.settle(1500)
    test.eq(test.ipc("games"), "games", "the arcade stayed open")
  end)

  test.it("keyboard layouts are picked from the xkb list, searched, and never all let go", function()
    load()
    test.ipc("settings", "input")
    test.settle(2000)
    test.eq(test.ipc("get", "keyboardLayouts"), '"us"')
    test.click(test.get { text = "Edit", visible = true })
    test.settle(500)
    test.type("germ")
    test.settle(500)
    local german = test.get { id = "layout-row-de", visible = true }
    test.falsy(test.find { id = "layout-row-fr", visible = true }, "the search leaves French out")
    test.click(german)
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"us,de"')
    -- A chip lets its layout go; the last one stays.
    test.click(test.get { id = "layout-chip-us", visible = true })
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"de"')
    test.click(test.get { id = "layout-chip-de", visible = true })
    test.settle(500)
    test.eq(test.ipc("get", "keyboardLayouts"), '"de"')
  end)
end)

-- The desk's squares as `{ key, family, col, row }`, from `desk list`.
local function squares()
  local out = {}
  for line in (test.ipc("desk", "list") .. "\n"):gmatch("([^\n]*)\n") do
    local key, cols, rows, col, row = line:match("^(%S+) (%d+)x(%d+) @(%d+),(%d+)$")
    if key then
      out[#out + 1] = { key = key, cols = tonumber(cols), rows = tonumber(rows), col = tonumber(col), row = tonumber(row) }
    end
  end
  return out
end

local function overlapping(list)
  for i = 1, #list do
    for j = i + 1, #list do
      local a, b = list[i], list[j]
      if a.col < b.col + b.cols and b.col < a.col + a.cols and a.row < b.row + b.rows and b.row < a.row + a.rows then
        return a.key .. " over " .. b.key
      end
    end
  end
  return nil
end

test.describe("impasto desk", function()
  test.it("a fresh desk made for a larger screen fits 1280x720 without overlapping", function()
    load()
    -- A fresh install takes Moon castle: eight widgets laid out for 1080p.
    test.eq(test.ipc("profile"), "Moon castle")
    local list = squares()
    test.eq(#list, 8, "every widget is on the board")
    test.falsy(overlapping(list))
    test.eq(test.ipc("desk", "grid"), "10x5")
    -- The layout itself is kept for a larger screen: the clock asked for
    -- 4x4 and is drawn smaller only here.
    test.contains(test.ipc("get", "desktopWidgets"), '"family":"4x4","id":"clock"')
    -- With room again it is its own size.
    test.ipc("desk", "remove", "weather-0-4")
    test.ipc("desk", "remove", "battery-2-6")
    test.settle(500)
    local clock
    for _, s in ipairs(squares()) do if s.key == "clock-0-0" then clock = s end end
    test.eq(clock.cols .. "x" .. clock.rows, "4x4")
    test.falsy(overlapping(squares()))
  end)

  test.it("the arranging card opens one row tall on a short screen, where it covers the fewest widgets", function()
    load()
    test.ipc("desk", "remove", "weather-0-4")
    test.ipc("desk", "remove", "battery-2-6")
    test.settle(500)
    test.ipc("desk", "edit")
    test.settle(1500)
    local card = test.get { id = "desk-tray", visible = true }
    test.truthy(card.height < 260, "the card is " .. card.height .. " tall")
    -- The bottom middle, its home, would cover the calendar, the timer
    -- and the record player; where it opens covers less.
    local widgets, lowest = {}, 0
    for _, s in ipairs(squares()) do
      local node = test.get(function(n) return n.id == "desk-widget-" .. s.key and n.visible end)
      widgets[#widgets + 1] = node
      lowest = math.max(lowest, node.y + node.height)
    end
    local function covered(x, y)
      local total = 0
      for _, node in ipairs(widgets) do
        local ox = math.min(x + card.width, node.x + node.width) - math.max(x, node.x)
        local oy = math.min(y + card.height, node.y + node.height) - math.max(y, node.y)
        if ox > 0 and oy > 0 then total = total + ox * oy end
      end
      return total
    end
    local at_home = covered((1280 - card.width) / 2, lowest - card.height)
    test.truthy(covered(card.x, card.y) < at_home,
      "the card covers " .. covered(card.x, card.y) .. " square pixels of widgets, its home " .. at_home)
  end)

  test.it("Escape closes the desk's menu", function()
    load()
    -- Between the clock and the weather: the wallpaper.
    test.click(250, 385, { button = "right" })
    test.settle(500)
    test.truthy(test.find { text = "Arrange widgets", visible = true }, "the menu opened")
    test.key("Escape")
    test.settle(500)
    test.falsy(test.find { text = "Arrange widgets", visible = true }, "the menu stayed open")
  end)

  test.it("a right click beside an open island closes it and opens no menu", function()
    load()
    test.eq(test.ipc("wifi"), "wifi")
    test.settle(2000)
    test.click(1150, 450, { button = "right" })
    test.settle(2000)
    test.ne(test.ipc("layer"), "panel", "the island stayed open")
    test.falsy(test.find { text = "Arrange widgets", visible = true }, "the desk's menu opened")
  end)
end)

test.describe("impasto control centre", function()
  test.it("arranging with the bar spread takes the whole bar, so the card is not cut off", function()
    load()
    test.ipc("set", "barStyle", '"spread"')
    test.settle(500)
    test.eq(test.ipc("controls"), "controls")
    test.settle(2000)
    test.eq(test.ipc("controls_edit"), "editing")
    test.settle(2000)
    local island = test.get { id = "island", visible = true }
    local tray = test.get { id = "controls-tray", visible = true }
    test.truthy(tray.x + tray.width <= island.x + island.width,
      "the card ends at " .. (tray.x + tray.width) .. ", the island at " .. (island.x + island.width))
    test.truthy(island.x >= 0 and island.x + island.width <= 1280, "the island is on the screen")
  end)
end)

test.describe("impasto bar", function()
  test.it("a chip's long figure never draws past its capsule while the capsule grows", function()
    load()
    test.ipc("set", "barRight", '["network", "bluetooth"]')
    test.settle(500)
    test.eq(test.ipc("bluetooth_demo", "Andreu's WH-1000XM4 Headphones"), "Andreu's WH-1000XM4 Headphones")
    test.advance(32)
    local text = test.get { text = "Andreu's WH-1000XM4 Headphones" }
    -- Up its ancestors: the capsule is the first box as tall as the bar's
    -- capsules that holds more than the chip, and it clips.
    local clipped_by = {}
    local handle = text.parent
    while handle do
      local node = test.find(function(n) return n.handle == handle end)
      if not node then break end
      if node.element == "ClipRect" then
        clipped_by[#clipped_by + 1] = node
      end
      handle = node.parent
    end
    local capsule = clipped_by[#clipped_by]
    test.truthy(capsule, "nothing clips the figure")
    local right = 0
    for _, node in ipairs(clipped_by) do right = math.max(right, node.x + node.width) end
    -- Every clip the figure is under ends inside the outermost, the capsule.
    test.truthy(right <= capsule.x + capsule.width + 0.5)
    -- And the outermost is the capsule, which holds the network chip too.
    test.truthy(capsule.height <= 40 and capsule.x < text.x - 40, "the outermost clip is the capsule")
    test.settle(1500)
    test.ipc("bluetooth_demo", "")
  end)
end)
