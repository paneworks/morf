-- impasto's compositor settings against a fake Hyprland: what the input
-- page, the displays page and the keys page send. The fake is served from
-- this spec (library/lib/testing/fake_hyprland.lua); the session's Hyprland is never reached
-- (`morf test` removes HYPRLAND_INSTANCE_SIGNATURE, and impasto is pointed
-- at the fake's runtime directory). Settings and keys.tsv land in the
-- scratch XDG folders `morf test` sets up.
--
--     morf test --private-bus examples/shells/impasto/tests/impasto_hyprland_spec.lua

local test = morf.test
local fake_hyprland = require("lib.testing.fake_hyprland")

local fake

-- Serves the fake and moves the clock until `done()` holds.
local function until_(done, message, timeout_ms)
  return test.wait(function()
    fake.serve()
    test.advance(25)
    fake.serve()
    return done()
  end, timeout_ms or 8000, message)
end

local function sent(text) return #fake.sent(text) > 0 end

local function load(env)
  -- The scratch state folder outlives a test within this spec too.
  for _, name in ipairs { "keys.tsv", "keys.lua", "keys.conf" } do
    pcall(morf.fs.remove, morf.fs.join(morf.fs.dir("state"), "impasto-morf", name))
  end
  local given = { IMPASTO_DRY_RUN = false, HYPRLAND_INSTANCE_SIGNATURE = fake.signature,
    XDG_RUNTIME_DIR = fake.runtime }
  for k, v in pairs(env or {}) do given[k] = v end
  test.load("../shell/init.lua", { size = { 1280, 720 }, env = given })
  -- The settings file outlives a test within this spec; start each clean.
  test.ipc("set", "displays", "{}")
  test.ipc("set", "keys", "{}")
  until_(function() return sent("/eval local _ = 0") end, "impasto never asked the flavour")
end

test.describe("impasto under a fake Hyprland", function()
  test.before_each(function() fake = fake_hyprland.new {} end)
  test.after_each(function() fake.close() end)

  test.it("pushes the animation preset, and input only once touched", function()
    load()
    until_(function() return sent('hl.curve("preset"') end, "no animation preset")
    test.eq(#fake.sent("kb_layout"), 0, "an untouched layout was pushed")
    test.eq(test.ipc("set", "keyRepeatRate", "40"), "40")
    until_(function() return sent("hl.config({ input = { repeat_rate = 40 } })") end, "the repeat rate was not pushed")
    test.eq(test.ipc("set", "keyboardLayouts", '"us,de"'), '"us,de"')
    until_(function() return sent('kb_layout = "us,de"') end, "the layouts were not pushed")
    test.eq(test.ipc("failed"), "")
  end)

  test.it("pushes it all again after a reload", function()
    load()
    test.eq(test.ipc("set", "pointerSensitivity", "0.5"), "0.5")
    if not pcall(until_, function() return sent("sensitivity = 0.5") end, "x") then
      test.fail("sensitivity not pushed: " .. table.concat(fake.transcript, "\n"):sub(-900))
    end
    local before = #fake.sent("sensitivity = 0.5")
    fake.emit("configreloaded>>")
    until_(function() return #fake.sent("sensitivity = 0.5") > before end, "not pushed after the reload")
  end)

  test.it("keeps a screen arrangement and pushes it as monitor rules", function()
    load()
    until_(function() return test.ipc("displays"):find("DP-2", 1, true) ~= nil end, "no screens")
    test.eq(test.ipc("display", "DP-2", "position", "0x-1440"), "ok")
    until_(function() return sent('hl.monitor({ output = "desc:Dell Inc. DELL U2720Q 7XJ1K", mode = "3840x2160@60.00", position = "0x-1440", scale = 1.5 })') end,
      "the position was not pushed")
    -- The fake moved it, so nothing is left to push.
    local count = #fake.sent("hl.monitor(")
    test.advance(2000)
    fake.serve()
    test.eq(#fake.sent("hl.monitor("), count, "pushed again though nothing differs")
    test.eq(test.ipc("display", "eDP-1", "off"), "ok")
    until_(function() return sent('hl.monitor({ output = "desc:BOE 0x0BCA", disabled = true })') end, "not switched off")
    -- The workspace left on the dark panel is brought over.
    until_(function() return sent('hl.dsp.workspace.move({ workspace = 1, monitor = "DP-2" })') end, "workspace 1 stranded")
    test.eq(test.ipc("display", "DP-2", "scale", "99"), "refused")
    -- Lit again in the mode it was switched off in, not the preferred one.
    test.eq(test.ipc("display", "eDP-1", "on"), "ok")
    until_(function() return sent('hl.monitor({ output = "desc:BOE 0x0BCA", disabled = false, mode = "1920x1200@60.00"') end,
      "not lit again in its own mode: " .. table.concat(fake.sent("BOE"), " | "):sub(-400))
  end)

  test.it("lights every screen when none is", function()
    for _, m in ipairs(fake.monitors) do m.disabled = true end
    load()
    until_(function() return sent('hl.monitor({ output = "desc:BOE 0x0BCA", disabled = false, mode = "preferred", position = "auto", scale = 1 })') end,
      "nothing was lit")
  end)

  -- Hyprland 0.56 stands a headless FALLBACK output in while nothing is lit,
  -- and offers it as a wl_output: the shell runs on it, and must not count
  -- it as a lit screen.
  test.it("lights every screen when only Hyprland's fallback output is", function()
    for _, m in ipairs(fake.monitors) do m.disabled = true end
    fake.monitors[#fake.monitors + 1] = { id = 2, name = "FALLBACK", description = "", make = "", model = "",
      serial = "", width = 1920, height = 1080, refreshRate = 60.0, x = 0, y = 0, scale = 1, transform = 0,
      focused = true, dpmsStatus = true, vrr = false, disabled = false, mirrorOf = "none",
      activeWorkspace = { id = 1, name = "1" }, specialWorkspace = { id = 0, name = "" },
      availableModes = { "1920x1080@60.00Hz" } }
    load()
    until_(function() return sent('hl.monitor({ output = "desc:BOE 0x0BCA", disabled = false, mode = "preferred", position = "auto", scale = 1 })') end,
      "nothing was lit")
    test.eq(#fake.sent('output = "FALLBACK"'), 0, "a rule was pushed for the fallback output")
  end)

  -- Every screen off: Hyprland offers no output at all, and the shell runs
  -- once with none (morf.surface.outputless), only to light one again.
  test.it("lights every screen when none is, with no output to draw on", function()
    for _, m in ipairs(fake.monitors) do m.disabled = true end
    test.load("../shell/init.lua", { screens = 0, env = { IMPASTO_DRY_RUN = false,
      HYPRLAND_INSTANCE_SIGNATURE = fake.signature, XDG_RUNTIME_DIR = fake.runtime } })
    test.eq(test.ipc("outputless"), true)
    until_(function() return sent('hl.monitor({ output = "desc:BOE 0x0BCA", disabled = false, mode = "preferred", position = "auto", scale = 1 })') end,
      "nothing was lit")
    until_(function() return sent('hl.monitor({ output = "desc:Dell Inc. DELL U2720Q 7XJ1K", disabled = false') end,
      "the second screen was not lit")
  end)

  test.it("rebinds a key from the keys page, writes keys.tsv and reloads", function()
    load()
    test.eq(test.ipc("settings", "keys", "shell"), "keys")
    test.settle(2000)
    until_(function() return test.find({ text = "SUPER + SPACE", visible = true }) ~= nil end, "the launcher's keys are not shown")
    test.click({ text = "SUPER + SPACE", visible = true })
    test.settle(500)
    test.truthy(test.find({ text = "Press the keys…", visible = true }), "the editor did not open")
    -- Recording holds the compositor's binds off the shell, or a combination
    -- Hyprland already binds would fire there and never reach the field.
    test.truthy(test.shortcuts_inhibited(), "the compositor's shortcuts were not held off while recording")
    test.key("k", "super", { surface = test.find({ text = "SUPER + SPACE", visible = true }).surface })
    test.settle(500)
    test.truthy(test.find({ text = "SUPER + K", visible = true }), "the draft is not shown")
    test.falsy(test.shortcuts_inhibited(), "the shortcuts were still held once the keys were caught")
    test.click({ text = "Apply", visible = true })
    until_(function() return sent("/reload") end, "Hyprland was not reloaded")
    local state = morf.fs.dir("state")
    local tsv = morf.fs.read(morf.fs.join(state, "impasto-morf", "keys.tsv")) or ""
    test.contains(tsv, "Shell · Open the launcher\tSUPER + K")
    test.contains(morf.fs.read(morf.fs.join(state, "impasto-morf", "keys.lua")) or "", "hl.bind = function")
    test.eq(test.ipc("get", "keys"):find("SUPER + K", 1, true) ~= nil, true)
  end)

  test.it("warns of a clash before applying, and Escape cancels", function()
    load()
    test.ipc("settings", "keys", "shell")
    test.settle(2000)
    until_(function() return test.find({ text = "SUPER + SPACE", visible = true }) ~= nil end, "no launcher row")
    test.click({ text = "SUPER + SPACE", visible = true })
    test.settle(300)
    test.key("a", "super", { surface = test.find({ text = "SUPER + SPACE", visible = true }).surface })
    test.settle(300)
    test.truthy(test.find(function(node)
      return node.visible and node.text and node.text:find("is already Open the control centre", 1, true)
    end), "no clash warning")
    test.click({ text = "SUPER + SPACE", visible = true })
    test.settle(300)
    test.key("Escape", nil, { surface = test.find({ text = "SUPER + SPACE", visible = true }).surface })
    test.settle(300)
    test.falsy(test.find({ text = "Apply", visible = true }), "the editor stayed open")
  end)
end)

test.describe("impasto under a fake hyprlang Hyprland", function()
  test.before_each(function() fake = fake_hyprland.new { flavour = "hyprlang" } end)
  test.after_each(function() fake.close() end)

  test.it("sends keywords, and monitor rules as keyword monitor", function()
    load()
    test.eq(test.ipc("set", "keyRepeatRate", "45"), "45")
    until_(function() return sent("/keyword input:repeat_rate 45") end, "no repeat rate keyword")
    until_(function() return test.ipc("displays"):find("DP-2", 1, true) ~= nil end, "no screens")
    test.eq(test.ipc("display", "DP-2", "scale", "2"), "ok")
    if not pcall(until_, function() return sent("/keyword monitor desc:Dell Inc. DELL U2720Q 7XJ1K,3840x2160@60.00,1536x0,2") end, "") then
      test.fail("no monitor keyword in: " .. table.concat(fake.transcript, "\n"):sub(-700))
    end
    test.eq(#fake.sent("/eval hl."), 0, "a Lua chunk went to a hyprlang Hyprland")
  end)

  test.it("keeps every moved bind in keys.conf, one move after another", function()
    load()
    test.ipc("settings", "keys", "shell")
    test.settle(2000)
    local function rebind(shown, key)
      until_(function() return test.find({ text = shown, visible = true }) ~= nil end, "no row on " .. shown)
      local surface = test.find({ text = shown, visible = true }).surface
      test.click({ text = shown, visible = true })
      test.settle(300)
      test.key(key, "super", { surface = surface })
      test.settle(300)
      local before = #fake.sent("/reload")
      test.click({ text = "Apply", visible = true })
      if not pcall(until_, function() return #fake.sent("/reload") > before end, "") then
        test.fail("no reload after " .. shown .. ": " .. table.concat(fake.transcript, "\n"):sub(-500) .. " | " .. tostring(test.ipc("get", "keys")))
      end
    end
    rebind("SUPER + SPACE", "k")
    -- As Hyprland would list it once keys.conf is sourced.
    fake.binds[2].key = "K"
    fake.emit("configreloaded>>")
    test.advance(600)
    fake.serve()
    rebind("SUPER + A", "j")
    local conf = morf.fs.read(morf.fs.join(morf.fs.dir("state"), "impasto-morf", "keys.conf")) or ""
    test.contains(conf, "unbind = SUPER, SPACE\nbindd = SUPER, K, Shell · Open the launcher, exec, morf ipc call launcher")
    test.contains(conf, "unbind = SUPER, A\nbindd = SUPER, J, Shell · Open the control centre, exec, morf ipc call controls")
  end)
end)

test.describe("impasto on a screen that is not the focused one", function()
  test.before_each(function()
    local monitor = function(id, name, x, focused)
      return { id = id, name = name, description = "", make = "", model = "", serial = "",
        width = 1280, height = 720, refreshRate = 60.0, x = x, y = 0, scale = 1, transform = 0,
        focused = focused, dpmsStatus = true, vrr = false, disabled = false, mirrorOf = "none",
        activeWorkspace = { id = id + 1, name = tostring(id + 1) }, specialWorkspace = { id = 0, name = "" },
        availableModes = { "1280x720@60.00Hz" } }
    end
    fake = fake_hyprland.new { monitors = { monitor(0, "HEADLESS-1", 0, false), monitor(1, "HEADLESS-2", 1280, true) } }
  end)
  test.after_each(function() fake.close() end)

  test.it("leaves the settings window to the focused screen's shell", function()
    -- Every screen's shell hears `settings`; two windows used to open, one
    -- over the other, and the one clicked was not the one that pushed.
    test.load("../shell/init.lua", { size = { 1280, 720 }, screens = 2,
      env = { IMPASTO_DRY_RUN = "1", HYPRLAND_INSTANCE_SIGNATURE = fake.signature, XDG_RUNTIME_DIR = fake.runtime } })
    until_(function() return (test.ipc("live") or ""):find("at rest", 1, true) ~= nil end, "HEADLESS-1 never saw HEADLESS-2 focused")
    test.eq(test.ipc("settings", "keys"), nil)
    test.settle(500)
    for _, surface in ipairs(test.surfaces()) do
      test.ne(surface.kind, "toplevel", "a settings window opened on the screen at rest")
    end
    -- A panel verb is answered by the screen that opens it, not this one.
    test.eq(test.ipc("launcher"), nil)
  end)
end)

test.describe("impasto under another compositor", function()
  test.it("says the pages are not available, and sends nothing", function()
    test.load("../shell/init.lua", { size = { 1280, 720 }, env = { IMPASTO_DRY_RUN = "1" } })
    test.settle(3000)
    test.eq(test.ipc("display", "DP-2", "off"), "not available here: this compositor is not Hyprland")
    for _, page in ipairs { { "input", "keyboard" }, { "monitors", "screen" }, { "keys", "shell" } } do
      test.ipc("settings", page[1], page[2])
      test.settle(1500)
      test.truthy(test.find({ text = "Not available here", visible = true }), page[1] .. " does not say so")
    end
    test.eq(test.ipc("failed"), "")
    test.eq(#test.logs("warn"), 0)
  end)
end)
