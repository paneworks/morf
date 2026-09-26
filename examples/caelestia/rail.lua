-- The rail: the workspaces again, down the frame's right edge, as a line
-- of pills -- ten of them, the group of ten the active workspace is in.
-- When the workspace changes, its pill pops out of the frame into the
-- opening as a numbered bud, slides to the new workspace, holds a moment
-- and tucks back into the line. The bud is a distance field with the pill
-- it left, joined only while it moves, so it leaves the frame like a drop
-- and is crisp at rest.
--
-- Laid out as the author's own quickshell ribbon: the track half the
-- screen's height, centred; pills 10 px apart on a 2160 px screen, scaled.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local config = require("config")
local services = require("services")

local C = theme.color
local M = {}

M.COUNT = 10
local PILL_W = 6   -- inside the frame's 10 px border
local BLEND = 6    -- the join between the bud and its pill, while it moves

local function screen()
  morf.screens_revision()
  local s = morf.screens[1]
  return (s and s.width) or 1920, (s and s.height) or 1080
end

--- The rail's measures on this screen.
function M.geometry()
  local w, h = screen()
  local scale = math.min(w, h) / 2160
  local track = math.floor(h * 0.5)
  local gap = math.max(3, math.floor(10 * scale + 0.5))
  local item = (track - gap * (M.COUNT - 1)) / M.COUNT
  return {
    w = w, h = h,
    top = math.floor((h - track) / 2),
    gap = gap, item = item,
    -- The pill, centred in the right border; the bud, a disc the pill's
    -- height, clear of the border by a gap.
    pill_x = w - theme.BORDER / 2 - PILL_W / 2,
    bud_x = w - theme.BORDER - math.max(8, math.floor(14 * scale)) - item,
  }
end

-- The first workspace of the group of ten `id` is in: 1, 11, 21, ...
local function base(id) return id - ((id - 1) % M.COUNT) end
local function slot_y(g, id) return g.top + ((id - 1) % M.COUNT) * (g.item + g.gap) end

function M.build()
  local enabled = function() return config.get("rail.enabled") ~= false end
  local covered = function() return require("osd").drawer.open:get() end

  local pills = {}
  for i = 1, M.COUNT do
    local function id() return base(services.workspace.active()) + i - 1 end
    local pill = ui.Rect {
      id = "rail-pill-" .. i,
      x = (theme.BORDER - PILL_W) / 2, width = PILL_W,
      height = function() return M.geometry().item end,
      radius = PILL_W / 2,
      color = function()
        if services.workspace.active() == id() then return C.primary end
        return services.workspace.occupied(id()) and C.onSurfaceVariant or C.outlineVariant
      end,
      behavior = { color = { duration = 200 }, opacity = { duration = 200 } },
    }
    local area = ui.MouseArea {
      id = "rail-slot-" .. i,
      cursor = "pointer",
      x = function() return M.geometry().w - theme.BORDER end,
      y = function() local g = M.geometry() return g.top + (i - 1) * (g.item + g.gap) end,
      width = theme.BORDER,
      height = function() return M.geometry().item end,
      on_clicked = function() services.workspace.go(id()) end,
      on_wheel = function(_, _, _, _, _, step_y)
        if step_y ~= 0 then services.workspace.step(step_y > 0 and 1 or -1) end
      end,
      pill,
    }
    -- Bound once the area exists: it reads the area's hover.
    pill.opacity = function()
      if services.workspace.active() == id() then return 1 end
      return area.hovered and 0.9 or 0.6
    end
    pills[i] = area
  end

  -- The bud and the pill it came out of: two boxes the field draws, each
  -- following an item that is moved (`track`); the number rides the bud.
  local g0 = M.geometry()
  local stem = ui.Item { id = "rail-stem", x = g0.pill_x, y = g0.top, width = PILL_W, height = g0.item }
  local label = kit.text {
    id = "rail-number",
    anchors = { center_in = true },
    text = function() return tostring(services.workspace.active()) end,
    font_size = math.max(12, math.floor(g0.item * 0.45)),
    font_weight = 800,
    color = function() return C.onPrimary end,
    opacity = 0,
  }
  local bud = ui.Item { id = "rail-bud", x = g0.pill_x, y = g0.top, width = PILL_W, height = g0.item, label }

  local shown = morf.signal("caelestia.rail.shown", false)
  local moving = morf.signal("caelestia.rail.moving", false)
  local field = ui.Sdf {
    id = "rail-field",
    anchors = { fill = true },
    fill_color = function() return C.primary end,
    opacity = function() return shown:get() and 1 or 0 end,
    blend = function() return moving:get() and BLEND or 0 end,
    behavior = { blend = { duration = 120 } },
    ui.SdfShape { shape = "box", radius = 9999, track = stem },
    ui.SdfShape { shape = "box", radius = 9999, track = bud, operation = "smooth_union" },
  }

  -- --------------------------------------------------------- motion --

  local morph, slide, hide
  local last -- the workspace the bud last showed

  local function stop(handle) if handle then handle:stop() end end

  local function tuck()
    local g = M.geometry()
    stop(morph)
    moving:set(true)
    morph = morf.animation.play {
      {
        parallel = {
          { node = bud, property = "x", to = g.pill_x, duration = 300, easing = "in_cubic" },
          { node = bud, property = "width", to = PILL_W, duration = 300, easing = "in_cubic" },
          { node = label, property = "opacity", to = 0, duration = 180, easing = "in_cubic" },
        },
      },
      on_finished = function(reason)
        if reason ~= "completed" then return end
        morph = nil
        moving:set(false)
        shown:set(false)
      end,
    }
  end

  local function pop(id)
    local g = M.geometry()
    local y = slot_y(g, id)
    local was_out = shown:get()
    if not was_out then
      -- Out of the pill it was last, or straight out of this one.
      local from = (last and base(last) == base(id)) and slot_y(g, last) or y
      stop(slide)
      for _, n in ipairs { stem, bud } do n.y = from n.height = g.item end
      bud.x, bud.width = g.pill_x, PILL_W
      stem.x = g.pill_x
      shown:set(true)
    end
    if not was_out or morph then
      -- Coming out, or called back while tucking in.
      stop(morph)
      moving:set(true)
      morph = morf.animation.play {
        {
          parallel = {
            { node = bud, property = "x", to = g.bud_x, duration = 400, easing = "out_cubic" },
            { node = bud, property = "width", to = g.item, duration = 400, easing = "in_out_cubic" },
            { node = label, property = "opacity", to = 1, duration = 260, delay = 120 },
          },
        },
        on_finished = function(reason)
          if reason ~= "completed" then return end
          morph = nil
          if not (slide and slide:active()) then moving:set(false) end
        end,
      }
    end
    if bud.y ~= y then
      stop(slide)
      moving:set(true)
      slide = morf.animation.play {
        {
          parallel = {
            { node = bud, property = "y", to = y, duration = 250, easing = "in_out_quad" },
            { node = stem, property = "y", to = y, duration = 250, easing = "in_out_quad" },
          },
        },
        on_finished = function(reason)
          if reason ~= "completed" then return end
          slide = nil
          if not morph then moving:set(false) end
        end,
      }
    end
    last = id
    if hide then hide:cancel() end
    hide = morf.timer(config.get("rail.hold") or 800, function()
      hide = nil
      tuck()
    end, false)
  end

  local first = true
  morf.effect("caelestia.rail.follow", function()
    local id = services.workspace.active()
    if first then first = false last = id return end
    if not enabled() or id == last then return end
    pop(id)
  end)

  return ui.Item {
    id = "rail",
    anchors = { fill = true },
    visible = enabled,
    -- The OSD comes out of the same stretch of the edge: the rail makes way.
    opacity = function() return covered() and 0 or 1 end,
    behavior = { opacity = { duration = 150 } },
    ui.Item { anchors = { fill = true }, table.unpack(pills) },
    stem,
    field,
    bud,
  }
end

return M
