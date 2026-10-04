-- lib.kit.sheet drawn by the default kit: the arrows walk cells and Shift
-- picks a range; a typed character starts an edit that Return commits and
-- Escape cancels; Ctrl+C puts the range on the clipboard as TSV and
-- Ctrl+V pastes it back; a step sequencer's pad toggles by a click and by
-- Space; a sheet of 10,000 rows builds only what shows and walks to its
-- end.
--
--     morf test --no-dbus library/tests/kit_sheet_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  morf.surface.width, morf.surface.height = 840, 640
  local kit = require("lib.kit.skins.default").make { variant = "light" }
  package.loaded.kit = kit
  local w = require("lib.kit.widgets")
  local log = {}
  local function note(s) log[#log + 1] = s end
  local copied
  morf.clipboard = morf.clipboard or {}
  morf.clipboard.set = function(text) copied = text end
  local data = {}
  for r = 1, 20 do data[r] = {} for c = 1, 5 do data[r][c] = r * 10 + c end end
  local sheet_node, sheet = w.spreadsheet { id = "sheet", x = 0, y = 0, width = 500, height = 300, rows = 20, columns = 5,
    cell = function(r, c) return data[r][c] end,
    on_edited = function(r, c, text) data[r][c] = text note(("edited:%d:%d:%s"):format(r, c, text)) end,
    on_edit_canceled = function() note("canceled") end,
    on_paste = function(r, c, grid)
      note(("paste:%d:%d:%dx%d:%s"):format(r, c, #grid, #grid[1], grid[2][2]))
    end }
  local pads = {}
  local seq_node, seq = w.step_sequencer { id = "seq", x = 520, y = 0, width = 300, height = 140, rows = 4, columns = 8,
    row_headers = { "Kick", "Snare", "Hat", "Clap" },
    cell = function(r, c) return pads[r .. ":" .. c] == true end,
    on_toggled = function(r, c) pads[r .. ":" .. c] = not pads[r .. ":" .. c] note(("toggled:%d:%d"):format(r, c)) end }
  local big_node, big = w.data_grid { id = "big", x = 0, y = 320, width = 500, height = 300, rows = 10000, columns = 3,
    headers = { "Row", "Square", "Half" },
    cell = function(r, c) if c == 1 then return r elseif c == 2 then return r * r else return r / 2 end end }
  ui.Item { width = 840, height = 640, sheet_node, seq_node, big_node }
  morf.ipc.log = function() local s = table.concat(log, ","); log = {}; return s end
  morf.ipc.at = function(name)
    local h = ({ sheet = sheet, seq = seq, big = big })[name]
    return ("%d,%d|%s"):format(h.t.row, h.t.column, h.t.range)
  end
  morf.ipc.editing = function() return sheet.editing() end
  morf.ipc.editor = function() local e = sheet.editor() return e and e.text or "" end
  morf.ipc.copied = function() return copied or "" end
  morf.ipc.pad = function(r, c) return pads[r .. ":" .. c] == true end
  morf.ipc.built = function() return big.rows_built() end
  morf.ipc.offset = function() local _, y = big.offset() return y end
]]

local function load() test.load { source = HOST, size = { 840, 640 } } test.settle(200) end

-- The spreadsheet: row header 44 wide, header 30 tall, five columns of 91
-- (filling 456), rows 30 tall. The centre of a cell:
local function cell(r, c) return 44 + (c - 1) * 91 + 45, 30 + (r - 1) * 30 + 15 end

test.it("the arrows walk cells and Shift picks a range", function()
  load()
  test.click(cell(1, 1)) test.settle(30)
  test.eq(test.ipc("at", "sheet"), "1,1|1,1,1,1")
  test.key("Right") test.key("Down")
  test.eq(test.ipc("at", "sheet"), "2,2|2,2,2,2")
  test.key("Down", "shift") test.key("Right", "shift")
  test.eq(test.ipc("at", "sheet"), "3,3|2,2,3,3")
  test.key("Left")
  test.eq(test.ipc("at", "sheet"), "3,2|3,2,3,2")
  -- A drag across cells picks the range under it.
  test.drag({ cell(1, 1) }, { cell(3, 4) }, { steps = 6 }) test.settle(30)
  test.eq(test.ipc("at", "sheet"), "3,4|1,1,3,4")
end)

test.it("a typed character starts an edit and Return commits it", function()
  load()
  test.click(cell(2, 3)) test.settle(30)
  test.key("7")
  test.settle(30)
  test.eq(test.ipc("editing"), true)
  test.eq(test.ipc("editor"), "7")
  test.type("2")
  test.key("Return") test.settle(30)
  test.eq(test.ipc("log"), "edited:2:3:72")
  test.eq(test.ipc("editing"), false)
  -- Committed, the cursor goes down a row, and the cell shows the value.
  test.eq(test.ipc("at", "sheet"), "3,3|3,3,3,3")
  test.truthy(test.find { text = "72", visible = true }, "the edited value shows")
end)

test.it("Escape cancels an edit and keeps the value", function()
  load()
  test.click(cell(1, 2)) test.settle(30)
  test.key("F2") test.settle(30)
  test.eq(test.ipc("editor"), "12", "F2 opens the editor on the cell's value")
  test.type("99")
  test.key("Escape") test.settle(30)
  test.eq(test.ipc("editing"), false)
  test.eq(test.ipc("log"), "canceled")
  test.truthy(test.find { text = "12", visible = true })
  -- The sheet has the keys again.
  test.key("Down")
  test.eq(test.ipc("at", "sheet"), "2,2|2,2,2,2")
end)

test.it("Ctrl+C puts the range on the clipboard as TSV, Ctrl+V pastes it", function()
  load()
  test.click(cell(1, 1)) test.settle(30)
  test.key("Down", "shift") test.key("Right", "shift")
  test.key("c", "ctrl") test.settle(30)
  test.eq(test.ipc("copied"), "11\t12\n21\t22")
  test.key("Right") test.key("Right")
  test.key("v", "ctrl") test.settle(30)
  test.eq(test.ipc("log"), "paste:2:4:2x2:22")
end)

test.it("a step sequencer's pad toggles by a click and by Space", function()
  load()
  -- The pad at row 2, column 3: the row headers are 96 wide, the rows
  -- share the height under a 26 px header, the columns the width.
  local rh = math.max(22, math.min(48, math.floor((140 - 26) / 4)))
  local cw = math.max(22, math.min(48, math.floor((300 - 96) / 8), math.floor(rh * 1.25)))
  local x, y = 520 + 96 + 2 * cw + cw / 2, 26 + rh + rh / 2
  test.click(x, y) test.settle(30)
  test.eq(test.ipc("log"), "toggled:2:3")
  test.eq(test.ipc("pad", 2, 3), true)
  test.key("space") test.settle(30)
  test.eq(test.ipc("log"), "toggled:2:3")
  test.eq(test.ipc("pad", 2, 3), false)
  test.key("Right") test.key("space")
  test.eq(test.ipc("log"), "toggled:2:4")
  test.eq(test.ipc("editing"), false)
end)

test.it("a sheet of 10,000 rows builds what shows and walks to its end", function()
  load()
  test.truthy(test.ipc("built") < 30, "rows built: " .. tostring(test.ipc("built")))
  test.click(100, 320 + 36 + 18) test.settle(30)
  test.key("End", "ctrl") test.settle(60)
  test.eq(test.ipc("at", "big"), "10000,3|10000,3,10000,3")
  test.truthy(test.find { text = "10000", visible = true }, "the last row shows")
  for _ = 1, 100 do test.key("Page_Up") end
  test.settle(60)
  test.eq(test.ipc("at", "big"), "9000,3|9000,3,9000,3")
  test.truthy(test.find { text = "81000000", visible = true }, "row 9000 shows")
  test.truthy(test.ipc("built") < 40, "rows built: " .. tostring(test.ipc("built")))
end)
