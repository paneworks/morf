-- Files on the web, as files on disk: a URL fetched once into the cache,
-- and its local path from then on.
--
--   local remote = require("lib.remote")
--   ui.Image { source = function() return remote.file(player().art_url) end }
--
-- `remote.file(url)` answers at once: the cached path when the file is
-- there, "" while it is being fetched (the fetch starts on the first ask,
-- off the main loop, through morf.http) -- and a binding that asked runs
-- again when it lands. A path or a `file://` URI is handed back as it is,
-- so a caller need not care which kind a player gave. A fetch that failed
-- is not tried again until `remote.forget(url)`.
--
-- Pictures come without an extension more often than not (a cover's URL is
-- a hash); the file is named for what its first bytes say it is, so the
-- decoder knows what it holds.

local morf = require("morf")

local remote = {}

remote.MAX_BYTES = 16 * 1024 * 1024
remote.TIMEOUT_MS = 15000

local known = {}   -- url -> local path, once fetched
local failed = {}  -- url -> why
local fetching = {}
local revision = morf.signal("lib.remote.revision", 0)

-- FNV-1a, 32 bits, as hex: a short stable name for a URL.
local function hash(text)
  local h = 2166136261
  for i = 1, #text do
    h = h ~ text:byte(i)
    h = (h * 16777619) & 0xffffffff
  end
  return ("%08x"):format(h)
end

-- What the first bytes say the file is.
local function extension(bytes, url)
  if bytes:sub(1, 3) == "\xff\xd8\xff" then return "jpg" end
  if bytes:sub(1, 8) == "\x89PNG\r\n\x1a\n" then return "png" end
  if bytes:sub(1, 4) == "RIFF" and bytes:sub(9, 12) == "WEBP" then return "webp" end
  if bytes:sub(1, 6) == "GIF87a" or bytes:sub(1, 6) == "GIF89a" then return "gif" end
  if bytes:find("<svg", 1, true) then return "svg" end
  return (url:match("%.(%w+)$") or "bin"):lower()
end

local function folder()
  local ok, core = pcall(require, "morf.core")
  if ok and core.cache_path then return core.cache_path("remote") end
  local base = (morf.env and morf.env("XDG_CACHE_HOME")) or (morf.fs.home() .. "/.cache")
  return base .. "/morf/remote"
end

local function fetch(url)
  fetching[url] = true
  morf.http.get(url, { timeout_ms = remote.TIMEOUT_MS, max_output = remote.MAX_BYTES }, function(response)
    fetching[url] = nil
    if not (response and response.ok and type(response.body) == "string" and #response.body > 0) then
      failed[url] = (response and (response.error or ("status " .. tostring(response.status)))) or "no answer"
      return
    end
    local dir = folder()
    pcall(morf.fs.mkdir, dir)
    local path = dir .. "/" .. hash(url) .. "." .. extension(response.body, url)
    local ok, err = morf.fs.write(path, response.body)
    if not ok then
      failed[url] = tostring(err)
      return
    end
    known[url] = path
    revision:set(revision:get() + 1)
  end)
end

--- The local path for `url`: at once when it is cached or not a web
--- address, "" while it is fetched (and nil, why when it could not be).
function remote.file(url)
  if type(url) ~= "string" or url == "" then return "" end
  if not (url:find("^https?://")) then return url end
  revision:get()
  if known[url] then return known[url] end
  if failed[url] then return "", failed[url] end
  -- Fetched in an earlier run: the name is the URL's, whatever the kind.
  local dir = folder()
  for _, ext in ipairs { "jpg", "png", "webp", "gif", "svg", "bin" } do
    local path = dir .. "/" .. hash(url) .. "." .. ext
    if morf.fs.exists(path) then
      known[url] = path
      return path
    end
  end
  if not fetching[url] then fetch(url) end
  return ""
end

--- Forgets `url`, so the next ask fetches it again.
function remote.forget(url)
  known[url], failed[url] = nil, nil
end

return remote
