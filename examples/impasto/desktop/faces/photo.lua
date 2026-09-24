-- A picture of your own, edge to edge, in any family.
--
-- Port of faces/PhotoFace.qml. The widget draws no capsule for it, so the
-- picture is the widget and takes the widget's corner. Empty, it is the
-- capsule the other widgets are drawn on, saying so; the picture is chosen
-- in the picker while arranging, and at rest a click opens it in imv.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")

local M = {}

function M.build(ctx)
  local w, h = ctx.width, ctx.height
  local ink = ctx.ink
  local function row() return ctx.row and ctx.row() or nil end
  local function path() return desk.picture_of(row()) end
  local function lost()
    local p = path()
    return p ~= "" and not morf.fs.is_file(p)
  end
  local function empty() return path() == "" or lost() end
  return ui.Item {
    width = w, height = h,
    ui.ClipRect {
      anchors = { fill = true }, radius = theme.desktop_radius,
      visible = function() return not empty() end,
      color = ink.raised,
      border_width = 1, border_color = ink.border, content_under_border = true,
      ui.Image {
        anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source_width = w * 2, source_height = h * 2,
        source = function() return empty() and "" or path() end,
      },
    },
    ui.Rect {
      anchors = { fill = true }, radius = theme.desktop_radius,
      visible = empty,
      color = function() return ink.ground():alpha(desk.opacity_of(row()) / 100) end,
      border_width = 1, border_color = ink.border,
      ui.Column {
        anchors = { center_in = true }, gap = 4, align = "center", width = w - 24,
        kit.glyph { glyph = "󰋩", size = ctx.family == "4x4" and 30 or 22, color = ink.text },
        kit.text { width = w - 24, horizontal_alignment = "center", elide = "right",
          text = function() return lost() and "Picture not found" or "No picture" end,
          size = theme.size.small, weight = 600, color = ink.text },
        kit.text { width = w - 24, horizontal_alignment = "center", elide = "right",
          text = function() return lost() and "Edit it to choose another" or "Edit it to choose one" end,
          size = theme.size.label, color = ink.muted },
      },
    },
    ui.MouseArea {
      anchors = { fill = true },
      visible = function() return row() ~= nil and not empty() end,
      cursor = "pointer",
      on_clicked = function() desk.open_picture(row()) end,
    },
  }
end

return M
