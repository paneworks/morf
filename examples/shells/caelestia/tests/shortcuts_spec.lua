-- caelestia's keyboard shortcuts (shell/shortcuts.lua) and swipe gestures:
-- the IPC verbs reached by keys while the shell has the keyboard, a mouse's
-- back button shutting what is open, and a notification flung away.
--
--     morf test --no-dbus examples/shells/caelestia/tests/shortcuts_spec.lua

local test = morf.test

local W, H = 1920, 1080

local function load()
  for _, program in ipairs { "systemctl", "loginctl" } do test.stub_run(program, { code = 0 }) end
  test.stub_run("task", { code = 0, stdout = "[]" })
  test.load("../shell/init.lua", {
    size = { W, H },
    env = { CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "", CAELESTIA_DRY_RUN = "1",
      LULE_A = "/nonexistent/lule", HOME = morf.env("XDG_CACHE_HOME") },
  })
  test.settle(2000)
end

local function open() return " " .. test.ipc("drawers") .. " " end

test.it("opens and shuts the drawers from the keyboard", function()
  load()
  test.key("d", { "ctrl" }) test.settle(1500)
  test.contains(open(), " dashboard ")
  -- Ctrl+W, the IPC's `close`, shuts everything.
  test.key("w", { "ctrl" }) test.settle(1500)
  test.eq(test.ipc("drawers"), "")
  test.key("q", { "ctrl" }) test.settle(1500)
  test.contains(open(), " session ")
  test.key("Left", { "alt" }) test.settle(1500)
  test.eq(test.ipc("drawers"), "")
  test.eq(#test.logs("error"), 0)
end)

test.it("shuts what is open on a mouse's back button", function()
  load()
  test.key("d", { "ctrl" }) test.settle(1500)
  test.contains(open(), " dashboard ")
  local panel = test.find { id = "drawer-dashboard" }
  test.click(panel.x + panel.width / 2, panel.y + panel.height / 2, { button = "back" })
  test.settle(1500)
  test.eq(test.ipc("drawers"), "")
end)

test.it("pops a settings page on Alt+Left before shutting the panel", function()
  load()
  test.ipc("utilities", "open") test.settle(1500)
  test.eq(test.ipc("settings", "sound/equalizer"), "sound/equalizer") test.settle(1000)
  test.key("Left", { "alt" }) test.settle(1000)
  test.truthy(test.find { id = "sound-page", visible = true }, "Alt+Left did not go up to Sound")
  local panel = test.find { id = "sound-page" }
  test.click(panel.x + panel.width / 2, panel.y + 20, { button = "back" }) test.settle(1000)
  test.truthy(test.find { id = "utilities", visible = true })
  test.truthy(test.ipc("drawers"):find("sidebar", 1, true), "the panel shut before its pages")
  test.key("Left", { "alt" }) test.settle(1500)
  test.eq(test.ipc("drawers"), "")
end)

test.it("lets a notification go when it is flung aside", function()
  load()
  test.ipc("notify", "Battery low", "10% remaining", "critical")
  test.settle(1500)
  local card = test.get { id = "notification-1" }
  local x, y = card.x + 40, card.y + card.height / 2
  test.drag({ x, y }, { x + 200, y }, { steps = 5 })
  test.settle(1500)
  test.falsy(test.find { id = "notification-1", visible = true })
end)
