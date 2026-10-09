local test = morf.test
local HOST = [[
  local s = require("lule_studio")
  require("morf.ui").Item { width = 100, height = 100 }
  morf.ipc.generate = function(apply) return { s.generate(apply), s.generate(apply) } end
  morf.ipc.options = function(logo, size) s.logo:set(logo) s.logo_size:set(size) end
  morf.ipc.source = s.set_source
  morf.ipc.state = function()
    return { selected=s.selected:get(),busy=s.busy:get(),failed=s.failed:get(),message=s.message:get() }
  end
]]
local root, logo
local function load(dry)
  root = morf.env("XDG_CACHE_HOME") .. "/lule-generation"
  logo = root .. "/logo with spaces.svg"
  morf.fs.write(logo, '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"/>')
  morf.fs.write(root .. "/settings.json", morf.json.encode {lule={source="generate",logo=logo,logo_size=40}})
  test.stub_run("lule", {code=0})
  test.load("../shell/init.lua", {source=HOST,size={408,900},env={
    CAELESTIA_SETTINGS=root .. "/settings.json", LULE_A=root .. "/palette",
    CAELESTIA_DRY_RUN=dry and "1" or "0" }})
end
local function state() return test.ipc("state") end
local function runs()
  local out = {} for _, argv in ipairs(test.runs()) do if argv[1]=="lule" then out[#out+1]=argv end end
  return out
end
local function output(argv)
  for _, arg in ipairs(argv) do if arg:sub(1,9)=="--output=" then return arg:sub(10) end end
end
test.it("generated preview uses safe arguments, preserves the desktop and rejects duplicates",function()
  load()
  test.eq(test.ipc("generate",false),{true,false})
  local argv=runs()[1]
  test.eq(argv[2],"wallpaper") test.eq(argv[3],"--logo="..logo) test.eq(argv[4],"--size=40")
  local path=output(argv)
  test.truthy(path:match("/lule/wallpapers/.*%.png$"))
  morf.fs.write(path,"generated image fixture") test.advance(32)
  test.eq(state().selected,path) test.falsy(state().failed) test.falsy(state().busy)
  test.eq(#runs(),1,"preview applied palette hooks")
  test.eq(test.ipc("generate",false),{true,false})
  local next_path=output(runs()[2])
  test.truthy(next_path~=path,"new generation would overwrite an existing wallpaper")
  morf.fs.write(next_path,"second image fixture") test.advance(32)
  test.eq(morf.fs.read(path),"generated image fixture")
end)
test.it("generate and apply waits for the image before running the palette hooks",function()
  load()
  test.ipc("generate",true)
  local path=output(runs()[1])
  test.eq(#runs(),1)
  morf.fs.write(path,"generated image fixture")
  morf.fs.write(root.."/palette/colors.json",morf.json.encode {wallpaper=path,colors={"#101010","#abcdef"}})
  test.advance(100)
  test.eq(runs()[2],{"lule","create","--image="..path,"--theme=dark","--palette=pigment","--","set"})
  test.falsy(state().busy) test.falsy(state().failed,state().message)
end)
test.it("invalid inputs, failed generators and preview mode never apply a previous image",function()
  load()
  test.ipc("options",logo,"0")
  test.eq(test.ipc("generate",true),{false,false}) test.eq(#runs(),0)
  test.ipc("options",root.."/missing.svg","40")
  test.eq(test.ipc("generate",true),{false,false}) test.eq(#runs(),0)
  test.ipc("options",logo,"40")
  test.stub_run("lule",{code=1,stderr="unknown command: wallpaper"})
  test.ipc("generate",true) test.advance(32)
  test.truthy(state().failed) test.falsy(state().busy) test.eq(#runs(),1)
  test.eq(state().message,"unknown command: wallpaper")
  test.stub_run("lule",{code=0})
  test.ipc("generate",true) test.advance(32)
  test.truthy(state().failed) test.eq(#runs(),2,"missing output was applied")
  load(true)
  test.eq(test.ipc("generate",true),{false,false}) test.eq(#runs(),0)
end)
