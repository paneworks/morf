-- caelestia, headless: it loads, the drawers open and close over IPC and on
-- the pointer, the launcher searches, and snapshots of each state.
--
--     morf test --no-dbus examples/tests/caelestia_spec.lua
--     nixVulkanIntel morf test --no-dbus examples/tests/caelestia_spec.lua   # with snapshots
--
-- `morf test` points the XDG folders at a scratch directory, so the settings
-- and the launch history are written there. The scheme is fixed to the
-- reference's own pink source so the snapshots do not depend on a picture.

local test = morf.test

local W, H = 1920, 1080

local function load(extra)
  for _, program in ipairs { "systemctl", "loginctl" } do test.stub_run(program, { code = 0 }) end
  test.load("../caelestia/init.lua", {
    size = { W, H },
    -- CAELESTIA_DRY_RUN: the session menu logs its commands instead of
    -- running them (and they are stubbed besides).
    env = { CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "", CAELESTIA_DRY_RUN = "1",
      -- Not the person's own colour tool's scheme.
      LULE_A = "/nonexistent/lule", HOME = morf.env("XDG_CACHE_HOME"),
      CAELESTIA_SETTINGS = extra and extra.settings or nil },
  })
  test.settle(2000)
end

local function drawer(name)
  return test.find { id = "drawer-" .. name }
end

local function shown(name)
  local d = drawer(name)
  return d ~= nil and d.visible
end

test.describe("caelestia", function()
  test.it("loads the frame, the bar and a wallpaper layer", function()
    load()
    local surfaces = test.surfaces()
    test.eq(surfaces[1].kind, "primary")
    test.eq(surfaces[1].width, W)
    test.eq(surfaces[1].height, H)
    local wallpaper
    for _, s in ipairs(surfaces) do
      if s.name == "caelestia-wallpaper" then wallpaper = s end
    end
    test.truthy(wallpaper and wallpaper.visible, "no wallpaper layer shown")
    test.eq(wallpaper.width, W)
    test.eq(wallpaper.height, H)
    test.truthy(test.find { id = "frame" }, "no frame")
    local bar = test.get { id = "bar" }
    test.eq(bar.width, 60)
    test.get { id = "workspaces" }
    test.get { id = "clock" }
    test.get { id = "status" }
    test.get { id = "power" }
    test.eq(test.get({ id = "window-title" }).text, "Desktop")
    test.falsy(shown("launcher"))
    test.falsy(shown("dashboard"))
    test.eq(#test.logs("error"), 0)
    test.snapshot("caelestia-rest.png", { surface = "screen" })
  end)

  test.it("opens and closes the launcher over IPC", function()
    load()
    test.eq(test.ipc("launcher"), true)
    test.settle(1500)
    test.truthy(shown("launcher"))
    local d = drawer("launcher")
    -- It sits on the frame's bottom edge, centred in the opening.
    test.near(d.y + d.height, H - 10, 1)
    test.near(d.x + d.width / 2, 60 + (W - 70) / 2, 1)
    test.truthy(test.find { id = "launcher-search" })
    test.snapshot("caelestia-launcher.png", { surface = "screen" })
    test.eq(test.ipc("launcher", "close"), false)
    test.settle(1500)
    test.falsy(shown("launcher"))
  end)

  test.it("lists the shell's actions after >", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">")
    test.settle(1000)
    test.truthy(test.find { text = "Calculator" }, "no Calculator action")
    test.truthy(test.find { text = "Dark" }, "no Dark action")
    test.type("li")
    test.settle(1000)
    test.truthy(test.find { text = "Light" })
    test.falsy(test.find { text = "Wallpaper", visible = true })
    test.snapshot("caelestia-launcher-actions.png", { surface = "screen" })
    test.key("Escape")
    test.settle(1500)
    test.falsy(shown("launcher"))
  end)

  test.it("opens and closes the dashboard over IPC", function()
    load()
    test.eq(test.ipc("dashboard", "open"), true)
    test.settle(1500)
    test.truthy(shown("dashboard"))
    local d = drawer("dashboard")
    test.near(d.y, 10, 1)
    test.eq(d.width, 872)
    test.truthy(test.find { id = "calendar-title", visible = true })
    test.truthy(test.find { id = "ring-cpu", visible = true })
    test.eq(test.ipc("drawers"), "dashboard")
    test.snapshot("caelestia-dashboard.png", { surface = "screen" })
    test.eq(test.ipc("close"), true)
    test.settle(1500)
    test.falsy(shown("dashboard"))
    test.eq(test.ipc("drawers"), "")
  end)

  test.it("opens the dashboard when the pointer reaches the top edge, and shuts it on leaving", function()
    load()
    test.move(W / 2 + 25, 4)
    test.settle(1500)
    test.truthy(shown("dashboard"), "the top edge did not open it")
    test.move(W / 2 + 25, 300)
    test.settle(1500)
    test.truthy(shown("dashboard"), "leaving the edge onto the panel shut it")
    -- Onto a tab: an area of its own over the panel, and still the panel.
    test.move { id = "dashboard-tab-media" }
    test.advance(500)
    test.truthy(shown("dashboard"), "a tab on the panel shut it")
    test.truthy(test.get({ id = "drawer-dashboard" }).contains_pointer)
    test.move(W / 2 + 25, 800)
    -- A moment's grace before it shuts: a timer, which settling does not wait for.
    test.advance(1500)
    test.falsy(shown("dashboard"), "leaving the panel did not shut it")
  end)

  test.it("opens the launcher from the bottom edge, and shuts it on leaving", function()
    load()
    test.move(W / 2 + 25, H - 4)
    test.settle(1500)
    test.truthy(shown("launcher"), "the bottom edge did not open it")
    -- Up onto the panel: still open.
    local l = drawer("launcher")
    test.move(l.x + l.width / 2, l.y + l.height / 2)
    test.advance(500)
    test.truthy(shown("launcher"), "moving onto the panel shut it")
    test.move(W / 2, 200)
    test.advance(1500)
    test.falsy(shown("launcher"), "leaving the panel did not shut it")
  end)

  test.it("switches dashboard tabs, the drawer easing to each tab's size", function()
    load()
    test.ipc("dashboard", "open")
    test.settle(1500)
    local d = drawer("dashboard")
    test.eq(d.width, 872)
    test.eq(d.height, 538)
    local sizes = {
      { "media", 1032, 418, "media-nothing" },
      { "performance", 987, 483, "performance-cpu" },
      { "weather", 870, 660, "weather-now" },
      { "dashboard", 872, 538, "dashboard-calendar" },
    }
    for _, want in ipairs(sizes) do
      test.click { id = "dashboard-tab-" .. want[1] }
      -- Part of the way there a moment later: it eases, it does not jump.
      test.advance(48)
      d = drawer("dashboard")
      test.truthy(d.width ~= want[2] or d.height ~= want[3], want[1] .. " jumped to its size")
      test.settle(1500)
      d = drawer("dashboard")
      test.eq(d.width, want[2], want[1] .. " width")
      test.eq(d.height, want[3], want[1] .. " height")
      test.near(d.x + d.width / 2, 60 + (W - 70) / 2, 1)
      test.truthy(test.find { id = want[4], visible = true }, want[1] .. " page not shown")
    end
  end)

  test.it("fades a drawer's background in with its contents", function()
    load()
    test.ipc("dashboard", "open")
    test.advance(60)
    local background = test.get { id = "drawer-dashboard-background" }
    test.truthy(background.opacity > 0 and background.opacity < 1,
      "half way in, the background is partly there: " .. background.opacity)
    test.snapshot("caelestia-dashboard-fading.png", { surface = "screen" })
    test.settle(1500)
    test.eq(test.get({ id = "drawer-dashboard-background" }).opacity, 1)
    -- Closing only slides.
    test.ipc("close")
    test.advance(60)
    test.eq(test.get({ id = "drawer-dashboard-background" }).opacity, 1)
  end)

  test.it("shows each dashboard tab", function()
    load()
    test.ipc("dashboard", "open")
    test.settle(1500)
    test.click { id = "dashboard-tab-media" }
    test.settle(1500)
    -- No player on the bus here: the reference's "Nothing playing".
    test.truthy(test.find { text = "Nothing playing" })
    test.falsy(test.find { id = "media-track", visible = true })
    test.snapshot("caelestia-dashboard-media.png", { surface = "screen" })
    test.click { id = "dashboard-tab-performance" }
    test.settle(1500)
    test.matches(test.get({ id = "performance-memory-space" }).text, "^[%d.]+ / [%d.]+ %a+$")
    test.matches(test.get({ id = "performance-usage" }).text, "^%d+%%$")
    test.truthy(test.find { id = "performance-disk-name", visible = true })
    test.snapshot("caelestia-dashboard-performance.png", { surface = "screen" })
    test.click { id = "dashboard-tab-weather" }
    test.settle(1500)
    test.truthy(test.find { id = "weather-day-1", visible = true })
    test.truthy(test.find { text = "7-day forecast" })
    test.snapshot("caelestia-dashboard-weather.png", { surface = "screen" })
  end)

  test.it("calculates after > calc, and copies the answer", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">calc 2+3*4")
    test.settle(1000)
    test.eq(test.get({ id = "launcher-calc" }).text, "2 + (3 * 4) = 14")
    test.snapshot("caelestia-launcher-calc.png", { surface = "screen" })
    test.key("BackSpace")
    test.key("BackSpace")
    test.type("/0")
    test.settle(500)
    test.eq(test.get({ id = "launcher-calc" }).text, "division by zero")
    for _ = 1, 5 do test.key("BackSpace") end
    test.type("sqrt(2)^2 + os.exit()")
    test.settle(500)
    test.eq(test.get({ id = "launcher-calc" }).text, "unexpected .")
    test.eq(#test.logs("error"), 0)
  end)

  test.it("picks a variant, and the colours follow", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">variant ")
    test.settle(1000)
    test.truthy(test.find { text = "Tonal Spot" })
    test.truthy(test.find { text = "Monochrome" } == nil, "more than seven listed")
    test.snapshot("caelestia-launcher-variants.png", { surface = "screen" })
    test.type("mono")
    test.settle(1000)
    test.truthy(test.find { text = "Monochrome" })
    test.key("Return")
    test.settle(1500)
    test.falsy(shown("launcher"))
    test.ipc("dashboard", "open")
    test.settle(1500)
    test.snapshot("caelestia-dashboard-monochrome.png", { surface = "screen" })
    test.eq(#test.logs("error"), 0)
    -- Back to the default, for the tests after this one (the settings file
    -- outlives a load).
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">variant tonal")
    test.settle(800)
    test.key("Return")
    test.settle(800)
  end)

  test.it("lists schemes, and a scheme sets the source colour", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">scheme ")
    test.settle(1000)
    test.truthy(test.find { text = "Dynamic" })
    test.truthy(test.find { text = "Rosé" })
    test.snapshot("caelestia-launcher-schemes.png", { surface = "screen" })
    test.type("lagoon")
    test.settle(800)
    test.key("Return")
    test.settle(1500)
    test.falsy(shown("launcher"))
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">scheme dynamic")
    test.settle(800)
    test.key("Return")
    test.settle(800)
  end)

  test.it("shows no results as the reference does", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type("zzqqxxnothing")
    test.settle(1000)
    test.truthy(test.find { id = "launcher-empty", visible = true })
    test.truthy(test.find { text = "Try searching for something else" })
  end)

  test.it("widens into a wallpaper carousel after > wallpaper", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">wallpaper ")
    test.settle(1500)
    local d = drawer("launcher")
    test.eq(d.width, 1270)
    test.truthy(test.find { id = "launcher-wallpapers", visible = true })
    test.snapshot("caelestia-launcher-wallpapers.png", { surface = "screen" })
    test.key("Escape")
    test.settle(1500)
    test.falsy(shown("launcher"))
  end)
  test.it("opens the session menu from the power button, and runs nothing until asked", function()
    load()
    test.click { id = "power" }
    test.settle(1500)
    test.truthy(shown("session"), "the power button did not open it")
    local d = drawer("session")
    test.near(d.x + d.width, W - 10, 1)
    test.near(d.y + d.height / 2, H / 2, 1)
    test.eq(#test.runs(), 0)
    test.snapshot("caelestia-session.png", { surface = "screen" })
    -- Down to shut down, then Escape: nothing ran.
    test.key("Down")
    test.key("Escape")
    test.settle(1500)
    test.falsy(shown("session"))
    test.eq(#test.runs(), 0)
    -- Again, and Return on shut down.
    test.ipc("session", "open")
    test.settle(1500)
    test.key("Down")
    test.key("Return")
    test.settle(1500)
    test.falsy(shown("session"))
    local said
    for _, line in ipairs(test.logs("info")) do
      if line.message:find("session shutdown (dry run): systemctl poweroff", 1, true) then said = true end
    end
    test.truthy(said, "shut down was not asked for")
    test.eq(#test.runs(), 0)
  end)

  test.it("shuts the session menu on a click on the desk", function()
    load()
    test.ipc("drawers", "toggle", "session")
    test.settle(1500)
    test.truthy(shown("session"))
    test.click(800, 500)
    test.settle(1500)
    test.falsy(shown("session"))
    test.eq(#test.runs(), 0)
  end)
  test.it("moves the launcher's highlight with the arrow keys", function()
    load()
    test.ipc("launcher", "open")
    test.settle(1500)
    test.type(">")
    test.settle(800)
    -- Calculator first, Scheme second: Down and Return opens the schemes.
    test.key("Down")
    test.key("Return")
    test.settle(800)
    test.eq(test.get({ id = "launcher-search" }).text, "> scheme ")
    test.truthy(test.find { text = "Dynamic" })
  end)
  test.it("grows the bar's popouts on hover, turning one into the next", function()
    load()
    local net = test.get { id = "status-network" }
    test.move(net.x + 20, net.y + 15)
    test.settle(1500)
    test.truthy(shown("popout"), "hovering the network icon opened nothing")
    local d = drawer("popout")
    test.near(d.x, 60, 1)
    test.eq(d.width, 352)
    test.truthy(test.find { id = "popout-network", visible = true })
    test.truthy(test.find { text = "Rescan networks" })
    test.snapshot("caelestia-popout-network.png", { surface = "screen" })
    local bt = test.get { id = "status-bluetooth" }
    test.move(bt.x + 20, bt.y + 15)
    test.advance(48)
    d = drawer("popout")
    test.truthy(d.width > 332 and d.width < 352, "it jumped to the next popout's width")
    test.settle(1500)
    d = drawer("popout")
    test.eq(d.width, 332)
    test.truthy(test.find { id = "popout-bluetooth", visible = true })
    test.falsy(test.find { id = "popout-network", visible = true })
    test.snapshot("caelestia-popout-bluetooth.png", { surface = "screen" })
    local power = test.get { id = "status-power" }
    test.move(power.x + 20, power.y + 15)
    test.settle(1500)
    test.truthy(test.find { text = "No battery detected" })
    test.snapshot("caelestia-popout-power.png", { surface = "screen" })
    -- Onto the panel: it stays.
    d = drawer("popout")
    test.move(d.x + d.width / 2, d.y + d.height / 2)
    test.advance(600)
    test.truthy(shown("popout"), "moving onto the panel shut it")
    -- Off both: it shuts.
    test.move(900, 500)
    test.advance(1500)
    test.falsy(shown("popout"), "leaving did not shut it")
  end)
  test.it("drops a notification in at the top right, and lets it go after its time", function()
    load()
    test.falsy(shown("notifications"))
    test.ipc("notify", "Download complete", "report.pdf has finished downloading")
    test.settle(1500)
    test.truthy(shown("notifications"), "no popup")
    local d = drawer("notifications")
    test.near(d.x + d.width, W - 10, 1)
    test.near(d.y, 10, 1)
    test.eq(test.get({ id = "notification-summary-1" }).text, "Download complete")
    test.snapshot("caelestia-notification.png", { surface = "screen" })
    -- A second one: the newest on top, the drawer taller.
    local before = d.height
    test.ipc("notify", "Battery low", "10% remaining, plug in the charger", "critical")
    test.settle(1500)
    test.eq(test.get({ id = "notification-summary-1" }).text, "Battery low")
    test.truthy(drawer("notifications").height > before)
    test.snapshot("caelestia-notification-critical.png", { surface = "screen" })
    -- The ordinary one expires; the critical one stays until clicked.
    test.advance(6000)
    test.settle(1500)
    test.truthy(shown("notifications"))
    test.falsy(test.find { id = "notification-2", visible = true })
    test.click { id = "notification-1" }
    test.settle(1500)
    test.falsy(shown("notifications"))
  end)

  test.it("shows the OSD on asking and shuts it after a while", function()
    load()
    test.falsy(shown("osd"))
    test.ipc("osd")
    test.settle(1000)
    test.truthy(shown("osd"))
    local d = drawer("osd")
    test.near(d.x + d.width, W - 10, 1)
    test.near(d.y + d.height / 2, H / 2, 1)
    test.truthy(test.find { id = "osd-volume", visible = true })
    test.truthy(test.find { id = "osd-brightness", visible = true })
    test.snapshot("caelestia-osd.png", { surface = "screen" })
    test.advance(3000)
    test.settle(1000)
    test.falsy(shown("osd"))
  end)

  test.it("takes the colour tool's colours from its own terminal", function()
    -- Settings of its own: earlier tests chose a scheme colour.
    load { settings = morf.fs.join(morf.env("XDG_CACHE_HOME"), "lule-test-settings.json") }
    local path, lule_before = test.ipc("lule")
    test.matches(path, "^/dev/pts/")
    -- What lule's `colors_to_tty` writes to every terminal.
    morf.fs.write(path, "\27]4;1;#3d8bff\27\\\27]11;#0b0d12\27\\\27]10;#e8ecf4\27\\", { atomic = false })
    local lule_now, primary
    test.wait(function()
      local _
      _, lule_now, primary = test.ipc("lule")
      return lule_now ~= lule_before
    end, 3000, "the shell did not hear the colours")
    test.settle(1000)
    _, lule_now, primary = test.ipc("lule")
    test.eq(lule_now, "#3d8bff")
    -- The Material scheme is built from that accent: a blue primary.
    local h = morf.color(primary):hct()
    test.truthy(h > 230 and h < 300, "primary " .. primary .. " is not the accent's blue")
  end)

  test.it("lines the workspaces down the right edge, the active one lit", function()
    load()
    test.falsy(test.find { id = "drawer-sidebar" }, "the sidebar is back")
    local track = math.floor(H * 0.5)
    for i = 1, 10 do
      local pill = test.get { id = "rail-pill-" .. i }
      test.near(pill.x + pill.width / 2, W - 5, 0.5)
      test.eq(pill.width, 6)
    end
    local first, last = test.get { id = "rail-pill-1" }, test.get { id = "rail-pill-10" }
    test.near(first.y, (H - track) / 2, 1)
    test.near(last.y + last.height, (H + track) / 2, 1)
    test.falsy(test.find { id = "rail-number", visible = true, opacity = 1 })
    test.near(test.get({ id = "rail-pill-5" }).opacity, 0.6, 0.01)
    test.move(W - 5, test.get({ id = "rail-pill-5" }).y + 10)
    test.settle(400)
    test.near(test.get({ id = "rail-pill-5" }).opacity, 0.9, 0.01, "hover did not lift it")
    test.eq(#test.logs("error"), 0)
  end)

  test.it("pops the new workspace out as a numbered bud, then tucks it back", function()
    load()
    test.ipc("workspace", 4)
    test.settle(600)
    local bud, pill = test.get { id = "rail-bud" }, test.get { id = "rail-pill-4" }
    test.near(bud.y, pill.y, 1, "the bud did not slide to the fourth pill")
    test.near(bud.width, pill.height, 1, "the bud is not a disc")
    test.truthy(bud.x + bud.width < W - 10, "the bud stayed in the frame")
    test.eq(test.get({ id = "rail-number" }).text, "4")
    test.snapshot("caelestia-rail-bud.png", { surface = "screen" })
    test.advance(1500)
    bud = test.get { id = "rail-bud" }
    test.near(bud.width, 6, 0.5, "the bud never tucked back")
    test.near(test.get({ id = "rail-field" }).opacity, 0, 0.01)
  end)

  test.it("switches workspace from a pill", function()
    load()
    test.click { id = "rail-slot-7" }
    test.settle(300)
    test.eq(test.get({ id = "rail-number" }).text, "7")
    test.eq(test.ipc("workspace", 7), 7)
  end)
end)
