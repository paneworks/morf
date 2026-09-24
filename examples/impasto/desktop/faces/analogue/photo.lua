-- A picture of your own as an object on the desk.
--
-- Port of analogue/PhotoFace.qml. In the square, wide and large families it
-- is an instant print: a paper border, a deep chin with the row's caption
-- in the signature hand, and a strip of tape along the top. On the band it
-- is a strip of film, the one picture running under three frames.
--
-- Each widget leans its own way, derived from its key, so it keeps its
-- angle across redraws and two photos side by side lean apart. At rest a
-- click opens the picture in the shell's viewer (desktop/viewer.lua).

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")
local svg = require("desktop.faces.analogue.svg")

local C = theme.color
local M = {}

-- 0.8 to 2.6 degrees either way; keys that differ in their last character
-- fall on opposite sides.
local function lean_of(key)
  local hash = 0
  for i = 1, #key do hash = (hash * 31 + key:byte(i)) % 1000 end
  return (hash % 2 == 0 and 1 or -1) * (0.8 + (hash % 100) / 100 * 1.8)
end

local function frame(w, h)
  local side = math.min(w, h) * 0.06
  local chin = h * 0.19
  return { side = side, chin = chin, width = w - 2 * side, height = h - side - chin }
end

function M.build(ctx)
  local w, h, ink = ctx.width, ctx.height, ctx.ink
  local function row() return ctx.row and ctx.row() or nil end
  local r0 = row()
  local lean = lean_of(r0 and r0.key or "")
  local function path() return desk.picture_of(row()) end
  local function lost() return require("desktop.faces.common").lost(path()) end
  local function empty() return path() == "" or lost() end
  local function caption() local r = row() return r and type(r.caption) == "string" and r.caption or "" end
  local paper = function() return svg.paper(ink) end
  local missing = function() return lost() and "Picture not found" or "No picture" end
  local body

  if ctx.family == "8x2" then
    local bw, bh = w * 0.98, h * 0.86
    local rebate = bh * 0.16
    local holes = math.max(1, math.floor((bw - 20) / 26))
    local pitch = (bw - 20) / holes
    local parts = {
      ui.Rect { width = bw, height = bh, radius = 3, color = ink.ground },
    }
    for i = 0, holes * 2 - 1 do
      local top = i < holes
      parts[#parts + 1] = ui.Rect {
        x = 10 + (i % holes) * pitch + (pitch - 10) / 2,
        y = (top and rebate or 2 * bh - rebate) / 2 - 4,
        width = 10, height = 8, radius = 2, color = paper,
      }
    end
    local fw, fh = bw - 24, bh - 2 * rebate
    parts[#parts + 1] = ui.ClipRect {
      x = 12, y = rebate, width = fw, height = fh, radius = 1.5, color = ink.raised,
      ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source_width = fw * 2, source_height = fh * 2,
        source = function() return empty() and "" or path() end },
      kit.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
        visible = empty, text = missing, font_family = function() return theme.font_signature() end,
        size = math.floor(math.min(26, fh * 0.24)), color = ink.muted },
      ui.Rect { x = fw / 3 - 4, width = 8, height = fh, color = ink.ground },
      ui.Rect { x = 2 * fw / 3 - 4, width = 8, height = fh, color = ink.ground },
    }
    body = ui.Item { x = (w - bw) / 2, y = (h - bh) / 2, width = bw, height = bh, rotation = lean * 0.35,
      table.unpack(parts) }
  else
    local cw, ch = w * 0.9, h * 0.9
    local cut = frame(cw, ch)
    local tape_h = math.min(24, ch * 0.11)
    body = ui.Item {
      x = (w - cw) / 2, y = (h - ch) / 2, width = cw, height = ch, rotation = lean,
      ui.Rect { width = cw, height = ch, radius = 3, color = paper,
        layer = { enabled = true, shadow_color = morf.color("#000000"):alpha(0.35), shadow_blur = 8, shadow_offset_y = 2 } },
      ui.ClipRect {
        x = cut.side, y = cut.side, width = cut.width, height = cut.height, radius = 1.5, color = C.paperLine,
        ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
          source_width = cut.width * 2, source_height = cut.height * 2,
          source = function() return empty() and "" or path() end },
        kit.text { x = 8, y = 0, width = cut.width - 16, height = cut.height, wrap = true,
          horizontal_alignment = "center", vertical_alignment = "center",
          visible = empty, text = missing, font_family = function() return theme.font_signature() end,
          size = math.floor(math.min(26, cut.height * 0.16)), color = C.paperInkMuted },
      },
      kit.text {
        x = cut.side, y = ch - cut.chin, width = cut.width, height = cut.chin,
        horizontal_alignment = "center", vertical_alignment = "center", elide = "right",
        text = caption, font_family = function() return theme.font_signature() end,
        size = math.floor(math.min(30, cut.chin * 0.62)), color = C.paperInk,
      },
      -- Tape, across the top edge and against the card's lean.
      ui.Rect { x = cw * 0.37, y = -tape_h / 2, width = cw * 0.26, height = tape_h,
        rotation = lean > 0 and -4 or 4, color = function() return paper():alpha(0.62) end },
    }
  end

  return ui.Item {
    width = w, height = h,
    body,
    ui.MouseArea {
      anchors = { fill = true },
      visible = function() return row() ~= nil and not empty() end,
      cursor = "pointer",
      on_clicked = function() desk.open_picture(row()) end,
    },
  }
end

return M
