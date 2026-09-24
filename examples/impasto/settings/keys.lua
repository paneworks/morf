-- Settings, Keys: the shell's own shortcuts and the compositor's, and
-- rebinding them (KeysSection and ShortcutRow).
--
-- Every row shows the combination the profile gives its bind (the
-- profile's `keys`, else what Hyprland reports). Clicking it opens the
-- editor under the row: press the new combination (Escape cancels), or
-- pick the modifiers as chips and press the key alone -- a combination
-- Hyprland has bound globally never reaches this window, a bare key does.
-- A combination already in use is allowed, since Hyprland fires both, but
-- the row says so before it is applied. Applying writes the profile's keys
-- and reloads Hyprland (services/shortcuts.lua), which reads them through
-- the one line the page shows. Mouse binds are shown, not edited. Without
-- Hyprland the rows are shown and the editor stays shut.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local shortcuts = require("services.shortcuts")
local tr = require("services.tr")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

local ESCAPE = 0xff1b

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
  return setting.text_box {
    field_width = W, field_height = 28, placeholder = tr("Find a key or an action"),
    value = filter:get(),
    on_edited = function(text) filter:set(text) end,
  }
end

--- A small chip: a modifier, lit when held.
local function chip(label, on, toggle)
  local hovered = controls.signal("keys.chip", false)
  return ui.Rect {
    width = 64, height = 28, radius = theme.radius_small,
    color = function()
      if on() then return C.accent() end
      return hovered:get() and C.islandSurfaceHover or C.island
    end,
    border_width = 1,
    border_color = function() return on() and C.accent() or C.islandBorder end,
    behavior = { color = fast() },
    kit.text { anchors = { center_in = true }, text = label, mono = true, size = theme.size.label,
      weight = function() return on() and 600 or 400 end,
      color = function() return on() and C.accentText() or C.textMuted() end },
    setting.hit { hovered = hovered, on_click = toggle },
  }
end

--- One bind: its name, and its combination, which is also the button that
--- opens the editor under it (ShortcutRow).
function M.row(W, description, label, filter, note, ruled)
  local editing = controls.signal("keys.editing", false)
  local capturing = controls.signal("keys.capturing", false)
  local mods = controls.signal("keys.mods", {})
  local key = controls.signal("keys.key", "")
  local hovered = controls.signal("keys.combo", false)
  local current = function() return shortcuts.current(description) end
  local fixed = function() return shortcuts.fixed(description) end
  local can = function() return shortcuts.can_rebind() and not fixed() end
  local draft = function()
    local parts = {}
    for _, m in ipairs(mods:get()) do parts[#parts + 1] = m end
    parts[#parts + 1] = key:get()
    return table.concat(parts, " + ")
  end
  local clash = function()
    if key:get() == "" then return "" end
    return shortcuts.clash(draft(), description)
  end

  -- The keys are caught by a text field drawn as nothing: the one kind of
  -- node that holds the keyboard until something takes it. A key that
  -- types (a bare letter) arrives as text, anything else (Super held,
  -- F5, the arrows) as a key.
  local capture
  local function cancel()
    capturing:set(false)
    editing:set(false)
    capture.focus = false
  end
  local function take(name, modifiers)
    -- A modifier on its way to the key: keep listening.
    if name == "" then return end
    key:set(name)
    local held = shortcuts.modifiers_of(modifiers)
    if #held > 0 then mods:set(held) end
    capturing:set(false)
    capture.focus = false
  end
  capture = ui.TextInput {
    anchors = { fill = true }, color = "#00000000", caret_color = "#00000000",
    selection_color = "#00000000", tab_navigation = false,
    on_key_pressed = function(keysym, _, modifiers)
      if not editing:get() then return false end
      if keysym == ESCAPE then cancel() return true end
      take(shortcuts.key_name(keysym), modifiers)
      return true
    end,
    on_text_changed = function(text)
      if text == "" then return end
      capture.text = ""
      if not editing:get() then return end
      take(shortcuts.key_name(utf8.codepoint(text, utf8.offset(text, -1))), "")
    end,
    on_accepted = function() if editing:get() then take("Return", "") end end,
    on_escape = cancel,
    on_focus_changed = function(on) if not on then capturing:set(false) end end,
  }
  local capture_hit = ui.MouseArea {
    anchors = { fill = true }, cursor = "pointer",
    on_clicked = function() capturing:set(true) capture.focus = true end,
  }

  local function begin()
    if not can() then return end
    local known = {}
    for _, m in ipairs(shortcuts.MODIFIERS) do known[m.name] = true end
    local seeded, bare = {}, ""
    for part in current():gmatch("[^+]+") do
      local word = part:match("^%s*(.-)%s*$")
      if known[word:upper()] then seeded[#seeded + 1] = word:upper() elseif bare == "" then bare = word end
    end
    mods:set(seeded)
    key:set(bare)
    editing:set(true)
    capturing:set(true)
    -- After the click that opened it, which moves the focus itself.
    morf.timer(1, function() capture.focus = true end, false)
  end

  local function toggle(name)
    local has = false
    for _, m in ipairs(mods:get()) do if m == name then has = true end end
    -- Kept in the compositor's order, so a combination is always spelled
    -- the same way for the clash check.
    local out = {}
    for _, m in ipairs(shortcuts.MODIFIERS) do
      local held = false
      for _, each in ipairs(mods:get()) do if each == m.name then held = true end end
      if (m.name == name and not has) or (m.name ~= name and held) then out[#out + 1] = m.name end
    end
    mods:set(out)
  end

  local shown = kit.text {
    anchors = { center_in = true }, mono = true, size = theme.size.label,
    text = function() local c = current() return c ~= "" and c or tr("unbound") end,
    color = function() return current() ~= "" and C.text() or C.textMuted() end,
  }
  local combination = ui.Rect {
    width = function() return math.max(104, (shown.layout_width or 0) + 18) end, height = 22,
    radius = theme.radius_small - 2,
    color = function() return (editing:get() or (hovered:get() and can())) and C.islandSurfaceHover or C.island end,
    border_width = 1,
    border_color = function() return (editing:get() or (hovered:get() and can())) and C.accent() or C.islandBorder end,
    behavior = { color = fast(), border_color = fast() },
    shown,
    setting.hit { hovered = hovered, enabled = can, on_click = function()
      if editing:get() then editing:set(false) else begin() end
    end },
  }

  local chips = { gap = 7, align = "center" }
  for _, m in ipairs(shortcuts.MODIFIERS) do
    chips[#chips + 1] = chip(m.name, function()
      for _, each in ipairs(mods:get()) do if each == m.name then return true end end
      return false
    end, function() toggle(m.name) end)
  end
  chips[#chips + 1] = ui.Rect {
    width = W - 28 - 4 * (64 + 7), height = 28, radius = theme.radius_small,
    color = function() return capturing:get() and C.islandSurfaceHover or C.island end,
    border_width = 1,
    border_color = function() return capturing:get() and C.accent() or C.islandBorder end,
    behavior = { border_color = fast() },
    kit.text { anchors = { center_in = true }, size = theme.size.label,
      text = function()
        if capturing:get() then return tr("Press the keys…") end
        return key:get() ~= "" and key:get() or tr("Click, then press a key")
      end,
      color = function()
        if capturing:get() then return C.accent() end
        return key:get() ~= "" and C.text() or C.textMuted()
      end },
    capture,
    capture_hit,
  }

  local message = kit.text {
    width = W - 28 - 84 - 92 - 20, wrap = true, size = theme.size.label,
    text = function()
      if key:get() == "" then return tr("Pick the key, and any modifiers to hold with it. Escape cancels.") end
      local other = clash()
      if other ~= "" then return draft() .. " " .. tr("is already") .. " " .. other .. ". " .. tr("Both would fire.") end
      return draft()
    end,
    color = function() return clash() ~= "" and C.yellow() or C.textMuted() end,
  }
  local editor = ui.Flex {
    direction = "column", gap = 8, align = "start", width = W - 28,
    ui.Row(chips),
    ui.Row { gap = 10, align = "center",
      message,
      controls.pill { text = tr("Cancel"), width = 84, height = 28,
        on_click = function() editing:set(false) capturing:set(false) end },
      controls.pill { text = tr("Apply"), icon = "󰄬", width = 92, height = 28, active = true,
        enabled = function() return key:get() ~= "" and draft() ~= current() end,
        on_click = function()
          if key:get() == "" or draft() == current() then return end
          shortcuts.rebind(description, draft())
          editing:set(false)
          capturing:set(false)
        end } },
  }
  local head = ui.Item {
    width = W, height = 48,
    setting.wheel_area(),
    -- Rows a Repeater makes get no rule from the group; they draw their own.
    ui.Rect { x = 0, y = 0, width = W, height = 1, color = C.islandBorder, visible = ruled == true },
    ui.Item { x = 14, anchors = { vertical_center = true }, width = W - 28 - 120, height = 34,
      ui.Flex { direction = "column", gap = 1, align = "start", anchors = { vertical_center = true },
        kit.text { text = label, size = theme.size.small, weight = 500, elide = "right", width = W - 28 - 130 },
        kit.text { text = note or "", size = theme.size.label, color = C.textMuted, elide = "right",
          width = W - 28 - 130, visible = (note or "") ~= "" } } },
    ui.Item { anchors = { right = true, right_margin = 14, vertical_center = true },
      width = function() return combination.layout_width or 0 end, height = 22, combination },
  }
  return ui.Flex {
    direction = "column", gap = 0, align = "start", width = W,
    visible = function() return matches(filter:get(), label, current(), note) end,
    head,
    ui.Item { x = 14, width = W - 28, visible = function() return editing:get() end,
      height = function() return (editor.layout_height or 0) + 14 end, editor },
  }
end

--- What reaching the compositor needs, said once above the rows.
local function status_group(W)
  local away = function() return not shortcuts.can_rebind() end
  return setting.group {
    width = W,
    visible = function() return away() or not shortcuts.sourced() end,
    setting.block { width = W, gap = 6,
      kit.text { size = theme.size.small, weight = 500,
        text = function() return away() and tr("Not available here") or tr("One line in Hyprland's configuration") end },
      kit.text { width = W - 28, wrap = true, size = theme.size.label, color = C.textMuted,
        text = function()
          if away() then
            local status = shortcuts.status()
            if status ~= "" and status ~= "ok" and status ~= "reading" then return status end
            return "Keys are Hyprland's binds, and this compositor is not Hyprland; they are shown, not changed."
          end
          return "A new combination is written to " .. shortcuts.tsv_path .. " and Hyprland is reloaded. "
            .. "It takes effect once Hyprland's configuration reads that file, with this line "
            .. (shortcuts.flavour() == "hyprlang" and "at the end of hyprland.conf:" or "near the top of hyprland.lua, before any bind:")
        end },
      kit.text { width = W - 28, wrap = true, mono = true, size = theme.size.label, color = C.text,
        visible = function() return not away() end,
        text = function()
          return shortcuts.flavour() == "hyprlang" and shortcuts.source_line.hyprlang or shortcuts.source_line.lua
        end },
    },
  }
end

function M.build(page)
  local filter = controls.signal("keys.filter", "")
  shortcuts.load()

  local own_part = function(W)
    local group = { width = W }
    for _, entry in ipairs(shortcuts.catalogue) do
      local note = morf.ipc[entry.name] ~= nil and ("morf ipc call " .. entry.name) or ""
      group[#group + 1] = M.row(W, entry.description, tr(entry.label), filter, note)
    end
    return {
      setting.heading { width = W, title = tr("The shell's own"),
        note = tr("Click a combination to change it. Every key here belongs to the profile in use."),
        hint = tr("Each profile has its own complete set of keys. A combination already in use is allowed, since Hyprland fires both binds, but the row warns you before you apply it.") },
      search(W, filter),
      status_group(W),
      setting.group(group),
    }
  end

  local compositor_part = function(W)
    local rows = morf.list_model({})
    local function refresh()
      local out = {}
      for _, row in ipairs(shortcuts.table()) do
        if not shortcuts.mine(row.description) then
          local category, action = row.description:match("^(.-) · (.+)$")
          out[#out + 1] = { key = row.description, category = category or "Other", action = action or row.description }
        end
      end
      rows:replace(out, "key")
    end
    local seen = nil
    return {
      setting.heading { width = W, title = tr("The compositor's"),
        note = "Windows, workspaces, the media keys — every bind Hyprland reports, by its description.",
        hint = "A bind keeps its action; only its keys move. Lua binds are identified by their description, so a bind without one cannot be changed from here." },
      search(W, filter),
      status_group(W),
      setting.watch(function()
        local now = #shortcuts.table()
        if now ~= seen then seen = now morf.timer(1, refresh, false) end
      end),
      setting.group { width = W,
        ui.Repeater {
          as = "flex", direction = "column", align = "start", width = W, gap = 0,
          model = rows,
          delegate = function(row)
            return M.row(W, row.key, row.action, filter, row.category, true)
          end,
        },
      },
    }
  end
  return setting.parts(page, {
    { id = "shell", build = own_part },
    { id = "compositor", build = compositor_part },
  })
end

return M
