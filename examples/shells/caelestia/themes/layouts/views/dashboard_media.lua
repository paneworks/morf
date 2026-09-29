-- The dashboard's Media tab: the cover cut into a cookie inside a ring of
-- dots that the music pushes outwards (the visualiser), turning slowly
-- while it plays; the track, a wavy progress bar and the controls; the
-- lyrics and a player picker on the right. With nothing playing, a
-- "Nothing playing" note instead. Faint shapes drift behind it all.
--
-- Measured off the reference at 1920x1080: the page 1000 x 318; the cover
-- 184 across, its centre at (167, 160); the ring of dots 224 across; the
-- track from x = 343; the progress bar 223 long from x = 387 at y = 186;
-- the controls 54 tall from y = 221; the lyrics from x = 695.

local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local shapes = require("lib.m3shapes")

local C = theme.color
local M = {}

M.WIDTH, M.HEIGHT = 1000, 350

local CX, CY = 167, 160
local COVER = 184
local RING = 112
local DOTS = 56
local BAR_X, BAR_W, BAR_Y = 387, 223, 186

local media = require("media_state")
local bars, beats, pulse = media.bars, media.beats, media.pulse
local lyrics = media.lyrics

function M.build(ctx)
  local active, something, playing = media.active, media.something, media.playing
  local on_screen = media.watch(ctx)
  local control = media.control

  -- ------------------------------------------------------- the shapes --
  local SHAPES = {
    { "cookie9", 20, 80, 92, 10 }, { "clover4", 200, 0, 70, 30 }, { "pentagon", 390, 10, 112, 8 },
    { "cookie12", 845, 16, 112, 0 }, { "circle", 350, 121, 60, 0 }, { "oval", 340, 151, 110, 20 },
    { "gem", 670, 131, 100, -12 }, { "circle", 800, 126, 36, 0 }, { "cookie6", 240, 170, 80, 5 },
    { "sunny", 925, 150, 60, 0 }, { "pill", 450, 262, 80, 25 }, { "flower", 600, 250, 70, 0 },
  }
  local backdrop = { id = "media-backdrop", width = M.WIDTH, height = M.HEIGHT }
  for i, s in ipairs(SHAPES) do
    local name, x, y, size, rot = s[1], s[2], s[3], s[4], s[5]
    backdrop[#backdrop + 1] = ui.Path {
      x = x, y = y, width = size, height = size, rotation = rot,
      view_box = { 0, 0, 100, 100 }, d = kit.shape_path(name, { segments = false }),
      fill_color = function() return C.surfaceContainerHigh:alpha(0.55) end,
      loop = function()
        if not (on_screen() and playing()) then return nil end
        local turn = (i % 2 == 0) and 360 or -360
        return { rotation = { from = rot, to = rot + turn, duration = 60000 + i * 7000, hold = true } }
      end,
    }
  end

  -- ------------------------------------------------------- the cover --
  -- A web address (Spotify's covers are) is fetched once to the cache.
  local art = function() return require("lib.remote").file(active().art_url) end
  -- The visualiser: a bar per band standing out from the cover's edge,
  -- rounded, growing with the music -- low notes at the top, round the
  -- ring clockwise and back up the other side, so the ring is symmetric.
  local INNER, REACH = COVER / 2 + 10, 34
  RING = INNER + REACH + 6
  local dots = {}
  for i = 1, DOTS do
    local a = (i - 1) / DOTS * 2 * math.pi
    -- Mirrored: band k shows on both sides of the vertical.
    local half = DOTS // 2
    local band = i <= half and i or (DOTS - i + 1)
    local function level() return (bars:get()[band * 2 - 1] or 0) end
    local function length() return 3 + REACH * level() end
    dots[#dots + 1] = ui.Item {
      -- A pivot at the centre, turned to the bar's angle; the bar stands
      -- on the inner radius and grows outwards.
      x = RING + 6, y = RING + 6, width = 0, height = 0,
      rotation = math.deg(a),
      kit.surface {
        x = -2, width = 4, radius = 2,
        y = function() return -(INNER + length()) end,
        height = length,
        color = function() return C.primary:alpha(0.45 + 0.55 * math.min(1, level() * 1.4)) end,
      },
    }
  end
  local ring = ui.Item {
    id = "media-visualiser",
    x = CX - RING - 6, y = CY - RING - 6, width = 2 * RING + 12, height = 2 * RING + 12,
    loop = function()
      if not (on_screen() and playing()) then return nil end
      return { rotation = { to = 360, duration = 90000, hold = true } }
    end,
    table.unpack(dots),
  }
  -- The cover's cookie: it turns while the music plays, swells a little
  -- on every beat the monitor hears, and every fourth beat morphs between
  -- a nine- and a twelve-point cookie; paused, it settles back to nine.
  local function cookie()
    if not playing() then return "cookie9" end
    return (beats:get() // 4) % 2 == 0 and "cookie9" or "cookie12"
  end
  local cover = ui.Item {
    id = "media-cover-cookie",
    x = CX - COVER / 2, y = CY - COVER / 2, width = COVER, height = COVER,
    scale = function() return 1 + 0.05 * pulse:get() end,
    behavior = { scale = kit.spring(500, 18) },
    loop = function()
      if not (on_screen() and playing()) then return nil end
      return { rotation = { to = 360, duration = 60000, hold = true } }
    end,
    kit.shape {
      id = "media-cover-shape",
      anchors = { fill = true }, shape = cookie, duration = 600,
      color = function() return C.surfaceContainerHigh end,
    },
    ui.Image {
      id = "media-tab-cover",
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source = art,
      visible = function() return art() ~= "" end,
      mask = kit.shape { shape = cookie, duration = 600, color = "#ffffff" },
    },
  }
  local cover_icon = kit.icon("art_track", 96, function() return C.onSurfaceVariant end, {
    x = CX - 48, y = CY - 48, width = 96, height = 96,
    visible = function() return art() == "" end,
  })

  -- ------------------------------------------------ nothing playing --
  local hexagon = kit.shape_path(shapes.regular(6, { rounding = 0.22 }), { segments = false })
  local nothing = ui.Item {
    id = "media-nothing",
    x = 330, width = 554, height = M.HEIGHT,
    visible = function() return not something() end,
    ui.Column {
      anchors = { horizontal_center = true }, y = 64, gap = 0, align = "center",
      ui.Item {
        width = 132, height = 92,
        ui.Path {
          anchors = { fill = true }, view_box = { 0, 0, 100, 100 }, d = hexagon,
          fill_mode = "stretch",
          fill_color = function() return C.primaryContainer end,
        },
        kit.icon("queue_music", 52, function() return C.onPrimaryContainer end, { anchors = { center_in = true } }),
      },
      ui.Item { width = 1, height = 16 },
      kit.text {
        text = "Nothing playing", font_size = 37, font_weight = 500,
      },
      ui.Item { width = 1, height = 10 },
      kit.text {
        text = "Play something for it to show up here!", font_size = theme.size.large,
        color = function() return C.onSurfaceVariant end,
      },
    },
  }

  -- --------------------------------------------------- the track --
  local field = media.field
  local fraction = media.fraction(on_screen)
  local progress = ui.Item {
    id = "media-progress",
    x = BAR_X, y = BAR_Y - 17, width = BAR_W, height = 34,
    kit.media_progress { width = BAR_W, value = fraction, active = on_screen, playing = playing },
    ctx.area {
      id = "media-seek",
      anchors = { fill = true }, cursor = "pointer",
      on_pressed = function(_, _, x)
        local a = active()
        if a.length and a.length > 0 then control("set_position", math.max(0, math.min(1, x / BAR_W)) * a.length) end
      end,
    },
  }

  local function button(id, icon, w, radius, action, strong, on, ignored)
    local area = ctx.area {
      id = id, width = w, height = 54, cursor = "pointer",
      on_clicked = action,
      kit.icon(icon, 24, function()
        if strong then return C.onPrimary end
        -- A player that takes this write and ignores it (Spotify, for
        -- shuffle and repeat): the button stays, dimmed, and says so.
        if ignored and ignored() then return C.onSecondaryContainer:alpha(0.3) end
        return (on and on()) and C.primary or C.onSecondaryContainer
      end, { anchors = { center_in = true }, fill = true }),
    }
    return kit.hover(area, function(hovered)
      local base = strong and C.primary or C.secondaryContainer
      return hovered and base:mix(strong and C.onPrimary or C.onSecondaryContainer, 0.08) or base
    end, radius)
  end
  local LOOPS = { none = "playlist", playlist = "track", track = "none" }
  local controls = ui.Row {
    id = "media-controls",
    x = 343, y = 221, gap = 4,
    button("media-shuffle", "shuffle", 40, 20, function() control("set_shuffle", not active().shuffle) end,
      false, function() return active().shuffle end, function() return active().ignores_shuffle end),
    button("media-tab-previous", "skip_previous", 52, 26, function() control("previous") end),
    button("media-tab-play", function() return playing() and "pause" or "play_arrow" end, 108, 14,
      function() control("play_pause") end, true),
    button("media-tab-next", "skip_next", 52, 26, function() control("next") end),
    button("media-repeat", function() return active().loop == "track" and "repeat_one" or "repeat" end, 40, 20,
      function() control("set_loop", LOOPS[active().loop or "none"] or "none") end,
      false, function() return (active().loop or "none") ~= "none" end, function() return active().ignores_loop end),
  }

  -- The player's own volume (MPRIS), under the controls.
  local volume = ui.Row {
    id = "media-volume",
    x = 343, y = 288, gap = 10, align = "center",
    kit.icon(function()
      local v = active().volume or 0
      return v <= 0 and "volume_off" or v < 0.5 and "volume_down" or "volume_up"
    end, 22, function() return C.onSurfaceVariant end),
    kit.slider {
      id = "media-volume-slider", width = 250, height = 22,
      value = function() return math.max(0, math.min(1, active().volume or 0)) end,
      set = function(v) control("set_volume", v) end,
    },
  }

  local track = ui.Item {
    id = "media-track",
    width = M.WIDTH, height = M.HEIGHT,
    visible = something,
    ui.Column {
      x = 343, y = 44, gap = 2,
      kit.text {
        id = "media-tab-title", width = 330, elide = "right",
        text = field("title"), font_size = 28, font_weight = 500,
      },
      kit.text {
        id = "media-tab-artist", width = 330, elide = "right",
        text = field("artist"), font_size = theme.size.large,
        color = function() return C.secondary end,
      },
      kit.text {
        id = "media-tab-album", width = 330, elide = "right",
        text = field("album"), font_size = theme.size.large,
        color = function() return C.secondary end,
      },
    },
    kit.text {
      id = "media-position",
      x = 343, y = BAR_Y - 10, width = 40,
      text = function() return media.duration(active().position) end,
      font_size = theme.size.normal,
    },
    progress,
    kit.text {
      id = "media-length",
      x = BAR_X + BAR_W + 8, y = BAR_Y - 10,
      text = function() return media.duration(active().length) end,
      font_size = theme.size.normal,
    },
    controls,
    volume,
  }

  -- --------------------------------------------------------- lyrics --
  local LX, LW = 680, 290
  local function lyric_line(offset)
    return kit.text {
      width = LW, horizontal_alignment = "center", elide = "right",
      text = function()
        local f = lyrics()
        if not f then return "" end
        local lines = f.lines:get()
        local line = lines[f.index:get() + offset]
        return line and line.text or ""
      end,
      font_size = offset == 0 and theme.size.large or theme.size.normal,
      font_weight = offset == 0 and 500 or 400,
      color = function() return offset == 0 and C.primary or C.onSurfaceVariant end,
      opacity = offset == 0 and 1 or (math.abs(offset) == 1 and 0.75 or 0.45),
    }
  end
  local function lyrics_status()
    local f = lyrics()
    return f and f.status:get() or "none"
  end
  local players_open = morf.signal("caelestia.media.players_open", false)
  local players = media.players
  local player_rows = {}
  for i = 1, 5 do
    local function row() return players()[i] end
    player_rows[#player_rows + 1] = kit.hover(ctx.area {
      id = "media-player-" .. i, width = 245, height = 36, cursor = "pointer",
      visible = function() return row() ~= nil end,
      on_clicked = function()
        local r = row()
        if r then control("set_active", r.name) end
        players_open:set(false)
      end,
      kit.text {
        x = 16, anchors = { vertical_center = true },
        text = function() local r = row() return r and (r.identity ~= "" and r.identity or r.name) or "" end,
      },
    }, function(hovered) return hovered and C.onSurface:alpha(0.08) or C.onSurface:alpha(0) end, 10)
  end
  local side = ui.Item {
    id = "media-lyrics",
    x = LX, width = M.WIDTH - LX, height = M.HEIGHT,
    visible = something,
    ui.Row {
      x = 12, y = 20, gap = 10, align = "center",
      kit.icon("lyrics", 22, function() return C.onSurface end),
      kit.heading { id="media-lyrics-title", text = "Lyrics", active=on_screen, level="section", font_size = theme.size.large, font_weight = 500 },
    },
    kit.hover(ctx.area {
      id = "media-lyrics-menu",
      x = 268, y = 13, width = 40, height = 40, cursor = "pointer",
      kit.icon("more_vert", 22, function() return C.onSurface end, { anchors = { center_in = true } }),
    }, function(hovered) return hovered and C.surfaceContainerHighest or C.surfaceContainerHigh end, 12),
    ui.Column {
      x = 0, y = 70, gap = 6, align = "center",
      visible = function() local s = lyrics_status() return s == "synced" or s == "plain" end,
      lyric_line(-2), lyric_line(-1), lyric_line(0), lyric_line(1), lyric_line(2),
    },
    ui.Column {
      id = "media-no-lyrics",
      x = 0, y = 78, width = LW, gap = 10, align = "center",
      visible = function() local s = lyrics_status() return s ~= "synced" and s ~= "plain" end,
      ui.Item {
        width = LW, height = 64,
        kit.icon("sentiment_dissatisfied", 64, function() return C.onSurfaceVariant end, {
          anchors = { center_in = true },
          visible = function() return lyrics_status() ~= "searching" end,
        }),
        kit.loading(52, function() return C.primary end, {
          id = "media-lyrics-loading",
          anchors = { center_in = true },
          active = function() return ctx.opened() and lyrics_status() == "searching" end,
          visible = function() return lyrics_status() == "searching" end,
        }),
      },
      kit.text {
        width = LW, horizontal_alignment = "center",
        text = function() return lyrics_status() == "searching" and "Looking for lyrics" or "No lyrics found" end,
        font_size = theme.size.large, color = function() return C.onSurfaceVariant end,
      },
    },
    kit.hover(ctx.area {
      id = "media-player",
      x = 30, y = 264, width = 245, height = 40, cursor = "pointer",
      on_clicked = function() players_open:set(not players_open:get()) end,
      ui.Row {
        anchors = { center_in = true }, gap = 8, align = "center",
        kit.icon("video_library", 18, function() return C.onSecondaryContainer end, { fill = true }),
        kit.text {
          id = "media-player-name",
          text = function()
            local a = active()
            return (a.identity and a.identity ~= "") and a.identity or (a.name or "")
          end,
          color = function() return C.onSecondaryContainer end,
        },
      },
    }, function(hovered)
      return hovered and C.secondaryContainer:mix(C.onSecondaryContainer, 0.08) or C.secondaryContainer
    end, 20),
    kit.hover(ctx.area {
      id = "media-player-more",
      x = 278, y = 264, width = 40, height = 40, cursor = "pointer",
      on_clicked = function() players_open:set(not players_open:get()) end,
      kit.icon(function() return players_open:get() and "expand_less" or "expand_more" end, 20,
        function() return C.onSecondaryContainer end, { anchors = { center_in = true } }),
    }, function(hovered)
      return hovered and C.secondaryContainer:mix(C.onSecondaryContainer, 0.08) or C.secondaryContainer
    end, 12),
    kit.surface {
      id = "media-player-menu",
      x = 30, z = 10, width = 245, radius = 12,
      height = function() return #players() * 36 + 8 end,
      y = function() return 258 - (#players() * 36 + 8) end,
      color = function() return C.surfaceContainerHighest end,
      visible = function() return players_open:get() end,
      ui.Column { y = 4, gap = 0, table.unpack(player_rows) },
    },
  }

  return ui.Item {
    id = "dashboard-media-tab",
    width = M.WIDTH, height = M.HEIGHT,
    ui.Item(backdrop),
    kit.heading {id="media-playback-heading",text="Playback",active=on_screen,level="caption",x=343,y=12,width=300},
    ring,
    cover,
    cover_icon,
    nothing,
    track,
    side,
  }
end

-- Built as the module loads, with its own instruction budget.
M.page = M.build(require("dashboard_state").context(2))

return M
