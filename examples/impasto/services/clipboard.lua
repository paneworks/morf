-- Clipboard history: every copy, anywhere, kept; any of them copied again.
--
-- Port of ClipboardService.qml and the substance of clipboard.py. There,
-- `wl-paste --watch` ran a script per copy that wrote a JSON index the shell
-- re-read. Here `morf.clipboard.watch` hears every copy through data
-- control, focused or not, and this file is the only writer:
--
--   index     `<settings dir>/clipboard.json`, metadata and a preview only,
--             rewritten whole a moment after a change
--   payloads  one file per entry under `<cache dir>/impasto-morf/clipboard`,
--             so the index stays small and an image is a file `ui.Image`
--             can show
--
-- Every screen's runtime runs this file, but only the primary one
-- (`morf.primary()`) keeps what is copied: one copy, one entry, one writer
-- of the index for what arrives. The others read the index back whenever it
-- changes (as the settings are), and what a person does to the history on
-- any screen -- forget one, wipe it -- is written there and read back by
-- the rest.
--
-- Password managers mark their offers with `x-kde-passwordManagerHint`;
-- those are never stored. A copy of something already kept moves it to the
-- top instead of adding it again, which is also what restoring one does.

local settings = require("services.settings")

local fs = morf.fs
local json = morf.json

local M = {}

-- Larger payloads are not kept; the preview is longer than a row because
-- the launcher searches it.
M.LIMIT = 16 * 1024 * 1024
M.PREVIEW = 240
M.SECRET = "x-kde-passwordManagerHint"

M.index_path = fs.join(settings.dir, "clipboard.json")
M.payload_dir = fs.join(fs.dir("cache") or (fs.home() .. "/.cache"), "impasto-morf", "clipboard")

local SUFFIXES = {
  ["image/png"] = ".png", ["image/jpeg"] = ".jpg", ["image/webp"] = ".webp",
  ["image/gif"] = ".gif", ["image/bmp"] = ".bmp", ["image/tiff"] = ".tiff",
}

-- Newest first:
--   key      digest of the content
--   kind     "text" or "image"
--   mime     what it is copied back as
--   file     the payload
--   preview  first characters, whitespace collapsed; shown and searched
--   bytes    size
--   copied   last time it reached the clipboard, ms since the epoch
local entries = {}
M.revision = morf.signal("impasto.clipboard.revision", 0)

local counter = 0
local function bump() counter = counter + 1 M.revision:set(counter) end

--- The entries, newest first. A binding that calls it follows the history.
function M.entries()
  M.revision:get()
  return entries
end

function M.count()
  M.revision:get()
  return #entries
end

-- ------------------------------------------------------------------ disk --

local saving = false
-- The index as this runtime last wrote or read it: a change to it that is
-- not ours is another screen's, and is read back.
local last_text

local function save()
  saving = false
  local out = {}
  for index, entry in ipairs(entries) do
    out[index] = {
      key = entry.key, hash = entry.hash, kind = entry.kind, mime = entry.mime,
      file = entry.file, preview = entry.preview, bytes = entry.bytes, copied = entry.copied,
    }
  end
  local text = json.encode({ entries = out })
  last_text = text
  local ok, err = fs.write(M.index_path, text)
  if not ok then morf.log("warn", "impasto: could not save the clipboard history: " .. tostring(err)) end
end

local function save_soon()
  if saving then return end
  saving = true
  morf.timer(300, save, false)
end

-- morf.json hands arrays back as tagged tables; the history keeps plain ones.
local function plain(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for k, v in pairs(value) do out[k] = plain(v) end
  return out
end

-- The entries an index holds, or nil when it cannot be read as one.
local function parse(text)
  if not text or text == "" then return {} end
  local ok, decoded = pcall(json.decode, text)
  if not ok or type(decoded) ~= "table" then return nil end
  decoded = plain(decoded)
  local out = {}
  for _, kept in ipairs(decoded.entries or {}) do
    if type(kept) == "table" and type(kept.key) == "string" and type(kept.file) == "string"
        and fs.exists(kept.file) then
      out[#out + 1] = {
        key = kept.key,
        hash = tostring(kept.hash or kept.key),
        kind = kept.kind == "image" and "image" or "text",
        mime = type(kept.mime) == "string" and kept.mime or "text/plain;charset=utf-8",
        file = kept.file,
        preview = type(kept.preview) == "string" and kept.preview or "",
        bytes = tonumber(kept.bytes) or 0,
        copied = tonumber(kept.copied) or 0,
      }
    end
  end
  return out
end

local function load()
  local text = fs.read(M.index_path)
  last_text = text
  local read = parse(text)
  if not read then
    morf.log("warn", "impasto: the clipboard history is not JSON; starting a new one")
    return
  end
  entries = read
end

--- The index changed under this runtime: another screen kept a copy, or
--- forgot one. Its entries replace ours -- unless ours are about to be
--- written, which is the newer of the two.
local function reread()
  if saving then return end
  local text = fs.read(M.index_path)
  if text == last_text then return end
  last_text = text
  local read = parse(text)
  if not read then return end
  entries = read
  bump()
end
M.reread = reread

-- Deletes an entry's payload unless another entry still names the file.
local function discard(entry)
  for _, other in ipairs(entries) do
    if other ~= entry and other.file == entry.file then return end
  end
  fs.remove(entry.file)
end

local function evict()
  local keep = math.max(1, math.floor(tonumber(settings.clipboardKeep) or 200))
  while #entries > keep do discard(table.remove(entries)) end
end

-- ---------------------------------------------------------------- digest --

-- The content's SHA-256, as clipboard.py named entries. It finds repeats
-- and names files.
local function digest(bytes)
  return morf.encoding.sha256(bytes)
end

--- A one-line preview, whitespace collapsed.
local function preview_of(text)
  local line = text:gsub("%s+", " "):match("^%s*(.-)%s*$") or ""
  if #line > M.PREVIEW then
    -- Cut on a character boundary, not inside one.
    local cut = M.PREVIEW
    while cut > 0 and (line:byte(cut + 1) or 0) & 0xC0 == 0x80 do cut = cut - 1 end
    line = line:sub(1, cut)
  end
  return line
end

-- ----------------------------------------------------------------- store --

--- Keeps a copy: `kind` "text" or "image", `mime`, the bytes.
function M.remember(kind, mime, bytes)
  if not bytes or #bytes == 0 or #bytes > M.LIMIT then return end
  if kind == "text" and not bytes:find("%S") then return end
  local hash = digest(bytes)
  local now = morf.time.now_ms()

  -- Copied again: to the top, with a new time. This also covers restore,
  -- since the clipboard announces our own copy like anyone else's.
  for index, entry in ipairs(entries) do
    if entry.hash == hash then
      entry.copied = now
      if index ~= 1 then table.insert(entries, 1, table.remove(entries, index)) end
      bump()
      save_soon()
      return entry
    end
  end

  local key = "clip-" .. hash:sub(1, 12)
  local suffix = SUFFIXES[mime] or (kind == "text" and ".txt" or ".bin")
  local file = fs.join(M.payload_dir, key .. suffix)
  local ok, err = fs.write(file, bytes)
  if not ok then
    morf.log("warn", "impasto: cannot keep the clipboard entry: " .. tostring(err))
    return
  end
  local entry = {
    key = key, hash = hash, kind = kind, mime = mime, file = file,
    preview = kind == "text" and preview_of(bytes) or "",
    bytes = #bytes, copied = now,
  }
  table.insert(entries, 1, entry)
  evict()
  bump()
  save_soon()
  return entry
end

local function entry_for(key)
  for index, entry in ipairs(entries) do
    if entry.key == key then return entry, index end
  end
end

--- Puts an entry back on the clipboard. It moves to the top when the
--- clipboard announces it.
function M.copy(key)
  local entry = entry_for(key)
  if not entry then return false end
  local bytes = fs.read(entry.file, M.LIMIT)
  if not bytes then
    morf.log("warn", "impasto: the payload of " .. key .. " is gone")
    return false
  end
  local ok, err = pcall(morf.clipboard.set, bytes, entry.kind == "image" and entry.mime or nil)
  if not ok then
    morf.log("warn", "impasto: could not copy: " .. tostring(err))
    return false
  end
  return true
end

function M.forget(key)
  local entry, index = entry_for(key)
  if not entry then return false end
  table.remove(entries, index)
  discard(entry)
  bump()
  save_soon()
  return true
end

--- The history and the live selection, both.
function M.wipe()
  for _, entry in ipairs(entries) do fs.remove(entry.file) end
  entries = {}
  bump()
  save_soon()
  pcall(morf.clipboard.set, "")
end

-- ---------------------------------------------------------------- search --

--- Newest first, no ranking; only the preview is matched.
function M.search(term, limit)
  local found = {}
  local wanted = tostring(term or ""):lower()
  for _, entry in ipairs(M.entries()) do
    if #found >= limit then break end
    if wanted == "" or entry.preview:lower():find(wanted, 1, true) then
      found[#found + 1] = entry
    end
  end
  return found
end

-- ------------------------------------------------------------- formatting --

function M.weigh(bytes)
  if bytes < 1024 then return bytes .. " B" end
  if bytes < 1024 * 1024 then return math.floor(bytes / 1024 + 0.5) .. " kB" end
  return ("%.1f MB"):format(bytes / (1024 * 1024))
end

--- "3 min ago" orders entries at a glance.
function M.since(copied)
  local seconds = math.max(0, math.floor((morf.time.now_ms() - copied) / 1000 + 0.5))
  if seconds < 60 then return "just now" end
  local minutes = seconds // 60
  if minutes < 60 then return minutes .. " min ago" end
  local hours = minutes // 60
  if hours < 24 then return hours .. " h ago" end
  return (hours // 24) .. " d ago"
end

function M.describe(entry)
  local kind = entry.kind == "image" and "Image" or "Text"
  return kind .. "  ·  " .. M.weigh(entry.bytes) .. "  ·  " .. M.since(entry.copied)
end

--- Images have no text preview; the row's thumbnail identifies them.
function M.title(entry)
  if entry.kind == "image" then return "Image" end
  return entry.preview ~= "" and entry.preview or "Not text"
end

-- ----------------------------------------------------------------- watch --

local started = false

local function image_type(offered)
  local images = {}
  for _, mime in ipairs(offered) do
    if mime:sub(1, 6) == "image/" then images[#images + 1] = mime end
  end
  if #images == 0 then return nil end
  for _, preferred in ipairs { "image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp", "image/tiff" } do
    for _, mime in ipairs(images) do
      if mime == preferred then return mime end
    end
  end
  return images[1]
end

--- Starts listening. Every copy is heard; what is kept follows the
--- settings at the time of the copy.
function M.start()
  if started then return end
  started = true
  load()
  bump()
  require("services.watch").file(M.index_path, reread)
  local ok, err = pcall(morf.clipboard.watch, function(offer)
    -- Only the primary runtime keeps copies; the others read them back.
    if not morf.primary() then return end
    -- nil: the clipboard was emptied.
    if not offer or not settings.clipboardHistory then return end
    local offered = offer.mime_types or {}
    for _, mime in ipairs(offered) do
      if mime:find(M.SECRET, 1, true) then return end
    end
    -- An image copied from a browser is also offered as HTML, so the types
    -- are looked at before choosing what to read.
    local image = image_type(offered)
    if image and not settings.clipboardImages then return end
    offer:read(image or "text", function(bytes, read_err)
      if not bytes then
        if read_err then morf.log("debug", "impasto: clipboard read: " .. tostring(read_err)) end
        return
      end
      if image then
        M.remember("image", image, bytes)
      else
        M.remember("text", "text/plain;charset=utf-8", bytes)
      end
    end)
  end)
  if not ok then morf.log("warn", "impasto: no clipboard history: " .. tostring(err)) end
end

-- `morf ipc call clipboard` is the clipboard key (shell.qml): the launcher
-- on its clipboard mode, or shut again when it is already there, on the
-- screen being worked on; `clipboard status` says how many entries are
-- kept and whether the compositor lets the shell hear copies;
-- `clipboard wipe` forgets them all;
-- `clipboard_add <text>` keeps a text as if it had been copied (a bench
-- without data control can still fill the history).
morf.ipc.clipboard = function(arg)
  if arg == nil or arg == "" or arg == "toggle" then
    if not require("services.live").here() then return "elsewhere" end
    return require("services.launcher").toggle_clipboard()
  end
  -- Every screen hears the verb; the primary one wipes, once.
  if arg == "wipe" and morf.primary() then M.wipe() end
  local ok, supported = pcall(morf.clipboard.supported)
  return ("%d kept, data control %s"):format(#entries, (ok and supported) and "on" or "off")
end

morf.ipc.clipboard_add = function(text)
  -- Kept once, by the primary runtime; the others read it back.
  if not morf.primary() then return nil end
  if not text or text == "" then return "nothing to add" end
  -- `png:<path>` keeps that picture as a copied image.
  local path = text:match("^png:(.+)$")
  if path then
    local bytes = fs.read(path, M.LIMIT)
    if not bytes then return "cannot read " .. path end
    local entry = M.remember("image", "image/png", bytes)
    return entry and entry.key or "not kept"
  end
  local entry = M.remember("text", "text/plain;charset=utf-8", text)
  return entry and entry.key or "not kept"
end

return M
