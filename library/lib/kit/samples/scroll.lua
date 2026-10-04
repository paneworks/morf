-- Gallery samples for the Scroll widgets (see samples/init.lua): a page of
-- settings rows, a sheet that scrolls both ways, an onboarding pager, a
-- shelf of albums and a list that loads more as it reaches its end.
local ui = require("morf.ui")

local M = {}

M.span = { shelf = { 2, 1 } }

local W, H = 280, 220

local function row(kit, i, label, value)
  return ui.Item { width = W, height = 40,
    kit.text { x = 14, anchors = { vertical_center = true }, text = label },
    kit.text { anchors = { right = true, right_margin = 18, vertical_center = true }, text = value,
      color = kit.ink("lo") },
    ui.Rect { anchors = { left = true, right = true, bottom = true, left_margin = 14 }, height = 1,
      color = kit.stroke("faint") } }
end

function M.scroll_view(kit, widgets)
  local items = {
    { "Wi-Fi", "Home" }, { "Bluetooth", "On" }, { "Display", "100 %" }, { "Sound", "62 %" },
    { "Power", "Balanced" }, { "Battery", "84 %" }, { "Keyboard", "US" }, { "Mouse", "Natural" },
    { "Printers", "None" }, { "Sharing", "Off" }, { "Privacy", "" }, { "About", "" },
  }
  local rows = {}
  for i, item in ipairs(items) do rows[i] = row(kit, i, item[1], item[2]) end
  return (widgets.scroll_view { id = "sample-scroll-view", width = W, height = H, clip = true, ui.Column(rows) })
end

function M.scroll_area(kit, widgets)
  -- A sheet wider and taller than its view: column letters, row numbers.
  local CW, CH, COLS, ROWS = 64, 30, 8, 12
  local sheet = ui.Item { width = CW * COLS + 36, height = CH * (ROWS + 1) }
  for c = 1, COLS do
    ui.reparent(kit.text { x = 36 + (c - 1) * CW + 8, y = 6, text = string.char(64 + c), color = kit.ink("lo") }, sheet)
  end
  for r = 1, ROWS do
    local y = r * CH
    ui.reparent(kit.text { x = 8, y = y + 6, text = tostring(r), color = kit.ink("lo") }, sheet)
    ui.reparent(ui.Rect { x = 0, y = y, width = CW * COLS + 36, height = 1, color = kit.stroke("faint") }, sheet)
    for c = 1, COLS do
      if (r * 7 + c * 3) % 5 ~= 0 then
        ui.reparent(kit.text { x = 36 + (c - 1) * CW + 8, y = y + 6, text = tostring((r * 37 + c * 11) % 900) }, sheet)
      end
    end
  end
  for c = 0, COLS do
    ui.reparent(ui.Rect { x = 36 + c * CW, y = 0, width = 1, height = CH * (ROWS + 1), color = kit.stroke("faint") },
      sheet)
  end
  return (widgets.scroll_area { id = "sample-scroll-area", width = W, height = H, clip = true, sheet })
end

function M.pager(kit, widgets)
  local pages = {
    { "Welcome", "Swipe to take the tour", "waving_hand", "accent" },
    { "Sync", "Your files, everywhere", "sync", "ok" },
    { "Privacy", "Only you hold the keys", "shield", "warn" },
    { "Ready", "Everything is set up", "check_circle", "extra" },
  }
  local row = {}
  for i, p in ipairs(pages) do
    local tone = kit.signal(p[4])
    row[i] = ui.Item { width = W, height = H,
      ui.Rect { anchors = { fill = true, margins = 6, bottom_margin = 34 }, radius = kit.round and kit.round(12) or 12,
        color = function() return tone():alpha(0.16) end,
        kit.icon(p[3], 36, tone, { anchors = { horizontal_center = true }, y = 34 }),
        kit.text { anchors = { horizontal_center = true }, y = 84, text = p[1], font_size = 20, font_weight = 700 },
        kit.text { anchors = { horizontal_center = true }, y = 116, text = p[2], color = kit.ink("lo") } } }
  end
  return (widgets.pager { id = "sample-pager", width = W, height = H, clip = true, ui.Row(row) })
end

function M.shelf(kit, widgets)
  local albums = { "Blue Hour", "Northern", "Low Tide", "Paper Moon", "Static", "Glasshouse", "Ember", "Driftwood",
    "Afterglow", "Monsoon" }
  local tones = { "accent", "ok", "warn", "extra", "info", "alert" }
  local cards = { y = 10 }
  for i, name in ipairs(albums) do
    local tone = kit.signal(tones[(i - 1) % #tones + 1])
    cards[i] = ui.Item { width = 120, height = 180,
      ui.Rect { x = 4, y = 6, width = 112, height = 112, radius = kit.round and kit.round(10) or 10,
        color = function() return tone():alpha(0.28) end,
        kit.icon("album", 40, tone, { anchors = { center_in = true } }) },
      kit.text { x = 6, y = 128, width = 108, elide = "right", text = name },
      kit.text { x = 6, y = 150, text = tostring(2016 + i), color = kit.ink("lo") } }
  end
  return (widgets.shelf { id = "sample-shelf", width = 600, height = H, item_size = 120, clip = true,
    ui.Row(cards) })
end

function M.infinite_scroll(kit, widgets)
  local column = ui.Column {}
  local count = 0
  local function add(n)
    for _ = 1, n do
      count = count + 1
      ui.reparent(row(kit, count, "Message " .. count, ("%d:%02d"):format(9 + count // 60, count % 60)), column)
    end
  end
  add(10)
  return (widgets.infinite_scroll { id = "sample-infinite", width = W, height = H, clip = true, column,
    on_load_more = function(done)
      morf.timer(700, function()
        if count < 60 then add(8) end
        done()
      end, false)
    end })
end

return M
