-- A counter, small enough to test every way `morf test` can drive a
-- configuration: a click, a key, typed text, the wheel, a timer, an
-- animation, a command it runs, and IPC. `counter_spec.lua` is its spec:
--
--     morf test examples/demos/tests/counter_spec.lua

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.namespace = "counter"
morf.surface.anchors = {}
morf.surface.width = 320
morf.surface.height = 200

local count = morf.signal("counter.count", 0)
local greeting = morf.signal("counter.greeting", "")
local host = morf.signal("counter.host", "?")

-- A second after it starts, the counter says it is ready.
local ready = morf.signal("counter.ready", false)
morf.timer(1000, function() ready:set(true) end, false)

-- What `uname -n` says, once: a command a spec can stub.
morf.run({ "uname", "-n" }, function(result)
  host:set(result.ok and (result.stdout:gsub("%s+$", "")) or "unknown")
end)

morf.ipc.count = function() return count:get() end
morf.ipc.reset = function() count:set(0) return 0 end
morf.ipc.host = function() return host:get() end

ui.Rect {
  id = "panel",
  anchors = { fill = true },
  color = "#1d2021",
  radius = 12,
  ui.Column {
    x = 16, y = 16, gap = 8,
    ui.Text {
      id = "count",
      text = function() return "count: " .. count:get() end,
      color = "#ebdbb2", font_size = 18,
    },
    ui.Text {
      id = "status",
      text = function() return ready:get() and "ready" or "starting" end,
      color = "#a89984", font_size = 12,
    },
    ui.Text {
      id = "host",
      text = function() return "on " .. host:get() end,
      color = "#a89984", font_size = 12,
    },
    ui.Rect {
      id = "button",
      width = 120, height = 32, radius = 8,
      color = "#458588",
      ui.MouseArea {
        id = "increment",
        anchors = { fill = true },
        on_clicked = function() count:set(count:get() + 1) end,
        on_wheel = function(_, _, _, _, _, step_y)
          count:set(count:get() + (step_y > 0 and 1 or -1))
        end,
      },
      ui.Text { text = "add one", x = 12, y = 7, color = "#ebdbb2", font_size = 14 },
    },
    ui.TextInput {
      id = "name",
      width = 200, height = 28,
      font_size = 14,
      color = "#ebdbb2",
      placeholder = "your name",
      on_accepted = function(text) greeting:set("hello, " .. text) end,
      on_escape = function() greeting:set("") end,
    },
    ui.Text {
      id = "greeting",
      text = function() return greeting:get() end,
      color = "#b8bb26", font_size = 14,
    },
  },
}
