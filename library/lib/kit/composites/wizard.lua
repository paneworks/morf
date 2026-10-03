-- A wizard, or stepper (composite: Navigation wizard + Selection header).
--
--     local node, wiz = composites.wizard {
--       id = "setup", width = 520, height = 340,
--       steps = {
--         { title = "Account", content = function(w, h) return form end,
--           validate = function() if name == "" then return false, "Enter a name" end return true end },
--         { title = "Theme", content = ... },
--         { title = "Done", content = ... },
--       },
--       on_finish = function() end, on_cancel = function() end,   -- on_cancel: a Cancel press
--       on_changed = function(index) end,
--     }
--     wiz.next() ; wiz.back() ; wiz.go(2) ; wiz.current() ; wiz.finished()
--
-- The header is a kit `stepper_header` (a Selection: each step's number,
-- or a tick once passed, and its title; the arrows walk it). The pages are
-- a kit `wizard` (a Navigation: each built when first shown, the next
-- sliding in from its side; Alt+Left goes back). Next and Finish ask the
-- step's `validate` first: false (and a message) keeps it there and says
-- why. Going on through the header asks every step on the way; going back
-- never asks. Alt+Right is Next. Ids: `<id>-header`, `<id>-step-<i>`,
-- `<id>-pages`, `<id>-page-<i>`, `<id>-back`, `<id>-next`, `<id>-finish`,
-- `<id>-cancel`, `<id>-error`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local W, H = spec.width or 520, spec.height or 340
  local HEAD = spec.header_height or 48
  local FOOT = 48
  local PH = H - HEAD - FOOT
  local steps = spec.steps or {}
  local n = #steps
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local st = morf.state { current = math.max(1, math.min(n, spec.current or 1)), reached = 1, error = "",
    tick = 0, finished = false }
  st.reached = st.current

  local names, pages, index_of = {}, {}, {}
  for i, step in ipairs(steps) do
    names[i] = "p" .. i
    index_of[names[i]] = i
    pages[names[i]] = function()
      local content = step.content
      if type(content) == "function" then content = content(W, PH) end
      local holder = ui.Item { id = sid("page-" .. i), width = W, height = PH }
      if content then ui.reparent(content, holder)
      else ui.reparent(kit.subtitle { anchors = { center_in = true }, text = step.title or "" }, holder) end
      return holder
    end
  end

  local nav_node, nav
  local function check(i)
    local step = steps[i]
    if not (step and step.validate) then return true end
    local ok, message = step.validate()
    if ok == false then
      st.error = message or "This step is not complete"
      return false
    end
    return true
  end
  local function show(i)
    st.current = i
    st.reached = math.max(st.reached, i)
    st.error = ""
    if nav then nav.go(names[i]) end
    if spec.on_changed then spec.on_changed(i) end
  end
  -- Forward asks every step on the way; back goes.
  local function go(i)
    if i < 1 or i > n or i == st.current then return false end
    if i > st.current then
      for k = st.current, i - 1 do
        if not check(k) then
          if k ~= st.current then show(k) end
          st.tick = st.tick + 1
          return false
        end
      end
    end
    show(i)
    return true
  end
  local function finish()
    for k = 1, n do
      if not check(k) then
        if k ~= st.current then show(k) end
        return false
      end
    end
    st.finished = true
    st.error = ""
    if spec.on_finish then spec.on_finish() end
    return true
  end

  nav_node, nav = widgets.wizard { id = sid("pages"), accessible_name = spec.accessible_name or "Steps",
    y = HEAD, width = W, height = PH, mode = "wizard", order = names, pages = pages,
    current = names[st.current],
    -- Alt+Left (the Navigation's back) lands here.
    on_current_changed = function(name)
      local i = index_of[name]
      if i and i ~= st.current then st.current = i st.error = "" if spec.on_changed then spec.on_changed(i) end end
    end }

  local items = {}
  for i, step in ipairs(steps) do items[i] = { label = step.title or ("Step " .. i) } end
  local SW = math.floor(W / math.max(1, n))
  local header = widgets.stepper_header { id = sid("header"), accessible_name = "Steps",
    items = items, item_width = SW, item_height = HEAD - 8, gap = 0, y = 4,
    current = function() local _ = st.tick return st.current end,
    item_id = function(i) return sid("step-" .. i) end,
    on_current_changed = function(i) go(i) end,
    delegate = function(i, item, s)
      local function done() return i < st.current or (st.finished and true or false) end
      return ui.Item { anchors = { fill = true },
        kit.surface { x = 10, anchors = { vertical_center = true }, width = 26, height = 26, radius = kit.round(13),
          border_width = function() return s.current() and 2 or 0 end, border_color = kit.signal("accent"),
          color = function()
            local c = kit.signal("accent")()
            if s.current() or done() then return c:alpha(0.22) end
            return c:alpha(i <= st.reached and 0.14 or 0.06)
          end,
          kit.text { anchors = { center_in = true }, text = function() return done() and "" or tostring(i) end,
            font_size = 13, font_weight = 700,
            color = function() return (s.current() and kit.ink("accent") or kit.ink("lo"))() end },
          kit.icon("check", 16, kit.ink("accent"), { anchors = { center_in = true }, visible = done }) },
        kit.text { x = 44, anchors = { vertical_center = true }, width = SW - 52, elide = "right",
          text = item.label, font_weight = 500,
          color = function() return (s.current() and kit.ink("hi") or kit.ink("lo"))() end } }
    end }

  local footer = ui.Item { y = H - FOOT, width = W, height = FOOT,
    kit.text { id = sid("error"), x = 8, anchors = { vertical_center = true }, width = W - 300, elide = "right",
      wrap = false, text = function() return st.error end, color = kit.signal("alert") },
    ui.Row { anchors = { right = true, vertical_center = true }, gap = 8,
      spec.on_cancel and widgets.flat { id = sid("cancel"), label = "Cancel", width = 84, height = 36,
        on_clicked = function() spec.on_cancel() end } or ui.Item { width = 1, height = 1 },
      widgets.push { id = sid("back"), label = "Back", width = 84, height = 36,
        enabled = function() return st.current > 1 end,
        opacity = function() return st.current > 1 and 1 or 0.4 end,
        on_clicked = function() go(st.current - 1) end },
      widgets.suggested { id = sid("next"), label = "Next", width = 96, height = 36,
        visible = function() return st.current < n end,
        on_clicked = function() go(st.current + 1) end },
      widgets.suggested { id = sid("finish"), label = "Finish", width = 96, height = 36,
        visible = function() return st.current >= n end,
        on_clicked = function() finish() end } } }

  local root = ui.Item { id = id, width = W, height = H,
    shortcuts = { ["alt+Right"] = function() if st.current >= n then return false end go(st.current + 1) end },
    header, kit.separator and kit.separator { y = HEAD, width = W } or ui.Item {}, nav_node, footer }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { node = root, nav = nav }
  function handle.next() return go(st.current + 1) end
  function handle.back() return go(st.current - 1) end
  function handle.go(i) return go(i) end
  function handle.finish() return finish() end
  function handle.current() return st.current end
  function handle.finished() return st.finished end
  function handle.error() return st.error end
  return root, handle
end
