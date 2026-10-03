-- The kit gallery: every display widget the contract requires
-- (library/lib/kit/contract.lua) drawn by each theme in a cell of its
-- own, at three screen sizes. A widget passes when every visible node it
-- draws stays inside its cell -- nothing overlaps a neighbour or spills
-- -- and nothing logs an error.
--
--     morf test --no-dbus examples/shells/caelestia/tests/kit_gallery_spec.lua

local test = morf.test

local CELL_W, CELL_H, COLUMNS = 300, 230, 4

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local contract = require("lib.kit.contract")
  local CELL_W, CELL_H, COLUMNS = 300, 230, 4
  local v = morf.signal("gallery.value", 0.62)
  local function value() return v:get() end
  local function series() local t = {} for i = 1, 40 do t[i] = 30 + 25 * math.sin(i / 4) end return t end
  local function bands() local t = {} for i = 1, 32 do t[i] = .5 + .4 * math.sin(i / 3) end return t end
  local signal = kit.signal("accent")
  -- What each display function is drawn with.
  local SAMPLES = {
    label = function() return kit.label { text = "Label" } end,
    heading = function() return kit.heading { text = "Heading", width = 260 } end,
    subtitle = function() return kit.subtitle { text = "Subtitle text", width = 260 } end,
    caption = function() return kit.caption { width = 260, text = "Caption", note = "note" } end,
    readout = function() return kit.readout { value = function() return "42" end, unit = "%" } end,
    facts = function() return kit.facts({ { "Speed", "4.2 GHz" }, { "Cores", "16" }, { "Cache", "24 MiB" } }, 260) end,
    text = function() return kit.text { text = "Body text", width = 260 } end,
    keycap = function() return kit.keycap { text = "Esc" } end,
    icon = function() return kit.icon("home", 24, signal) end,
    emblem = function() return kit.emblem { kind = "warn", size = 48 } end,
    status_line = function() return kit.status_line { kind = "alert", title = "Warning", subtitle = "Disk full", width = 260 } end,
    status = function() return kit.status { width = 260, kind = function() return "ok" end,
      title = function() return "Normal" end, subtitle = function() return "Running smoothly" end } end,
    chip = function() return kit.chip { text = "Online" } end,
    loading = function() return kit.loading(32, signal, {}) end,
    gauge = function() return kit.gauge { size = 90, value = value } end,
    ring = function() return kit.ring { size = 180, value = value, label = "Load" } end,
    mini_ring = function() return kit.mini_ring { size = 80, value = value, text = function() return "62" end, label = "Mem" } end,
    bar = function() return kit.bar { width = 260, stroke = 6, value = value } end,
    meter = function() return kit.meter { width = 260, height = 8, value = value } end,
    fill = function() return kit.fill { width = 260, height = 14, value = value } end,
    vmeter = function() return kit.vmeter { width = 14, height = 150, value = value } end,
    dial = function() return kit.dial { size = 150, value = value } end,
    radar = function() return kit.radar { size = 170, values = function() return { .6, .4, .8, .3, .5, .7 } end } end,
    cell = function() return kit.cell { width = 70, height = 50, value = 37 } end,
    stat = function() return kit.stat { width = 140, height = 58, label = "Speed", value = function() return "4.2 GHz" end, level = value } end,
    triplet = function() return kit.triplet { width = 260, series = series, top = function() return 100 end,
      format = function(x) return ("%d%%"):format(math.floor(x)) end } end,
    chart = function() return (kit.chart { width = 260, height = 150, first = series, samples = 60 }) end,
    spectrum = function() return kit.spectrum { width = 260, height = 100, values = bands } end,
    card = function() return kit.card { width = 260, height = 150 } end,
    panel = function() return kit.panel { width = 260, height = 150, title = "Panel" } end,
    header = function() return kit.header { width = 260, title = "Header", status = "Live" } end,
    surface = function() return kit.surface { width = 260, height = 120 } end,
  }
  -- One cell per display function due at the contract's stage.
  local names, seen = {}, {}
  for _, list in pairs(contract.display) do
    for _, entry in ipairs(list) do
      if entry.stage <= contract.stage and not seen[entry.fn] then
        seen[entry.fn] = true
        names[#names + 1] = entry.fn
      end
    end
  end
  table.sort(names)
  local root = ui.Item { width = COLUMNS * CELL_W, height = math.ceil(#names / COLUMNS) * CELL_H }
  local missing = {}
  for i, name in ipairs(names) do
    local col, row = (i - 1) % COLUMNS, math.floor((i - 1) / COLUMNS)
    local cell = ui.Item { id = "gallery-cell-" .. name, x = col * CELL_W, y = row * CELL_H, width = CELL_W, height = CELL_H }
    local make = SAMPLES[name]
    if make then
      local node = make()
      ui.reparent(ui.Item { id = "gallery-" .. name, x = 20, y = 20, width = CELL_W - 40, height = CELL_H - 40, node }, cell)
    else
      missing[#missing + 1] = name
    end
    ui.reparent(cell, root)
  end
  morf.ipc.names = function() return table.concat(names, " ") end
  morf.ipc.missing = function() return table.concat(missing, " ") end
]]

local function load(style, size)
  test.load("../shell/init.lua", { size = size, env = { CAELESTIA_STYLE = style }, source = SOURCE })
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
        if node.x < cell.x - slack or node.y < cell.y - slack
          or node.x + node.width > cell.x + cell.width + slack
          or node.y + node.height > cell.y + cell.height + slack then
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
    test.it(("%s draws every display widget inside its cell at %dx%d"):format(style, size[1], size[2]), function()
      load(style, size)
      test.eq(test.ipc("missing"), "", "display functions with no gallery sample")
      local problems = spills()
      test.eq(#problems, 0, table.concat(problems, "\n"))
      test.eq(#test.logs("error"), 0, "errors were logged")
    end)
  end
end
