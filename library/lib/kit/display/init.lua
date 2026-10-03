-- Display widgets every theme draws: text, media, status, readings,
-- charts and structure that take no input (library/lib/kit/contract.lua,
-- `display`). The composition is shared -- what goes where, the numbers a
-- chart is laid out from, which data channel feeds which path -- and the
-- look is the theme's, through its style (themes/<name>/display_style.lua):
-- its colours, corners, strokes, type and marks.
--
--     require("lib.kit.display").install(kit, style)
--
-- adds every display function `kit` does not already have; a theme that
-- draws one its own way defines it first and keeps it. Each function is
-- `kit.<name>(spec)` and returns a node; `spec` takes `id`, `x`, `y`,
-- `anchors` and its own fields, documented on it.
--
-- Charts and readings that move read data channels (`morf.channel`;
-- `lib.channel.from` takes a channel, a list or a function of one), drawn
-- by a `ui.Path`'s `series` and `plot` -- none builds a path in Lua per
-- sample.

local M = {}

M.GROUPS = { "text", "media", "status", "readings", "charts", "structure" }
M.DOMAINS = { "aviation", "hud_game", "hud_fui", "audio", "editor" }

--- Adds the shared display functions `kit` lacks, each drawn in `style`.
function M.install(kit, style)
  style.kit = kit
  local function add(module)
    for name, draw in pairs(require(module)) do
      if kit[name] == nil then
        kit[name] = function(spec) return draw(spec or {}, style) end
      end
    end
  end
  for _, group in ipairs(M.GROUPS) do add("lib.kit.display." .. group) end
  -- The domain instruments over them (lib/kit/domain): aviation, HUD.
  for _, group in ipairs(M.DOMAINS) do add("lib.kit.domain." .. group) end
  return kit
end

return M
