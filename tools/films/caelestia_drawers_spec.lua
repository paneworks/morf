-- caelestia's sidebar and utilities, headless: each drawer at rest (to lay
-- beside the reference's shots) and filmed opening and closing, a frame
-- every 32 ms of virtual time. Only useful with snapshots on:
--
--     nixVulkanIntel morf test --no-dbus --snapshots DIR tools/films/caelestia_drawers_spec.lua
--
-- tools/films/strip.sh DIR NAME GEOMETRY lays a film's frames side by side.

local test = morf.test

local W, H = 1920, 1080

local function load()
  for _, program in ipairs { "systemctl", "loginctl" } do test.stub_run(program, { code = 0 }) end
  test.load("../../examples/shells/caelestia/shell/init.lua", {
    size = { W, H },
    env = { CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "", CAELESTIA_DRY_RUN = "1" },
  })
  test.settle(2000)
end

local function film(name, frames, step)
  for i = 0, frames - 1 do
    test.snapshot(("%s-%03d.png"):format(name, i), { surface = "screen" })
    test.advance(step or 32)
  end
end

local function notify()
  test.ipc("notify", "Download complete", "report.pdf has finished downloading", "normal", "Firefox")
  test.ipc("notify", "Battery low", "10% remaining, plug in the charger", "critical", "Battery")
  test.ipc("notify", "Alice", "are you coming tonight?", "normal", "Discord")
  test.advance(7000)
  test.settle(500)
end

test.describe("caelestia drawers", function()
  test.it("utilities: at rest, opening and closing", function()
    load()
    test.ipc("utilities", "open")
    film("utilities-open", 20)
    test.settle(1000)
    test.snapshot("rest-utilities.png", { surface = "screen" })
    test.ipc("utilities", "close")
    film("utilities-close", 12)
  end)

  test.it("utilities: the toggles and the switch", function()
    load()
    test.ipc("utilities", "open")
    test.settle(1500)
    test.click { id = "utilities-awake-switch" }
    film("utilities-switch", 16, 16)
    test.settle(1000)
    -- Off again, with the card settled (it shrinks after the thumb).
    test.click { id = "utilities-awake-switch" }
    film("utilities-switch-off", 16, 16)
    test.settle(1000)
    test.click { id = "utilities-awake-switch" }
    test.settle(1000)
    test.click { id = "utilities-toggle-dnd" }
    film("utilities-toggle", 16, 16)
    test.settle(1000)
    test.click { id = "utilities-toggle-gamemode" }
    test.click { id = "utilities-toggle-bluetooth" }
    test.settle(1000)
    test.snapshot("rest-utilities-on.png", { surface = "screen" })
    test.click { id = "utilities-recordings" }
    test.settle(1000)
    test.snapshot("rest-utilities-recordings.png", { surface = "screen" })
    test.click { id = "utilities-recordings" }
    test.settle(1000)
    -- The recorder's modes: the selection slides to the row under the
    -- pointer.
    test.click { id = "utilities-record-mode" }
    test.settle(800)
    local row = test.get { id = "utilities-mode-region" }
    test.move(row.x + row.width / 2, row.y + row.height / 2)
    film("utilities-mode", 14, 16)
  end)

  test.it("sidebar: empty, at rest, opening and closing", function()
    load()
    test.ipc("sidebar", "open")
    film("sidebar-open", 20)
    test.settle(1000)
    test.snapshot("rest-sidebar.png", { surface = "screen" })
    test.ipc("sidebar", "close")
    film("sidebar-close", 12)
  end)

  test.it("sidebar: with notifications, opening, a group opening, clearing", function()
    load()
    notify()
    test.ipc("sidebar", "open")
    film("sidebar-notifs-open", 20)
    test.settle(1000)
    test.snapshot("rest-sidebar-notifs.png", { surface = "screen" })
    test.click { id = "sidebar-group-expand-1" }
    film("sidebar-group", 14)
    test.settle(1000)
    test.snapshot("rest-sidebar-group.png", { surface = "screen" })
    test.click { id = "sidebar-clear" }
    film("sidebar-clear", 14)
  end)
end)
