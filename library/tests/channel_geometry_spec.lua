-- Data channels (morf.channel) read reactively and drawn by a Path with
-- `series`, and the marks morf.geometry draws with.
--
--     morf test library/tests/channel_geometry_spec.lua

local test = morf.test

local HOST = [[
  local ui = require("morf.ui")
  local ch = morf.channel { size = 4 }
  local frame = morf.channel { size = 3, mode = "frame" }
  local runs = 0
  local seen = morf.signal("test.seen", 0)
  morf.effect("test.peak", function() runs = runs + 1 seen:set(ch:peak() or 0) end)
  ui.Item { width = 200, height = 100,
    ui.Path { id = "plot", width = 100, height = 50, view_box = { 0, 0, 100, 50 },
      series = ch.id, plot = { kind = "steps" }, fill_color = "transparent", stroke_color = "#ffffff" },
    ui.Path { id = "bars", y = 50, width = 100, height = 50, series = frame.id,
      plot = { kind = "bars", width = 100, height = 50, gap = 2, radius = 4 } },
  }
  morf.ipc.push = function(v) ch:push(v) end
  morf.ipc.state = function() return { peak = seen:get(), list = ch:get(), last = ch:last(), len = ch:len(), runs = runs } end
  morf.ipc.frame = function() frame:set({ 0.2, 0.9, 2, 5 }) return frame:get() end
]]

local function load() test.load { source = HOST, size = { 200, 100 } } test.settle(50) end

test.it("a channel keeps its newest numbers and wakes what reads it", function()
  load()
  for _, v in ipairs { 1, 5, 2, 3, 4 } do test.ipc("push", v) end
  test.settle(50)
  local s = test.ipc("state")
  test.eq(s.len, 4) test.eq(s.last, 4) test.eq(s.peak, 5)
  test.eq(s.list[1], 5) test.eq(s.list[4], 4)
  test.truthy(s.runs >= 2, "the effect never re-ran")
  -- A frame keeps only as many as it holds, and is replaced whole.
  local f = test.ipc("frame")
  test.eq(#f, 3) test.near(f[3], 2, 1e-6)
  test.eq(#test.logs("error"), 0)
end)

test.it("geometry draws arcs, hatching, ticks, rulers, segments and plots", function()
  test.load { source = [[
    local g = morf.geometry
    morf.ipc.all = function()
      return {
        arc = g.arc(0, 0, 10, 0, 180),
        hatch = g.hatch(20, 10, 6),
        under = g.hatch_under(0, 10, { 5, 2 }, 20, 10, 6),
        ticks = g.ticks(50, 50, 40, 45, { from = -150, sweep = 300, count = 60, major = 5, major_r0 = 38 }),
        ruler = g.ruler(40, 6, { pitch = 8, major = 5 }),
        segments = g.segments(100, 6, 10, 3),
        plot = g.plot({ 0, 1, 0.5 }, { kind = "bars", width = 30, height = 10, gap = 0 }),
      }
    end
  ]] }
  local d = test.ipc("all")
  test.truthy(d.arc:match("^M0%.000 %-10%.000 A10"), d.arc)
  test.eq(select(2, d.arc:gsub("A", "")), 2)
  test.truthy(d.hatch:match("^M"), d.hatch)
  test.truthy(#d.under > 4, d.under)
  test.eq(select(2, d.ticks:gsub("M", "")), 61)
  test.eq(select(2, d.ruler:gsub("M", "")), 6)
  test.eq(select(2, d.segments:gsub("Z", "")), 10)
  test.eq(select(2, d.plot:gsub("Z", "")), 3)
end)
