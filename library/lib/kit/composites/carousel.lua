-- A carousel (composite: Navigation carousel + Selection dots + Drag swipe).
--
--     local node, car = composites.carousel {
--       id = "tips", width = 520, height = 300,
--       slides = {
--         { title = "Welcome", subtitle = "Swipe to go on", icon = "waving_hand" },
--         { content = function(w, h) return my_node end },   -- a node or a builder
--       },
--       current = 1, wrap = false,
--       on_changed = function(index) end,
--     }
--     car.next() ; car.previous() ; car.go(3) ; car.current()
--
-- The slides are a kit `carousel` (a Navigation: each slide is built
-- when first shown and slides in from its side; Left and Right walk it
-- once it has focus). A swipe across the slide (a Drag swipe: the slide
-- follows the finger and springs back when let go short) turns it; the
-- arrows at its sides and the dots under it (a kit `carousel_dots`
-- Selection) go too. Ids: `<id>-slides`, `<id>-slide-<i>`,
-- `<id>-swipe`, `<id>-previous`, `<id>-next`, `<id>-dots`, `<id>-dot-<i>`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")
local drag = require("lib.kit.drag")

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 300
  local DOTS = 28
  local SH = H - DOTS
  local slides = spec.slides or {}
  local n = #slides
  local wrap = spec.wrap == true
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local st = morf.state { current = math.max(1, math.min(n, spec.current or 1)) }

  local names, pages = {}, {}
  for i, slide in ipairs(slides) do
    local name = "s" .. i
    names[i] = name
    pages[name] = function()
      local content = slide.content
      if type(content) == "function" then content = content(W, SH) end
      local holder = ui.Item { id = sid("slide-" .. i), width = W, height = SH }
      if content then
        ui.reparent(content, holder)
      else
        ui.reparent(ui.Item { anchors = { fill = true, margins = 8 },
          kit.card { anchors = { fill = true } },
          kit.surface { anchors = { fill = true }, radius = kit.round(16),
            color = function() return kit.signal(slide.kind or "accent")():alpha(0.10) end },
          (function()
            local column = { anchors = { center_in = true }, gap = 10, align = "center" }
            if slide.icon then column[#column + 1] = kit.icon(slide.icon, 44, kit.signal(slide.kind or "accent")) end
            column[#column + 1] = kit.heading { text = slide.title or "" }
            if slide.subtitle then
              column[#column + 1] = kit.subtitle { text = slide.subtitle, width = W - 140, wrap = true,
                horizontal_alignment = "center" }
            end
            return ui.Column(column)
          end)() }, holder)
      end
      return holder
    end
  end
  local index_of = {}
  for i, name in ipairs(names) do index_of[name] = i end

  local nav_node, nav
  nav_node, nav = widgets.carousel { id = sid("slides"), accessible_name = spec.accessible_name or "Slides",
    width = W, height = SH, mode = "carousel", order = names, pages = pages, wrap = wrap,
    current = names[st.current], focus_policy = "strong",
    on_current_changed = function(name)
      local i = index_of[name]
      if i and i ~= st.current then
        st.current = i
        if spec.on_changed then spec.on_changed(i) end
      end
    end }
  local function go(i)
    if i < 1 or i > n then
      if not wrap or n == 0 then return end
      i = (i - 1) % n + 1
    end
    nav.go(names[i])
  end
  local function step(by)
    if by > 0 then nav.next() else nav.previous() end
  end

  -- The swipe: over the slide, the slide following it.
  local swipe = ui.MouseArea { id = sid("swipe"), width = W, height = SH, z = 1, cursor = "grab",
    accessible_hidden = true }
  local behaviour = drag.swipe(swipe, { axis = "x", distance = spec.swipe_distance or math.min(120, W / 5),
    on_pressed = function() morf.focus.set(nav_node, true) end,
    on_swiped = function(direction) step(direction == "left" and 1 or -1) end })
  swipe.translate_x = 0
  nav_node.translate_x = function() return behaviour.t.delta_x * 0.6 end

  local function arrow(dir)
    local can = function() return wrap or (dir < 0 and st.current > 1) or (dir > 0 and st.current < n) end
    return widgets.icon { id = sid(dir < 0 and "previous" or "next"),
      accessible_name = dir < 0 and "Previous slide" or "Next slide", width = 36, height = 36, size = 22,
      icon_off = dir < 0 and "chevron_left" or "chevron_right", z = 2,
      x = dir < 0 and 8 or W - 44, y = SH / 2 - 18,
      enabled = can, opacity = function() return can() and 1 or 0.35 end,
      on_clicked = function() step(dir) end }
  end

  local dot_w = 22
  local items = {}
  for i, slide in ipairs(slides) do items[i] = { label = slide.title or ("Slide " .. i) } end
  local dots = widgets.carousel_dots { id = sid("dots"), accessible_name = "Slide",
    items = items, item_width = dot_w, item_height = 20, gap = 2,
    x = math.floor((W - n * (dot_w + 2)) / 2), y = SH + (DOTS - 20) / 2,
    current = function() return st.current end,
    item_id = function(i) return sid("dot-" .. i) end,
    on_current_changed = function(i) go(i) end,
    delegate = function(_, _, s)
      return ui.Item { anchors = { fill = true },
        kit.surface { anchors = { center_in = true }, radius = kit.round(4),
          width = function() return s.current() and 16 or 8 end, height = 8,
          behavior = { width = { duration = 200, easing = "out_cubic" } },
          color = function()
            local c = kit.signal("accent")()
            return s.current() and c or c:alpha(s.hovered() and 0.55 or 0.3)
          end } }
    end }

  local root = ui.Item { id = id, width = W, height = H,
    ui.Item { width = W, height = SH, clip = true, nav_node, swipe, arrow(-1), arrow(1) }, dots }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, nav = nav }
  function handle.next() step(1) end
  function handle.previous() step(-1) end
  function handle.go(i) go(i) end
  function handle.current() return st.current end
  return root, handle
end
