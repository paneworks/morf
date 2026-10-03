-- Composites: widgets built from archetypes (library/lib/kit/contract.lua,
-- `composites`), shared by every theme -- each looks the way its parts'
-- skins make it look.
--
--     local composites = require("lib.kit.composites")
--     local node, combo = composites.combo_box { items = { "One", "Two" }, current = 1 }
--
-- Each composite is a module `lib.kit.composites.<name>` returning its
-- constructor, `make(spec) -> node, handle`; this table finds them by name,
-- loading each the first time it is asked for.
local M = {}

-- The gallery samples' groups (samples_<group>.lua: name -> function(kit,
-- composites) returning a node for a 560x380 cell).
local GROUPS = { "inputs", "pickers", "surfaces", "layouts" }

setmetatable(M, { __index = function(t, name)
  local ok, made = pcall(require, "lib.kit.composites." .. name)
  if not ok then
    if tostring(made):find("module 'lib.kit.composites." .. name .. "' not found", 1, true) then return nil end
    error(made, 2)
  end
  local make = type(made) == "table" and made.make or made
  rawset(t, name, make)
  return make
end })

--- The sample groups, for the gallery.
function M.groups() return GROUPS end

--- Whether a composite named `name` exists.
function M.has(name)
  local ok, found = pcall(function() return M[name] end)
  return ok and type(found) == "function"
end

return M
