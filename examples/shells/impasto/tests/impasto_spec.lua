-- impasto, headless: every panel opens over IPC, and nothing fails to load.
--
--     morf test --private-bus examples/shells/impasto/tests/impasto_spec.lua
--
-- `--private-bus` gives the run a session bus of its own, which impasto's
-- notification server needs to own its name; `morf test` already points
-- the XDG folders at a scratch directory, so impasto's settings are written
-- there and not over the real ones. `IMPASTO_DRY_RUN` makes every action
-- that would change the machine log what it would do instead.

local test = morf.test

local PANELS = {
  "controls", "launcher", "overview", "wifi", "bluetooth", "session",
  "notes", "board", "games", "keys", "stats",
}

local function load()
  test.load("../shell/init.lua", {
    size = { 1280, 720 },
    env = { IMPASTO_DRY_RUN = "1" },
  })
  test.settle(3000)
end

test.describe("impasto", function()
  test.it("loads every part", function()
    load()
    test.eq(test.ipc("failed"), "")
    local surface = test.surfaces()[1]
    test.eq(surface.kind, "primary")
    test.eq(surface.width, 1280)
    -- The wallpaper is a background layer of its own, shown from the start
    -- (it was built closed once, and a real compositor showed no wallpaper).
    local wallpaper
    for _, each in ipairs(test.surfaces()) do
      if each.name == "impasto-wallpaper" then wallpaper = each end
    end
    test.truthy(wallpaper, "no wallpaper layer")
    test.truthy(wallpaper.visible, "the wallpaper layer is not shown")
    test.eq(wallpaper.width, 1280)
  end)

  test.it("opens each panel over IPC, and closes it again", function()
    load()
    for _, panel in ipairs(PANELS) do
      test.eq(test.ipc(panel), panel, "opening " .. panel)
      test.settle(2000)
      test.eq(test.ipc("close"), "", "closing " .. panel)
      test.settle(2000)
    end
    test.eq(test.ipc("failed"), "")
  end)

  test.it("a panel toggles shut when asked twice", function()
    load()
    test.eq(test.ipc("controls"), "controls")
    test.settle(2000)
    test.ne(test.ipc("controls"), "controls")
  end)

  test.it("reads and writes a setting", function()
    load()
    local before = test.ipc("get", "clockShowsDate")
    test.contains({ "true", "false" }, before)
    local flipped = before == "true" and "false" or "true"
    test.eq(test.ipc("set", "clockShowsDate", flipped), flipped)
    test.eq(test.ipc("get", "clockShowsDate"), flipped)
  end)

  test.it("logs no Lua errors while its panels open", function()
    load()
    test.clear_logs()
    for _, panel in ipairs(PANELS) do
      test.ipc(panel)
      test.settle(1000)
      test.ipc("close")
    end
    for _, entry in ipairs(test.logs("warn")) do
      test.falsy(entry.message:find("%.lua:%d+:"), entry.message)
    end
  end)
end)
