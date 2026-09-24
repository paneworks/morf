-- Wallpapers: which pictures there are, which one is up, and putting one up.
--
-- Port of WallpaperService.qml. The original listed a folder and handed the
-- picture to awww with a transition; here the shell draws the wallpaper
-- itself (desktop/wallpaper.lua), so applying one is setting a path and a
-- transition, and the picture's colours follow through the theme service.
--
-- Extensions are matched whatever their case (a camera's .JPG). The picture
-- up is also a stable link, `$XDG_STATE_HOME/impasto-morf/current-wallpaper`,
-- for anything outside the shell that wants it (hyprlock run by hand), as
-- upstream's theme manager keeps one.

local settings = require("services.settings")

local M = {}

M.EXTENSIONS = { jpg = true, jpeg = true, png = true, webp = true }
M.TRANSITIONS = {
  { id = "fade", label = "Fade" }, { id = "wipe", label = "Wipe" },
  { id = "wave", label = "Wave" }, { id = "circle", label = "Circle" },
  { id = "outer", label = "Outer" }, { id = "none", label = "None" },
  { id = "random", label = "Random" },
}

M.current = morf.signal("impasto.wallpaper.current", "")
M.previous = morf.signal("impasto.wallpaper.previous", "")
M.transition = morf.signal("impasto.wallpaper.transition", "none")
-- Bumps every time a picture is put up, so the drawing can restart its
-- transition even when the same picture comes back.
M.generation = morf.signal("impasto.wallpaper.generation", 0)
M.revision = morf.signal("impasto.wallpaper.revision", 0)

local listed = {}

--- The folder the pictures are kept in.
function M.dir() return morf.fs.expand(settings.wallpaperDir) end

--- Lists the folder again; the list is sorted by name.
function M.scan()
  local out = {}
  local entries = morf.fs.list(M.dir(), { depth = 2 }) or {}
  for _, entry in ipairs(entries) do
    if entry.is_file and M.EXTENSIONS[tostring(entry.extension or ""):lower()] then
      out[#out + 1] = entry.path
    end
  end
  table.sort(out)
  listed = out
  M.revision:set(M.revision:get() + 1)
  return out
end

--- The pictures, as paths. A binding follows rescans.
function M.list()
  M.revision:get()
  return listed
end

local function transition_type(id)
  if id == "random" then
    local pool = { "fade", "wipe", "wave", "circle", "outer" }
    return pool[math.random(#pool)]
  end
  return id or "wipe"
end

M.state_dir = morf.fs.join(morf.fs.dir("state") or ((morf.fs.home() or "") .. "/.local/state"), "impasto-morf")
M.link_path = morf.fs.join(M.state_dir, "current-wallpaper")

--- Points the current-wallpaper link at `path`.
local function link(path)
  local fs = morf.fs
  fs.mkdir(M.state_dir, { parents = true })
  local ok, target = pcall(fs.read_link, M.link_path)
  if ok and target == path then return end
  fs.remove(M.link_path)
  local made, err = fs.symlink(path, M.link_path)
  if not made then morf.log("warn", "impasto: cannot update the current-wallpaper link: " .. tostring(err)) end
end

--- Puts a picture up, with the transition Settings names.
function M.apply(path, transition)
  if not path or path == "" then return end
  if not morf.fs.is_file(path) then
    morf.log("warn", "impasto: no wallpaper at " .. path)
    return
  end
  M.previous:set(M.current:get())
  M.transition:set(transition_type(transition or settings.wallpaperTransition))
  M.current:set(path)
  M.generation:set(M.generation:get() + 1)
  settings.set("wallpaper", path)
  link(path)
end

--- The next or previous picture in the folder.
function M.step(offset)
  local list = #listed > 0 and listed or M.scan()
  if #list == 0 then return end
  local at = 0
  for index, path in ipairs(list) do
    if path == M.current:get() then at = index end
  end
  local next_index = ((at - 1 + offset) % #list) + 1
  M.apply(list[next_index])
end

-- Back to the picture that was up, or the first one there is.
M.scan()
local saved = settings.wallpaper
if saved ~= "" and morf.fs.is_file(saved) then
  M.current:set(saved)
elseif listed[1] then
  M.current:set(listed[1])
end

return M
