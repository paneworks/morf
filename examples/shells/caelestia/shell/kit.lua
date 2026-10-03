-- Visual components supplied by the selected theme. Services and state live
-- outside theme packages; both themes receive the same wallpaper palette.
local theme = require("theme")
local kit = require(theme.appearance.components)(theme)
local widgets = require("lib.kit.widgets")

-- The theme's skins draw the kit's archetypes (lib.kit.skin).
if kit.skins then
  local skin = require("lib.kit.skin")
  skin.define(theme.appearance.id, { skins = kit.skins, defaults = kit.skin_defaults })
  skin.use(theme.appearance.id)
end

local function copy(spec)
  local out = {}
  for k, v in pairs(spec or {}) do out[k] = v end
  return out
end

-- Every press and range in the shell is a kit widget: the behaviour is the
-- archetype's (crates/morf-kit), the look the theme's skin. These keep the
-- shapes the layouts already call them with.
kit.widgets = widgets

--- A pressable area: the MouseArea a layout builds a row, a tile or a card
--- action from, as a kit Press (`area`) -- Tab reaches it, Return and
--- Space click it, and the theme's skin marks hover, press and focus. Its
--- properties and children are the layout's, kept across a theme switch.
function kit.action(props)
  local spec, node, children = { widget = "area" }, {}, {}
  for key, value in pairs(props or {}) do
    if type(key) == "number" then children[key] = value
    elseif type(key) == "string" and key:match("^on_") then spec[key] = value
    else node[key] = value end
  end
  -- A disabled area takes no press, as a MouseArea's `enabled` says, and
  -- its archetype knows.
  if node.enabled ~= nil then spec.enabled = node.enabled end
  return (require("lib.kit.control").make("Press", "area", spec, { props = node, children = children }))
end

--- A filled button: `label`, `icon`, `on_clicked`, `width`, `height` (32),
--- `color`/`ink`.
function kit.pill(spec)
  local s = copy(spec)
  s.height = s.height or 32
  return widgets.pill(s)
end

--- A switch: `on` (fn), `on_toggled(on)`.
function kit.switch(spec)
  local s = copy(spec)
  s.checked, s.on = spec.on, nil
  return widgets.switch(s)
end

--- A small icon toggle: `icon_on`, `icon_off`, `on` (fn: the alert tone
--- while on), `on_clicked`, `width`, `height`, `size`.
function kit.icon_button(spec)
  return widgets.icon(copy(spec))
end

--- A slider: `id`, `width`, `height` (the bar's, 44), `value` (fn, 0..1),
--- `set(v)`, `icon`, `label` (false hides the reading).
function kit.slider(spec)
  local s = copy(spec)
  s.bar_height = spec.height or 44
  s.height = s.bar_height + 8
  s.value, s.set = spec.value, nil
  s.on_moved = spec.set
  return widgets.slider(s)
end

--- The media position, and seeking it: `width`, `value` (fn, 0..1),
--- `seek(v)`, `active` and `playing` (fns).
function kit.media_progress(spec)
  local s = copy(spec)
  s.height = 34
  s.id = s.id or "media-seek"
  s.on_moved, s.seek = spec.seek, nil
  return widgets.seek_bar(s)
end

return kit
