-- Settings, Keys: the shell's own shortcuts and the compositor's, on one
-- sheet (KeysSection and ShortcutRow).
--
-- The original bound the shell's keys through Hyprland and rewrote a file
-- keybinds.lua read when one was changed. This port never writes the
-- compositor's configuration, so the sheet is read-only: the shell's rows
-- come from `services/shortcuts.lua` when a port of that service is
-- present (its `catalogue` and `current(description)`), else from the
-- original's defaults, each with the IPC verb a compositor bind would call;
-- the compositor's rows are Hyprland's own list (`j/binds`), when there is
-- a Hyprland to ask.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local tr = require("services.tr")

local C = theme.color

local M = {}

-- The shell's own binds with impasto's default combinations, and the verb
-- a compositor bind runs (`morf ipc call <verb>`).
M.defaults = {
  { verb = "launcher", label = tr("Launcher"), keys = "SUPER + Space" },
  { verb = "controls", label = tr("Control centre"), keys = "SUPER + A" },
  { verb = "overview", label = tr("Workspace overview"), keys = "SUPER + TAB" },
  { verb = "settings", label = tr("Settings"), keys = "SUPER + comma" },
  { verb = "appearance", label = tr("Appearance"), keys = "SUPER + T" },
  { verb = "palette", label = tr("Palette"), keys = "SUPER + SHIFT + T" },
  { verb = "stats", label = tr("System statistics"), keys = "SUPER + U" },
  { verb = "session", label = tr("Session menu"), keys = "SUPER + X" },
  { verb = "lock", label = tr("Lock the screen"), keys = "SUPER + L" },
  { verb = "pet", label = tr("Pet"), keys = "SUPER + SHIFT + P" },
  { verb = "games", label = tr("Games"), keys = "SUPER + G" },
  { verb = "notes", label = tr("Notes"), keys = "SUPER + S" },
  { verb = "board", label = tr("Task board"), keys = "SUPER + K" },
  { verb = "keys", label = tr("Keys"), keys = "SUPER + H" },
  { verb = "packages", label = tr("Packages"), keys = "SUPER + I" },
  { verb = "clipboard", label = tr("Clipboard history"), keys = "SUPER + V" },
  { verb = "capture", label = tr("Capture"), keys = "SUPER + SHIFT + S" },
  { verb = "record", label = tr("Record the screen"), keys = "SUPER + SHIFT + R" },
}

--- The shell's rows: `{ label, keys, note }`.
function M.own()
  local ok, service = pcall(require, "services.shortcuts")
  if ok and type(service) == "table" and type(service.catalogue) == "table" then
    local out = {}
    for _, entry in ipairs(service.catalogue) do
      local keys = type(service.current) == "function" and service.current(entry.description) or ""
      out[#out + 1] = { label = entry.label, keys = keys, note = entry.description or "" }
    end
    return out
  end
  local out = {}
  for _, entry in ipairs(M.defaults) do
    out[#out + 1] = { label = entry.label, keys = entry.keys, note = "morf ipc call " .. entry.verb }
  end
  return out
end

--- A combination drawn as key caps: "SUPER + SHIFT + T".
function M.caps(keys)
  local row = { gap = 4, align = "center" }
  if keys == nil or keys == "" then
    row[1] = kit.text { text = "Unbound", size = theme.size.label, color = C.textMuted }
    return ui.Row(row)
  end
  for part in tostring(keys):gmatch("[^+]+") do
    local word = part:match("^%s*(.-)%s*$")
    local label = kit.text { anchors = { center_in = true }, text = word, mono = true,
      size = theme.size.label, weight = 600 }
    row[#row + 1] = ui.Rect {
      width = function() return math.max(22, (label.layout_width or 0) + 14) end, height = 22,
      radius = 6, color = C.island, border_width = 1, border_color = C.islandBorder,
      label,
    }
  end
  return ui.Row(row)
end

local function matches(term, ...)
  term = (term or ""):match("^%s*(.-)%s*$"):lower()
  if term == "" then return true end
  for _, text in ipairs { ... } do
    if tostring(text or ""):lower():find(term, 1, true) then return true end
  end
  return false
end

--- The search field both parts share, quieter than the sidebar's.
local function search(W, filter)
  local box = setting.text_box {
    field_width = W, field_height = 28, placeholder = tr("Find a key or an action"),
    value = filter:get(),
    on_edited = function(text) filter:set(text) end,
  }
  return box
end

-- Hyprland's own binds, asked for once per page.
local function compositor_binds(on_done)
  local ok, hyprland = pcall(require, "lib.hyprland")
  if not ok or not hyprland.available() then on_done(nil, "Hyprland is not running here") return end
  hyprland.binds(function(value, err)
    if type(value) ~= "table" then on_done(nil, err or "no answer") return end
    on_done(value)
  end)
end

local MODMASK = { { 64, "SUPER" }, { 8, "ALT" }, { 4, "CTRL" }, { 1, "SHIFT" } }
local function combination(bind)
  local parts = {}
  local mask = tonumber(bind.modmask) or 0
  for _, pair in ipairs(MODMASK) do
    if math.floor(mask / pair[1]) % 2 == 1 then parts[#parts + 1] = pair[2] end
  end
  parts[#parts + 1] = bind.key ~= "" and bind.key or ("code:" .. tostring(bind.keycode or ""))
  return table.concat(parts, " + ")
end

function M.build(page)
  local filter = controls.signal("keys.filter", "")
  local own_part = function(W)
    local group = { width = W }
    for _, entry in ipairs(M.own()) do
      group[#group + 1] = setting.row {
        width = W, label = entry.label, reading = entry.note,
        visible = function() return matches(filter:get(), entry.label, entry.keys, entry.note) end,
        control = M.caps(entry.keys),
      }
    end
    return {
      setting.heading { width = W, title = tr("The shell's own"),
        note = "Every key the shell answers, with the verb a compositor bind calls.",
        hint = "Changing a key is changing the compositor's bind; this shell never writes that file, so the sheet is read-only." },
      search(W, filter),
      setting.group(group),
    }
  end
  local compositor_part = function(W)
    local rows = morf.list_model({})
    local status = controls.signal("keys.status", "Asking Hyprland…")
    compositor_binds(function(binds, err)
      if not binds then status:set(err or "") return end
      local out = {}
      for index, bind in ipairs(binds) do
        local description = tostring(bind.description or "")
        local action = description ~= "" and description
          or (tostring(bind.dispatcher or "") .. " " .. tostring(bind.arg or ""))
        out[#out + 1] = { key = tostring(index), action = action, keys = combination(bind) }
      end
      status:set(#out == 0 and "No binds" or "")
      rows:replace(out, "key")
    end)
    return {
      setting.heading { width = W, title = tr("The compositor's"),
        note = "Windows, workspaces, the media keys — as Hyprland reports them.",
        hint = "Read from Hyprland's own list; edit them in its configuration." },
      search(W, filter),
      setting.row { width = W, label = function() return status:get() end,
        visible = function() return status:get() ~= "" end },
      ui.Repeater {
        as = "flex", direction = "column", align = "start", width = W, gap = 0,
        model = rows,
        delegate = function(row)
          return setting.row {
            width = W, label = row.action,
            visible = function() return matches(filter:get(), row.action, row.keys) end,
            control = M.caps(row.keys),
          }
        end,
      },
    }
  end
  return setting.parts(page, {
    { id = "shell", build = own_part },
    { id = "compositor", build = compositor_part },
  })
end

return M
