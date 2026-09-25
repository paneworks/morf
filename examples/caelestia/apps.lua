-- The launcher's lists: installed applications, ranked by `morf.text.fuzzy`
-- and lifted by how often and how lately each was launched from here
-- (lib/frecency.lua); and, after the action prefix (">"), the shell's own
-- actions.

local morf = require("morf")
local frecency = require("lib.frecency")
local config = require("config")

local M = {}

local index
local rows = {}

--- Reads the desktop entries again; true when they changed.
function M.refresh()
  if not index then
    local ok, made = pcall(morf.desktop_entries)
    if not ok then
      morf.log("warn", "caelestia: cannot read the applications: " .. tostring(made))
      return false
    end
    index = made
  else
    local ok, changed = pcall(index.refresh, index)
    if not ok or not changed then return false end
  end
  local list, seen = {}, {}
  local ok, all = pcall(index.applications, index)
  if not ok or type(all) ~= "table" then return false end
  for _, entry in ipairs(all) do
    local name = (entry.name or ""):match("^%s*(.-)%s*$")
    if not entry.no_display and name ~= "" and not seen[entry.id] then
      seen[entry.id] = true
      local comment = (entry.comment or ""):match("^%s*(.-)%s*$")
      if comment == "" then comment = entry.generic_name or "" end
      list[#list + 1] = {
        kind = "app",
        id = entry.id,
        name = name,
        description = comment,
        icon = entry.icon or "",
        keywords = table.concat(entry.keywords or {}, " "),
      }
    end
  end
  table.sort(list, function(a, b) return a.name:lower() < b.name:lower() end)
  rows = list
  return true
end

M.refresh()

local used = frecency.open { path = morf.state_path("caelestia-launches.json") }

-- ---------------------------------------------------------------- actions --

-- The shell's own actions, as the reference lists them after ">".
-- `run(query)` returns what the launcher does next: "close", or a new query.
M.ACTIONS = {
  { id = "calc", name = "Calculator", description = "Do simple maths equations", icon = "calculate",
    run = function() return "=" end },
  { id = "scheme", name = "Scheme", description = "Change the current colour scheme", icon = "palette",
    run = function() return "> scheme " end },
  { id = "wallpaper", name = "Wallpaper", description = "Change the current wallpaper", icon = "image",
    run = function() return "> wallpaper " end },
  { id = "variant", name = "Variant", description = "Change the current scheme variant", icon = "format_paint",
    run = function() return "> variant " end },
  { id = "random", name = "Random", description = "Switch to a random wallpaper", icon = "casino",
    run = function()
      local dir = morf.fs.home() .. "/Pictures/Wallpapers"
      local ok, entries = pcall(morf.fs.list, dir)
      local pics = {}
      local PICTURE = { jpg = true, jpeg = true, png = true, webp = true }
      if ok and type(entries) == "table" then
        for _, e in ipairs(entries) do
          if e.is_file and PICTURE[(e.extension or ""):lower()] then pics[#pics + 1] = e.path end
        end
      end
      if #pics > 0 then require("wallpaper").set(pics[math.random(#pics)]) end
      return "close"
    end },
  { id = "light", name = "Light", description = "Change the scheme to light mode", icon = "light_mode",
    run = function() config.set("theme.mode", "light") return "close" end },
  { id = "dark", name = "Dark", description = "Change the scheme to dark mode", icon = "dark_mode",
    run = function() config.set("theme.mode", "dark") return "close" end },
}
for _, a in ipairs(M.ACTIONS) do a.kind = "action" end

-- ----------------------------------------------------------------- search --

--- What `query` finds: a list of rows (apps or actions), best first.
function M.search(query, limit)
  local prefix = config.get("launcher.action_prefix")
  if query:sub(1, #prefix) == prefix then
    local term = query:sub(#prefix + 1):match("^%s*(.-)%s*$")
    -- TODO(phase 2): the scheme, wallpaper and variant pickers that
    -- "> scheme " and friends open in the reference.
    local hits = morf.text.fuzzy(term, M.ACTIONS, { key = { "name", { "description", 0.3 } } })
    local out = {}
    for _, hit in ipairs(hits) do out[#out + 1] = hit.item end
    return out
  end
  local hits = used.rank(query, rows, {
    key = { "name", { "keywords", 0.5 }, { "description", 0.3 } },
    id = "id",
    limit = limit,
  })
  local out = {}
  for _, hit in ipairs(hits) do out[#out + 1] = hit.item end
  return out
end

--- Runs a row: launches an app (and remembers it), or an action. Returns
--- "close" or a new query.
function M.activate(row)
  if not row then return "keep" end
  if row.kind == "app" then
    used.record(row.id)
    local ok, err = pcall(index.launch, index, row.id)
    if not ok then morf.log("warn", "caelestia: could not launch " .. row.id .. ": " .. tostring(err)) end
    return "close"
  end
  return row.run() or "close"
end

-- ------------------------------------------------------------------ icons --

local found = {}

--- `{ name }` for a themed icon, `{ path }` for a file, or nil.
function M.icon(name)
  name = tostring(name or "")
  if name == "" then return nil end
  if found[name] ~= nil then return found[name] or nil end
  local hit
  if name:sub(1, 1) == "/" then
    hit = morf.fs.exists(name) and { path = name } or nil
  else
    local ok, has = pcall(morf.has_icon, name)
    if ok and has then hit = { name = name }
    else
      for _, ext in ipairs { ".svg", ".png" } do
        local path = "/usr/share/pixmaps/" .. name .. ext
        if morf.fs.exists(path) then hit = { path = path } break end
      end
    end
  end
  found[name] = hit or false
  return hit
end

return M
