-- Settings, Session: you, the lock screen and what happens when the desk is
-- left alone (SessionSection).
--
-- The name and picture are read from services/account.lua; a name or
-- picture chosen here is kept in impasto's settings (`userName`,
-- `userAvatar`) and shown on the lock, and the account itself is left as
-- it is. A picture is given by dropping an image on its row or by typing
-- its path; the original's file dialog has no counterpart in the engine.
-- Face unlock (howdy) is not part of this port.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local account = require("services.account")
local wallpaper = require("services.wallpaper")
local thumbnails = require("services.thumbnails")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")

local C = theme.color

local M = {}

local IMAGES = { png = true, jpg = true, jpeg = true, webp = true, bmp = true }

local function avatar(size)
  return ui.ClipRect {
    width = size, height = size, radius = size / 2, color = C.islandSurfaceHover,
    border_width = 2, border_color = C.hairline, content_under_border = true,
    ui.Image {
      anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
      source_width = size * 2, source_height = size * 2,
      source = function() return account.avatar() end,
      visible = function() return account.avatar() ~= "" end,
    },
    kit.text {
      anchors = { center_in = true }, text = account.initials,
      size = math.floor(size * 0.36 + 0.5), weight = 300,
      visible = function() return account.avatar() == "" end,
    },
  }
end

-- On the account itself where the machine can (services/account.lua), in
-- Settings where it cannot.
local function set_picture(path)
  local clean = tostring(path or ""):gsub("^file://", ""):match("^%s*(.-)%s*$")
  if clean == "" then account.clear_picture() return end
  local ext = (clean:match("%.([%w]+)$") or ""):lower()
  if IMAGES[ext] and morf.fs.is_file(clean) then account.set_picture(clean) end
end

local function you_rows(W)
  local over = controls.signal("session.drop", false)
  local picture_row = ui.Item {
    width = W, height = 72,
    setting.wheel_area(),
    ui.Rect { anchors = { fill = true, margins = 4 }, radius = theme.radius_small,
      color = C.islandSurfaceHover, visible = function() return over:get() end },
    ui.Row {
      x = 14, anchors = { vertical_center = true }, gap = 16, align = "center",
      avatar(48),
      setting.label {
        label = "Picture", width = W - 28 - 48 - 16 - 16 - 100,
        reading = function()
          if account.busy() == "picture" then return "Changing it on the account…" end
          if account.failure() ~= "" then return "Not changed: " .. account.failure() end
          if settings.userAvatar ~= "" then return "The lock screen's, chosen here" end
          if account.avatar() ~= "" then return "The account's own" end
          return "Drop an image here, or type its path below"
        end,
      },
    },
    ui.Item {
      anchors = { right = true, right_margin = 14, vertical_center = true }, width = 92, height = 30,
      visible = function() return account.avatar() ~= "" end,
      controls.pill { text = "Clear", icon = "󰜉", height = 30, width = 92,
        on_click = function()
          if settings.userAvatar ~= "" then settings.set("userAvatar", "") else account.clear_picture() end
        end },
    },
    ui.DropArea {
      anchors = { fill = true }, keys = { "files" },
      on_entered = function() over:set(true) end,
      on_exited = function() over:set(false) end,
      on_dropped = function(drop)
        local first = drop.paths and drop.paths[1] or (drop.uris and drop.uris[1])
        set_picture(first)
      end,
    },
  }
  return {
    width = W, title = "You",
    note = "Your name and picture, on the lock screen.",
    hint = "Changed on the account itself, so the lock and the login screen agree: the name through AccountsService, the picture through the setup's helper. Where the machine has neither, it is kept by the shell.",
    picture_row,
    setting.field { width = W, label = "Picture file", placeholder = "A path to an image",
      value = settings.userAvatar,
      on_edited = function(text) set_picture(text) end },
    setting.field { width = W, label = "Name", placeholder = account.full_name ~= "" and account.full_name or account.user,
      value = settings.userName ~= "" and settings.userName or account.full(),
      on_edited = function(text) account.set_name(text) end },
  }
end

local function lock_part(W)
  local LockClock = require("lock.clock")
  local clock_tiles = {}
  for _, style in ipairs { { id = "stacked", label = "Stacked" }, { id = "inline", label = "Inline" } } do
    clock_tiles[#clock_tiles + 1] = function(tw)
      return setting.tile {
        width = tw, stage_height = 120, caption = style.label,
        selected = function() return settings.lockClock == style.id end,
        on_picked = function() settings.set("lockClock", style.id) end,
        stage = function()
          return ui.Item {
            anchors = { fill = true },
            ui.Image {
              anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
              source = function() return thumbnails.of(wallpaper.current:get(), 320, 200, 4) end,
            },
            ui.Rect { anchors = { fill = true }, color = "#00000055" },
            LockClock {
              anchors = { center_in = true },
              style = function() return style.id end,
              -- 9:41 on the first of January, as the original's preview.
              at = 1767260460,
              scale = style.id == "stacked" and 0.17 or 0.2,
            },
          }
        end,
      }
    end
  end
  local blur_sigma = function() return math.max(0.5, settings.lockBlur / 6) end
  return {
    setting.group(you_rows(W)),
    setting.group {
      width = W, title = "Clock", note = "The login screen always draws it stacked.",
      setting.tiles { width = W, label = "Style", tiles = clock_tiles },
    },
    setting.group {
      width = W, title = "Background",
      note = "Just enough to make the text underneath unreadable.",
      hint = "The preview uses the wallpaper, since the lock screen's own photograph of the desk is taken when it locks. It is blurred the same way.",
      setting.slider { width = W, label = "Blur", from = 8, to = 64, unit = " px",
        value = function() return settings.lockBlur end,
        on_moved = function(v) settings.set("lockBlur", v) end },
      setting.block { width = W,
        ui.ClipRect {
          width = W - 28, height = 160, radius = theme.radius_medium, color = C.island,
          ui.Image {
            anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
            -- A quarter-size copy, blurred as the lock blurs its own.
            source = function()
              local path = wallpaper.current:get()
              return thumbnails.of(path, math.floor((W - 28) / 2), 80, blur_sigma() / 2)
            end,
          },
          kit.text { anchors = { center_in = true }, text = "No wallpaper to show",
            size = theme.size.small, color = C.textMuted,
            visible = function() return wallpaper.current:get() == "" end },
          ui.Rect {
            anchors = { center_in = true }, width = 196, height = 38, radius = 19, color = C.island,
            kit.text { anchors = { center_in = true }, text = "Type to unlock", size = theme.size.small,
              color = C.textMuted },
          },
        },
      },
    },
  }
end

local function minutes(value) return value == 0 and "Never" or (value .. " min") end

local function idle_part(W)
  return {
    setting.group {
      width = W, title = "When you leave it alone", note = "All three are off by default.",
      hint = "The shell uses the compositor's idle notifications, and media that inhibits idle holds all three off.",
      setting.slider { width = W, label = "Lock after", from = 0, to = 60, unit = " min",
        value = function() return settings.idleLock end,
        reading = function() return minutes(settings.idleLock) end,
        on_moved = function(v) settings.set("idleLock", v) end },
      -- Warns when the screen would go dark before the lock: the lock
      -- photographs the desk as it goes up.
      setting.slider { width = W, label = "Screen off after", from = 0, to = 60, unit = " min",
        figure_width = 150,
        value = function() return settings.idleScreen end,
        reading = function()
          local screen, lock = settings.idleScreen, settings.idleLock
          if screen == 0 then return "Never" end
          if lock == 0 or screen >= lock then return screen .. " min" end
          return screen .. " min · before the lock"
        end,
        on_moved = function(v) settings.set("idleScreen", v) end },
      setting.slider { width = W, label = "Suspend after", from = 0, to = 60, unit = " min",
        value = function() return settings.idleSuspend end,
        reading = function() return minutes(settings.idleSuspend) end,
        on_moved = function(v) settings.set("idleSuspend", v) end },
    },
  }
end

function M.build(page)
  return setting.parts(page, {
    { id = "lock", build = lock_part },
    { id = "idle", build = idle_part },
  })
end

return M
