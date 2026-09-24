-- The island's "module" panel: whichever module detail is open.
--
-- Port of DetailFace.qml and the module half of DynamicIsland.qml. The
-- size is the module's (`modules.open_size`), declared so the capsule gets
-- there first; the island gives a detail 4 px, since each brings its own
-- margins, and keeps it pill-shaped while it is short. Switching from one
-- module to another swaps the detail inside while the capsule morphs.
--
-- `morf ipc call detail <id>` opens one, as a click on its chip does.

local ui = require("morf.ui")
local island = require("bar.island")
local modules = require("services.modules")

island.register("module", {
  padding = 4,
  pill = true,
  size = function()
    local id = modules.open_id:get()
    if id == "" then return 340, 116 end
    return modules.open_size(id)
  end,
  build = function()
    local children = { anchors = { fill = true } }
    for id, provider in pairs(modules.providers) do
      if provider.detail then
        children[#children + 1] = ui.Loader {
          anchors = { fill = true },
          active = function() return modules.open_id:get() == id end,
          source = provider.detail,
        }
      end
    end
    return ui.Item(children)
  end,
})

morf.ipc.detail = function(id)
  modules.activate(id or "")
  return modules.open_id:get()
end
