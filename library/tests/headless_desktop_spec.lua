local test = morf.test

test.describe("desktop protocols with no compositor", function()
  test.before_each(function()
    test.load { size = { 200, 100 }, source = [[
      local ui = require("morf.ui")
      morf.surface.width, morf.surface.height = 200, 100
      ui.Rect { width = 200, height = 100 }
      local answer = ""
      morf.ipc.capture = function()
        morf.screencopy.capture(false, function(frame, err) answer = err or "frame" end)
        return true
      end
      morf.ipc.answer = function() return answer end
      morf.ipc.ask = function()
        for _ = 1, 40 do
          morf.toplevels.close("x")
          morf.output_power.set("off")
        end
        return #morf.windows
      end
    ]] }
  end)

  test.it("answers a capture with an error rather than never", function()
    test.truthy(test.ipc("capture"))
    test.advance(16)
    test.eq(test.ipc("answer"), "screen capture is unavailable")
  end)

  test.it("takes requests every frame so the queues never fill", function()
    for _ = 1, 4 do
      test.eq(test.ipc("ask"), 0)
      test.advance(16)
    end
    test.eq(#test.logs("error"), 0)
  end)
end)
