-- Pending updates on a chip: the count; the detail says what is pending
-- and when it was checked, and opens the packages panel on its updates.
-- No upgrade button here: pacman wants a terminal and a password, which
-- the panel's Update everything provides.
--
-- Port of UpdatesModule.qml.

local ui = require("morf.ui")
local theme = require("theme")
local modules = require("services.modules")
local kit = require("components.kit")
local controls = require("components.controls")
local updates = require("services.updates")

local C = theme.color

local function title()
  local n = updates.count()
  if n == 0 then return updates.checking() and "Checking…" or "Up to date" end
  return n == 1 and "1 update" or (n .. " updates")
end

local function subtitle()
  local parts = {}
  -- The fallback reads the last synced database; checkupdates is current.
  if updates.tool() == "pacman" then parts[#parts + 1] = "as of the last sync" end
  if updates.age() ~= "" then parts[#parts + 1] = "checked " .. updates.age() end
  return table.concat(parts, " · ")
end

modules.define("updates", {
  glyph = function() return "󰏖" end,
  value = function() return tostring(updates.count()) end,
  has = updates.available,
  -- The chip keeps the count current while it is on the bar
  -- (ModuleService.qml:127-131), not only while the detail is open.
  watch = function(on) if on then updates.subscribe() else updates.release() end end,
  detail = function()
    local w = modules.entry("updates").width
    local inner = w - 28
    updates.subscribe()
    local check = controls.pill {
      height = 28,
      text = function() return updates.checking() and "Checking…" or "Check" end,
      enabled = function() return not updates.checking() end,
      on_click = function() updates.refresh() end,
    }
    local open = controls.pill {
      height = 28, text = "Open",
      active = function() return updates.count() > 0 end,
      on_click = function()
        require("services.packages").set_view("updates")
        modules.request_panel("packages")
      end,
    }
    local text_w = function()
      return inner - 44 - 13 - (check.layout_width or 60) - (open.layout_width or 60) - 20
    end
    return ui.Item {
      anchors = { fill = true },
      on_destroyed = function() updates.release() end,
      ui.Column {
        anchors = { left = true, top = true, left_margin = 14, top_margin = 12 },
        gap = 10,
        ui.Row {
          gap = 13, align = "center",
          controls.ring {
            size = 44, thickness = 2.5, progress = 0, track_color = C.indicatorDim,
            kit.glyph {
              anchors = { center_in = true }, glyph = "󰏖", size = 18,
              color = function() return updates.count() > 0 and C.indicator or C.textMuted() end,
            },
          },
          ui.Column {
            gap = 2,
            kit.text { text = title, size = theme.size.medium, weight = 600, width = text_w, elide = "right" },
            kit.text { text = subtitle, size = theme.size.small, color = C.textMuted, width = text_w, elide = "right" },
          },
          ui.Row { gap = 10, align = "center", check, open },
        },
        kit.text {
          visible = function() return updates.count() > 0 end,
          text = function() return table.concat(updates.names(12), "  ") end,
          width = inner, elide = "right", mono = true, size = theme.size.label, color = C.textMuted,
        },
      },
    }
  end,
})
