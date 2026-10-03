-- Gallery samples for the Transform archetype's widgets (lib.kit.transform):
-- each as it is used -- an inspector floating over a workspace, a photo
-- being cropped, a sticker selected on a canvas with its handles, a video
-- playing picture-in-picture over a page, an afternoon of a calendar day
-- with its events. The layouts are the same in every theme; the skins
-- draw them. (The photo and the video frame are pictures, not the theme.)
local morf = require("morf")
local ui = require("morf.ui")

local S = { span = {} }
local serial = 0
-- A transform's handle, known once it is made: the content it carries is
-- made first and reads the box through this.
local function later(name)
  serial = serial + 1
  local known = morf.signal("kit.sample.transform." .. name .. "." .. serial, false)
  local box
  return function() if known:get() then return box end end, function(b) box = b known:set(true) end
end

local function picture(w, h) return require("lib.kit.samples.canvas").picture(w, h) end

-- A row of a property and its value, the value read live.
local function prop(kit, name, value)
  return ui.Row { height = 28, gap = 12, align = "center",
    kit.text { text = name, width = 92, color = kit.ink("lo") },
    kit.text { text = value, font_weight = 600 } }
end

function S.floating_panel(kit, w)
  local box, known = later("panel")
  local function read(f) return function() local b = box() return b and ("%d"):format(math.floor(f(b.t) + 0.5)) or "" end end
  local body = ui.Column { anchors = { fill = true, margins = 16 }, gap = 4,
    prop(kit, "Position", function()
      local b = box()
      return b and ("%d, %d"):format(math.floor(b.t.x + 0.5), math.floor(b.t.y + 0.5)) or ""
    end),
    prop(kit, "Width", read(function(t) return t.box_width end)),
    prop(kit, "Height", read(function(t) return t.box_height end)),
    prop(kit, "Layer", "Sketch"),
    prop(kit, "Blend", "Multiply") }
  local node, handle = w.floating_panel { id = "sample-floating-panel", title = "Inspector",
    x = 60, y = 48, width = 340, height = 260, body }
  known(handle)
  return node
end
S.span.floating_panel = { 2, 2 }

function S.image_cropper(_, w)
  return (w.image_cropper { id = "sample-image-cropper", image = picture(600, 480),
    x = 150, y = 70, width = 300, height = 240 })
end
S.span.image_cropper = { 2, 2 }

function S.resize_box(kit, w)
  local sticker = kit.card { anchors = { fill = true },
    kit.icon("star", 56, kit.signal("warning"), { anchors = { center_in = true } }) }
  return (w.resize_box { id = "sample-resize-box", x = 70, y = 50, width = 140, height = 110, sticker })
end

function S.pip_window(kit, w)
  -- A page under it: a heading and a few lines of text.
  local page = ui.Column { anchors = { fill = true, margins = 16 }, gap = 10,
    kit.heading { text = "Release notes", level = "section" },
    kit.text { text = "A video keeps to a corner.", color = kit.ink("lo") },
    kit.text { text = "Shift keeps its aspect.", color = kit.ink("lo") },
    kit.text { text = "Return fills the page.", color = kit.ink("lo") } }
  local frame = picture(256, 200)
  frame.y = -10
  local video = ui.Item { anchors = { fill = true }, frame,
    ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 4,
      color = kit.ink("hi"), opacity = 0.35 },
    ui.Rect { anchors = { left = true, bottom = true }, width = 96, height = 4, color = kit.signal("accent") } }
  local node = w.pip_window { id = "sample-pip-window", x = 332, y = 64, width = 256, height = 144, video }
  return ui.Item { anchors = { fill = true }, page, node }
end
S.span.pip_window = { 2, 1 }

function S.event_block(kit, w)
  local HOUR, FROM, LEFT = 64, 13, 52
  local day = ui.Item { anchors = { fill = true } }
  for i = 0, 6 do
    ui.reparent(kit.text { x = 0, y = i * HOUR - 9, width = LEFT - 10, height = 18, text = ("%d:00"):format(FROM + i),
      font_size = 13, color = kit.ink("lo"), horizontal_alignment = "right" }, day)
    ui.reparent(ui.Rect { x = LEFT, y = i * HOUR, height = 1, anchors = { left = true, right = true, left_margin = LEFT },
      color = kit.stroke("faint") }, day)
  end
  local function clock(y) local m = math.floor(y / HOUR * 60 + 0.5) return ("%d:%02d"):format(FROM + m // 60, m % 60) end
  local function event(id, title, tone, y, h)
    local box, known = later(id)
    local label = ui.Column { anchors = { fill = true, left_margin = 14, top_margin = 6, right_margin = 8 }, gap = 2,
      kit.text { text = title, font_weight = 700, elide = "right" },
      kit.text { text = function()
        local b = box()
        return b and (clock(b.t.y) .. " – " .. clock(b.t.y + b.t.box_height)) or ""
      end, font_size = 13, color = kit.ink("lo") } }
    local node, handle = w.event_block { id = id, tone = tone, grid = HOUR / 4, x = 4, y = y, width = 196, height = h,
      anchors = { fill = true, left_margin = LEFT }, label }
    known(handle)
    return node
  end
  ui.reparent(event("sample-event-block", "Design review", "accent", HOUR * 1, HOUR * 1.5), day)
  ui.reparent(event("sample-event-lunch", "Walk", "success", HOUR * 3, HOUR), day)
  return day
end
S.span.event_block = { 1, 2 }

return S
