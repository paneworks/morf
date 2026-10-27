-- The default kit: a complete Adwaita-like (GNOME HIG) theme, so an
-- application needs none of its own. Its tokens (contract.lua, `tokens`)
-- and a skin for every archetype, display widget and editor archetype.
--
--     local default = require("lib.kit.skins.default")
--     package.loaded.kit = default.make { variant = "dark" }   -- or "light", "high_contrast"
--     -- or simply: package.loaded.kit = default.kit
--
-- `make(options)`: `variant` ("dark", "light", "high_contrast"; left out,
-- the desktop's preference -- `morf.prefers.contrast` "high" picks high
-- contrast, `color_scheme` "dark" dark, anything else light -- followed
-- live), `reduced_motion` (default `morf.prefers.reduced_motion`: every
-- duration 0, springs instant), `font`, `name` (the skin theme's name,
-- "default"), `use` (false: define the skins without switching to them;
-- `kit.use()` switches later). `M.kit` is the default instance, made the
-- first time it is read.
--
-- The skins are assembled from one file per concern below, each a
-- function of `(S, theme, draw)` that adds skins to `S`; `draw` is the skin's
-- own drawing helpers, shared between the files and given to no one else.
local morf = require("morf")
local tokens = require("lib.kit.skins.default.tokens")
local contract = require("lib.kit.contract")

local M = { tokens = tokens, VARIANTS = tokens.VARIANTS }

-- In order: the helpers the later files draw with come first.
local FILES = { "text", "containers", "display", "press", "range", "plane", "selection", "popup", "text_field",
  "scroll", "collection", "disclosure", "drag", "navigation", "shell", "composites", "canvas", "dock" }

local converted = {}
--- A variant's palette as morf colours.
function M.palette(variant)
  if not converted[variant] then
    local src = assert(tokens.palettes[variant], "unknown variant " .. tostring(variant))
    local out = {}
    for k, v in pairs(src) do out[k] = type(v) == "string" and morf.color(v) or v end
    converted[variant] = out
  end
  return converted[variant]
end

local function prefers(field)
  local ok, value = pcall(function() return morf.prefers and morf.prefers[field] end)
  if ok then return value end
  return nil
end

--- The variant the desktop asks for (read in a binding, it is followed).
function M.preferred()
  if prefers("contrast") == "high" then return "high_contrast" end
  if prefers("color_scheme") == "dark" then return "dark" end
  return "light"
end

function M.make(options)
  options = options or {}
  local fixed = options.variant
  if fixed == "auto" or fixed == "" then fixed = nil end
  if fixed then M.palette(fixed) end
  local reduced = options.reduced_motion
  if reduced == nil then reduced = prefers("reduced_motion") == true end
  local theme = {
    variant = fixed, reduced = reduced,
    size = tokens.size, font = options.font or tokens.font, mono = tokens.mono, icon_font = tokens.icon_font,
    radius = tokens.radius, PAD = tokens.PAD, GAP = tokens.GAP, type = tokens.type,
    control_height = tokens.control_height, ease = tokens.ease, duration = tokens.duration(reduced),
    motion = tokens.motion(reduced),
  }
  if fixed then
    local p = M.palette(fixed)
    theme.P = function() return p end
  else
    theme.P = function() return M.palette(M.preferred()) end
  end

  local S, draw = {}, {}
  for _, file in ipairs(FILES) do require("lib.kit.skins.default." .. file)(S, theme, draw) end
  local press = {}
  for _, entry in ipairs(contract.archetypes.Press.widgets) do press[#press + 1] = entry.name end
  require("lib.kit.archetypes.press").fade(S, press, 0.5)

  -- The contract's tokens: each colour a binding of the live palette.
  local colors = {}
  for _, name in ipairs(contract.tokens.colors) do
    colors[name] = function() return theme.P()[name] end
  end
  local kit = {
    skins = S, theme = theme, variant = fixed or "auto",
    tokens = { colors = colors, type = tokens.type, shape = tokens.shape, motion = theme.motion },
  }

  local name = options.name or "default"
  --- Makes this kit's skins the ones every control draws with.
  function kit.use()
    local skin = require("lib.kit.skin")
    skin.define(name, { skins = kit.skins })
    skin.use(name)
  end
  if options.use ~= false then kit.use() end
  return kit
end

setmetatable(M, { __index = function(t, key)
  if key == "kit" then
    local kit = M.make()
    rawset(t, "kit", kit)
    return kit
  end
end })

return M
