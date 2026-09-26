-- The notification history kept by the shell's own server:
-- NotificationList.qml. The control centre's notifications block, and the
-- notifications module's detail (bare: the island is the card).

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local notify = require("services.notifications")

local C = theme.color
local M = {}

-- The history mirrored into a list model of plain rows, so a Repeater keeps
-- a row's nodes while the rest of the list changes.
M.rows = morf.list_model({})
M.count = morf.signal("impasto.notify.count", 0)

local function plain(markup)
  local text = tostring(markup or "")
  text = text:gsub("<[^>]*>", ""):gsub("&amp;", "&"):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("\n", " ")
  return text
end

local function picture(entry)
  local hints = entry.hints or {}
  local path = entry.image_path or hints["image-path"] or hints["image_path"] or ""
  if path == "" and type(entry.icon) == "string" and entry.icon:sub(1, 1) == "/" then path = entry.icon end
  if path:sub(1, 7) == "file://" then path = path:sub(8) end
  return path
end

morf.effect("impasto.notify.mirror", function()
  local rows = {}
  for _, entry in ipairs(notify.history()) do
    rows[#rows + 1] = {
      id = entry.id, summary = tostring(entry.summary or ""), body = plain(entry.body),
      app = tostring(entry.app or ""), critical = entry.urgency == 2, image = picture(entry),
    }
  end
  M.rows:replace(rows, "id")
  M.count:set(#rows)
end)

local function row(entry, width)
  local current = entry
  local tick = controls.signal("notify.row", 0)
  local hovered = controls.signal("notify.hover", false)
  local function field(name) tick:get() return current[name] end
  local text_w = width - 9 - 30 - 9 - 30 - 6
  return ui.Rect {
    width = width, height = 54, radius = theme.radius_small,
    color = function() return hovered:get() and C.islandSurfaceHover or "#00000000" end,
    behavior = { color = theme.behave("fast") },
    ui.MouseArea {
      anchors = { fill = true },
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
    },
    ui.Row {
      anchors = { left = true, left_margin = 9, vertical_center = true },
      gap = 9, align = "center",
      ui.ClipRect {
        width = 30, height = 30, radius = 30 * theme.picture_corner,
        color = function() return field("critical") and C.red() or C.islandSurfaceHover end,
        ui.Image { anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
          source = function() return field("image") or "" end,
          visible = function() return (field("image") or "") ~= "" end },
        kit.glyph { anchors = { center_in = true }, size = 13,
          visible = function() return (field("image") or "") == "" end,
          glyph = function() return field("critical") and "󰀪" or "󰂚" end,
          color = function() return field("critical") and C.accentText() or C.accent() end },
      },
      ui.Column {
        gap = 1, width = text_w,
        kit.text { text = function() return field("summary") end, size = theme.size.small, weight = 600,
          width = text_w, elide = "right" },
        kit.text { text = function() return field("body") end, size = theme.size.label, color = C.textMuted,
          width = text_w, elide = "right", visible = function() return (field("body") or "") ~= "" end },
        kit.text { text = function() return field("app") end, size = 9, color = C.textMuted, opacity = 0.7,
          width = text_w, elide = "right" },
      },
    },
    ui.Item {
      anchors = { right = true, right_margin = 6, vertical_center = true },
      width = 30, height = 28,
      visible = function() return hovered:get() end,
      controls.icon_button { icon = "󰅖", icon_size = 11, width = 30, height = 28,
        on_click = function() notify.remove(current.id) end },
    },
  }, function(next)
    current = next
    tick:set(tick:get() + 1)
  end
end

--- The list, `width` x `height`. `bare` drops the card, for the island.
function M.build(options)
  local width, height = options.width, options.height
  local padding = options.padding or 14
  local inner_w, inner_h = width - 2 * padding, height - 2 * padding
  local list = ui.Repeater {
    as = "column", gap = 6,
    model = M.rows,
    delegate = function(entry) return row(entry, inner_w) end,
  }
  local badge_label = kit.text {
    text = function() return tostring(M.count:get()) end,
    size = theme.size.label, weight = 600, color = C.textMuted,
  }
  return controls.card {
    bare = options.bare, padding = padding,
    width = width, height = height,
    ui.Column {
      gap = 10,
      ui.Item {
        width = inner_w, height = 20,
        ui.Row {
          anchors = { left = true, vertical_center = true },
          gap = 8, align = "center",
          kit.text { text = "Notifications", size = theme.size.medium, weight = 600 },
          ui.Rect {
            visible = function() return M.count:get() > 0 end,
            width = function() return math.max(18, (badge_label.layout_width or 0) + 10) end,
            height = 17, radius = 8.5, color = C.islandSurfaceHover,
            ui.Item { anchors = { center_in = true },
              width = function() return badge_label.layout_width or 0 end, height = 12, badge_label },
          },
        },
        ui.Item {
          anchors = { right = true, vertical_center = true },
          width = 40, height = 20,
          controls.link { anchors = { right = true, vertical_center = true },
            text = "Clear", visible = function() return M.count:get() > 0 end,
            on_click = notify.clear },
        },
      },
      ui.ClipRect {
        width = inner_w, height = inner_h - 30, color = "#00000000",
        kit.text { anchors = { center_in = true }, text = "Nothing new",
          size = theme.size.small, color = C.textMuted,
          visible = function() return M.count:get() == 0 end },
        -- Longer than the card, it scrolls (the wheel reaches it past the rows).
        ui.Flickable { anchors = { fill = true }, list },
      },
    },
  }
end

return M
