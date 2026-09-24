-- The spectrum on a square: its bars and nothing else, rising from the
-- square's bottom in the look its row gives it. The same in both themes.
--
-- Port of faces/SpectrumFace.qml. Without a row -- the card's tile -- it
-- keeps a street from the tile's edges. Away (under a fullscreen window,
-- or `spectrumOnEmpty` on a busy workspace) it neither draws nor listens,
-- except while arranging.

local ui = require("morf.ui")
local theme = require("theme")
local desk = require("services.desktop")
local bars = require("desktop.spectrum_bars")

local M = {}

function M.build(ctx)
  local w, h = ctx.width, ctx.height
  local margin = ctx.row and 0 or theme.desktop_gutter
  local function row() return ctx.row and ctx.row() or nil end
  local function looks() return desk.spectrum_of(row()) end
  local function listening()
    if not ctx.row then return true end
    return desk.editing:get() or not desk.spectrum_away()
  end
  return ui.Item {
    width = w, height = h,
    bars.build {
      x = margin, y = margin, width = w - 2 * margin, height = h - 2 * margin,
      looks = looks, listening = listening, edge = "bottom",
    },
  }
end

return M
