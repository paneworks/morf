-- The levels: the workspace rail's mirror down the frame's right edge, as
-- the author's quickshell had it -- two pills the size of the workspace
-- pills, centred, the output's volume above and the screen's brightness
-- below. When either changes (from anywhere: keys, another program), the
-- frame swells out beside its pill, as a drawer grows out of it, carrying
-- a disc the pill's height with the icon and the value, and the pill
-- lights; a moment after the last change it sinks back. Only to look at:
-- the pointer near them opens the sidebar, and they ride out with it to
-- its near edge, between the desk and the panel.
--
-- This is the OSD: `osd` over IPC, or osd.lua's follow, calls `pop`.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")

local C = theme.color
local M = {}

local PILL_W = 6
local HOLD = 1500

local function screen()
  morf.screens_revision()
  local s = morf.screens[1]
  return (s and s.width) or 1920, (s and s.height) or 1080
end

--- The pills' measures: the workspace rail's pills exactly -- their
--- height, gap and width -- two of them, centred down the edge; the bud a
--- disc their height, as the rail's.
function M.geometry()
  local r = require("rail").geometry()
  local track = 2 * r.item + r.gap
  local top = math.floor((r.h - track) / 2)
  local scale = math.min(r.w, r.h) / 2160
  return {
    w = r.w, h = r.h, gap = r.gap, pill_h = r.item, item = r.item,
    pill_x = r.w - theme.BORDER / 2 - PILL_W / 2,
    clear = math.max(8, math.floor(14 * scale)),
    tops = { volume = top, brightness = top + r.item + r.gap },
  }
end

local KINDS = { "volume", "brightness" }

function M.build()
  local osd = require("osd")
  local value = {
    volume = function() return math.max(0, math.min(1, (osd.volume()))) end,
    brightness = function() return math.max(0, math.min(1, (osd.brightness()))) end,
  }
  local icon = { volume = osd.volume_icon, brightness = osd.brightness_icon }
  local function muted() local _, m = osd.volume() return m end

  local shown = morf.signal("caelestia.levels.shown", "")
  local pills = {}
  for _, kind in ipairs(KINDS) do
    pills[#pills + 1] = ui.Rect {
      id = "levels-" .. kind,
      x = function() return M.geometry().pill_x end,
      y = function() return M.geometry().tops[kind] end,
      width = PILL_W, height = function() return M.geometry().pill_h end,
      radius = PILL_W / 2,
      color = function() return shown:get() == kind and C.primary or C.outlineVariant end,
      opacity = function() return shown:get() == kind and 1 or 0.6 end,
      behavior = { color = { duration = 200 }, opacity = { duration = 200 } },
    }
  end

  -- The swell: a box in the frame's field (`M.shape`, which init.lua adds
  -- to it), the frame bulging out leftwards beside a pill, at its level.
  local g0 = M.geometry()
  local D = g0.item
  local PAD = math.max(5, math.floor(D * 0.12))
  local SW, SH = D + 2 * PAD + g0.clear + theme.BORDER - PAD, D + 2 * PAD
  local function out_x() return M.geometry().w - SW end
  local function tucked_x() return M.geometry().w + theme.SEAM + 2 end
  -- The value, and its icon above it, in a disc the pill's height.
  local bud = ui.Rect {
    id = "levels-bud",
    x = PAD, y = PAD, width = D, height = D, radius = D / 2,
    color = function()
      if shown:get() == "volume" and muted() then return C.onSurfaceVariant end
      return C.primary
    end,
    behavior = { color = { duration = theme.duration.small } },
    ui.Column {
      anchors = { center_in = true }, gap = 0, align = "center",
      kit.icon(function()
        local k = shown:get()
        return k ~= "" and icon[k]() or "volume_up"
      end, math.max(16, math.floor(D * 0.3)), function() return C.onPrimary end),
      -- The value's digits morph from one reading to the next.
      kit.morph_number {
        id = "levels-value",
        value = function()
          local k = shown:get()
          if k == "" then return "" end
          return math.floor(value[k]() * 100 + 0.5)
        end,
        size = math.max(9, math.floor(D * 0.22)),
        color = function() return C.onPrimary end,
        duration = 220,
      },
    },
  }
  local swell = ui.Item {
    id = "levels-swell",
    x = tucked_x(), width = SW, height = SH,
    y = function()
      local k = shown:get()
      return M.geometry().tops[k ~= "" and k or "volume"] - PAD
    end,
    -- Eased, not sprung: a spring stepped by slow frames never settles.
    behavior = { y = { duration = 200, easing = theme.ease.standard } },
    bud,
  }
  M.shape = ui.SdfShape {
    id = "levels-swell-background",
    shape = "box",
    operation = "smooth_union",
    blend_group = 1001,
    track = swell,
    top_right_radius = 0, bottom_right_radius = 0,
    top_left_radius = 9999, bottom_left_radius = 9999,
  }

  local across, hide
  local function stop(h) if h then h:stop() end end

  local function sink()
    stop(across)
    across = morf.animation.play {
      {
        parallel = {
          { node = bud, property = "scale", to = 0.6, duration = 160, easing = "in_cubic" },
          { node = bud, property = "opacity", to = 0, duration = 140, easing = "in_cubic" },
          { node = swell, property = "x", to = tucked_x(), duration = theme.duration.drawer_close,
            easing = theme.ease.emphasized_accel, delay = 60 },
        },
      },
      on_finished = function(reason) if reason == "completed" then shown:set("") end end,
    }
  end

  --- Shows `kind` ("volume" or "brightness") for a moment: the frame
  --- swells out at its level with the value, or, out already, slides to it.
  function M.pop(kind)
    if not value[kind] then return end
    local was = shown:get()
    shown:set(kind)
    if was == "" or (across and across:active() and swell.x > out_x() + 0.5) then
      stop(across)
      across = morf.animation.play {
        {
          parallel = {
            { node = swell, property = "x", to = out_x(), duration = 300, easing = theme.ease.spatial },
            { node = bud, property = "scale", from = 0.6, to = 1, duration = 300, easing = theme.ease.spatial, delay = 60 },
            { node = bud, property = "opacity", from = 0, to = 1, duration = 140, delay = 60 },
          },
        },
      }
    end
    if hide then hide:cancel() end
    hide = morf.timer(HOLD, function()
      hide = nil
      sink()
    end, false)
  end

  bud.opacity, bud.scale = 0, 0.6
  local root = ui.Item {
    id = "levels",
    anchors = { fill = true },
    ui.Item { anchors = { fill = true }, table.unpack(pills) },
    swell,
  }
  -- The sidebar opening carries the pills out with it, to the strip on its
  -- near side: between the desk and the panel.
  local sidebar = require("sidebar")
  kit.ride("levels", root, sidebar.drawer,
    function() return -(theme.SIDE_W + theme.STRIP / 2 + theme.BORDER / 2) end,
    function() return sidebar.drawer.panel.width + theme.SEAM + theme.BORDER + 2 end)
  M.shown = shown
  return root
end

return M
