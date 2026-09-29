-- GAME-inspired title decode and registration lights. See REFERENCE.md.
-- One finite decode on appearance/text change; no idle glitch polling.
local morf = require("morf")
local ui = require("morf.ui")
local symbols = { "!", "<", ">", "-", "_", "/", "[", "]", "{", "}", "=", "+", "*", "^", "?", "#" }
return function(theme, kit, props)
  local C, source = theme.color, props.text
  local active = props.active or function() return true end
  local pace = 0.5
  local tick = 40 * pace
  local delay = (props.reveal_delay or 560) * pace
  local lead, stagger = (props.decode_lead or 650) * pace, (props.decode_stagger or 80) * pace
  local size = props.font_size or 14
  local id = assert(props.id, "animated headings need a stable id")
  local function caption()
    local value = source
    if type(value) == "function" then value = value() end
    return tostring(value or ""):upper()
  end
  local height = props.height or math.ceil(size * 1.5)
  local width = props.width or function() return utf8.len(caption()) * size * 0.65 + 18 end
  local function label(suffix, color)
    return kit.text { id = id .. suffix, x = 16, width = function()
      return math.max(0, (type(width) == "function" and width() or width) - 16)
    end, height = height, font_size = size, font_weight = props.font_weight or 500,
      horizontal_alignment = props.horizontal_alignment, vertical_alignment = props.vertical_alignment,
      elide = props.elide or "right", text = caption(), color = color }
  end
  local a = label("-ghost-a", function() return C.secondary end)
  local b = label("-ghost-b", function() return C.tertiary end)
  local title = label("-text", props.color or function() return C.primary end)
  local pip = ui.Rect { id = id .. "-light", x = 0, y = math.floor((height - 5) / 2),
    width = 5, height = 5, opacity = 0.3, color = props.color or function() return C.primary end }
  a.opacity, b.opacity = 0, 0
  local node = ui.Item { id = id, x = props.x, y = props.y, anchors = props.anchors,
    width = width, height = height, visible = props.visible, a, b, title, pip }
  local timer, flash
  local elapsed, duration, letters, final = 0, 0, {}, ""
  local function finish()
    timer.running = false
    if flash then flash:finish() flash = nil end
    title.text, a.text, b.text = final, final, final
    a.opacity, b.opacity, pip.opacity = 0, 0, 0.3
  end
  local function paint()
    local output, t = {}, elapsed - delay
    for i, char in ipairs(letters) do
      if char:match("%s") or t >= lead + math.min(i - 1, 9) * stagger then
        output[i] = char
      else
        output[i] = symbols[(i * 7 + math.floor(math.max(0, t) / tick) * 11) % #symbols + 1]
      end
    end
    local value = table.concat(output)
    title.text, a.text, b.text = value, value, value
  end
  timer = ui.Timer { id = id .. "-decode", interval = tick, ["repeat"] = true, running = false,
    on_triggered = function()
      elapsed = elapsed + tick
      if elapsed >= duration then finish() else paint() end
    end }
  ui.reparent(timer, node)
  local was, previous = false, nil
  local function in_view()
    local viewports = props.viewports or (props.viewport and {props.viewport}) or {}
    if #viewports == 0 then return true end
    -- Read our geometry before the parent exists; its first layout re-runs
    -- this effect after the enclosing viewport has finished construction.
    local x, y = node.layout_x, node.layout_y
    if not x or not y then return false end
    local right, bottom = x + (node.layout_width or 0), y + (node.layout_height or 0)
    for _, get_view in ipairs(viewports) do
      local view = get_view()
      if not view then return false end
      local vx, vy = view.layout_x, view.layout_y
      if not vx or not vy then return false end
      x, y = math.max(x, vx), math.max(y, vy)
      right = math.min(right, vx + (view.layout_width or 0))
      bottom = math.min(bottom, vy + (view.layout_height or 0))
      if right <= x or bottom <= y then return false end
    end
    return true
  end
  morf.effect(id .. ".appearance", function()
    local visible = props.visible
    if type(visible) == "function" then visible = visible() end
    local on, value = active() and visible ~= false and in_view(), caption()
    if on == was and value == previous then return end
    was, previous, final = on, value, value
    finish()
    if not on or value == "" then return end
    letters = {}
    for _, code in utf8.codes(value) do letters[#letters + 1] = utf8.char(code) end
    elapsed, duration = 0, delay + lead + math.min(#letters - 1, 9) * stagger
    paint()
    timer.running = true
    -- Small split-color flash and two registration pulses; native channels
    -- keep this out of the Lua timer and stop automatically at rest.
    local tracks = {}
    for i, ghost in ipairs { a, b } do
      tracks[#tracks + 1] = { node = ghost, property = "translate_x", from = i == 1 and -3 or 3,
        to = 0, delay = delay, duration = 280 * pace, easing = "out_cubic" }
      tracks[#tracks + 1] = { node = ghost, property = "opacity", delay = delay, duration = 280 * pace,
        keyframes = { {at=0,value=0}, {at=0.25,value=0.65}, {at=1,value=0} } }
    end
    tracks[#tracks + 1] = { node = pip, property = "opacity", delay = delay, duration = 600 * pace,
      keyframes = { {at=0,value=0.3}, {at=0.1,value=1}, {at=0.3,value=0.3},
        {at=0.6,value=0.3}, {at=0.7,value=1}, {at=1,value=0.3} } }
    flash = morf.animation.play { { parallel = tracks } }
  end, { owner = node })
  return node
end
