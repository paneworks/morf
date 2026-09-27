-- The redesigned bottom workspace and left planner. Taskwarrior is stubbed
-- here; library/tests/taskwarrior_spec.lua exercises the real CLI separately.
local test = morf.test
local UUID = "12345678-1234-1234-1234-123456789abc"
local HOST = [[
  require("init")
  require("config").set("capture.folder", morf.env("XDG_CACHE_HOME") .. "/captures")
  local capture = require("capture")
  morf.ipc.panel_state = function()
    return { phase = capture.phase:get(), recording = capture.recording:get(), message = capture.message:get(),
      bottom = require("bottom").drawer.open:get(),
      assistant = require("bottom").drawer.open:get() and require("bottom").panel.showing("assistant"), tasks = require("leftbar").panel.showing("tasks"),
      calendar = require("leftbar").panel.showing("calendar"), error = require("planner").client.error:get(),
      keyboard = morf.surface.keyboard_focus }
  end
  morf.ipc.choose_day = function(day) require("planner").selected_day:set(day) end
]]
local function load(dry)
  test.stub_run("task", { code = 0, stdout = morf.json.encode({ {
    uuid = UUID, description = "Plan the launch", project = "work", priority = "H", status = "pending",
    scheduled = "20301001T073000Z", due = "20301001T150000Z", tags = { "calls" },
  } }) })
  test.load("../shell/init.lua", { source = HOST, size = { 1920, 1080 }, env = {
    CAELESTIA_DRY_RUN = dry == false and "0" or "1", CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "",
    LULE_A = "/nonexistent/lule", HOME = morf.env("XDG_CACHE_HOME"),
  } })
  test.settle(1200)
end
local function state() return test.ipc("panel_state") end

test.describe("caelestia panels", function()
  test.it("dismisses the bottom workspace outside either tab and after leaving its hover area", function()
    load()
    test.ipc("assistant", "open") test.settle(800)
    local panel = test.get("drawer-bottom")
    test.click(panel.x + panel.width / 2, panel.y + panel.height / 2)
    test.settle(300)
    test.truthy(state().bottom, "clicking inside the assistant dismissed it")
    test.click(100, 100) test.settle(800)
    test.falsy(state().bottom, "clicking outside the assistant did not close it")
    test.falsy(test.get("drawer-bottom").visible)

    test.ipc("bottom", "open", "drop") test.settle(800)
    test.click("drop-placeholder") test.settle(300)
    test.truthy(state().bottom, "clicking inside Drop dismissed it")
    test.click("bottom-tab-assistant") test.settle(600)
    test.truthy(state().assistant, "switching tabs dismissed the workspace")
    test.click("bottom-tab-drop") test.settle(600)
    test.click(100, 100) test.settle(800)
    test.falsy(state().bottom, "clicking outside Drop did not close it")

    test.move(985, 1076) test.advance(1400)
    test.truthy(state().bottom)
    test.move("bottom-tab-assistant") test.advance(300)
    test.truthy(state().bottom, "crossing onto the panel closed it")
    test.leave() test.advance(800)
    test.falsy(state().bottom, "leaving the monitor kept the hover panel open")
    test.falsy(test.get("drawer-bottom").visible)
    test.eq(#test.logs("error"), 0)
  end)

  test.it("separates the assistant and compact capture popup, and cancels queued recording", function()
    load()
    test.move(985, 1076) test.advance(1600)
    test.eq(state().assistant, true)
    test.falsy(test.get("drawer-capture").visible)
    test.get("bottom-tab-assistant")
    test.click("bottom-tab-drop") test.settle(650)
    test.truthy(test.find { id = "drop-placeholder", visible = true })
    test.click("bottom-tab-assistant") test.settle(650)
    test.truthy(test.get("assistant-page").width > 900)
    test.falsy(test.find { id = "bottom-tab-capture" })
    test.ipc("capture", "open") test.settle(1000)
    test.eq(state().assistant, false)
    test.truthy(test.get("drawer-capture").width <= 480)
    test.truthy(test.get("drawer-capture").height <= 240)
    test.get("capture-target-region")
    local resting_width = test.get("capture-selection").width
    test.click("capture-target-screen") test.advance(160)
    test.truthy(test.get("capture-selection").width > resting_width + 20,
      "the target selection did not stretch during its transition")
    test.advance(650)
    test.near(test.get("capture-selection").width, resting_width, 1)
    test.falsy(test.find { id = "capture-library" })
    test.falsy(test.find { id = "capture-close" })
    test.click(100, 100) test.settle(800)
    test.falsy(test.get("drawer-capture").visible)
    test.ipc("utilities", "open") test.settle(800)
    test.click("utilities-capture") test.settle(800)
    test.truthy(test.get("drawer-capture").visible)
    test.falsy(test.get("drawer-sidebar").visible)
    test.click(100, 100) test.settle(800)
    test.falsy(test.get("drawer-capture").visible)
    test.ipc("capture", "open") test.settle(800)
    test.click("capture-delay-5")
    test.ipc("record", "screen")
    test.eq(state().phase, "countdown")
    test.ipc("record") -- cancels the queued start, never starts a recorder
    test.advance(6500)
    test.eq(state().recording, false)
    test.eq(state().phase, "ready")
    for _, log in ipairs(test.logs("info")) do
      test.falsy(log.message:find("capture record_screen", 1, true), "cancelled recording still started")
    end
    test.ipc("assistant", "open") test.settle(600)
    test.eq(state().assistant, true)
    test.eq(#test.logs("error"), 0)
  end)

  test.it("loads tasks, edits dates, and opens a day's agenda", function()
    load()
    test.ipc("tasks") test.settle(1200)
    test.eq(state().tasks, true)
    test.eq(state().error, "")
    test.ipc("launcher", "open") test.settle(600)
    test.eq(state().keyboard, "exclusive")
    test.ipc("launcher", "close") test.settle(600)
    test.eq(state().keyboard, "on_demand")
    test.get("task-row-" .. UUID)
    test.click("task-edit-" .. UUID) test.settle(300)
    test.eq(test.get("task-description").text, "Plan the launch")
    test.eq(test.get("task-project").text, "work")
    test.click("task-description")
    test.key("a", "Ctrl") test.type("Prepare the release")
    test.click("task-save") test.settle(600)
    local modified = false
    for _, run in ipairs(test.runs()) do
      for _, arg in ipairs(run) do if arg == "description:Prepare the release" then modified = true end end
    end
    test.truthy(modified, "the editor did not send the change to Taskwarrior")
    test.ipc("calendar") test.ipc("choose_day", "2030-10-01") test.settle(1000)
    test.eq(state().calendar, true)
    test.get("planner-task-" .. UUID)
    test.click("planner-add") test.settle(500)
    test.eq(state().tasks, true)
    test.eq(test.get("task-scheduled").text, "2030-10-01T09:00")
    test.eq(#test.logs("error"), 0)
  end)

  test.it("shows recorder failures and clears recording state", function()
    test.stub_run("gpu-screen-recorder", { code = 1, stderr = "Recorder could not start" })
    load(false)
    test.ipc("record", "screen") test.advance(1000)
    test.eq(state().recording, false)
    test.eq(state().phase, "error")
    test.eq(state().message, "Recorder could not start")
  end)
end)
