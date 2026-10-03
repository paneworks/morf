-- The default kit's widget gallery: every widget of every archetype the
-- contract requires, drawn by the default kit (library/lib/kit/skins/
-- default) from its sample (library/lib/kit/samples/<archetype>.lua), in
-- each of its three variants. KIT_WIDGETS="Range Plane" draws only those.
--
--     morf test --no-dbus library/tests/default_widgets_gallery_spec.lua
--     KIT_WIDGETS=Canvas KIT_GALLERY_SNAPSHOTS=1 nixVulkanIntel morf test --no-dbus --snapshots DIR \
--       library/tests/default_widgets_gallery_spec.lua

local test = morf.test
local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end
local SOURCE = [[
  local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end
  local kit = require("lib.kit.skins.default").make { variant = env("MORF_KIT_VARIANT") or "light" }
  package.loaded.kit = kit
]] .. require("lib.kit.samples").SOURCE

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

for _, variant in ipairs { "light", "dark", "high_contrast" } do
  test.it(("the default kit (%s) draws every archetype widget inside its cell"):format(variant), function()
    test.load { source = SOURCE, size = { 1920, 1080 },
      env = { MORF_KIT_VARIANT = variant, KIT_WIDGETS = env("KIT_WIDGETS") or "" } }
    test.settle(1500)
    test.eq(test.ipc("missing"), "", "widgets with no gallery sample")
    local problems = spills()
    test.eq(#problems, 0, table.concat(problems, "\n"))
    test.eq(#test.logs("error"), 0, "errors were logged")
    if env("KIT_GALLERY_SNAPSHOTS") then test.snapshot("widgets-default-" .. variant .. ".png") end
  end)
end
