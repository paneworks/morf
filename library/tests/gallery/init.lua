-- Gallery samples for every widget of the contract, a file per group:
-- gallery/<group>.lua (press, range, selection, ..., display, composites,
-- shell, canvas, dock) returns a table of widget name -> function(kit,
-- widgets) giving the node that shows it -- each variant of it where it
-- has them -- and optionally `span`, a table of widget name -> { columns,
-- rows } for one that needs more than a cell. A popup's sample gives the
-- button that opens it. Used by library/tests/default_widgets_gallery_spec.lua.
local M = {}

-- Each group: its sample file and the contract's widgets in it.
function M.groups()
  local contract = require("lib.kit.contract")
  local out = {}
  local ORDER = { "Press", "Range", "Selection", "Popup", "TextField", "Scroll", "Collection", "Disclosure", "Drag",
    "Navigation", "Shell" }
  local FILE = { TextField = "text_field" }
  local function names(list) local n = {} for i, w in ipairs(list) do n[i] = w.name end return n end
  for _, archetype in ipairs(ORDER) do
    out[#out + 1] = { name = archetype, file = FILE[archetype] or archetype:lower(),
      widgets = names(contract.archetypes[archetype].widgets) }
  end
  out[#out + 1] = { name = "display", file = "display", widgets = names(contract.display) }
  out[#out + 1] = { name = "composites", file = "composites", widgets = names(contract.composites) }
  out[#out + 1] = { name = "shell", file = "shell_group", widgets = names(contract.shell) }
  out[#out + 1] = { name = "Canvas", file = "canvas", widgets = names(contract.editor.Canvas.widgets) }
  out[#out + 1] = { name = "Dock", file = "dock", widgets = names(contract.editor.Dock.widgets) }
  return out
end

--- The gallery's source: every widget of the groups GALLERY names (all
--- when it is unset) in a cell of its own, 320 x 260 px, six across.
--- Exposes `morf.ipc.missing` (widgets with no sample) and `names`.
M.SOURCE = [==[
  local ui = require("morf.ui")
  local kit = require("kit")
  local widgets = require("lib.kit.widgets")
  local gallery = require("gallery")
  local function env(name) local v = morf.env(name) if v == false or v == "" then return nil end return v end
  local wanted = env("GALLERY")
  -- (A picture is taken of the surface: as tall as the gallery, not a bar.)
  if env("KIT_GALLERY_SNAPSHOTS") then morf.surface.width, morf.surface.height = 1920, 4320 end
  local CELL_W, CELL_H, COLUMNS = 320, 260, 6
  local entries, missing = {}, {}
  for _, group in ipairs(gallery.groups()) do
    if not wanted or (" " .. wanted .. " "):find(" " .. group.name .. " ", 1, true) then
      local set = require("gallery." .. group.file)
      for _, widget in ipairs(group.widgets) do
        if set[widget] then
          local span = (set.span or {})[widget] or { 1, 1 }
          entries[#entries + 1] = { name = widget, make = set[widget], span = span }
        else
          missing[#missing + 1] = group.name .. "." .. widget
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
    local ok, node = pcall(e.make, kit, widgets)
    if not ok then morf.log("error", "gallery " .. e.name .. ": " .. tostring(node)) node = nil end
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
