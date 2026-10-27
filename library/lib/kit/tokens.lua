-- The installed theme's tokens (contract.lua, `tokens`), for what is built
-- the same in every theme -- the composites, the shell group -- to read.
--
--     local tokens = require("lib.kit.tokens")
--     ui.Rect { color = tokens.color("card_bg"), radius = tokens.shape("card_radius") }
--     widgets.label { text = "Title", font_size = tokens.type("title_2").size }
--
-- The theme is `require("kit")`; with none installed, the default kit's.
-- `color` returns a binding, so a theme that follows the desktop's light
-- or dark preference is followed; the others return the value.
local M = {}

local function kit()
  local ok, k = pcall(require, "kit")
  if ok and type(k) == "table" and k.tokens then return k end
  return require("lib.kit.skins.default").kit
end

local function value(v) if type(v) == "function" then return v() end return v end

--- The installed theme's tokens table.
function M.all() return kit().tokens end

--- A colour token as a binding: `tokens.color("accent_bg")`; `alpha`
--- (0..1) takes it at that opacity.
function M.color(name, alpha)
  return function()
    local c = value(kit().tokens.colors[name])
    if c == nil then error("no colour token " .. tostring(name), 2) end
    if alpha then return c:alpha(alpha) end
    return c
  end
end

--- A type token: `{ size, weight, family }`.
function M.type(name) return kit().tokens.type[name] end

--- A shape token: a radius in px or an opacity.
function M.shape(name) return kit().tokens.shape[name] end

--- A motion token: a spring's `{ mass, damping, stiffness }`, a duration
--- in ms, or the easing's name.
function M.motion(name) return kit().tokens.motion[name] end

return M
