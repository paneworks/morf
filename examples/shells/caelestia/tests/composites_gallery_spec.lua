-- The composites gallery: every composite the contract requires
-- (library/lib/kit/contract.lua, `composites`) drawn in each theme in a
-- cell of its own. A composite passes when it builds, nothing it draws
-- spills out of its cell, and nothing logs an error. How each one works
-- is tried in the composites_<group>_spec.lua beside this.
--
--     morf test --no-dbus examples/shells/caelestia/tests/composites_gallery_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local contract = require("lib.kit.contract")
  local composites = require("lib.kit.composites")
  local CELL_W, CELL_H, COLUMNS = 560, 380, 3
  morf.surface.height = 2000
  local SAMPLES = {}
  for _, group in ipairs(composites.groups()) do
    for name, make in pairs(require("lib.kit.composites.samples_" .. group)) do SAMPLES[name] = make end
  end
  local names = {}
  for name, entry in pairs(contract.composites) do
    if entry.stage <= contract.stage then names[#names + 1] = name end
  end
  table.sort(names)
  local root = ui.Item { width = COLUMNS * CELL_W, height = math.ceil(#names / COLUMNS) * CELL_H }
  local missing = {}
  for i, name in ipairs(names) do
    local col, row = (i - 1) % COLUMNS, math.floor((i - 1) / COLUMNS)
    local cell = ui.Item { id = "gallery-cell-" .. name, x = col * CELL_W, y = row * CELL_H, width = CELL_W, height = CELL_H }
    if SAMPLES[name] then
      local node = SAMPLES[name](kit, composites)
      ui.reparent(ui.Item { id = "gallery-" .. name, x = 20, y = 20, width = CELL_W - 40, height = CELL_H - 40, node }, cell)
    else
      missing[#missing + 1] = name
    end
    ui.reparent(cell, root)
  end
  morf.ipc.missing = function() return table.concat(missing, " ") end
]]

local function spills()
  local by_handle = {}
  for _, node in ipairs(test.nodes()) do by_handle[node.handle] = node end
  local function owner(node)
    local current = node
    while current do
      if current.id and current.id:match("^gallery%-cell%-") then return current end
      current = current.parent and by_handle[current.parent]
    end
  end
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
          out[#out + 1] = ("%s: %s %s"):format(cell.id:sub(14), node.element, node.id or "")
        end
      end
    end
  end
  return out
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " draws every composite inside its cell", function()
    test.load("../shell/init.lua", { size = { 1920, 1080 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
    test.settle(1500)
    test.eq(test.ipc("missing"), "", "composites with no gallery sample")
    local problems = spills()
    test.eq(#problems, 0, table.concat(problems, "\n"))
    test.eq(#test.logs("error"), 0, "errors were logged")
  end)
end
