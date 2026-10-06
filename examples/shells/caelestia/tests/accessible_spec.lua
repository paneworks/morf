-- caelestia to a screen reader: every drawer, opened in each theme, gives
-- every control it shows a role and a name, its panels a landmark, and the
-- shell no unnamed thing a screen reader could land on.
--
--     morf test --no-dbus examples/shells/caelestia/tests/accessible_spec.lua

local test = morf.test

local W, H = 1920, 1080

-- Roles a screen reader reads by name.
local NAMED = { button = true, toggle_button = true, check_box = true, radio_button = true, switch = true,
  link = true, menu_item = true, menu_item_check = true, menu_item_radio = true, slider = true, spin_button = true,
  tab = true, text_field = true, password_text = true, text_area = true, search_field = true,
  list_box_option = true, grid_cell = true, list_item = true, image = true, dialog = true, alert_dialog = true,
  splitter = true }

local function load(style)
  for _, program in ipairs { "systemctl", "loginctl" } do test.stub_run(program, { code = 0 }) end
  test.stub_run("task", { code = 0, stdout = "[]" })
  test.load("../shell/init.lua", {
    size = { W, H },
    env = { CAELESTIA_STYLE = style, CAELESTIA_WALLPAPER = "", CAELESTIA_FONT_FILE = "", CAELESTIA_DRY_RUN = "1",
      LULE_A = "/nonexistent/lule", HOME = morf.env("XDG_CACHE_HOME") },
  })
  test.settle(2000)
end

-- Every drawer and page, each opened from shut: (name, how).
local function ipc(...) local a = { ... } return function() test.ipc(table.unpack(a)) end end
local function tab(drawer, key)
  return function() test.ipc(drawer, "open") test.settle(1200) test.click(drawer .. "-tab-" .. key) end
end
local OPEN = {
  { "launcher", ipc("launcher", "open") },
  { "session", ipc("session", "open") },
  { "capture", ipc("capture", "open") },
  { "tasks", ipc("tasks", "open") },
  { "calendar", ipc("calendar", "open") },
  { "assistant", ipc("bottom", "open", "assistant") },
  { "drop", ipc("bottom", "open", "drop") },
  { "settings", ipc("utilities", "open") },
  { "notifications", ipc("sidebar", "open", "notifications") },
}
for _, key in ipairs { "dashboard", "media", "perf", "battery", "weather", "terminal" } do
  OPEN[#OPEN + 1] = { "dashboard " .. key, tab("dashboard", key) }
end
for _, page in ipairs { "network", "bluetooth", "sound", "sound/equalizer", "sound/equalizer/audiogram", "microphone",
  "power", "bar", "wired", "mesh", "tunnel", "focus" } do
  OPEN[#OPEN + 1] = { "settings " .. page, function() test.ipc("utilities", "open") test.settle(800) test.ipc("settings", page) end }
end

local function unnamed()
  local out = {}
  for _, row in ipairs(test.accessible()) do
    if row.name == "" and (row.focusable or NAMED[row.role]) then
      out[#out + 1] = row.role .. " " .. (row.id ~= "" and row.id or "?")
    end
  end
  return out
end

for _, style in ipairs { "material", "tsugumori" } do
  test.it(style .. " names every control a screen reader reaches, drawer by drawer", function()
    load(style)
    local problems = {}
    for _, entry in ipairs(OPEN) do
      entry[2]() test.settle(700)
      for _, p in ipairs(unnamed()) do problems[#problems + 1] = entry[1] .. ": " .. p end
      test.ipc("close") test.settle(500)
    end
    test.eq(#problems, 0, table.concat(problems, "\n"))
    -- The landmarks a screen reader walks: the rail, and each panel as it opens.
    test.eq(test.accessible({ id = "rail" })[1].role, "navigation")
    test.ipc("dashboard", "open") test.settle(900)
    local panel = test.accessible({ id = "drawer-dashboard" })[1]
    test.eq(panel.role, "region") test.eq(panel.name, "Dashboard")
    test.ipc("session", "open") test.settle(900)
    test.eq(test.accessible({ id = "drawer-session" })[1].role, "dialog")
    -- Headings Orca can walk by, named by their words.
    test.ipc("close") test.settle(500)
    test.ipc("utilities", "open") test.settle(900)
    test.truthy(#test.accessible { role = "heading", name = "Settings" } > 0, "Settings has no Settings heading")
  end)
end
