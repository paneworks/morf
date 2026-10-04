-- A table and a tree (kit Collections) in each caelestia theme: they draw
-- through the theme's skin, inside their box, and answer the keys.
--
--     morf test --no-dbus examples/shells/caelestia/tests/kit_collection_gallery_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  morf.surface.height = 500
  local files = morf.list_model({})
  local rows = {}
  for i = 1, 200 do rows[i] = { key = "f" .. i, name = "file-" .. i .. ".png", size = i * 3 } end
  files:replace(rows, "key")
  local table_node, table_handle = kit.widgets.data_table { id = "gallery-table", x = 20, y = 20, width = 360,
    height = 220, rows = files, focus_policy = "strong",
    columns = { { key = "name", title = "Name", width = 240, sortable = true }, { key = "size", title = "Size", width = 120 } } }
  local tree_node = kit.widgets.tree_view { id = "gallery-tree", x = 400, y = 20, width = 280, height = 220,
    tree = { { key = "home", label = "Home", children = { { key = "docs", label = "Documents" },
      { key = "pics", label = "Pictures" } } }, { key = "etc", label = "Etc" } } }
  ui.Item { width = 720, height = 500, table_node, tree_node }
  morf.ipc.focus = function(which) morf.focus.set(which == "tree" and tree_node or table_node, true) end
  morf.ipc.built = function() return table_handle.delegates_built() end
]]

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " draws a table and a tree inside their boxes", function()
    test.load("../shell/init.lua", { size = { 720, 500 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
    test.settle(300)
    local box = test.get("gallery-table")
    local drawn = 0
    for _, node in ipairs(test.nodes()) do
      if node.visible and node.element == "Text" and node.x >= box.x - 1 and node.x < box.x + box.width
        and node.y >= box.y - 1 and node.y + node.height <= box.y + box.height + 1 then drawn = drawn + 1 end
    end
    test.truthy(drawn > 4, "the table drew " .. drawn .. " texts")
    test.ipc("focus", "table") test.key("End") test.settle(100)
    test.truthy(test.ipc("built") < 40)
    test.ipc("focus", "tree") test.key("Down") test.key("Right") test.settle(100)
    local opened = false
    for _, node in ipairs(test.nodes()) do
      if node.visible and node.text and (node.text == "Documents" or node.text == "DOCUMENTS") then opened = true end
    end
    test.truthy(opened, "the tree did not open")
    test.eq(#test.logs("error"), 0)
  end)
end
