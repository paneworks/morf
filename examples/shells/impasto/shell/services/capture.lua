-- Capture: a photo of the screen, cut to a region, a window or the whole
-- of it, and sent where it was asked to go; or the box a recording takes.
--
-- Port of CaptureService.qml and scripts/capture.py, without the script.
-- `open()` photographs the screen with `morf.screencopy.save`, the overlay
-- (capture/overlay.lua) draws that still picture with the selection on
-- top, and `fire()` cuts the selection out of the same pixels with
-- `morf.image.process`. Nothing is frozen, and the result is exactly what
-- was on screen when the key was pressed, menus included.
--
-- Where it goes: a file in the pictures folder (and the clipboard too, as
-- the original did), the clipboard alone (`morf.clipboard.set`), satty to
-- annotate it, or tesseract to read it as text. The last two are other
-- programs, started by argv when they are installed and not offered when
-- they are not. The result is flashed on the island (the original's OSD),
-- not kept in the notification history.
--
-- Every screen's shell hears a capture key; only the screen being worked on
-- opens the surface (services/live.lua, in services/auto/capture.lua), and
-- it photographs its own output.
--
-- Shape and kind are remembered (`captureShape`, `captureKind`); the
-- destination is a file again every time.

local settings = require("services.settings")
local theme = require("theme")
local act = require("services.act")

local fs = morf.fs
local M = {}

M.shapes = { "region", "window", "screen" }
M.kinds = { "photo", "video" }
M.destinations = { "file", "clipboard", "editor", "text" }

local s = {
  active = morf.signal("impasto.capture.active", false),
  busy = morf.signal("impasto.capture.busy", false),
  photo = morf.signal("impasto.capture.photo", ""),
  photo_w = morf.signal("impasto.capture.photo_w", 0),
  photo_h = morf.signal("impasto.capture.photo_h", 0),
  to = morf.signal("impasto.capture.to", "file"),
  -- Bumped on every opening, so the overlay can reset its selection.
  opened = morf.signal("impasto.capture.opened", 0),
  last = morf.signal("impasto.capture.last", ""),
}
M.signals = s

-- ------------------------------------------------------------------ tools --

local tools = { editor = act.which("satty"), text = act.which("tesseract") }

function M.can(what) return tools[what] ~= nil end

--- Destinations whose program is missing are not offered.
function M.offers(id)
  if id == "editor" or id == "text" then return M.can(id) end
  return id == "file" or id == "clipboard"
end

--- Where saved captures go: $IMPASTO_CAPTURES or $HYPRSHOT_DIR (both set
--- in upstream's env.lua), then $XDG_PICTURES_DIR, then the XDG pictures
--- folder (user-dirs.dirs), then the home folder.
function M.directory()
  for _, name in ipairs { "IMPASTO_CAPTURES", "HYPRSHOT_DIR", "XDG_PICTURES_DIR" } do
    local value = morf.env(name)
    if value and value ~= "" then return fs.expand and fs.expand(value) or value end
  end
  return fs.dir("pictures") or fs.home()
end

local function runtime()
  return fs.dir("runtime") or "/tmp"
end

-- ---------------------------------------------------------------- options --

function M.shape() return settings.captureShape end
function M.kind() return settings.captureKind end
function M.to() return s.to:get() end
function M.active() return s.active:get() end
function M.busy() return s.busy:get() end
function M.photo() return s.photo:get() end

function M.set_shape(id) settings.set("captureShape", id) end
function M.set_kind(id) settings.set("captureKind", id) end
function M.set_to(id) if M.offers(id) then s.to:set(id) end end

-- ------------------------------------------------------------------ screen --

-- The screen being photographed: this shell's own. One surface covers one
-- screen, and the shell that opens it is the one on the screen being worked
-- on (services/live.lua).
M.screen_name = ""

local function screen_entry()
  for _, screen in ipairs(morf.screens or {}) do
    if screen.name == M.screen_name then return screen end
  end
  return (morf.screens or {})[1] or {}
end

function M.screen()
  local screen = screen_entry()
  return {
    name = screen.name or "",
    x = tonumber(screen.x) or 0, y = tonumber(screen.y) or 0,
    width = tonumber(screen.width) or 1920, height = tonumber(screen.height) or 1080,
  }
end

--- Physical pixels per layout unit: the photo is the output's own pixels.
function M.ratio()
  local w = s.photo_w:get()
  local screen = M.screen()
  if w <= 0 or screen.width <= 0 then return 1 end
  return w / screen.width
end

-- Windows on screen, for window mode: read once per opening, as the
-- picture is still. `{ x, y, width, height }` in the surface's coordinates,
-- bottom to top. Only the workspaces this screen shows (its active one, and
-- a special workspace open over it): a window on another workspace is not
-- in the picture (CaptureOverlay.qml's `clientsOn(activeId)`).
M.windows = {}

local function load_windows()
  M.windows = {}
  local ok, hyprland = pcall(require, "lib.integrations.hyprland")
  if not ok or not hyprland.available or not hyprland.available() then return end
  hyprland.json("j/monitors", function(monitors)
    hyprland.json("j/clients", function(clients)
      monitors = type(monitors) == "table" and monitors or {}
      clients = type(clients) == "table" and clients or {}
      local screen = M.screen()
      local shown = {}
      for _, monitor in ipairs(monitors) do
        if monitor.name == screen.name then
          local active = type(monitor.activeWorkspace) == "table" and monitor.activeWorkspace.id
          local special = type(monitor.specialWorkspace) == "table" and monitor.specialWorkspace.id
          if active then shown[active] = true end
          if special and special ~= 0 then shown[special] = true end
        end
      end
      local out = {}
      for _, client in ipairs(clients) do
        local at, size = client.at or {}, client.size or {}
        local workspace = type(client.workspace) == "table" and client.workspace.id or nil
        local visible = next(shown) == nil or (workspace ~= nil and shown[workspace])
          or client.pinned == true
        if client.mapped ~= false and client.hidden ~= true and visible then
          out[#out + 1] = {
            x = (tonumber(at[1]) or 0) - screen.x, y = (tonumber(at[2]) or 0) - screen.y,
            width = tonumber(size[1]) or 0, height = tonumber(size[2]) or 0,
            title = tostring(client.title or ""),
            workspace = type(client.workspace) == "table" and client.workspace.id or nil,
          }
        end
      end
      M.windows = out
    end)
  end)
end

--- The topmost window under a point, or nil.
function M.window_under(x, y)
  local found
  for _, w in ipairs(M.windows) do
    if x >= w.x and x <= w.x + w.width and y >= w.y and y <= w.y + w.height then found = w end
  end
  return found
end

-- ---------------------------------------------------------------- notify --

-- What happened, as the island's flash (CaptureService.qml `took`): the
-- file's name, "Copied", "Opening the editor", the characters read, or why
-- nothing was. `last` keeps the whole of it for `capture.last`.
local function say(summary, body, icon)
  s.last:set(summary .. (body and body ~= "" and (": " .. body) or ""))
  local ok, osd = pcall(require, "services.osd")
  if ok and osd.request then osd.request(icon or "󰹑", summary, -1) end
end
M.say = say

-- ---------------------------------------------------------- open and close --

local function stamp() return morf.time.format("%Y-%m-%d-%H%M%S") end

--- Photographs the screen and shows the surface over it. Empty `shape` or
--- `kind` keep the last choice; `after` waits for a closing panel to leave
--- the picture.
function M.open(shape, kind, destination, after)
  if s.active:get() or s.busy:get() then return false end
  if shape and shape ~= "" then M.set_shape(shape) end
  if kind and kind ~= "" then M.set_kind(kind) end
  s.to:set((destination and destination ~= "" and M.offers(destination)) and destination or "file")
  s.busy:set(true)
  M.screen_name = ((morf.screens or {})[1] or {}).name or ""
  local path = fs.join(runtime(), "impasto-grab-" .. morf.time.now_ms() .. ".png")
  morf.timer(math.max(1, after or 1), function()
    local queued, err = pcall(morf.screencopy.save, {
      path = path, output = M.screen().name ~= "" and M.screen().name or nil,
      on_done = function(done, info)
        s.busy:set(false)
        if not done then
          say("Nothing was captured", tostring(info or ""), "󰀦")
          return
        end
        local w, h = tonumber(info and info.width) or 0, tonumber(info and info.height) or 0
        if w <= 0 then
          local read = morf.image.info(path)
          w, h = read and read.width or 0, read and read.height or 0
        end
        s.photo:set(path)
        s.photo_w:set(w)
        s.photo_h:set(h)
        load_windows()
        s.opened:set(s.opened:get() + 1)
        s.active:set(true)
      end,
    })
    if not queued then
      s.busy:set(false)
      say("Nothing was captured", tostring(err), "󰀦")
    end
  end, false)
  return true
end

--- Escape or a right click: the photo goes too, so nothing is left behind.
function M.cancel()
  if not s.active:get() then return end
  s.active:set(false)
  local photo = s.photo:get()
  if photo ~= "" then fs.remove(photo) end
  s.photo:set("")
end

-- ---------------------------------------------------------------- deliver --

-- The clipboard takes at most 32 MiB in one offer. The PNG is read whole
-- (a blocking read, but of a file just written to tmpfs or the pictures
-- folder, some milliseconds for a 4K screen); one past the cap is refused
-- rather than cut short.
local CLIPBOARD_MAX = 32 * 1024 * 1024
local function copy_image(path)
  local stat = fs.stat(path)
  if not stat or (stat.size or 0) <= 0 or stat.size > CLIPBOARD_MAX then return false end
  local bytes = fs.read(path, CLIPBOARD_MAX)
  if not bytes or bytes == "" then return false end
  local ok = pcall(morf.clipboard.set, bytes, "image/png")
  return ok
end

-- Where tesseract keeps its language packs: $TESSDATA_PREFIX, then where
-- the distributions put them.
local TESSDATA = {
  "/usr/share/tessdata", "/usr/share/tesseract-ocr/5/tessdata", "/usr/share/tesseract-ocr/4.00/tessdata",
  "/usr/share/tesseract/tessdata", "/usr/local/share/tessdata",
}

--- The installed packs, by name ("eng", "spa"), from the *.traineddata in
--- the tessdata folder: what `tesseract --list-langs` prints, without a
--- process.
function M.tesseract_languages()
  local dirs = {}
  local prefix = morf.env("TESSDATA_PREFIX")
  if prefix and prefix ~= "" then
    dirs[#dirs + 1] = prefix
    dirs[#dirs + 1] = fs.join(prefix, "tessdata")
  end
  for _, dir in ipairs(TESSDATA) do dirs[#dirs + 1] = dir end
  local installed = {}
  for _, dir in ipairs(dirs) do
    for _, entry in ipairs(fs.list(dir) or {}) do
      local name = entry.name:match("^(.+)%.traineddata$")
      if name then installed[name] = true end
    end
    if next(installed) then break end
  end
  return installed
end

-- tesseract's language packs are separate packages, and asking for a
-- missing one fails: Spanish and English where installed, else English.
local function languages(callback)
  local installed = M.tesseract_languages()
  local wanted = {}
  for _, name in ipairs { "spa", "eng" } do if installed[name] then wanted[#wanted + 1] = name end end
  callback(#wanted > 0 and table.concat(wanted, "+") or "eng")
end

local function deliver(cut, destination, name)
  if destination == "file" then
    -- Saved and copied, so there is nothing to choose at capture time; the
    -- file's name only, as the folder is always the same.
    local copied = copy_image(cut)
    say(cut:match("([^/]+)$") or cut, copied and "copied too" or "", "󰹑")
    M.saved = cut
    return copied
  end
  if destination == "clipboard" then
    local copied = copy_image(cut)
    fs.remove(cut)
    say(copied and "Copied" or "Not copied", "", "󰹑")
    return
  end
  if destination == "editor" then
    local folder = M.directory()
    fs.mkdir(folder, { parents = true })
    local saved = fs.join(folder, name)
    local command = { tools.editor, "--filename", cut, "--output-filename", saved, "--early-exit" }
    -- satty's own copy lives in its window, which --early-exit closes at
    -- once; wl-copy keeps serving the picture after it has gone.
    local wl_copy = act.which("wl-copy")
    if wl_copy then
      command[#command + 1] = "--copy-command"
      command[#command + 1] = wl_copy
    end
    local child = morf.spawn { command = command, detached = true }
    if not child then fs.remove(cut) end
    say(child and "Opening the editor" or "satty would not start", "", child and "󰏫" or "󰀦")
    return
  end
  -- Read as text.
  languages(function(langs)
    morf.run({ tools.text, cut, "-", "-l", langs }, { timeout_ms = 60000 }, function(result)
      fs.remove(cut)
      local text = tostring(result.stdout or ""):match("^%s*(.-)%s*$")
      if not result.ok or text == "" then
        say("No text found", "", "󱄽")
        return
      end
      local copied = pcall(morf.clipboard.set, text)
      local count = utf8.len(text) or #text
      say(copied and (count .. " characters copied") or (count .. " characters"), text:sub(1, 120), "󱄽")
    end)
  end)
end

--- Takes the capture. Layout coordinates on the surface; an empty box is
--- the whole screen.
function M.fire(x, y, width, height)
  if not s.active:get() then return end
  local whole = (width or 0) <= 0 or (height or 0) <= 0
  local picture, kind, destination = s.photo:get(), M.kind(), s.to:get()
  -- The surface goes first, or it would be in a recording.
  s.active:set(false)
  s.photo:set("")

  if kind == "video" then
    fs.remove(picture)
    local screen = M.screen()
    local box = not whole and {
      x = math.floor(x + screen.x + 0.5), y = math.floor(y + screen.y + 0.5),
      width = math.floor(width + 0.5), height = math.floor(height + 0.5),
    } or nil
    -- A frame for the surface to leave the screen before the encoder looks.
    morf.timer(theme.duration_fast(), function()
      require("services.recorder").start(whole and "screen" or M.shape(), box)
    end, false)
    return
  end

  local name = stamp() .. "_impasto.png"
  local cut
  if destination == "file" then
    local dir = M.directory()
    fs.mkdir(dir, { parents = true })
    cut = fs.join(dir, name)
  else
    cut = fs.join(runtime(), "impasto-cut-" .. morf.time.now_ms() .. ".png")
  end
  local r = M.ratio()
  local ops = {}
  if not whole then
    local px = math.max(0, math.floor(x * r + 0.5))
    local py = math.max(0, math.floor(y * r + 0.5))
    local pw = math.max(1, math.min(s.photo_w:get() - px, math.floor(width * r + 0.5)))
    local ph = math.max(1, math.min(s.photo_h:get() - py, math.floor(height * r + 0.5)))
    ops[1] = { "crop", px, py, pw, ph }
  end
  local queued, err = morf.image.process {
    source = picture, output = cut, ops = ops, format = "png",
    on_done = function(done, info)
      fs.remove(picture)
      if not done then
        say("Could not cut the picture", tostring(info or ""), "󰀦")
        return
      end
      deliver(cut, destination, name)
    end,
  }
  if not queued then
    fs.remove(picture)
    say("Could not cut the picture", tostring(err), "󰀦")
  end
end

return M
