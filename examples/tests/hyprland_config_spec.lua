-- lib/hyprland_config: what it would send, and what it sends to a fake
-- Hyprland. Never the real one: `morf test` removes the session's
-- HYPRLAND_INSTANCE_SIGNATURE, and the configuration is pointed at the fake.
--
--     morf test examples/tests/hyprland_config_spec.lua

local test = morf.test
local fake_hyprland = require("fake_hyprland")

local ok_lib, config = pcall(require, "lib.hyprland_config")

test.describe("hyprland_config plans", function()
  test.it("loads without a Hyprland", function()
    test.truthy(ok_lib, tostring(config))
    test.falsy(config.available())
  end)

  test.it("sets options as one eval, or keywords", function()
    local list = { { "input:kb_layout", "str", "us,de" }, { "input:repeat_rate", "int", 30.4 },
      { "input:sensitivity", "float", -0.25 } }
    test.eq(config.options_plan(list, "lua"), { { eval =
      'hl.config({ input = { kb_layout = "us,de" } }) hl.config({ input = { repeat_rate = 30 } }) '
      .. 'hl.config({ input = { sensitivity = -0.25 } })' } })
    test.eq(config.options_plan(list, "hyprlang"), {
      { keyword = "input:kb_layout", value = "us,de" }, { keyword = "input:repeat_rate", value = "30" },
      { keyword = "input:sensitivity", value = "-0.25" } })
  end)

  test.it("refuses a value that could end the chunk", function()
    test.eq(config.options_plan({ { "input:kb_layout", "str", 'us" }) os.exit() --' } }, "lua"), nil)
    test.eq(config.options_plan({ { "input:kb_options", "str", "a;b" } }, "hyprlang"), nil)
  end)

  test.it("builds monitor rules, switching off first", function()
    local rules = {
      { output = "desc:Dell Inc. DELL U2720Q 7XJ1K", mode = "2560x1440@59.95", position = "1536x0", scale = 1.5 },
      { output = "eDP-1", disabled = true },
    }
    test.eq(config.monitors_plan(rules, "lua"), { { eval =
      'hl.monitor({ output = "eDP-1", disabled = true }) '
      .. 'hl.monitor({ output = "desc:Dell Inc. DELL U2720Q 7XJ1K", mode = "2560x1440@59.95", position = "1536x0", scale = 1.5 })' } })
    test.eq(config.monitors_plan(rules, "hyprlang"), {
      { keyword = "monitor", value = "eDP-1,disable" },
      { keyword = "monitor", value = "desc:Dell Inc. DELL U2720Q 7XJ1K,2560x1440@59.95,1536x0,1.5" } })
  end)

  test.it("lights before it darkens when a plan does both", function()
    local plan = config.monitors_plan({ { output = "A", disabled = true }, { output = "B", disabled = false,
      mode = "preferred", position = "auto", scale = 1 } }, "hyprlang")
    test.eq(plan[1].value, "B,preferred,auto,1")
    test.eq(plan[2].value, "A,disable")
  end)

  test.it("refuses a bad rule whole", function()
    test.eq(config.monitors_plan({ { output = "A", mode = "big" } }, "lua"), nil)
    test.eq(config.monitors_plan({ { output = "A", position = "0x0", scale = 1, mode = "preferred", colour = 1 } }, "lua"), nil)
    test.eq(config.monitors_plan({ { output = 'A" }) os.exit() --' } }, "lua"), nil)
  end)

  test.it("parses and groups modes", function()
    local modes = config.parse_modes { "1920x1080@60.00Hz", "2560x1440@144.00Hz", "2560x1440@59.95Hz", "1920x1080@60.00Hz" }
    test.eq(#modes, 3)
    test.eq(modes[1].mode, "2560x1440@144.00")
    local groups = config.group_modes(modes)
    test.eq(groups[1], { width = 2560, height = 1440, refreshes = { 144, 59.95 } })
  end)

  test.it("brings stranded workspaces home and shows full ones", function()
    local lit = { { name = "eDP-1", workspace = 1 }, { name = "DP-2", workspace = 2 } }
    local workspaces = {
      { id = 1, monitor = "eDP-1", windows = 2 }, { id = 2, monitor = "DP-2", windows = 0 },
      { id = 3, monitor = "DP-2", windows = 1 }, { id = 7, monitor = "HDMI-A-1", windows = 3 },
      { id = -98, monitor = "HDMI-A-1", windows = 1 },
    }
    test.eq(config.rehome_plan(lit, workspaces, "eDP-1", "hyprlang"), {
      { dispatch = "moveworkspacetomonitor", arg = "7 eDP-1" },
      { dispatch = "focusmonitor", arg = "DP-2" }, { dispatch = "workspace", arg = "3" } })
    test.contains(config.rehome_plan(lit, workspaces, "eDP-1", "lua")[1].eval,
      'hl.dispatch(hl.dsp.workspace.move({ workspace = 7, monitor = "eDP-1" }))')
  end)

  test.it("spells an animation preset both ways", function()
    local spec = { curve = { name = "preset", points = { 0.32, 0.72, 0, 1 } },
      leaves = { { name = "windowsIn", speed = 2.6, style = "popin 80%" }, { name = "fadeIn", enabled = false } } }
    test.contains(config.animation_plan(spec, "lua")[1].eval,
      'hl.animation({ leaf = "windowsIn", enabled = true, speed = 2.6, bezier = "preset", style = "popin 80%" })')
    test.eq(config.animation_plan(spec, "hyprlang"), {
      { keyword = "animations:enabled", value = "true" },
      { keyword = "bezier", value = "preset,0.32,0.72,0,1" },
      { keyword = "animation", value = "windowsIn,1,2.6,preset,popin 80%" },
      { keyword = "animation", value = "fadeIn,0" } })
  end)
end)


for _, flavour in ipairs { "lua", "hyprlang" } do
  test.describe("hyprland_config against a fake " .. flavour .. " Hyprland", function()
    local fake
    test.before_each(function()
      fake = fake_hyprland.new { flavour = flavour }
      test.load("hyprland_client.lua",
        { env = { HYPRLAND_INSTANCE_SIGNATURE = fake.signature, XDG_RUNTIME_DIR = fake.runtime } })
    end)
    test.after_each(function() fake.close() end)

    test.it("finds the flavour and sends options as it speaks", function()
      test.eq(test.ipc("push"), "sent")
      test.wait(function() fake.serve() return test.ipc("answer", "push") ~= "" end, 5000, "no answer")
      test.eq(test.ipc("flavour"), flavour)
      test.eq(test.ipc("answer", "push"), "true")
      if flavour == "lua" then
        test.eq(#fake.sent('/eval hl.config({ input = { kb_layout = "us,de" } }) hl.config({ input = { repeat_rate = 30 } })'), 1)
      else
        test.eq(#fake.sent("[[BATCH]]/keyword input:kb_layout us,de;/keyword input:repeat_rate 30"), 1)
      end
    end)

    test.it("reads every output, disabled ones too", function()
      fake.monitors[2].disabled = true
      test.wait(function() fake.serve() return test.ipc("outputs") ~= "" end, 5000, "no outputs")
      test.eq(test.ipc("outputs"), "DP-2:true:preferred eDP-1:false:1920x1200@60.00")
    end)
  end)
end
