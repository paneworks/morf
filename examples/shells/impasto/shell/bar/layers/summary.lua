-- The glance: the time large, the day, what is playing and the readings,
-- with nothing to press. It opens under a resting pointer; a click anywhere
-- opens the control centre (the island's own area takes it).
--
-- Port of IslandSummary.qml, sized by `modules.summary_width` and
-- `summary_height` (taller while a player is there).

local ui = require("morf.ui")
local theme = require("theme")
local settings = require("services.settings")
local island = require("bar.island")
local modules = require("services.modules")
local media = require("services.media")
local battery = require("services.battery")
local audio = require("services.audio")
local kit = require("components.kit")
local controls = require("components.controls")
local bars = require("components.spectrum")
local card = require("bar.controls.media_card")
local list = require("bar.controls.notification_list")

local C = theme.color

-- The day and the month in the shell's language (`settings.language`), as
-- Qt.locale(language) spells them: impasto ships English and Spanish.
local NAMES = {
  en = {
    days = { "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday" },
    months = { "January", "February", "March", "April", "May", "June", "July", "August",
      "September", "October", "November", "December" },
  },
  es = {
    days = { "domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado" },
    months = { "enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto",
      "septiembre", "octubre", "noviembre", "diciembre" },
  },
}

local function names() return NAMES[settings.language] or NAMES.en end

local function day_name()
  return names().days[(tonumber(morf.time.format("%w")) or 0) + 1]
end

local function day_and_month()
  return morf.time.format("%-d") .. " " .. names().months[tonumber(morf.time.format("%m")) or 1]
end

-- One reading: a glyph and a figure. Built only while it shows: a hidden
-- child keeps its room in a Row, and no size is ever zero.
local function reading(values)
  return ui.Loader {
    active = values.shows,
    source = function()
      return ui.Row {
        gap = 6, align = "center", height = 18,
        kit.glyph { glyph = values.glyph, size = 13, color = values.tint or C.textMuted },
        kit.text { text = values.text, size = theme.size.small, color = C.textMuted },
      }
    end,
  }
end

island.register_layer("summary", {
  size = function() return modules.summary_width, modules.summary_height(), 0 end,
  build = function()
    local w = modules.summary_width
    local inner = w - 36
    local readings = {
      gap = 16, align = "center",
      reading { shows = battery.available, glyph = battery.icon,
        text = function() return battery.percent() .. "%" end,
        tint = function() return modules.tint_of("battery") end },
      reading { shows = audio.ready, glyph = audio.icon,
        text = function() return audio.muted() and "Muted" or (audio.volume() .. "%") end },
      reading { shows = function() return list.count:get() > 0 end, glyph = "󰂚",
        text = function() return tostring(list.count:get()) end },
      reading { shows = function() return modules.runs("recorder") end, glyph = "●",
        text = function() return modules.value_of("recorder") end, tint = C.indicatorBad },
      reading { shows = function() return modules.runs("timer") end, glyph = "󰔛",
        text = function() return modules.value_of("timer") end },
    }
    local time = kit.text {
      text = function()
        local pattern = settings.clockFormat
        if pattern:find("%%[STr]") then morf.clock:get() else morf.minute_clock:get() end
        return morf.time.format(pattern)
      end,
      size = 34, weight = 600,
    }
    local weather = modules.providers.weather
    return ui.Item {
      anchors = { fill = true },
      ui.Column {
        anchors = { left = true, top = true, left_margin = 18, top_margin = 14 },
        gap = 11,
        ui.Item {
          width = inner, height = 42,
          ui.Row {
            anchors = { left = true, vertical_center = true }, gap = 12, align = "center",
            time,
            ui.Column {
              gap = 0,
              kit.text { text = function() morf.hour_clock:get() return day_name() end,
                size = theme.size.small, color = C.textMuted },
              kit.text { text = function() morf.hour_clock:get() return day_and_month() end,
                size = theme.size.medium, weight = 600 },
            },
          },
          -- Only when a weather module has been ported and reads something.
          ui.Row {
            anchors = { right = true, vertical_center = true }, gap = 6, align = "center",
            visible = function() return weather ~= nil and modules.has("weather") end,
            kit.glyph { glyph = function() return modules.glyph_of("weather") end, size = 20 },
            kit.text { text = function() return modules.value_of("weather") end,
              size = theme.size.medium, weight = 600 },
          },
        },
        controls.hairline { width = inner },
        -- The player takes its row, and the gap under it, only when there is one.
        ui.Column {
          gap = 0,
          ui.Loader {
            active = media.available,
            source = function()
              return ui.Item {
                width = inner, height = 45,
                ui.Row {
                  anchors = { left = true, top = true }, gap = 10, align = "center",
                  card.art { size = 34, glyph_size = 17 },
                  ui.Column {
                    gap = 1,
                    kit.text { text = function()
                        local title = media.title()
                        return title ~= "" and title or media.identity()
                      end,
                      size = theme.size.small, weight = 600, width = inner - 34 - 10 - 40, elide = "right" },
                    kit.text { text = media.artist, size = theme.size.small, color = C.textMuted,
                      width = inner - 34 - 10 - 40, elide = "right" },
                  },
                },
                ui.Item { anchors = { right = true, top = true, top_margin = 10 }, width = 30, height = 14,
                  bars { height = 14, bar_width = 2, bars = 6 } },
              }
            end,
          },
          ui.Row(readings),
        },
      },
    }
  end,
})

-- `morf ipc call glance` shows it (and again hides it), as resting the
-- pointer on the island does.
morf.ipc.glance = function()
  local on = island.state.layer() ~= "summary"
  island.state.set_summary(on)
  return on and "summary" or "modules"
end
