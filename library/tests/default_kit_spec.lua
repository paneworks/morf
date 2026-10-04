-- The default kit's gallery (library/lib/kit/skins/default): every display
-- widget and domain instrument the contract requires, and every composite,
-- drawn by the default kit in a cell of its own, in each of its three
-- variants. A cell passes when everything visible it draws stays inside it
-- -- nothing overlaps a neighbour or spills -- and nothing logs an error.
--
--     morf test --no-dbus library/tests/default_kit_spec.lua
--
-- With KIT_GALLERY_SNAPSHOTS set, it also renders the gallery page by page
-- in every variant (a GPU is needed for the images):
--
--     KIT_GALLERY_SNAPSHOTS=1 nixVulkanIntel morf test --no-dbus --snapshots DIR library/tests/default_kit_spec.lua

local test = morf.test

-- The configuration under test: the default kit as `require("kit")`, in the
-- variant MORF_KIT_VARIANT names, drawing GALLERY_GROUP ("display" or
-- "composites") -- all of it, or page GALLERY_PAGE of it.
local SOURCE = [[
  local ui = require("morf.ui")
  local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end
  local variant = env("MORF_KIT_VARIANT") or "light"
  local default = require("lib.kit.skins.default")
  local kit = default.make { variant = variant }
  package.loaded.kit = kit
  local contract = require("lib.kit.contract")
  local display = require("lib.kit.display")
  local composites = require("lib.kit.composites")
  local group = env("GALLERY_GROUP") or "display"
  local page = tonumber(env("GALLERY_PAGE") or "0")
  local CELL_W, CELL_H, COLUMNS, PER = 300, 230, 6, 24
  if group == "composites" then CELL_W, CELL_H, COLUMNS, PER = 560, 380, 3, 6 end

  local SAMPLES, names = {}, {}
  if group == "composites" then
    for _, g in ipairs(composites.groups()) do
      for name, make in pairs(require("lib.kit.composites.samples_" .. g)) do
        SAMPLES[name] = function() return make(kit, composites) end
      end
    end
    for name, entry in pairs(contract.composites) do
      if entry.stage <= contract.stage then names[#names + 1] = name end
    end
  else
    local v = morf.signal("gallery.value", 0.62)
    local function value() return v:get() end
    local function series() local t = {} for i = 1, 40 do t[i] = 30 + 25 * math.sin(i / 4) end return t end
    local function bands() local t = {} for i = 1, 32 do t[i] = .5 + .4 * math.sin(i / 3) end return t end
    local signal = kit.signal("accent")
    SAMPLES = {
      label = function() return kit.label { text = "Label" } end,
      heading = function() return kit.heading { text = "Heading", width = 260 } end,
      subtitle = function() return kit.subtitle { text = "Subtitle text", width = 260 } end,
      caption = function() return kit.caption { width = 260, text = "Caption", note = "note" } end,
      readout = function() return kit.readout { value = function() return "42" end, unit = "%" } end,
      facts = function() return kit.facts({ { "Speed", "4.2 GHz" }, { "Cores", "16" }, { "Cache", "24 MiB" } }, 260) end,
      text = function() return kit.text { text = "Body text", width = 260 } end,
      keycap = function() return ui.Row { gap = 6, kit.keycap { text = "Ctrl" }, kit.keycap { text = "Esc" } } end,
      icon = function() return kit.icon("home", 24, signal) end,
      emblem = function() return ui.Row { gap = 8, kit.emblem { kind = "ok", size = 40 }, kit.emblem { kind = "warn", size = 40 },
        kit.emblem { kind = "alert", size = 40 }, kit.emblem { kind = "info", size = 40 } } end,
      status_line = function() return kit.status_line { kind = "alert", title = "Warning", subtitle = "Disk full", width = 260 } end,
      status = function() return kit.status { width = 260, kind = function() return "ok" end,
        title = function() return "Normal" end, subtitle = function() return "Running smoothly" end } end,
      chip = function() return ui.Row { gap = 6, kit.chip { text = "Online" }, kit.chip { text = "Beta", filled = true },
        kit.chip { text = "Error", color = kit.signal("alert") } } end,
      loading = function() return kit.loading(32, nil, { active = function() return false end }) end,
      gauge = function() return kit.gauge { size = 90, value = value } end,
      ring = function() return kit.ring { size = 170, value = value, label = "Load" } end,
      mini_ring = function() return kit.mini_ring { size = 80, value = value, text = function() return "62" end, label = "Mem" } end,
      bar = function() return kit.bar { width = 260, stroke = 6, value = value } end,
      meter = function() return ui.Column { gap = 12, kit.meter { width = 260, height = 8, value = value },
        kit.meter { width = 260, height = 8, value = value, count = 10 } } end,
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
      surface = function() return kit.surface { width = 260, height = 120, radius = 12,
        color = function() return kit.theme.P().view end } end,
    }
    local function samples(module)
      for name, make in pairs(require(module)) do
        if not SAMPLES[name] then SAMPLES[name] = function() return make(kit) end end
      end
    end
    for _, g in ipairs(display.GROUPS) do samples("lib.kit.display.samples_" .. g) end
    for _, g in ipairs(display.DOMAINS) do samples("lib.kit.domain.samples_" .. g) end
    local seen = {}
    for _, list in pairs(contract.display) do
      for _, entry in ipairs(list) do
        if entry.stage <= contract.stage and not seen[entry.fn] then seen[entry.fn] = true names[#names + 1] = entry.fn end
      end
    end
    for _, domain in pairs(contract.domain) do
      if (domain.stage or 1) <= contract.stage then
        for _, fn in ipairs(domain.widgets or {}) do
          if not seen[fn] then seen[fn] = true names[#names + 1] = fn end
        end
      end
    end
  end
  table.sort(names)
  morf.ipc.pages = function() return tostring(math.ceil(#names / PER)) end
  local shown = names
  if page > 0 then
    shown = {}
    for i = (page - 1) * PER + 1, math.min(#names, page * PER) do shown[#shown + 1] = names[i] end
  end
  local rows = math.max(1, math.ceil(#shown / COLUMNS))
  morf.surface.height = rows * CELL_H
  local P = kit.theme.P
  local root = ui.Rect { width = COLUMNS * CELL_W, height = rows * CELL_H, color = function() return P().window end }
  local missing = {}
  for i, name in ipairs(shown) do
    local col, row = (i - 1) % COLUMNS, math.floor((i - 1) / COLUMNS)
    local cell = ui.Item { id = "gallery-cell-" .. name, x = col * CELL_W, y = row * CELL_H, width = CELL_W, height = CELL_H }
    ui.reparent(ui.Text { x = 8, y = 3, width = CELL_W - 16, height = 15, text = name, elide = "right",
      font_family = kit.theme.font, font_size = 11, color = function() return P().ink_dim end }, cell)
    local make = SAMPLES[name]
    if make then
      -- A sample that fails is logged by name and leaves its cell empty.
      local ok, node = pcall(make)
      if ok then
        ui.reparent(ui.Item { id = "gallery-" .. name, x = 20, y = 20, width = CELL_W - 40, height = CELL_H - 40, node }, cell)
      else
        morf.log("error", "gallery " .. name .. ": " .. tostring(node))
      end
    else
      missing[#missing + 1] = name
    end
    ui.reparent(cell, root)
  end
  morf.ipc.missing = function() return table.concat(missing, " ") end
  morf.ipc.count = function() return tostring(#shown) end
]]

--- Visible nodes inside a gallery cell that reach outside it: what shows
--- of each is its box cut by its clipping ancestors, and a turned drawing
--- counts by its centre (its box is the bounds of its turned square).
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

local SIZE = { display = { 1800, 1080 }, composites = { 1680, 1080 } }

local function load(variant, group, page)
  test.load { source = SOURCE, size = SIZE[group],
    env = { MORF_KIT_VARIANT = variant, GALLERY_GROUP = group, GALLERY_PAGE = tostring(page or 0) } }
  test.settle(1500)
end

local shoot = morf.env and morf.env("KIT_GALLERY_SNAPSHOTS")
if shoot == false or shoot == "" then shoot = nil end

for _, variant in ipairs { "light", "dark", "high_contrast" } do
  for _, group in ipairs { "display", "composites" } do
    test.it(("the default kit (%s) draws every %s sample inside its cell"):format(variant, group), function()
      load(variant, group)
      test.eq(test.ipc("missing"), "", "contract entries with no gallery sample")
      local problems = spills()
      test.eq(#problems, 0, table.concat(problems, "\n"))
      local errors = {}
      for _, e in ipairs(test.logs("error")) do errors[#errors + 1] = tostring(e.message or e.text or e) end
      test.eq(#errors, 0, "errors were logged:\n" .. table.concat(errors, "\n"))
    end)
    if shoot then
      test.it(("the default kit (%s) renders the %s gallery"):format(variant, group), function()
        load(variant, group, 1)
        local pages = tonumber(test.ipc("pages")) or 1
        for page = 1, pages do
          if page > 1 then load(variant, group, page) end
          test.snapshot(("default-%s-%s-%02d.png"):format(variant, group, page))
        end
      end)
    end
  end
end
