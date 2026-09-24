-- Picks a widget's face by module, family and theme.
--
-- Port of Face.qml. Modern has one registry per family (squares, wides,
-- larges, bands), falling back to the wide face, then the square, for a
-- module without a row; Analogue has one registry whose faces lay
-- themselves out at any size. Notes and the spectrum are the same in both
-- themes. A face that fails to build is logged and drawn as its name, so
-- one broken face never takes the desk with it.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local desk = require("services.desktop")

local M = {}

local registries = {
  ["2x2"] = "desktop.faces.squares",
  ["4x2"] = "desktop.faces.wides",
  ["4x4"] = "desktop.faces.larges",
  ["8x2"] = "desktop.faces.bands",
}

local function registry(name)
  local ok, mod = pcall(require, name)
  if ok then return mod end
  morf.log("error", "impasto: " .. name .. " did not load: " .. tostring(mod))
  return {}
end

local function builder(ctx)
  local analogue = ctx.theme == "analogue" and ctx.id ~= "notes" and ctx.id ~= "spectrum"
  if analogue then
    local faces = registry("desktop.faces.analogue")
    if faces[ctx.id] then return faces[ctx.id] end
  end
  local own = registry(registries[ctx.family] or registries["4x2"])
  if own[ctx.id] then return own[ctx.id] end
  -- A module with no wide face shows its island detail (Wides.qml's
  -- fallback), not a square stretched to the width.
  local detail = registry("desktop.faces.module_detail")
  if ctx.family == "4x2" and detail.has and detail.has(ctx.id) then return detail.build end
  return registry(registries["4x2"])[ctx.id] or registry(registries["2x2"])[ctx.id]
end

local function placeholder(ctx, why)
  return ui.Item { width = ctx.width, height = ctx.height,
    kit.text { anchors = { center_in = true }, text = (desk.entry(ctx.id) or { name = ctx.id }).name,
      size = theme.size.small, color = ctx.ink.muted },
    kit.text { anchors = { bottom = true, left = true, margins = 12 }, text = why or "",
      size = theme.size.label, color = ctx.ink.dim },
  }
end

--- The face for `ctx` (see desktop/faces/common.lua for its fields).
function M.build(ctx)
  local build = builder(ctx)
  if not build then return placeholder(ctx, "no face") end
  local ok, node = pcall(build, ctx)
  if ok and node then return node end
  morf.log("error", "impasto: the " .. ctx.id .. " face (" .. ctx.family .. ", " .. ctx.theme .. ") failed: " .. tostring(node))
  return placeholder(ctx, "failed")
end

return M
