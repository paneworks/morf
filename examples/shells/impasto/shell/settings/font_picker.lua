-- A list of installed families, each row drawn in the family it names
-- (FontPicker). The families come from `morf.font_families()`, the faces
-- the shaper can actually use, so a typed name can never look applied when
-- it is not.
--
-- A desk has hundreds of families and every row shapes its own face, so
-- only the rows in view exist: a fixed set of slots whose names follow the
-- scroll.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

local MONO_HINTS = { "mono", "code", "consol", "courier", "terminal", "fixed", "hack", "iosevka", "fira code" }

local function is_mono(name)
  local lower = name:lower()
  for _, hint in ipairs(MONO_HINTS) do
    if lower:find(hint, 1, true) then return true end
  end
  return false
end

local families
local function all()
  if not families then
    families = {}
    local ok, list = pcall(morf.font_families)
    for _, name in ipairs(ok and list or {}) do families[#families + 1] = name end
    table.sort(families, function(a, b) return a:lower() < b:lower() end)
  end
  return families
end

--- The families for `kind`: "mono" or "sans" (everything that is not mono).
function M.families(kind)
  local out = {}
  for _, name in ipairs(all()) do
    if (kind == "mono") == is_mono(name) then out[#out + 1] = name end
  end
  return out
end

--- `width`, `height` (132), `kind`, `current()` (the stored stack; its
--- first name is the one ringed), `sample`, `warning`, `on_picked(family)`.
function M.new(values)
  local width = values.width
  local height = values.height or 132
  local list = M.families(values.kind)
  local ROW = 30
  local warning_h = (values.warning or "") ~= "" and 28 or 0
  local view_h = height - warning_h
  local slots = math.ceil(view_h / ROW) + 1
  local preferred = function()
    return (tostring(values.current() or ""):match("^%s*([^,]*)") or ""):match("^(.-)%s*$")
  end
  -- The list opens on the chosen family.
  local start = 0
  for index, name in ipairs(list) do if name == preferred() then start = index - 1 end end
  local max_offset = math.max(0, #list * ROW - view_h)
  local offset = controls.signal("fonts.offset", math.min(max_offset, math.max(0, start * ROW - ROW)))

  local rows = {}
  for slot = 1, slots do
    local hovered = controls.signal("fonts.row", false)
    local index = function() return math.floor(offset:get() / ROW) + slot end
    local name = function() return list[index()] or "" end
    local active = function() return name() ~= "" and name() == preferred() end
    rows[#rows + 1] = ui.Rect {
      x = 0, width = width, height = ROW, radius = theme.radius_small,
      y = function() return (index() - 1) * ROW - offset:get() end,
      visible = function() return name() ~= "" end,
      color = function()
        if active() then return C.islandSurfaceHover end
        return hovered:get() and C.islandBorder or "#00000000"
      end,
      behavior = { color = fast() },
      ui.Text {
        anchors = { left = true, left_margin = 11, vertical_center = true },
        text = name, font_family = name, font_size = theme.size.small,
        width = math.floor(width * 0.45), elide = "right",
        color = function() return active() and C.accent() or C.text() end,
      },
      ui.Text {
        anchors = { right = true, right_margin = 30, vertical_center = true },
        text = values.sample or "", font_family = name, font_size = theme.size.small,
        width = math.floor(width * 0.4), elide = "right", horizontal_alignment = "right",
        color = C.textMuted,
      },
      kit.glyph {
        anchors = { right = true, right_margin = 11, vertical_center = true },
        glyph = "󰄬", size = 12, color = C.accent, visible = active,
      },
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() if name() ~= "" then values.on_picked(name()) end end,
        on_wheel = function(_, _, _, py, _, sy)
          local delta = (sy and sy ~= 0) and sy * ROW or (py or 0)
          offset:set(math.max(0, math.min(max_offset, offset:get() + delta)))
        end,
      },
    }
  end

  local children = {
    width = width, height = height,
    ui.ClipRect {
      x = 0, y = 0, width = width, height = view_h, color = "#00000000",
      table.unpack(rows),
    },
    kit.text {
      anchors = { center_in = true }, visible = #list == 0,
      text = "No families to choose from", size = theme.size.small, color = C.textMuted,
    },
  }
  if warning_h > 0 then
    children[#children + 1] = ui.Rect {
      x = 0, y = view_h + 4, width = width, height = 24, radius = theme.radius_small,
      color = C.islandSurfaceHover,
      kit.text { anchors = { left = true, left_margin = 11, vertical_center = true },
        text = values.warning, size = 9, color = C.yellow },
    }
  end
  children[#children + 1] = setting.wheel_area()
  return ui.Item(children)
end

return M
