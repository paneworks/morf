-- caelestia's motion, frame by frame, headless: each test starts a
-- transition and snapshots the screen every 16 ms of virtual time, so the
-- frames can be laid side by side (target/p2tmp/strip.sh does). Only useful
-- with snapshots on:
--
--     nixVulkanIntel morf test --no-dbus --snapshots DIR examples/tests/caelestia_motion_spec.lua

local test = morf.test

local W, H = 1920, 1080

local function load()
  for _, program in ipairs { "systemctl", "loginctl" } do test.stub_run(program, { code = 0 }) end
  test.load("../caelestia/init.lua", {
    size = { W, H },
    env = { CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "", CAELESTIA_DRY_RUN = "1" },
  })
  test.settle(2000)
end

--- `frames` snapshots named NAME-000.png ..., one every `step` ms.
local function film(name, frames, step)
  for i = 0, frames - 1 do
    test.snapshot(("%s-%03d.png"):format(name, i), { surface = "screen" })
    test.advance(step or 16)
  end
end

test.describe("caelestia motion", function()
  test.it("workspaces: the disc rolls from 1 to 4 and back to 2", function()
    load()
    test.ipc("workspace", 4)
    film("ws-1-4", 24, 16)
    test.settle(1000)
    test.ipc("workspace", 2)
    film("ws-4-2", 24, 16)
  end)

  test.it("dashboard tabs: the indicator stretches ahead and draws in", function()
    load()
    test.ipc("dashboard", "open")
    test.settle(1500)
    test.click { id = "dashboard-tab-weather" }
    film("tab-1-4", 30, 16)
    test.settle(1500)
    test.click { id = "dashboard-tab-media" }
    film("tab-4-2", 30, 16)
  end)

  test.it("launcher: the highlight slides between rows", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">")
    test.settle(1000)
    test.key("Down")
    test.key("Down")
    test.key("Down")
    film("launcher-sel", 24, 16)
    test.settle(1000)
    test.key("Up")
    test.key("Up")
    film("launcher-sel-up", 20, 16)
  end)

  test.it("session: the drawer grows out of the frame, the focus morphs", function()
    load()
    test.ipc("session", "open")
    film("session-open", 30, 16)
    test.settle(1000)
    test.key("Down")
    film("session-focus", 24, 16)
  end)

  test.it("popouts: the network panel grows, then turns into Bluetooth", function()
    load()
    test.ipc("popout", "network")
    film("popout-open", 24, 16)
    test.settle(1000)
    test.ipc("popout", "bluetooth")
    film("popout-switch", 24, 16)
  end)

  test.it("calendar: today's marker morphs in as the dashboard opens", function()
    load()
    test.ipc("dashboard", "open")
    film("dashboard-open", 36, 16)
  end)

  test.it("loading: the weather's loading indicator cycles its shapes", function()
    load()
    test.ipc("dashboard", "open")
    test.settle(1500)
    film("loading", 40, 80)
  end)
end)
