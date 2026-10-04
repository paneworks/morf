-- The default kit: a complete, Adwaita-like (GNOME HIG) kit, so an
-- application needs no theme. Every function of the contract
-- (lib.kit.contract), a skin for every archetype, and the shared display
-- widgets, domain instruments and composites drawn through them.
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
local morf = require("morf")
local tokens = require("lib.kit.skins.default.tokens")

local M = { tokens = tokens, VARIANTS = tokens.VARIANTS }

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
    radius = tokens.radius, ROUNDING = tokens.ROUNDING, PAD = tokens.PAD, GAP = tokens.GAP,
    control_height = tokens.control_height, ease = tokens.ease, duration = tokens.duration(reduced),
  }
  if fixed then
    local p = M.palette(fixed)
    theme.P = function() return p end
  else
    theme.P = function() return M.palette(M.preferred()) end
  end

  local kit = require("lib.kit.skins.default.components")(theme)
  kit.skins = require("lib.kit.skins.default.skins")(theme, kit)
  require("lib.kit.display").install(kit, require("lib.kit.skins.default.display_style")(theme, kit))
  kit.theme, kit.tokens = theme, tokens
  kit.variant = fixed or "auto"
  kit.widgets = require("lib.kit.widgets")

  --- A pressable area as a kit Press `widget` (the MouseArea a layout
  --- builds a row or a tile from); `settings` are the Press's own.
  function kit.press_area(widget, props, settings, archetype)
    local s, node, children = { widget = widget }, {}, {}
    for key, value in pairs(props or {}) do
      if type(key) == "number" then children[key] = value
      elseif type(key) == "string" and key:match("^on_") then s[key] = value
      else node[key] = value end
    end
    for key, value in pairs(settings or {}) do s[key] = value end
    if node.enabled ~= nil then s.enabled = node.enabled end
    return (require("lib.kit.control").make(archetype or "Press", widget, s, { props = node, children = children }))
  end

  --- Names a node for a screen reader, and returns it.
  function kit.named(node, name)
    node.accessible_name = name
    return node
  end

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
