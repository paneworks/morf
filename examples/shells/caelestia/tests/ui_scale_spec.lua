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
test.it("compositor scaling persists the output scale and sends no additional Morf zoom", function()
  remove(path)
  test.load("../shell/init.lua", {source=[[
    local calls={}
    package.loaded["lib.integrations.hyprland"]={available=function() return true end,
      eval=function(chunk) calls[#calls+1]=chunk end}
    local scale=require("themes.ui_scale")
    scale.apply()
    require("morf.ui").Item {width=620,height=1380}
    morf.ipc.set=function() scale.set(.85) return {factor=scale.factor(),calls=calls} end
  ]],size={620,1380},env={CAELESTIA_SCALE_FILE=path,CAELESTIA_SCALE_MODE="compositor"}})
  local state=test.ipc("set")
  test.eq(state.factor,1)
  test.truthy(state.calls[1]:find("bresilla_set_display_scale(1.80",1,true))
  local saved=morf.json.decode(morf.fs.read(path))
  test.near(saved.scale,1.8,.001)
  test.near(saved.zoom,.85,.001)
end)
for _,part in ipairs {"lock","greet"} do
  test.it(part .. " follows compositor units without applying the saved zoom twice",function()
    morf.fs.write(path,'{"zoom":0.85,"scale":1.8}')
    local before=morf.fs.read(path)
    test.load("../"..part.."/init.lua",{size={620,1380},
      args=part=="lock" and {"window","preview"} or {"preview"},
      env={CAELESTIA_SCALE_FILE=path,CAELESTIA_SCALE_MODE="compositor",CAELESTIA_DRY_RUN="1"},
      source=[[
        require("init")
        morf.ipc.scale_state=function() return {factor=require("themes.ui_scale").factor(),
          density=morf.screens[1].density_scale,height=morf.screens[1].height} end
      ]]})
    test.advance(100) test.ipc("stage","sheet") test.type("x") test.advance(2500)
    local state=test.ipc("scale_state")
    test.eq(state.factor,1) test.eq(state.density,1)
    local field=test.get(part.."-field")
    test.truthy(field.visible and field.y+field.height<=state.height+1)
    test.eq(morf.fs.read(path),before)
    test.eq(test.logs("error"),{})
  end)
end
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
