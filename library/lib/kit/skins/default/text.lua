-- The default kit's type: the `label` display widget, and the two helpers
-- every other file writes with -- `draw.text` (a `ui.Text` in the kit's face
-- and ink) and `draw.icon` (a symbolic icon by name, in the icon face).
--
-- A label's variants are libadwaita's type classes (title_1 .. title_4,
-- heading, body, caption, caption_heading, monospace, numeric) and its
-- colour classes (dim, accent, success, warning, error).
local ui = require("morf.ui")

local TYPE = { "title_1", "title_2", "title_3", "title_4", "heading", "body", "caption", "caption_heading",
  "monospace", "numeric" }
local TONE = { dim = "dim_fg", accent = "accent", success = "success", warning = "warning", error = "error" }

return function(S, theme, draw)
  local P = theme.P
  local press = require("lib.kit.archetypes.press")

  --- Text in the kit's face; `props` as a `ui.Text`'s.
  function draw.text(props)
    props.font_family = props.font_family or theme.font
    props.font_size = props.font_size or theme.size.normal
    if props.color == nil then props.color = function() return P().window_fg end end
    return ui.Text(props)
  end

  --- A symbolic icon by name (`"wifi_off"`), in the icon face; `props.fill`
  --- (a boolean or a binding) fills it in.
  function draw.icon(name, size, color, props)
    props = props or {}
    props.text = name
    if props.accessible_name == nil and props.accessible_hidden == nil then props.accessible_hidden = true end
    props.font_family = theme.icon_font
    props.font_size = size or 16
    props.color = color or function() return P().window_fg end
    local fill = props.fill
    props.fill = nil
    props.axes = function()
      local on = fill
      if type(fill) == "function" then on = fill() end
      return { FILL = on and 1 or 0, wght = 400, GRAD = 0 }
    end
    return ui.Text(props)
  end

  --- GtkLabel: `text` and a `ui.Text`'s props; `variant` its classes.
  function S.label(spec)
    local props = {}
    for k, v in pairs(spec) do
      if k ~= "variant" and k ~= "widget" then props[k] = v end
    end
    for _, class in ipairs(TYPE) do
      if press.has(spec, class) then
        local face = theme.type[class]
        props.font_size = props.font_size or face.size
        props.font_weight = props.font_weight or face.weight
        props.font_family = props.font_family or face.family
        if class == "title_1" or class == "title_2" or class == "title_3" or class == "title_4" or class == "heading" then
          props.accessible_role = props.accessible_role or "heading"
        end
      end
    end
    for class, role in pairs(TONE) do
      if press.has(spec, class) and props.color == nil then props.color = function() return P()[role] end end
    end
    return draw.text(props)
  end
end
