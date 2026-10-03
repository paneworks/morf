-- Scrolled views (the Scroll archetype) around the engine's flickable.
--
--     local node, flick = scroll.make("scroll_view", {
--       id = "page", width = 400, height = 300, clip = true,
--       ui.Column { ... },                        -- the content
--     })
--
-- `node` is the control, the one to place -- it takes no pointer input of
-- its own --; `flick` the `ui.Flickable`
-- inside it, which keeps the spec's `id` and flickable properties
-- (`content_y`, `interactive`, ...). The archetype follows where it has
-- scrolled (`t.position_y`, `t.size_y`, `t.at_end`, ...), answers the
-- arrows, Page keys, Home, End and Space, and snaps (`snap = "items" |
-- "pages"`); the skin draws `scroll_bar_x`/`scroll_bar_y` from that,
-- `edge_fade` and `overscroll`. `scroll_policy_x`/`_y` say when a bar
-- shows. `on_scrolled(x, y)`, `on_reached_start`, `on_reached_end` follow
-- it.
local ui = require("morf.ui")
local control = require("lib.kit.control")

local M = {}

local CONTROL = { x = true, y = true, z = true, anchors = true, visible = true, opacity = true, layout = true,
  width = true, height = true }
local SETTINGS = { content = true, scroll_policy_x = true, scroll_policy_y = true, snap = true, item_size = true, step = true,
  widget = true, on_scrolled = true, on_reached_start = true, on_reached_end = true, focus_policy = true }

local DEFAULTS = { pager = { snap = "pages" }, shelf = { scroll_policy_y = "never" } }

function M.make(widget, spec)
  local given = spec or {}
  spec = {}
  for k, v in pairs(DEFAULTS[widget] or {}) do spec[k] = v end
  for k, v in pairs(given) do spec[k] = v end
  local flick_props, control_props = { anchors = { fill = true } }, {}
  for k, v in pairs(spec) do
    if type(k) == "number" then flick_props[k] = v
    elseif CONTROL[k] then control_props[k] = v
    elseif not SETTINGS[k] then flick_props[k] = v end
  end
  local flick = ui.Flickable(flick_props)
  local settings = { widget = widget, id = spec.id and (spec.id .. "-scroll") or nil, flick = flick }
  for k, v in pairs(spec) do if SETTINGS[k] and type(k) == "string" then settings[k] = v end end
  settings.on_scroll_to = function(x, y) flick.content_x, flick.content_y = x, y end
  -- Not a target for the pointer: presses go to what is in it, or under
  -- it, and the flickable scrolls itself. Keys reach it once it has focus
  -- by a policy that says so; its scroll bar, a Range, is reached by Tab.
  control_props.focus_policy = spec.focus_policy or "none"
  control_props.accepted_buttons = {}
  local root, t, ctl = control.make("Scroll", widget, settings, { children = { flick }, props = control_props })
  -- Where the flickable is, for the archetype (and the skin through it).
  -- The content's size is its children's, as laid out.
  -- (Or what `spec.content` names, or `set_content` hands it later: content
  -- reparented into the flickable after it was made.)
  local content = {}
  for _, child in ipairs(spec) do content[#content + 1] = child end
  if spec.content then content = { spec.content } end
  local generation = morf.signal("kit.scroll.content." .. tostring(flick), 0)
  morf.effect("kit.scroll.geometry." .. ctl.id, function()
    generation:get()
    local w, h = 0, 0
    for _, child in ipairs(content) do
      w = math.max(w, (child.layout_width or 0) + (tonumber(child.x) or 0))
      h = math.max(h, (child.layout_height or 0) + (tonumber(child.y) or 0))
    end
    ctl.send("geometry", flick.content_x or 0, flick.content_y or 0, w, h,
      root.layout_width or 0, root.layout_height or 0)
  end, { owner = root })
  ctl.set_content = function(node)
    content = { node }
    generation:set(generation:get() + 1)
  end
  return root, flick, t, ctl
end

return M
