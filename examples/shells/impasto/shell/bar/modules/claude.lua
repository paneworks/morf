-- Claude Code: the account's session and week on a chip, and a detail with
-- the tokens and messages of both.
--
-- Port of ClaudeModule.qml, ClaudeMark.qml and the Claude rows of
-- ModuleService.qml and ChipFace.qml. The ring is the larger of the
-- session's and the week's share of the account's limits when those are
-- known, else the time gone in the five-hour block; it turns yellow past
-- 60%, red past 85% or while rate-limited. The chip's figure is the
-- session's percentage, or the block's tokens with no account figures.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local claude = require("services.claude")
local kit = require("components.kit")
local controls = require("components.controls")

local C = theme.color
local M = {}

-- The Claude mark (Anthropic's) as an outline, traced with potrace and
-- reduced to about a hundred points; -0.5..0.5 on both axes, y down.
M.OUTLINE = {
  -0.2292,-0.4973, -0.2374,-0.4948, -0.2693,-0.4525, -0.2588,-0.3972, -0.1075,-0.1341,
  -0.1103,-0.1297, -0.1155,-0.1297, -0.2307,-0.2150, -0.3492,-0.3069, -0.3676,-0.3113,
  -0.3975,-0.3113, -0.4236,-0.2817, -0.4169,-0.2399, -0.4029,-0.2215, -0.1108,-0.0226,
  -0.1015,-0.0127, -0.1073,-0.0070, -0.2407,-0.0202, -0.4783,-0.0358, -0.4978,-0.0229,
  -0.4998,-0.0082, -0.4781,0.0209, -0.4647,0.0249, -0.1088,0.0398, -0.1043,0.0443,
  -0.1073,0.0557, -0.3029,0.1650, -0.3875,0.2235, -0.3932,0.2330, -0.3947,0.2613,
  -0.3751,0.2810, -0.3270,0.2743, -0.0655,0.1043, -0.0602,0.1043, -0.2760,0.3843,
  -0.2787,0.4119, -0.2491,0.4276, -0.2295,0.4199, -0.1600,0.3442, -0.0095,0.1391,
  -0.0030,0.1391, -0.0650,0.4505, -0.0513,0.4823, -0.0281,0.5000, 0.0022,0.4913,
  0.0174,0.4756, 0.0450,0.1752, 0.0513,0.1685, 0.1531,0.3328, 0.2150,0.4204,
  0.2417,0.4253, 0.2678,0.4171, 0.2755,0.4007, 0.2705,0.3569, 0.1538,0.1814,
  0.1538,0.1765, 0.1605,0.1765, 0.2795,0.2778, 0.3626,0.3407, 0.3815,0.3457,
  0.3950,0.3265, 0.3885,0.3021, 0.1563,0.0856, 0.1563,0.0819, 0.1692,0.0819,
  0.4462,0.1488, 0.4960,0.1224, 0.4998,0.1035, 0.4818,0.0772, 0.4470,0.0540,
  0.3293,0.0448, 0.2446,0.0445, 0.1605,0.0346, 0.3243,-0.0050, 0.4039,-0.0204,
  0.4841,-0.0421, 0.4950,-0.0717, 0.4935,-0.0886, 0.4582,-0.1070, 0.1909,-0.0555,
  0.1894,-0.0587, 0.1882,-0.0622, 0.2195,-0.1160, 0.3537,-0.2919, 0.3676,-0.3400,
  0.3355,-0.3885, 0.2937,-0.3885, 0.2666,-0.3664, 0.2046,-0.2977, 0.0879,-0.1483,
  0.0767,-0.1468, 0.1292,-0.4445, 0.1098,-0.4719, 0.0851,-0.4853, 0.0530,-0.4639,
  0.0371,-0.4246, 0.0072,-0.1167, 0.0020,-0.1200, -0.0110,-0.1633, -0.0996,-0.3375,
  -0.1620,-0.4803, -0.1822,-0.4950, -0.2208,-0.5000,
}

local points = {}
for i = 1, #M.OUTLINE, 2 do
  points[#points + 1] = ("%.2f,%.2f"):format(M.OUTLINE[i] * 100, M.OUTLINE[i + 1] * 100)
end
local POINTS = table.concat(points, " ")

local function hex(color)
  return require("pets.draw").hex(type(color) == "function" and color() or color)
end

--- The mark, filled in `color` (a colour or a binding), `size` square.
function M.mark(values)
  local size = values.size or 16
  local color = values.color or C.indicator
  return ui.Image {
    x = values.x, y = values.y, anchors = values.anchors,
    width = size, height = size,
    source = function()
      return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="-50 -50 100 100">'
        .. '<polygon points="%s" fill="%s"/></svg>'):format(POINTS, hex(color))
    end,
  }
end

--- The ring face: the gauge with the mark inside it.
function M.widget(size, thickness, mark_size)
  return controls.ring {
    size = size, thickness = thickness,
    progress = claude.gauge,
    track_color = C.indicatorDim,
    fill_color = claude.tint,
    M.mark { anchors = { center_in = true }, size = mark_size, color = C.indicator },
  }
end

-- ---------------------------------------------------------------- detail --

local function column(values, width)
  local bar_w = width - 7 - 30
  return ui.Column {
    gap = 5, width = width,
    controls.figure {
      width = width, label = values.label,
      value = function() return claude.compact(values.tokens()) .. " tokens" end,
      -- Messages as the note: they tell a long session apart from one
      -- large file.
      note = function() return claude.messages(values.count()) end,
    },
    -- Only against the account's own figures.
    ui.Row {
      gap = 7, align = "center", visible = claude.measured,
      controls.usage_bar { width = bar_w, height = 5, progress = values.fraction, fill_color = claude.tint },
      kit.text { text = function() return claude.percent(values.fraction()) end,
        mono = true, size = theme.size.label, color = C.textMuted },
    },
  }
end

function M.detail()
  claude.subscribe()
  local w = modules.entry("claude").width
  local inner = w - 8 - 28
  local text_w = inner - 44 - 13
  local half = (inner - 14) / 2
  return ui.Item {
    anchors = { fill = true },
    on_destroyed = function() claude.release() end,
    ui.Column {
      anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
      gap = 12,
      ui.Row {
        gap = 13, align = "center",
        M.widget(44, 3, 24),
        ui.Column {
          gap = 2,
          kit.text { text = "Claude Code", size = theme.size.medium, weight = 600,
            width = text_w, elide = "right" },
          kit.text {
            width = text_w, elide = "right", size = theme.size.small,
            color = function() return claude.limited() and C.indicatorBad or C.textMuted() end,
            -- A block runs five hours from its first message, so the
            -- reset time is exact.
            text = function()
              if claude.limited() then return "Rate limited · " .. claude.resets_in() end
              if claude.available() or claude.measured() then return "Session " .. claude.resets_in() end
              return "No sessions on disk"
            end,
          },
        },
      },
      ui.Row {
        gap = 14,
        column({ label = "SESSION", tokens = claude.block_tokens, count = claude.block_messages,
          fraction = claude.session_fraction }, half),
        column({ label = "WEEK", tokens = claude.week_tokens, count = claude.week_messages,
          fraction = claude.weekly_fraction }, half),
      },
    },
  }
end

modules.define("claude", {
  glyph = function() return "" end,
  value = function()
    if claude.measured() then return claude.percent(claude.session_fraction()) end
    local tokens = claude.block_tokens()
    return tokens > 0 and claude.compact(tokens) or "0%"
  end,
  tint = function() return claude.measured() and claude.tint() or C.indicator end,
  -- Reading it builds the reader, which runs its first pass; it turns true
  -- a moment later.
  has = claude.available,
  watch = function(on) if on then claude.subscribe() else claude.release() end end,
  chip_mark = function()
    return M.mark { x = 1, y = 1, size = math.floor(theme.capsule_height() * 0.44 + 0.5),
      color = function() return modules.tint_of("claude") end }
  end,
  chip = function()
    return M.widget(theme.capsule_height(), 2.5, math.floor(theme.capsule_height() * 0.53 + 0.5))
  end,
  detail = M.detail,
})

-- `morf ipc call claude.limits 0.42 0.18 150 [rate_limited]`: a test
-- bench's account figures, since a dry run never asks the API.
morf.ipc["claude.limits"] = function(session, week, minutes, status)
  claude.sample_limits(tonumber(session), tonumber(week), tonumber(minutes), status)
  return claude.percent(claude.session_fraction()) .. " / " .. claude.percent(claude.weekly_fraction())
end

return M
