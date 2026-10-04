-- A tour (coachmarks): a card beside each thing it points out in turn,
-- with a ring round that thing, Back / Next / Skip and a dot per step
-- (Popup anchored to its targets + Navigation, a wizard of steps).
--
--     local node, tour = composites.tour {
--       id = "tour", root = surface_root,
--       steps = {
--         { target = search_field, title = "Search", body = "Find anything from here." },
--         { target = function() return sidebar end, title = "Sidebar", body = "..." },
--       },
--       on_finished = function() end, on_skipped = function(step) end,
--     }
--     tour.start() tour.next() tour.back() tour.skip() tour.step() tour.is_open()
--
-- Each step's card opens beside its `target` (a node, or a function
-- returning one) by the tour's `placement` ("bottom" -- it flips where
-- there is no room); a step without a target is centred on `root`. A ring, the theme's
-- accent, is drawn round the target over everything. Next on the last
-- step finishes; Skip or Escape ends it early. Right / Left (and Return
-- for Next) walk it while the card has focus. `inline = true` returns
-- the card as a node to place (it points at nothing: a gallery's, or a
-- page's onboarding). Other fields: `width` (300), `ring_padding` (6),
-- `labels` ({ next, back, skip, done }). Ids: `<id>-card`, `<id>-steps`,
-- `<id>-next`, `<id>-back`, `<id>-skip`, `<id>-dot-<n>`, `<id>-ring`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local control = require("lib.kit.control")
local navigation = require("lib.kit.navigation")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

local serial = 0

local function make(spec)
  spec = spec or {}
  local kit = K()
  serial = serial + 1
  local key = "kit.composites.tour." .. tostring(spec.id) .. "." .. serial
  local id = spec.id
  local steps = spec.steps or {}
  local N = #steps
  local W = spec.width or 300
  local PAD = 16
  local BODY_H = spec.body_height or 92
  local FOOT_H = 36
  local H = PAD + BODY_H + 12 + FOOT_H + PAD
  local labels = spec.labels or {}
  local step = morf.signal(key .. ".step", 1)
  local running = morf.signal(key .. ".running", false)
  local nav, card, card_popup, ring, ring_shown
  -- Closes of the card asked for here (a step moving it, the end), which
  -- its popup reports later: the layer closes at its next turn.
  local expected = 0

  local names = {}
  for n = 1, N do names[n] = "step" .. n end

  local function target_of(n) local s = steps[n] return s and get(s.target) or nil end

  -- ------------------------------------------------------------- the ring --
  local RP = spec.ring_padding or 6
  local function ring_node()
    if ring then return ring end
    local function tw() local t = target_of(step:get()) return t and (t.layout_width or t.width or 0) or 0 end
    local function th() local t = target_of(step:get()) return t and (t.layout_height or t.height or 0) or 0 end
    ring = ui.Item { id = id and (id .. "-ring") or nil,
      width = function() return tw() + 2 * RP end, height = function() return th() + 2 * RP end,
      accessible_hidden = true,
      -- A soft halo that breathes out from the ring, and the ring itself.
      kit.surface { anchors = { fill = true, margins = -4 }, radius = kit.round and kit.round(18) or 18,
        color = "transparent", border_width = 4,
        border_color = function() return kit.signal("accent")():alpha(0.25) end },
      kit.surface { anchors = { fill = true }, radius = kit.round and kit.round(14) or 14, color = "transparent",
        border_width = 2, border_color = kit.signal("accent") } }
    return ring
  end

  local finish
  local function close_all(reason)
    if card_popup and card_popup.is_open() then
      expected = expected + 1
      card_popup.close(reason or "closed")
    end
    if ring and ring_shown then morf.overlay.close(ring) ring_shown = false end
  end

  local function open_at(n)
    local target = target_of(n)
    if target then
      morf.overlay.open(ring_node(), { anchor = target, placement = "center", gap = 0, focus = false,
        escape = false, outside = false, modal = false })
      ring_shown = true
    end
    card_popup.open(target)
  end

  -- The layer closes at its next turn, and an overlay is anchored as it
  -- opens: a step that moves the card closes it and the ring, and opens
  -- them beside the next target once they have gone.
  local function place(n)
    if spec.inline then return end
    if card_popup.is_open() then
      close_all("moved")
      morf.timer(16, function() if running:get() and step:get() == n then open_at(n) end end, false)
    else
      open_at(n)
    end
  end

  local function go(n)
    n = math.max(1, math.min(N, n))
    step:set(n)
    if nav then nav.go(names[n]) end
    if running:get() then place(n) end
  end

  finish = function(how)
    if not running:get() then return end
    running:set(false)
    close_all(how)
    if how == "finished" then
      if spec.on_finished then spec.on_finished() end
    elseif spec.on_skipped then
      spec.on_skipped(step:get())
    end
  end
  local function next_step() if step:get() >= N then finish("finished") else go(step:get() + 1) end end
  local function back_step() if step:get() > 1 then go(step:get() - 1) end end

  -- ------------------------------------------------------------- the card --
  local function page(n)
    local s = steps[n]
    return function()
      return ui.Column { width = W - 2 * PAD, gap = 6,
        kit.text { text = s.title or "", width = W - 2 * PAD, elide = "right", font_size = 17, font_weight = 600,
          color = kit.ink("hi"), accessible_role = "heading" },
        kit.text { text = s.body or "", width = W - 2 * PAD, wrap = true, color = kit.ink("lo") } }
    end
  end
  local function button(name, label, widget, on_clicked, visible)
    return (control.make("Press", widget, { widget = widget, id = id and (id .. "-" .. name) or nil, label = label,
      height = 34, width = math.max(64, 28 + math.ceil(utf8.len(get(label) or "") * 8)), on_clicked = on_clicked,
      visible = visible, accessible_name = label }))
  end

  local function build()
    if card then return card end
    local pages = {}
    for n = 1, N do pages[names[n]] = page(n) end
    local steps_node
    steps_node, nav = navigation.make("onboarding", {
      id = id and (id .. "-steps") or nil, x = PAD, y = PAD, width = W - 2 * PAD, height = BODY_H, mode = "wizard",
      order = names, pages = pages, current = names[step:get()] or names[1],
    })
    -- The dots: the current one stretched to a pill (and sprung there).
    local dots = {}
    for n = 1, N do
      dots[#dots + 1] = kit.surface { id = id and (id .. "-dot-" .. n) or nil, height = 8,
        width = function() return step:get() == n and 22 or 8 end, radius = 4,
        color = function()
          local c = kit.signal("accent")()
          return step:get() == n and c or c:alpha(0.3)
        end,
        behavior = { width = ui.spring { stiffness = 420, damping = 22 } } }
    end
    local last = function() return step:get() >= N end
    local foot = ui.Item { x = PAD, y = PAD + BODY_H + 12, width = W - 2 * PAD, height = FOOT_H,
      ui.Row { anchors = { vertical_center = true }, gap = 6, align = "center", table.unpack(dots) },
      ui.Row { anchors = { right = true, vertical_center = true }, gap = 6, align = "center",
        button("skip", labels.skip or "Skip", "menu_item", function() finish("skipped") end,
          function() return not last() end),
        button("back", labels.back or "Back", "menu_item", back_step, function() return step:get() > 1 end),
        button("next", function() return last() and (labels.done or "Done") or (labels.next or "Next") end, "pill",
          next_step) } }
    card = ui.Item { id = id and (id .. "-card") or nil, width = W, height = H, z = 1, focus_policy = "none",
      on_key_pressed = function(_, _, _, _, name)
        if name == "Right" then next_step() return true end
        if name == "Left" then back_step() return true end
        return false
      end,
      steps_node, foot }
    return card
  end

  local handle = {}
  local node
  if spec.inline then
    build()
    node = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = H, anchors = spec.anchors,
      kit.card { anchors = { fill = true } }, card }
    running:set(true)
  end
  -- Built when it first starts: until then the card would hang from nothing.
  local function ensure()
    if card_popup or spec.inline then return end
    build()
    card_popup = popup.make("tour_step", {
      id = id and (id .. "-popup") or nil, content = card, width = W, height = H, root = spec.root,
      placement = spec.placement or "bottom", close_policy = "escape", focus_on_open = true,
      on_closed = function(reason)
        if expected > 0 then expected = expected - 1 return end
        finish(reason == "escape" and "skipped" or reason)
      end,
    })
  end
  function handle.start(n)
    if N == 0 then return end
    ensure()
    running:set(true)
    go(n or 1)
  end
  handle.next, handle.back = next_step, back_step
  handle.skip = function() finish("skipped") end
  handle.step = function() return step:get() end
  handle.count = function() return N end
  handle.is_open = function() return running:get() end
  handle.node = node
  return node, handle
end

return { make = make }
