-- The widget gallery: every widget of every archetype the contract
-- requires, drawn by each caelestia theme from its sample
-- (library/lib/kit/samples/<archetype>.lua) in a cell of its own, at three
-- screen sizes. A widget passes when every visible node it draws stays
-- inside its cell and nothing logs an error. KIT_WIDGETS="Range Plane"
-- draws only those archetypes.
--
--     morf test --no-dbus examples/shells/caelestia/tests/kit_widgets_gallery_spec.lua
--     KIT_WIDGETS=Canvas KIT_GALLERY_SNAPSHOTS=1 nixVulkanIntel morf test --no-dbus --snapshots DIR \
--       examples/shells/caelestia/tests/kit_widgets_gallery_spec.lua

local test = morf.test
local SOURCE = require("lib.kit.samples").SOURCE
local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end

local function load(style, size)
  test.load("../shell/init.lua", { size = size, env = { CAELESTIA_STYLE = style, KIT_WIDGETS = env("KIT_WIDGETS") or "" },
    source = SOURCE })
  test.settle(2000)
end

--- Visible nodes inside a gallery cell that reach outside it.
local function spills()
  local by_handle = {}
  for _, node in ipairs(test.nodes()) do by_handle[node.handle] = node end
  local cell_of = {}
  local function owner(node)
    local current = node
    while current do
      if current.id and current.id:match("^gallery%-cell%-") then return current end
      current = current.parent and by_handle[current.parent]
    end
  end
  -- Only what draws counts: an Item that is rotated to carry a needle
  -- reports the box of its turned square, though nothing is drawn there.
  local DRAWS = { Rect = true, Path = true, Text = true, Image = true, Icon = true, Sdf = true }
  local out = {}
  for _, node in ipairs(test.nodes()) do
    if DRAWS[node.element] and node.visible and node.width > 0 and node.height > 0 then
      local cell = owner(node)
      if cell and node ~= cell then
        local slack = 3
        -- What shows of it: its box cut by every clipping ancestor; a
        -- turned drawing (or one under a turn) by where its centre is,
        -- since its box is the bounds of its turned square.
        local x0, y0, x1, y1 = node.x, node.y, node.x + node.width, node.y + node.height
        local turned = (node.rotation or 0) % 360 ~= 0
        local up = node.parent and by_handle[node.parent]
        while up and up ~= cell do
          if (up.rotation or 0) % 360 ~= 0 then turned = true end
          if up.clip then
            x0, y0 = math.max(x0, up.x), math.max(y0, up.y)
            x1, y1 = math.min(x1, up.x + up.width), math.min(y1, up.y + up.height)
          end
          up = up.parent and by_handle[up.parent]
        end
        if turned then
          local cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
          x0, y0, x1, y1 = cx, cy, cx, cy
        end
        if x1 > x0 + 0.5 and y1 > y0 + 0.5 or turned then
          node = { element = node.element, id = node.id, x = x0, y = y0, width = x1 - x0, height = y1 - y0 }
        else
          node = nil
        end
        if node and (node.x < cell.x - slack or node.y < cell.y - slack
          or node.x + node.width > cell.x + cell.width + slack
          or node.y + node.height > cell.y + cell.height + slack) then
          out[#out + 1] = ("%s: %s %s %.0fx%.0f at %.0f,%.0f"):format(cell.id:sub(14), node.element, node.id or "",
            node.width, node.height, node.x - cell.x, node.y - cell.y)
        end
      end
    end
  end
  return out
end

for _, style in ipairs { "material", "tsugumori" } do
  for _, size in ipairs { { 1920, 1080 }, { 1280, 800 }, { 3840, 2160 } } do
    test.it(("%s draws every archetype widget inside its cell at %dx%d"):format(style, size[1], size[2]), function()
      load(style, size)
      test.eq(test.ipc("missing"), "", "widgets with no gallery sample")
      local problems = spills()
      test.eq(#problems, 0, table.concat(problems, "\n"))
      test.eq(#test.logs("error"), 0, "errors were logged")
      if env("KIT_GALLERY_SNAPSHOTS") then test.snapshot(("widgets-%s-%dx%d.png"):format(style, size[1], size[2])) end
    end)
  end
end
