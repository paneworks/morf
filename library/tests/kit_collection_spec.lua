-- lib.kit.collection on the Collection archetype and the engine's list
-- view, with a plain skin: ten thousand rows scrolled with no delegate
-- built after the first screen, rows of mixed heights, a sortable table and
-- a tree the arrows open.
--
--     morf test library/tests/kit_collection_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local skin = require("lib.kit.skin")
  local w = require("lib.kit.widgets")
  morf.surface.height = 600
  skin.define("plain", { skins = { Collection = {
    row = function() return function(row, s)
      local text = ui.Text { text = tostring(row.label or row.key), x = function() return 8 + 12 * s.depth() end,
        color = function() return s.current() and "#ffffff" or "#888888" end }
      return text, function(next_row) text.text = tostring(next_row.label or next_row.key) end
    end end,
    cell = function() return function(row, column)
      local text = ui.Text { text = tostring(row[column.key]) }
      return text, function(next_row) text.text = tostring(next_row[column.key]) end
    end end,
    header = function() return function(column) return ui.Text { text = column.title } end end,
  } } })
  skin.use("plain")
  local rows = {}
  for i = 1, 10000 do rows[i] = { key = "r" .. i, label = "Row " .. i } end
  local big, big_handle = w.list { id = "big", x = 0, y = 0, width = 300, height = 360, rows = rows, row_height = 36 }
  local mixed_rows = {}
  for i = 1, 50 do mixed_rows[i] = { key = "m" .. i, label = "M" .. i, kind = i % 5 == 1 and "header" or "row",
    height = i % 5 == 1 and 24 or 40 } end
  local mixed, mixed_handle = w.list { id = "mixed", x = 320, y = 0, width = 200, height = 200, rows = mixed_rows,
    row_height = 40, size_field = "height", kind_field = "kind" }
  local sorts = {}
  local files = morf.list_model({ { key = "a", name = "b.txt", size = 3 }, { key = "b", name = "a.txt", size = 9 } })
  local table_node = w.data_table { id = "files", x = 0, y = 380, width = 300, height = 120, rows = files,
    columns = { { key = "name", title = "Name", width = 200, sortable = true }, { key = "size", title = "Size", width = 100 } },
    on_sort_changed = function(key, asc) sorts[#sorts + 1] = key .. ":" .. tostring(asc) end }
  local tree = w.tree_view { id = "tree", x = 320, y = 220, width = 200, height = 200,
    tree = { { key = "docs", label = "Docs", children = { { key = "a", label = "A" }, { key = "b", label = "B" } } },
             { key = "pics", label = "Pics" } } }
  ui.Item { width = 800, height = 600, big, mixed, table_node, tree }
  morf.ipc.built = function() return big_handle.delegates_built() end
  morf.ipc.scroll = function(y) big_handle.scroll_to(tonumber(y)) end
  morf.ipc.mixed_built = function() return mixed_handle.delegates_built() end
  morf.ipc.mixed_scroll = function(y) mixed_handle.scroll_to(tonumber(y)) end
  morf.ipc.sorts = function() return table.concat(sorts, ",") end
  morf.ipc.focus = function(which) morf.focus.set(({ big = big, tree = tree })[which], true) end
]]

local function load() test.load { source = HOST, size = { 800, 600 } } test.settle(100) end

test.it("scrolls ten thousand rows building no delegate after the first screen", function()
  load()
  test.truthy(test.ipc("built") < 30, "built " .. test.ipc("built") .. " for the first screen")
  -- The first scroll fills the pool (the rows above, overscanned); after
  -- that no row is built.
  test.ipc("scroll", "400") test.settle(16)
  local first = test.ipc("built")
  for step = 1, 20 do test.ipc("scroll", tostring(step * 9000)) test.settle(16) end
  test.ipc("scroll", "359640") test.settle(30)
  test.eq(test.ipc("built"), first)
  test.truthy(test.find { text = "Row 10000", visible = true })
end)

test.it("rows of mixed heights reuse delegates only within their kind", function()
  load()
  local first = test.ipc("mixed_built")
  for y = 0, 2000, 100 do test.ipc("mixed_scroll", tostring(y)) test.settle(16) end
  test.truthy(test.ipc("mixed_built") <= first + 6, "built " .. test.ipc("mixed_built") .. " from " .. first)
  test.truthy(test.find { text = "M50", visible = true })
end)

test.it("the keys walk the list and keep the current row in sight", function()
  load()
  test.ipc("focus", "big") test.settle(20)
  for _ = 1, 15 do test.key("Down") end
  test.settle(30)
  test.truthy(test.find { text = "Row 16", visible = true })
  test.key("End") test.settle(30)
  test.truthy(test.find { text = "Row 10000", visible = true })
end)

test.it("a table sorts by its header", function()
  load()
  test.click { text = "Name" } test.settle(20)
  test.click { text = "Name" } test.settle(20)
  test.eq(test.ipc("sorts"), "name:true,name:false")
end)

test.it("a tree opens and closes with the arrows", function()
  load()
  test.falsy(test.find { text = "A", visible = true })
  test.ipc("focus", "tree") test.settle(20)
  test.key("Down") test.key("Right") test.settle(30)
  test.truthy(test.find { text = "A", visible = true })
  test.key("Left") test.settle(30)
  test.falsy(test.find { text = "A", visible = true })
end)
