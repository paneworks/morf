-- Gallery samples for every archetype's widgets: samples/<archetype>.lua
-- (snake case: press, range, text_field, ...) returns a table of widget
-- name -> function(kit, widgets) giving the node that shows it, and
-- optionally `span`, a table of widget name -> { columns, rows } for one
-- that needs more than a cell. A popup's sample gives the press that opens
-- it (and may open it). Used by the widget galleries
-- (examples/shells/caelestia/tests/kit_widgets_gallery_spec.lua,
-- library/tests/default_widgets_gallery_spec.lua).
local M = {}

M.FILES = { Press = "press", Range = "range", Plane = "plane", Selection = "selection", Popup = "popup",
  TextField = "text_field", Scroll = "scroll", Collection = "collection", Disclosure = "disclosure", Drag = "drag",
  Navigation = "navigation", Shell = "shell", Canvas = "canvas", Dock = "dock" }

--- The samples of one archetype (an empty table while it has none).
function M.of(archetype)
  local ok, samples = pcall(require, "lib.kit.samples." .. M.FILES[archetype])
  if ok then return samples end
  if not tostring(samples):find("is not available", 1, true) then error(samples, 0) end
  return {}
end

--- The gallery's source: every widget of `archetypes` (a list; all when
--- nil) in a cell of its own, `cell` px square-ish, `columns` across.
--- Exposes `morf.ipc.missing` (widgets with no sample) and `names`.
M.SOURCE = [==[
  local ui = require("morf.ui")
  local kit = require("kit")
  local widgets = require("lib.kit.widgets")
  local contract = require("lib.kit.contract")
  local samples = require("lib.kit.samples")
  local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end
  local wanted = env("KIT_WIDGETS")
  local CELL_W, CELL_H, COLUMNS = 320, 260, 6
  local order = { "Press", "Range", "Plane", "Selection", "Popup", "TextField", "Scroll", "Collection", "Disclosure",
    "Drag", "Navigation", "Shell", "Canvas", "Dock" }
  local entries, missing = {}, {}
  for _, archetype in ipairs(order) do
    local entry = contract.archetypes[archetype]
    if entry and entry.stage <= contract.stage and (not wanted or (" " .. wanted .. " "):find(" " .. archetype .. " ", 1, true)) then
      local set = samples.of(archetype)
      for _, widget in ipairs(entry.widgets) do
        if set[widget] then
          local span = (set.span or {})[widget] or { 1, 1 }
          entries[#entries + 1] = { name = widget, make = set[widget], span = span }
        else
          missing[#missing + 1] = archetype .. "." .. widget
        end
      end
    end
  end
  -- Cells flow left to right; a wide one starts a row when it does not fit.
  local root = ui.Item { width = COLUMNS * CELL_W }
  local col, row, row_h = 0, 0, 1
  for _, e in ipairs(entries) do
    local cw, ch = math.min(COLUMNS, e.span[1]), e.span[2]
    if col + cw > COLUMNS then col, row, row_h = 0, row + row_h, 1 end
    local cell = ui.Item { id = "gallery-cell-" .. e.name, x = col * CELL_W, y = row * CELL_H,
      width = cw * CELL_W, height = ch * CELL_H }
    local node = e.make(kit, widgets)
    if node then
      ui.reparent(ui.Item { id = "gallery-" .. e.name, x = 20, y = 20, width = cw * CELL_W - 40, height = ch * CELL_H - 40,
        node }, cell)
    end
    ui.reparent(cell, root)
    col = col + cw
    row_h = math.max(row_h, ch)
  end
  root.height = (row + row_h) * CELL_H
  morf.ipc.names = function() local out = {} for _, e in ipairs(entries) do out[#out + 1] = e.name end return table.concat(out, " ") end
  morf.ipc.missing = function() return table.concat(missing, " ") end
]==]

return M
