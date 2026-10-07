local test = morf.test
local path = morf.env("XDG_CACHE_HOME") .. "/caelestia-scale-test.json"
local legacy = morf.env("XDG_CACHE_HOME") .. "/caelestia-scale-legacy.json"
local function remove(file) if morf.fs.exists(file) then morf.fs.remove(file) end end
local HOST = [[
  local ui = require("morf.ui")
  local config = require("config")
  local scale = require("themes.ui_scale")
  ui.Item { width=400, height=600 }
  morf.ipc.zoom = function(value)
    if value then config.set("appearance.zoom", tonumber(value)) end
    return { zoom=config.get("appearance.zoom"), factor=scale.factor(), path=scale.path }
  end
]]
test.it("desktop scale migrates once and survives a different shell settings file", function()
  remove(path)
  morf.fs.write(legacy, '{"appearance":{"zoom":0.5}}')
  test.load("../shell/init.lua", {source=HOST, size={400,600}, env={
    CAELESTIA_SCALE_FILE=path, CAELESTIA_SETTINGS=legacy}})
  test.near(test.ipc("zoom").zoom, .5, .001)
  test.ipc("zoom", "-0.5")
  local saved=morf.json.decode(morf.fs.read(path))
  test.eq(saved.zoom, -.5)
  test.load("../shell/init.lua", {source=HOST, size={400,600}, env={
    CAELESTIA_SCALE_FILE=path, CAELESTIA_SETTINGS=legacy .. ".different"}})
  test.eq(test.ipc("zoom").zoom, -.5)
  test.near(test.ipc("zoom").factor, math.sqrt(.5), .001)
end)
for _, part in ipairs {"lock", "greet"} do
  for _, style in ipairs {"material", "tsugumori"} do
    test.it(style .. " " .. part .. " reads shared scale and keeps its authentication controls visible", function()
      morf.fs.write(path, '{"zoom":0.5}')
      local before=morf.fs.read(path)
      test.load("../" .. part .. "/init.lua", {size={1116,2484},
        args=part=="lock" and {"window","preview"} or {"preview"},
        env={CAELESTIA_SCALE_FILE=path, CAELESTIA_STYLE=style, CAELESTIA_DRY_RUN="1"},
        source=[[
          require("init")
          morf.ipc.scale_state=function()
            local screen=morf.screens[1]
            return {factor=require("themes.ui_scale").factor(), density=screen.density_scale,
              width=screen.width,height=screen.height}
          end
        ]]})
      test.advance(100) test.ipc("stage","sheet") test.type("x") test.advance(2500)
      local state=test.ipc("scale_state")
      test.near(state.factor, math.sqrt(2), .001)
      test.near(state.density, math.sqrt(2), .02)
      local field=test.get(part .. "-field")
      test.truthy(field and field.visible)
      test.truthy(field.y+field.height <= state.height+1, "Password field is clipped")
      test.eq(morf.fs.read(path),before,"Authentication process wrote the shared preference")
      test.eq(#test.logs("error"),0)
    end)
  end
end
