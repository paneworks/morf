-- Starts the clipboard history with the shell, as impasto's shell.qml does:
-- a copy made before the launcher was first opened is still a copy.
local clipboard = require("services.clipboard")
local settings = require("services.settings")
clipboard.start()

-- Optional wipe on lock (shell.qml:242-249), joined here so neither service
-- depends on the other. Password-manager copies are never kept in the first
-- place; this covers everything else.
require("services.lock").on_change(function(locked)
  if locked and settings.clipboardWipeOnLock then clipboard.wipe() end
end)

-- The control centre's one-shot "Clear clipboard", also listed in the
-- launcher's `>` (ControlsService.qml).
require("services.controls").define_tile("clearClipboard", {
  detail = function()
    local count = clipboard.count()
    return count == 1 and "1 entry kept" or (count .. " entries kept")
  end,
  available = function() return settings.clipboardHistory and clipboard.count() > 0 end,
  action = function() clipboard.wipe() end,
})
