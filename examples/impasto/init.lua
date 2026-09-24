-- impasto, on morf: a Hyprland shell whose colours come from a painting.
--
-- A port of https://github.com/andreumassanet/impasto (Quickshell/QML) to
-- morf, in Lua only. The layout follows the original's folders:
--
--   theme.lua            colours, sizes, type and motion
--   services/            the state of the machine and the shell's own
--   components/          the small pieces everything is built from
--   bar/                 the bar, the island and its panels and modules
--
-- Run it:
--
--   EXAMPLE=examples/impasto/init.lua oslo make run
--
-- Every screen runs this file once. The island is live on the screen being
-- worked on; the others show it at rest.

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local bar = require("bar.bar")
local island = require("bar.island")
local clock = require("bar.modules.clock")
local kit = require("components.kit")

local C = theme.color
local screen = (morf.screens or {})[1] or {}
local SCREEN_W = tonumber(screen.width) or 1920
local SCREEN_H = tonumber(screen.height) or 1080

-- ---------------------------------------------------------------- surface --

-- One surface along the top of the screen, as tall as the largest panel
-- needs. It is transparent where nothing is drawn, and its input region is
-- only where there are MouseAreas, so windows under the empty part still
-- get their clicks. The bar's own height is reserved; the island opens over
-- the windows.
morf.surface.namespace = "impasto-bar"
morf.surface.anchors = { top = true, left = true, right = true }
morf.surface.height = math.min(SCREEN_H, 820)
morf.surface.layer = "top"
morf.surface.keyboard_focus = "none"
morf.surface.exclusive_zone = -1
morf.surface.reserve = { top = theme.bar_reserve() }
morf.surface.backdrop = false

-- A click beside an open island closes it, and so does Escape; while a panel
-- is open the keyboard is the island's.
morf.effect("impasto.keyboard", function()
  local open = island.state.expanded()
  morf.surface.keyboard_focus = open and "exclusive" or "none"
  morf.surface.backdrop = open
end)
morf.on_backdrop_click(function() island.close() end)

-- ------------------------------------------------------------------ layers --

island.register_layer("modules", {
  size = function()
    return settings.clockShowsDate and 240 or 150, theme.capsule_height(), 0
  end,
  build = clock.build,
})

-- The control centre, until its port lands: the time and a close button,
-- enough to see the morph.
island.register("controls", {
  size = function() return 560, 420 end,
  build = function()
    return ui.Column {
      gap = 12,
      kit.text { text = clock.text, size = theme.size.widget, weight = 700 },
      kit.text { text = "Control centre", color = C.textMuted },
    }
  end,
})

-- ------------------------------------------------------------------ pieces --

local function button_piece(glyph, panel)
  return {
    build = function()
      return kit.icon_button {
        glyph = glyph, diameter = theme.capsule_height() - 6,
        color = "#00000000", hover_color = C.islandSurfaceHover,
        on_click = function() island.toggle(panel) end,
      }
    end,
  }
end

bar.register("launcher", button_piece("󰍉", "launcher"))
bar.register("overview", button_piece("󰕰", "overview"))
bar.register("notifications", button_piece("󰂚", "notifications"))
bar.register("network", button_piece("󰤨", "wifi"))
bar.register("bluetooth", button_piece("󰂯", "bluetooth"))
bar.register("volume", button_piece("󰕾", "volume"))
bar.register("battery", button_piece("󰁹", "battery"))

-- ----------------------------------------------------------------- plug-ins --

-- Every file in these folders registers itself when required: a panel with
-- `island.register`, a bar piece with `bar.register`, a layer with
-- `island.register_layer`. New parts need no line here, and a part that
-- fails to load is reported by `morf ipc call failed` instead of taking the
-- shell down with it.
local failed = {}
local root = morf.shell_dir()
for _, folder in ipairs { "services/auto", "bar/modules", "bar/pieces", "bar/layers", "bar/panels" } do
  local entries = morf.fs.list(morf.fs.join(root, folder)) or {}
  for _, entry in ipairs(entries) do
    if entry.is_file and entry.extension == "lua" then
      local name = (folder .. "/" .. entry.name:sub(1, -5)):gsub("/", ".")
      local ok, err = pcall(require, name)
      if not ok then
        failed[#failed + 1] = name .. ": " .. tostring(err)
        morf.log("error", "impasto: " .. name .. " did not load: " .. tostring(err))
      end
    end
  end
end
morf.ipc.failed = function() return table.concat(failed, "\n") end

-- -------------------------------------------------------------------- IPC --

-- `morf ipc call <panel>` toggles it, as impasto's keybinds do through qs ipc.
for _, name in ipairs { "controls", "launcher", "overview", "wifi", "bluetooth", "session", "notes", "board", "games", "keys", "stats" } do
  morf.ipc[name] = function()
    island.toggle(name)
    return island.state.open_panel()
  end
end
morf.ipc.close = function() island.close() return "" end
morf.ipc.layer = function() return island.state.layer() end
morf.ipc.flash = function(label) island.state.flash("󰕾", label or "Volume", 0.6) return "ok" end

-- ------------------------------------------------------------------- root --

ui.Item {
  width = SCREEN_W,
  height = morf.surface.height,
  bar.build(SCREEN_W),
}
