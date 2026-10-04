-- The picker composites (library/lib/kit/composites/: date_picker,
-- time_picker, colour_picker, font_picker, emoji_picker, calendar) worked
-- by the pointer and by the keyboard, in each theme.
--
--     morf test --no-dbus examples/shells/caelestia/tests/composites_pickers_spec.lua

local test = morf.test

local SOURCE = [[
  local ui = require("morf.ui")
  local kit = require("kit")
  local composites = require("lib.kit.composites")
  morf.surface.height = 900
  local got = {}
  local function keep(name) return function(...) got[name] = table.concat({ ... }, " ") end end
  local date = composites.date_picker { id = "date", value = "2026-10-03", on_changed = keep("date") }
  local dates = composites.date_picker { id = "dates", range = true, on_changed = keep("dates") }
  local time = composites.time_picker { id = "time", value = "14:30", on_changed = keep("time") }
  local clock = composites.time_picker { id = "clock", inline = true, twelve_hour = true, value = "09:05",
    on_changed = keep("clock") }
  local colour = composites.colour_picker { id = "colour", width = 300, height = 300, value = "#3366cc",
    on_changed = keep("colour") }
  local font = composites.font_picker { id = "font", width = 320, height = 300,
    fonts = { "Rubik", "IBM Plex Mono", "JetBrains Mono", "Noto Sans", "Noto Serif", "Adwaita Sans" },
    on_picked = keep("font") }
  local emoji = composites.emoji_picker { id = "emoji", width = 340, height = 300, on_picked = keep("emoji") }
  local cal_node, cal = composites.calendar { id = "cal", width = 280, value = "2026-10-03",
    marked = function(d) return d == "2026-10-20" end, on_changed = keep("cal"), on_picked = keep("cal_picked") }
  kit.card { width = 1400, height = 900,
    ui.Item { x = 20, y = 20, date }, ui.Item { x = 260, y = 20, dates },
    ui.Item { x = 520, y = 20, time }, ui.Item { x = 720, y = 20, clock },
    ui.Item { x = 20, y = 460, colour }, ui.Item { x = 360, y = 460, font },
    ui.Item { x = 720, y = 460, emoji }, ui.Item { x = 1080, y = 20, cal_node } }
  morf.ipc.got = function(name) return got[name] or "" end
  morf.ipc.month = function() return cal.month() end
]]

local function load(style)
  test.load("../shell/init.lua", { size = { 1400, 900 }, env = { CAELESTIA_STYLE = style }, source = SOURCE })
  test.settle(600)
end
local function got(name) return test.ipc("got", name) end
local function shot(name) if morf.env("MORF_THEME_SNAPSHOTS") == "1" then test.snapshot(name .. ".png") end end
local function clean()
  test.eq(#test.logs("error"), 0, "errors were logged")
  test.eq(#test.logs("warn"), 0, "warnings were logged")
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " date picker picks by pointer and keys, and turns months", function()
    load(style)
    test.click("date-field") test.settle(400)
    shot(style .. "-pickers-date")
    test.truthy(test.get("date-day-2026-10-14").visible)
    test.click("date-day-2026-10-14") test.settle(300)
    test.eq(got("date"), "2026-10-14")
    -- The keys: Right, Down across into November, Return picks and shuts.
    test.click("date-field") test.settle(400)
    test.key("Right") test.key("Down") test.key("Down") test.key("Down") test.settle(400)
    test.key("Return") test.settle(300)
    test.eq(got("date"), "2026-11-05")
    -- The arrows by the title turn the month; Page Down too, keeping the day.
    test.click("date-field") test.settle(400)
    test.click("date-next") test.settle(500)
    test.click("date-day-2026-12-24") test.settle(300)
    test.eq(got("date"), "2026-12-24")
    test.click("date-field") test.settle(400)
    test.key("Page_Down") test.settle(400) test.key("Home") test.key("Return") test.settle(300)
    test.eq(got("date"), "2027-01-01")
    -- A range: two picks, either way round.
    test.click("dates-field") test.settle(400)
    test.click("dates-day-" .. morf.time.format("%Y-%m-") .. "12") test.settle(100)
    test.key("Left") test.key("Left") test.key("Return") test.settle(300)
    local from, to = got("dates"):match("(%S+) (%S+)")
    test.truthy(from and to and from < to, got("dates"))
    clean()
  end)

  test.it(style .. " time picker steps by pointer and keys", function()
    load(style)
    test.click("time-field") test.settle(400)
    shot(style .. "-pickers-time")
    test.click("time-hours-up") test.settle(50)
    test.eq(got("time"), "15:30")
    test.key("Tab") test.key("Down") test.key("Down") test.settle(50)
    test.eq(got("time"), "15:28")
    test.type("45") test.settle(50)
    test.eq(got("time"), "15:45")
    test.key("Escape") test.settle(300)
    -- Twelve hours: 9 AM up to 10, then PM.
    test.click("clock-hours-up") test.settle(50)
    test.eq(got("clock"), "10:05")
    local meridiem = test.get("clock-meridiem")
    test.click(meridiem.x + meridiem.width / 2, meridiem.y + meridiem.height - 10) test.settle(50)
    test.eq(got("clock"), "22:05")
    local minutes = test.get("clock-minutes")
    test.wheel(0, -1, { x = minutes.x + 10, y = minutes.y + 10 }) test.settle(50)
    test.eq(got("clock"), "22:06")
    clean()
  end)

  test.it(style .. " colour picker takes a drag, a hex and a swatch", function()
    load(style)
    local plane = test.get("colour-plane")
    test.drag({ plane.x + 10, plane.y + 10 }, { plane.x + plane.width * 0.5, plane.y + plane.height * 0.25 })
    test.settle(100)
    test.eq(got("colour"), "#6080bf")
    test.click("colour-hex") test.key("a", "ctrl") test.type("#ff8800") test.key("Return") test.settle(100)
    test.eq(got("colour"), "#ff8800")
    test.eq(test.get("colour-hex").text, "#ff8800")
    test.click("colour-swatch-5") test.settle(100)
    test.eq(got("colour"), "#39e639")
    test.key("Right") test.settle(100)
    test.eq(got("colour"), "#39e68f")
    -- The hue takes the arrows once dragged.
    local hue = test.get("colour-hue")
    test.click(hue.x + hue.width * 0.5, hue.y + hue.height / 2) test.settle(100)
    test.truthy(got("colour") ~= "#39e68f")
    shot(style .. "-pickers-colour")
    clean()
  end)

  test.it(style .. " font picker filters and picks by pointer and keys", function()
    load(style)
    test.click("font-search") test.type("mono") test.settle(100)
    test.truthy(test.get("font-option-1").visible)
    test.falsy(test.find("font-option-3"))
    test.key("Down") test.key("Down") test.key("Return") test.settle(100)
    test.eq(got("font"), "JetBrains Mono")
    test.click("font-option-1") test.settle(100)
    test.eq(got("font"), "IBM Plex Mono")
    test.click("font-search") test.key("a", "ctrl") test.type("serif") test.key("Return") test.settle(100)
    test.eq(got("font"), "Noto Serif")
    shot(style .. "-pickers-font")
    clean()
  end)

  test.it(style .. " emoji picker filters and picks by pointer and keys", function()
    load(style)
    test.click("emoji-category-food") test.settle(100)
    test.click("emoji-emoji-3") test.settle(100)
    test.eq(got("emoji"), "🍊 tangerine")
    test.click("emoji-search") test.type("pizza") test.key("Return") test.settle(100)
    test.eq(got("emoji"), "🍕 pizza")
    test.key("a", "ctrl") test.type("rocket") test.settle(100)
    test.key("Down") test.key("Return") test.settle(100)
    test.eq(got("emoji"), "🚀 rocket")
    shot(style .. "-pickers-emoji")
    clean()
  end)

  test.it(style .. " calendar marks days and walks months", function()
    load(style)
    test.truthy(test.get("cal-day-2026-10-20").visible)
    test.click("cal-day-2026-10-30") test.settle(100)
    test.eq(got("cal"), "2026-10-30")
    test.key("Right") test.key("Right") test.settle(400)
    test.eq(got("cal"), "2026-11-01")
    test.eq(test.ipc("month"), "2026-11")
    test.key("Page_Up") test.settle(400)
    test.eq(test.ipc("month"), "2026-10")
    test.eq(got("cal"), "2026-10-01")
    test.key("End") test.key("Return") test.settle(100)
    test.eq(got("cal_picked"), "2026-10-31")
    test.click("cal-previous") test.settle(400)
    test.eq(test.ipc("month"), "2026-09")
    test.truthy(test.get("cal-day-2026-09-15").visible)
    clean()
  end)
end
