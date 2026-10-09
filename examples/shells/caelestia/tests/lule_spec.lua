local test = morf.test
local HOST = [[
  require("services").here = function() return true end
  require("init")
  local s = require("lule_studio")
  local copied = ""
  morf.clipboard.set = function(value) copied = value return true end
  morf.ipc.studio = function()
    return { selected = s.selected:get(), busy = s.busy:get(), failed = s.failed:get(), message = s.message:get(),
      preview = s.preview:get() ~= "", preview_error = s.preview_error:get(), active = s.active:get(),
      here = require("services").here(), bottom = require("bottom").drawer.open:get(), tab = require("utilities").displayed:get() == "theme/lule", dashboard = require("sidebar").drawer.open:get(),
      copied = copied, mode = s.mode:get(), method = s.method:get(), count = #s.files:get(), keyboard = morf.surface.keyboard_focus,
      folder = s.folder:get(), saved_folder = require("config").get("lule.folder") }
  end
  morf.ipc.choose = s.select
  morf.ipc.apply_twice = function() return { s.apply(), s.apply() } end
  morf.ipc.random_twice = function() return { s.random_apply(), s.random_apply() } end
]]
local root, first, second
local function load(size)
  root = morf.env("XDG_CACHE_HOME") .. "/lule-ui-spec"
  first, second = root .. "/images/01 garden.png", root .. "/images/02 blue.png"
  local png = morf.encoding.base64_decode("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")
  assert(morf.fs.write(first, png)) assert(morf.fs.write(second, png))
  local colors = {} for i = 1, 256 do colors[i] = i % 2 == 0 and "#90bacc" or "#26333a" end
  assert(morf.fs.write(root .. "/colors.json", morf.json.encode { wallpaper = first, theme = "dark", colors = colors,
    special = { background = "#152026", foreground = "#dfeef5", cursor = "#90bacc" } }))
  test.stub_run("lule", { code = 0 })
  test.load("../shell/init.lua", { source = HOST, size = size or { 2160, 1440 }, env = {
    CAELESTIA_STYLE = morf.env("CAELESTIA_STYLE") or "material",
    HYPRLAND_INSTANCE_SIGNATURE = false, CAELESTIA_DRY_RUN = "0", CAELESTIA_SETTINGS = root .. "/settings.json",
    CAELESTIA_WALLPAPER = "", LULE_A = root, LULE_W = root .. "/images", LULE_C = false,
  } })
  test.settle(600)
end
local function state() return test.ipc("studio") end
local function lule_runs()
  local out = {} for _, args in ipairs(test.runs()) do if args[1] == "lule" then out[#out + 1] = args end end
  return out
end
local function click(target, ...)
  if type(target) == "string" then
    local scroll = test.find { id = "lule-scroll" }
    local item = test.get(target)
    local panel = test.find { id = "drawer-sidebar" }
    local bottom = scroll and math.min(scroll.y + scroll.height, panel and panel.y + panel.height - 24 or math.huge)
    if scroll and (item.y < scroll.y or item.y + item.height > bottom) then
      test.wheel(0, item.y - scroll.y - 30, { x = scroll.x + 40, y = scroll.y + 60 })
      test.advance(300)
    end
  end
  return test.click(target, ...)
end
test.describe("caelestia Lule", function()
  test.it("previews without applying, uses the SVG tab, and applies one command with chosen options", function()
    load()
    test.falsy(state().active)
    test.eq(#lule_runs(), 0)
    test.ipc("lule", "open") test.settle(800)
    -- Lule is a page of the quick settings: Settings, Theme, Lule.
    test.truthy(state().tab)
    test.eq(state().count, 2)
    test.truthy(state().dashboard)
    test.falsy(state().bottom)
    test.falsy(test.find { id = "bottom-tab-lule" })
    local panel = test.get("drawer-sidebar")
    -- The panel's width is a side panel's: the cards stack, and the page
    -- scrolls down to the controls.
    test.truthy(test.get("lule-colors-card").y >= test.get("lule-wallpaper-card").y + test.get("lule-wallpaper-card").height)
    for _, id in ipairs { "lule-preview", "lule-color-15", "lule-cursor", "lule-apply", "lule-random-apply" } do
      local item = test.get(id)
      test.truthy(item.x >= panel.x and item.x + item.width <= panel.x + panel.width, id .. " overflows horizontally")
    end
    test.eq(state().keyboard, "on_demand")
    test.wait(function() return state().preview end, 5000, "wallpaper preview")
    test.eq(state().preview_error, "")
    click("lule-color-1")
    test.eq(state().copied, "#90bacc")
    click("lule-browse") test.settle(200)
    click("lule-file-2") test.settle(200)
    test.eq(state().selected, second)
    test.eq(#lule_runs(), 0, "browsing changed the desktop")
    click("lule-mode-light") click("lule-method-tonal")
    test.eq(test.ipc("apply_twice"), { true, false }, "duplicate application was started")
    test.advance(32)
    test.falsy(state().busy)
    test.falsy(state().failed, state().message)
    test.eq(lule_runs(), { { "lule", "create", "--image=" .. second, "--theme=light", "--palette=tonal", "--", "set" } })
    click("sidebar-tab-notifications") test.settle(700)
    test.falsy(state().active) test.falsy(state().preview)
    test.eq(state().keyboard, "none")
    test.eq(#test.logs("error"), 0)
  end)
  test.it("randomly selects and applies in one click without duplicate or empty-library runs", function()
    load({ 1920, 1080 })
    test.ipc("lule", "open") test.settle(800)
    click("lule-mode-light") click("lule-method-tonal")
    click("lule-random-apply") test.advance(32)
    test.eq(state().selected, second)
    test.eq(lule_runs(), { { "lule", "create", "--image=" .. second, "--theme=light", "--palette=tonal", "--", "set" } })
    test.eq(test.ipc("random_twice"), { true, false }) test.advance(32)
    test.eq(#lule_runs(), 2, "a second random apply ran while busy")
    assert(morf.fs.remove(first)) assert(morf.fs.remove(second))
    click("lule-random-apply") test.advance(32)
    test.eq(#lule_runs(), 2, "an empty folder reapplied the previous selection")
    test.truthy(state().failed)
    test.eq(#test.logs("error"), 0)
  end)
  test.it("reports failure, permits retry, and cancels invalid selections", function()
    load({ 1920, 1080 })
    test.ipc("lule", "open") test.settle(800)
    local page = test.get("lule-scroll")
    test.wheel(0, 3000, { x = page.x + 40, y = page.y + 60 }) test.advance(500)
    local apply = test.get("lule-apply")
    local panel = test.get("drawer-sidebar")
    test.truthy(apply.y + apply.height < panel.y + panel.height, "Apply is outside the panel")
    test.falsy(test.ipc("choose", root .. "/missing.png"))
    test.eq(#lule_runs(), 0)
    test.stub_run("lule", { code = 1, stderr = "Image could not be decoded" })
    click("lule-apply") test.advance(32)
    test.truthy(state().failed)
    test.eq(state().message, "Image could not be decoded")
    test.falsy(state().busy)
    test.stub_run("lule", { code = 0 })
    click("lule-apply") test.advance(32)
    test.falsy(state().failed)
    click(100, 100) test.settle(700)
    test.falsy(state().active)
    test.ipc("lule", "open") test.settle(700)
    click("lule-folder") test.key("Escape") test.settle(700)
    test.falsy(state().dashboard, "Escape from the path field did not close the panel")
    test.eq(state().keyboard, "none")
    test.eq(#test.logs("error"), 0)
  end)
  test.it("refreshes added and removed wallpapers on navigation and reopening", function()
    load({ 1920, 1080 })
    test.ipc("lule", "open") test.settle(800)
    local png = morf.fs.read(first)
    local added = root .. "/images/03 new.png"
    assert(morf.fs.write(added, png))
    assert(morf.fs.remove(second))
    click("lule-shuffle") test.advance(32)
    test.eq(state().count, 2)
    test.eq(state().selected, added, "Shuffle used the stale file list")
    test.eq(#lule_runs(), 0)
    assert(morf.fs.remove(added))
    click("lule-random-apply") test.advance(32)
    test.eq(state().count, 1)
    test.eq(state().selected, first, "Random & apply picked a removed image")
    test.eq(#lule_runs(), 1)
    assert(morf.fs.write(second, png))
    click("lule-next") test.advance(32)
    test.eq(state().selected, second, "Next missed a new image")
    assert(morf.fs.write(added, png))
    click("lule-browse") test.settle(100)
    test.eq(state().count, 3, "Images used the stale file list")
    click("sidebar-tab-notifications") test.settle(700)
    assert(morf.fs.remove(added))
    -- Back on the settings the overview shows again; Lule is reopened.
    test.ipc("lule", "open") test.settle(800)
    test.eq(state().count, 2, "reopening the tab did not refresh the folder")
    test.eq(#lule_runs(), 1, "browsing applied a wallpaper")
    test.eq(#test.logs("error"), 0)
  end)
  test.it("changes and remembers the wallpaper folder without applying an image", function()
    load({ 1920, 1080 })
    test.ipc("lule", "open") test.settle(800)
    test.eq(test.get("lule-folder").text, root .. "/images")
    local folder = root .. "/other wallpapers"
    local image = folder .. "/new wallpaper.png"
    assert(morf.fs.write(image, morf.fs.read(first)))
    click("lule-folder") test.key("a", "Ctrl") test.type(folder)
    click("lule-use-folder") test.settle(300)
    test.eq(state().folder, folder)
    test.eq(state().saved_folder, folder)
    test.eq(state().count, 1)
    test.eq(state().selected, first, "changing the collection changed the selected wallpaper")
    test.eq(#lule_runs(), 0, "changing the collection applied a wallpaper")
    local saved = morf.json.decode(morf.fs.read(root .. "/settings.json"))
    test.eq(saved.lule.folder, folder)
    click("lule-folder") test.key("a", "Ctrl") test.type(root .. "/missing") test.key("Return")
    test.settle(200)
    test.truthy(state().failed)
    test.eq(state().folder, folder)
    test.eq(state().saved_folder, folder)
    test.eq(state().count, 1)
    click("lule-folder") test.key("a", "Ctrl") test.type(image) test.key("Return")
    test.settle(200)
    test.falsy(state().failed, "the path field rejected an image")
    test.eq(state().selected, image)
    test.eq(#lule_runs(), 0, "selecting a file applied it immediately")
    test.eq(state().folder, folder)
    load({ 1920, 1080 })
    test.ipc("lule", "open") test.settle(800)
    test.eq(test.get("lule-folder").text, folder, "saved folder was lost on restart")
    test.eq(state().count, 1)
    click("lule-random-apply") test.advance(32)
    test.eq(state().selected, image)
    test.eq(lule_runs(), { { "lule", "create", "--image=" .. image, "--theme=dark", "--palette=pigment", "--", "set" } })
    test.eq(#test.logs("error"), 0)
  end)
end)
