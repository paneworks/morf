-- GtkMediaControls: transport buttons, a seek scale between the position
-- and the length, and a volume scale (Press + Range).
--
--     local node = composites.media_controls {
--       id = "player", width = 480,
--       playing = function() return state.playing end,
--       position = function() return state.position end,   -- seconds
--       length = function() return state.length end,       -- seconds
--       volume = function() return state.volume end,       -- 0..1
--       on_play_pause = toggle, on_previous = prev, on_next = next,
--       on_seek = function(seconds) end, on_volume = function(v) end,
--     }
--
-- The buttons are `<id>-previous`, `<id>-play`, `<id>-next`; the seek
-- scale `<id>-seek` (it seeks on the release; the arrows step it) and the
-- volume `<id>-volume`. Leave out `on_volume` (or `volume = false`) for
-- no volume scale. `height` is the transport row's (48).
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

local function get(v) if type(v) == "function" then return v() end return v end

local function clock(seconds)
  seconds = math.max(0, math.floor(tonumber(seconds) or 0))
  local h, m, s = seconds // 3600, (seconds % 3600) // 60, seconds % 60
  if h > 0 then return ("%d:%02d:%02d"):format(h, m, s) end
  return ("%d:%02d"):format(m, s)
end

return function(spec)
  spec = spec or {}
  local id = spec.id or "media"
  local W = spec.width or 460
  local H = spec.height or 48
  local function length() return math.max(0, tonumber(get(spec.length)) or 0) end
  local function position() return math.max(0, tonumber(get(spec.position)) or 0) end
  local function playing() return get(spec.playing) == true end
  local B = H - 8
  local function button(name, icon, accessible, handler)
    return widgets.button { id = id .. "-" .. name, width = B, height = B, icon = icon, variant = "flat circular",
      accessible_name = accessible, on_clicked = function() if handler then handler() end end }
  end
  local play = widgets.button { id = id .. "-play", width = H, height = H, variant = "suggested circular",
    icon = function() return playing() and "pause" or "play_arrow" end,
    accessible_name = function() return playing() and "Pause" or "Play" end,
    on_clicked = function() if spec.on_play_pause then spec.on_play_pause(not playing()) end end }
  local transport = ui.Row { anchors = { horizontal_center = true }, gap = 12, align = "center",
    button("previous", "skip_previous", "Previous", spec.on_previous), play,
    button("next", "skip_next", "Next", spec.on_next) }
  local TW = 52
  local SW = W - 2 * TW - 16
  local function label(text, align)
    return widgets.label { text = text, width = TW, height = 34, vertical_alignment = "center",
      horizontal_alignment = align, variant = "caption dim numeric" }
  end
  local seek = widgets.scale { id = id .. "-seek", width = SW, height = 34,
    from = 0, to = 1, live = false, step = 0.01, page_step = 0.1,
    value = function() local l = length() return l > 0 and math.min(1, position() / l) or 0 end,
    enabled = function() return length() > 0 end, accessible_name = "Position",
    on_moved = function(v) if spec.on_seek then spec.on_seek(v * length()) end end }
  local seek_row = ui.Row { gap = 8, align = "center",
    label(function() return clock(position()) end, "right"), seek,
    label(function() return clock(length()) end, "left") }
  local rows = { gap = 10, x = spec.x, y = spec.y, width = W, transport, seek_row }
  if spec.volume ~= false and (spec.on_volume or spec.volume ~= nil) then
    local VW = math.min(W, spec.volume_width or 240)
    local volume = widgets.scale { id = id .. "-volume", width = VW, height = 36, from = 0, to = 1, step = 0.05,
      icon = "volume_up", value = function() return math.max(0, math.min(1, tonumber(get(spec.volume)) or 0)) end,
      accessible_name = "Volume",
      on_moved = function(v) if spec.on_volume then spec.on_volume(v) end end }
    rows[#rows + 1] = ui.Item { width = W, height = 36, ui.Item { anchors = { horizontal_center = true },
      width = VW, height = 36, volume } }
  end
  return ui.Column(rows)
end
