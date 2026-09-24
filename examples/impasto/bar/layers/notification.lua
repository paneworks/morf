-- The notification layer: the one that just arrived, on the island.
--
-- Port of NotificationLayer.qml: its picture (or a bell, red when
-- critical), its summary and application, one line of body, and a close
-- button. The island shows it until it expires or is closed.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local notify = require("services.notifications")

local C = theme.color

morf.effect("impasto.notification.layer", function()
  island.state.set_notification(notify.active())
end)
-- Opening a panel sets the notification aside, unanswered.
island.state.on_panel_opened = function() notify.dismiss() end

local function field(name)
  return function()
    local shown = notify.shown()
    return shown and tostring(shown[name] or "") or ""
  end
end

-- The body is markup; one line of it is read as text.
local function plain_body()
  local body = field("body")()
  body = body:gsub("<[^>]*>", ""):gsub("&amp;", "&"):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("\n", " ")
  return body
end

local function picture()
  local shown = notify.shown()
  if not shown then return "" end
  local hints = shown.hints or {}
  local path = hints["image-path"] or hints["image_path"] or ""
  if path == "" and shown.icon and shown.icon:sub(1, 1) == "/" then path = shown.icon end
  if path:sub(1, 7) == "file://" then path = path:sub(8) end
  return path
end

island.register_layer("notification", {
  size = function() return 430, 68, 13 end,
  build = function()
    local text_width = 430 - 2 * 13 - 38 - 11 - 24 - 11
    return ui.Item { anchors = { fill = true },
      ui.Row {
        anchors = { left = true, left_margin = 13, vertical_center = true },
        gap = 11, align = "center",
        ui.ClipRect {
          width = 38, height = 38, radius = 38 * theme.picture_corner,
          color = function() return notify.critical() and C.red() or C.islandSurfaceHover end,
          behavior = { color = theme.behave("fast") },
          ui.Image {
            anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
            source = picture, visible = function() return picture() ~= "" end,
          },
          kit.glyph {
            anchors = { center_in = true }, size = 17,
            visible = function() return picture() == "" end,
            glyph = function() return notify.critical() and "󰀪" or "󰂚" end,
            color = function() return notify.critical() and C.accentText() or C.accent() end,
          },
        },
        ui.Column {
          gap = 1, width = text_width,
          ui.Row {
            gap = 6, align = "center",
            kit.text { text = field("summary"), size = theme.size.small, weight = 600,
              width = text_width - 80, elide = "right" },
            kit.text { text = field("app"), size = 9, width = 74, elide = "right",
              horizontal_alignment = "right",
              color = function() return notify.critical() and C.red() or C.textMuted() end },
          },
          kit.text { text = plain_body, size = 10, color = C.textMuted, width = text_width, elide = "right",
            visible = function() return plain_body() ~= "" end },
        },
        kit.icon_button { glyph = "󰅖", glyph_size = 12, diameter = 24, color = "#00000000",
          hover_color = C.islandSurfaceHover, on_click = notify.close },
      },
    }
  end,
})
