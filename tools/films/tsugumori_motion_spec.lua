-- Run with morf test --no-dbus --snapshots DIR. Capture actual intermediate
-- poses so a passing final-state assertion cannot conceal a static effect.
local test = morf.test
local function env()
  return { CAELESTIA_STYLE="tsugumori", CAELESTIA_DRY_RUN="1",
    CAELESTIA_SETTINGS=morf.env("XDG_CACHE_HOME").."/tsugumori-film-settings.json",
    CAELESTIA_APPEARANCE=morf.env("XDG_CACHE_HOME").."/tsugumori-film-appearance.json" }
end
local function film(prefix, frames, step)
  for i=1,frames do
    test.advance(step)
    test.snapshot(("%s-%03d.png"):format(prefix,i))
  end
end
test.it("Tsugumori lock registers its field and assembles the access panel",function()
  test.load("../../examples/shells/caelestia/lock/init.lua",{env=env(),args={"window","preview"},size={1280,900}})
  test.advance(50)
  test.ipc("stage","sheet")
  film("lock-register",16,100)
  test.near(test.get("lock-folio").opacity,1,0.001)
  test.truthy(test.get("lock-phase-field").visible)
  test.click("lock-method")
  test.type("example")
  film("lock-glyph",8,100)
  test.ipc("stage","rest")
  film("lock-dismiss",10,100)
  test.falsy(test.get("lock-sheet").visible)
  test.eq(#test.logs("warn"),0)
  test.eq(#test.logs("error"),0)
end)
test.it("Tsugumori Lule entrance and exit use a cover then a wipe",function()
  test.load("../../examples/shells/caelestia/shell/init.lua",{env=env(),size={1920,1080}})
  test.advance(2000)
  test.ipc("lule","open")
  film("lule-open",9,100)
  test.falsy(test.get("drawer-dashboard-curtain").visible)
  test.ipc("dashboard","close")
  film("lule-close",9,100)
  test.falsy(test.get("drawer-dashboard").visible)
  test.eq(#test.logs("error"),0)
end)
