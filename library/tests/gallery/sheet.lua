-- Gallery samples for the Sheet archetype's widgets: each a grid as it is
-- used -- a budget in a spreadsheet with a range picked, a team in a data
-- grid, a drum pattern on a step sequencer with its playhead, a cinema's
-- seats with some taken and two chosen, a times table in a plain grid.
local S = { span = {} }

local function show(v)
  if type(v) ~= "number" then return v end
  return v
end

function S.spreadsheet(kit, w)
  local items = { "Rent", "Food", "Transport", "Power", "Internet", "Insurance", "Gym", "Books", "Travel", "Gifts" }
  local data = {}
  for r = 1, 200 do
    data[r] = {}
    local item = items[(r - 1) % #items + 1]
    data[r][1] = r <= #items and item or (item .. " " .. ((r - 1) // #items + 1))
    for c = 2, 6 do data[r][c] = ((r * 37 + c * 91) % 400) + 20 end
  end
  local node, sheet
  node, sheet = w.spreadsheet { id = "sample-spreadsheet", width = 560, height = 440, rows = 200, columns = 6,
    headers = { "Item", "Jan", "Feb", "Mar", "Apr", "May" },
    column_widths = { 128, 70, 70, 70, 70, 70 },
    cell = function(r, c) return show(data[r] and data[r][c]) end,
    on_edited = function(r, c, text) data[r][c] = tonumber(text) or text end,
    on_cleared = function(r0, c0, r1, c1)
      for r = r0, r1 do for c = c0, c1 do data[r][c] = nil end end
    end,
    on_paste = function(r, c, grid)
      for i, line in ipairs(grid) do
        for j, field in ipairs(line) do
          if data[r + i - 1] then data[r + i - 1][c + j - 1] = tonumber(field) or field end
        end
      end
    end }
  -- A range picked, as someone summing a quarter would.
  sheet.go(2, 2)
  sheet.send("key", "Down", "shift", "", 0)
  sheet.send("key", "Down", "shift", "", 0)
  sheet.send("key", "Right", "shift", "", 0)
  sheet.send("key", "Right", "shift", "", 0)
  return node
end
S.span.spreadsheet = { 2, 2 }

function S.data_grid(kit, w)
  local people = {
    { "Ada Lovelace", "Analyst", "London", 36 }, { "Grace Hopper", "Admiral", "New York", 85 },
    { "Alan Turing", "Cryptanalyst", "Manchester", 41 }, { "Katherine Johnson", "Mathematician", "Hampton", 101 },
    { "Edsger Dijkstra", "Professor", "Austin", 72 }, { "Barbara Liskov", "Professor", "Boston", 84 },
    { "Ken Thompson", "Engineer", "Berkeley", 81 }, { "Margaret Hamilton", "Director", "Cambridge", 88 },
    { "Dennis Ritchie", "Engineer", "Murray Hill", 70 }, { "Frances Allen", "Fellow", "Yorktown", 88 },
  }
  local node, grid = w.data_grid { id = "sample-data-grid", width = 560, height = 220, rows = #people, columns = 4,
    headers = { "Name", "Role", "City", "Age" }, column_widths = { 190, 140, 140, 88 },
    read_only = { 4 },
    cell = function(r, c) return people[r][c] end,
    on_edited = function(r, c, text) people[r][c] = text end }
  grid.go(2, 1)
  return node
end
S.span.data_grid = { 2, 1 }

function S.step_sequencer(kit, w)
  local voices = { "Kick", "Snare", "Hi-hat", "Open hat", "Clap", "Perc" }
  local pattern = {
    "x...x...x...x..x", "....x.......x...", "x.x.x.x.x.x.x.xx", "..x...x...x...x.",
    "....x..x....x...", ".x.....x..x...x.",
  }
  local on = {}
  for r, line in ipairs(pattern) do
    on[r] = {}
    for c = 1, #line do on[r][c] = line:sub(c, c) == "x" end
  end
  local playhead = morf.signal("sample.step_sequencer.playhead", 5)
  local node, seq = w.step_sequencer { id = "sample-step-sequencer", width = 560, height = 220, rows = #voices,
    columns = 16, row_headers = voices,
    playhead = function() return playhead:get() end,
    cell = function(r, c) return on[r][c] end,
    on_toggled = function(r, c) on[r][c] = not on[r][c] end }
  seq.go(3, 7)
  return node
end
S.span.step_sequencer = { 2, 1 }

function S.seat_map(kit, w)
  local taken = {}
  for r = 1, 6 do
    taken[r] = {}
    for c = 1, 14 do taken[r][c] = ((r * 7 + c * 5) % 9) < 3 end
  end
  local chosen = { [3] = { [7] = true, [8] = true } }
  taken[3][7], taken[3][8] = false, false
  local node, seats = w.seat_map { id = "sample-seat-map", width = 560, height = 220, rows = 6, columns = 14,
    cell = function(r, c) return chosen[r] and chosen[r][c] or false end,
    disabled = function(r, c) return taken[r][c] end,
    on_toggled = function(r, c)
      chosen[r] = chosen[r] or {}
      chosen[r][c] = not chosen[r][c] or nil
    end }
  seats.go(3, 8)
  return node
end
S.span.seat_map = { 2, 1 }

function S.cell_grid(kit, w)
  local node = w.cell_grid { id = "sample-cell-grid", width = 280, height = 220, rows = 12, columns = 12,
    column_widths = 46, row_height = 34,
    cell = function(r, c) return r * c end }
  return node
end

return S
