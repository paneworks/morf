-- clipse's clipboard history (github.com/savedra1/clipse), read as it keeps
-- it: `~/.config/clipse/clipboard_history.json`, newest first, text and
-- pictures (a picture's `filePath` is a file clipse saved).
--
--   local clipse = require("lib.clipse")
--   for _, item in ipairs(clipse.history()) do print(item.value, item.image) end
--   clipse.copy(item)          -- back on the clipboard, with wl-copy

local morf = require("morf")

local clipse = {}

--- Where clipse keeps it: `$CLIPSE_HISTORY`, else its default.
function clipse.path()
  local set = morf.env and morf.env("CLIPSE_HISTORY")
  if set and set ~= "" then return set end
  return morf.fs.home() .. "/.config/clipse/clipboard_history.json"
end

--- The history, newest first: `{ value, recorded, image, pinned }`, where
--- `image` is a picture's file ("" for text). Empty when there is none.
function clipse.history(path)
  local ok, text = pcall(morf.fs.read, path or clipse.path())
  if not ok or type(text) ~= "string" or text == "" then return {} end
  local fine, data = pcall(morf.json.decode, text)
  local items = fine and type(data) == "table" and data.clipboardHistory or {}
  local out = {}
  for _, item in ipairs(items) do
    if type(item) == "table" then
      out[#out + 1] = {
        value = tostring(item.value or ""),
        recorded = tostring(item.recorded or ""),
        image = type(item.filePath) == "string" and item.filePath ~= "null" and item.filePath or "",
        pinned = item.pinned == true,
      }
    end
  end
  return out
end

--- Puts `item` back on the clipboard: its text, or its picture.
function clipse.copy(item, done)
  local argv
  if item.image ~= "" then
    argv = { "sh", "-c", 'exec wl-copy < "$0"', item.image }
  else
    argv = { "wl-copy", "--", item.value }
  end
  morf.run(argv, {}, done or function() end)
end

return clipse
