-- Composites: widgets built from archetypes (library/lib/kit/contract.lua,
-- `composites` and `shell`), shared by every theme -- each looks the way
-- its parts' skins make it look, with what a theme's skin adds for it.
--
--     local composites = require("lib.kit.composites")
--     local node, drop = composites.drop_down { items = { "One", "Two" }, selected = 1 }
--
-- Each composite is a module `lib.kit.composites.<name>` (the rows and
-- the preference groups and pages in `lib.kit.composites.rows`) returning
-- its constructor, `make(spec) -> node, handle`; this table finds them by
-- name, loading each the first time it is asked for.
local M = {}

-- The boxed-list rows and what holds them.
local ROWS = { action_row = true, switch_row = true, combo_row = true, entry_row = true, password_entry_row = true,
  spin_row = true, expander_row = true, button_row = true, preferences_group = true, preferences_page = true }

setmetatable(M, { __index = function(t, name)
  local module = (ROWS[name] and "lib.kit.composites.rows." or "lib.kit.composites.") .. name
  local ok, made = pcall(require, module)
  if not ok then
    if tostring(made):find("`" .. module .. "` is not available", 1, true) then return nil end
    error(made, 2)
  end
  local make = type(made) == "table" and made.make or made
  rawset(t, name, make)
  return make
end })

--- Whether a composite named `name` exists.
function M.has(name)
  local ok, found = pcall(function() return M[name] end)
  return ok and type(found) == "function"
end

return M
