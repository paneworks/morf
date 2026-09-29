local test = morf.test
local HOST = [[
  local lule = require("lib.lule")
  local root = morf.env("LULE_TEST_ROOT")
  local result
  morf.ipc.generate = function()
    result = nil
    local child, err = lule.generate({
      image = root .. "/image with spaces.png", theme = "light", palette = "tonal",
      configs = root .. "/config", cache = root .. "/cache",
      env = { TMPDIR = root .. "/tmp", TEMP = root .. "/tmp", TMP = root .. "/tmp", LULE_A = root .. "/cache" },
    }, function(r) result = r end)
    return child ~= nil, err
  end
  morf.ipc.result = function() return result end
  morf.ipc.scheme = function() return lule.read(root .. "/cache/colors.json") end
]]
test.describe("Lule client", function()
  test.it("generates a real palette and invokes only the isolated configuration's hook", function()
    if not require("lib.poll").which("lule") then test.note("Lule unavailable; skipping real CLI") return end
    local root = morf.env("XDG_CACHE_HOME") .. "/lule-client-spec"
    morf.fs.mkdir(root .. "/tmp", { parents = true })
    local image = root .. "/image with spaces.png"
    test.load("../lib/lule.lua", { source = HOST, env = { LULE_TEST_ROOT = root } })
    assert(morf.fs.write(image, morf.encoding.base64_decode("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")))
    assert(morf.fs.write(root .. "/config/init.lua", 'local lule = require("lule")\nlule.on.colors(function(c) lule.write(' .. string.format("%q", root .. "/hook.txt") .. ', c.wallpaper) end)\n'))
    test.truthy(test.ipc("generate"))
    test.wait(function() return test.ipc("result") ~= nil end, 15000)
    local r = test.ipc("result") test.truthy(r.ok, r.stderr or r.error)
    local s = test.ipc("scheme")
    test.eq(s.wallpaper, image) test.eq(s.theme, "light") test.eq(#s.colors, 256)
    test.eq(morf.fs.read(root .. "/hook.txt"), image)
    test.eq(#test.logs("error"), 0)
  end)
end)
