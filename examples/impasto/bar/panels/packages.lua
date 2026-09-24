-- The packages: three lists under one field -- updates, what is installed,
-- and what the repositories and the AUR offer -- switched on the right of
-- the field or with Tab. The field filters the first two and searches the
-- third. A status line at the bottom says where the list came from and
-- carries the actions on the whole of it.
--
-- Port of PackagesPanel.qml. Install, Remove and Update run in a terminal
-- (services/packages.lua), only on a click, and never on a dry run.
--
-- `morf ipc call packages` toggles it; `packages.view <id>`,
-- `packages.query <text>`, `packages.key up|down|tab|enter` and
-- `updates.sample` (a made-up pending list) are for a test bench.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local controls = require("components.controls")
local packages = require("services.packages")
local updates = require("services.updates")

local C = theme.color
local P = packages

local PAD = theme.panel_padding
local INNER_W = P.panel_width - 2 * PAD
local INNER_H = P.panel_height - 2 * PAD
local GAP = 10
local LIST_H = INNER_H - P.field_height - P.footer_height - 2 * (GAP + 1) - 2 * GAP
local ROW_GAP = 2
local ROWS = (LIST_H + ROW_GAP) // (P.row_height + ROW_GAP)

local KEY = { UP = 0xff52, DOWN = 0xff54, LEFT_TAB = 0xfe20, PAGE_UP = 0xff55, PAGE_DOWN = 0xff56 }

local VIEW_ICONS = { updates = "󰚰", installed = "󰏗", find = "󰍉" }

local selected = morf.signal("impasto.packages.selected", 1)
local first = morf.signal("impasto.packages.first", 1)
local count = morf.signal("impasto.packages.count", 0)

local rows_now = {}
morf.effect("impasto.packages.rows", function()
  -- Read only while the panel is open: reading the updates checks them.
  rows_now = island.state.open_panel() == "packages" and P.rows() or {}
  count:set(#rows_now)
  selected:set(1)
  first:set(1)
end)

local function row_at(index) count:get() return rows_now[index] end

local function select(index)
  local n = count:get()
  if n == 0 then selected:set(1) first:set(1) return end
  index = (index - 1) % n + 1
  selected:set(index)
  if index < first:get() then first:set(index) end
  if index > first:get() + ROWS - 1 then first:set(index - ROWS + 1) end
end
local function move(delta) select(selected:get() + delta) end

-- The terminal takes the keyboard, so the island closes.
local function act(fn)
  fn()
  island.close()
end

-- Enter installs from Find and nothing elsewhere: removing takes a click on
-- the row's own button, and updates go all at once from the status line.
local function activate_selected()
  local row = row_at(selected:get())
  if P.view() == "find" and P.installable(row) then act(function() P.install(row) end) end
end

-- Opening reads the database and watches the updates; closing lets them go
-- and clears the field once the island has shrunk.
local watching = false
local field
morf.effect("impasto.packages.lifecycle", function()
  local open = island.state.open_panel() == "packages"
  morf.timer(1, function()
    if open and not watching then
      watching = true
      updates.subscribe()
      P.load()
    elseif not open and watching then
      watching = false
      updates.release()
      P.set_query("")
      field = nil
    end
  end, false)
end)

-- ------------------------------------------------------------------- rows --

local function row(slot_row)
  local slot = slot_row.slot
  local index = function() return first:get() + slot - 1 end
  local entry = function() return row_at(index()) or {} end
  local is_selected = function() return selected:get() == index() end
  local hovered = kit.hover_signal("packages.row")
  local view = P.view
  local update = function() return view() == "updates" end
  local aur = function() return entry().source == "aur" end
  local installed = function() return entry().installed == true end
  local action_shown = function()
    if update() then return false end
    if installed() then return hovered:get() and view() ~= "find" end
    return is_selected() and P.installable(entry())
  end

  local name = kit.text {
    text = function() return entry().name or "" end,
    size = theme.size.regular, weight = 600, elide = "right",
    layout = { maximum_width = 380 },
  }
  local right_w = function()
    if update() then return 230 end
    return action_shown() and 96 or 110
  end
  local text_w = function() return INNER_W - 20 - 26 - 12 - 12 - right_w() end

  local pill = controls.pill {
    height = 26,
    text = function() return installed() and "Remove" or "Install" end,
    icon = function() return installed() and "󰆴" or "󰇚" end,
    active = function() return not installed() end,
    enabled = function() return not P.busy() end,
    on_click = function()
      local r = entry()
      act(function() if r.installed then P.remove(r) else P.install(r) end end)
    end,
  }

  return ui.Rect {
    width = INNER_W, height = P.row_height, radius = theme.radius_small,
    color = function() return is_selected() and C.islandSurfaceHover or "#00000000" end,
    behavior = { color = theme.behave("fast") },
    -- On movement, not on entry: a row appearing under a resting pointer
    -- would otherwise take the selection.
    ui.MouseArea {
      anchors = { fill = true },
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_position_changed = function() if not is_selected() then selected:set(index()) end end,
    },
    -- The row's kind in a disc: pending, installed or available.
    ui.Rect {
      x = 10, y = (P.row_height - 26) / 2, width = 26, height = 26, radius = 13,
      color = C.islandSurfaceHover,
      kit.glyph {
        anchors = { center_in = true }, size = 13,
        glyph = function()
          if update() then return "󰚰" end
          return (view() == "find" and installed()) and "󰄬" or "󰏗"
        end,
        color = function()
          if update() or aur() or (view() == "find" and installed()) then return C.accent() end
          return C.textMuted()
        end,
      },
    },
    ui.Column {
      x = 48, y = 0, height = P.row_height, justify = "center", gap = 1,
      ui.Row {
        gap = 8, align = "center",
        name,
        kit.text {
          visible = function() return not update() and (entry().version or "") ~= "" end,
          text = function() return entry().version or "" end,
          mono = true, size = theme.size.label, color = C.textMuted,
        },
        kit.text {
          visible = function() return aur() or (view() == "find" and (entry().source or "") ~= "") end,
          text = function() return aur() and "AUR" or (entry().source or "") end,
          size = theme.size.label, weight = 600, letter_spacing = 0.4,
          color = function() return aur() and C.accent() or C.textMuted() end,
        },
        kit.text {
          visible = function() return entry().out_of_date == true end,
          text = "out of date", size = theme.size.label, color = C.indicatorWarn,
        },
      },
      kit.text {
        visible = function() return (entry().description or "") ~= "" end,
        text = function() return entry().description or "" end,
        width = text_w, elide = "right", size = theme.size.small, color = C.textMuted,
      },
    },
    -- An update says what it was and what it will be.
    ui.Row {
      anchors = { right = true, right_margin = 10, vertical_center = true },
      gap = 6, align = "center",
      visible = update,
      kit.text { text = function() return entry().from or "" end, mono = true, size = theme.size.label, color = C.textMuted },
      kit.text { text = "→", size = theme.size.small, color = C.textMuted },
      kit.text { text = function() return entry().to or "" end, mono = true, size = theme.size.label },
    },
    -- Votes for the AUR, "Installed" in Find.
    kit.text {
      anchors = { right = true, right_margin = 10, vertical_center = true },
      visible = function() return not update() and not action_shown() end,
      text = function()
        local e = entry()
        if view() == "find" and installed() then return "Installed" end
        if aur() and e.votes then return e.votes .. " votes" end
        return ""
      end,
      size = theme.size.small,
      color = function() return (view() == "find" and installed()) and C.accent() or C.textMuted() end,
    },
    ui.Item {
      anchors = { right = true, right_margin = 10, vertical_center = true },
      width = function() return pill.layout_width or 90 end, height = 26,
      visible = action_shown,
      pill,
    },
  }
end

local slots = morf.list_model({})
morf.effect("impasto.packages.slots", function()
  local shown = math.max(0, math.min(ROWS, count:get() - first:get() + 1))
  local out = {}
  for slot = 1, shown do out[slot] = { slot = slot } end
  slots:replace(out, "slot")
end)

-- ------------------------------------------------------------------ texts --

local function empty_text()
  local view, term = P.view(), P.term()
  if view == "updates" then
    if updates.checking() and updates.count() == 0 then return "Checking…" end
    return term == "" and "Everything is up to date." or "No pending update is called that."
  end
  if view == "installed" then
    return not P.loaded() and "Reading the database…" or "Nothing installed is called that."
  end
  if term == "" then return "Type a name — the repositories and the AUR both answer." end
  if #term < 2 then return "Two letters at least." end
  if P.searching() or P.found().term ~= term then return "Searching…" end
  return "Nothing is called that."
end

local function status_text()
  local view = P.view()
  if view == "updates" then
    local aur = 0
    for _, row in ipairs(updates.updates()) do if row.source == "aur" then aur = aur + 1 end end
    local parts = { (updates.count() - aur) .. " from the repositories"
      .. (updates.tool() == "pacman" and " as of the last sync" or "") }
    if updates.helper ~= "" then
      parts[#parts + 1] = updates.aur() and (aur .. " from the AUR") or "the AUR did not answer"
    end
    if updates.checking() then parts[#parts + 1] = "checking…"
    elseif updates.age() ~= "" then parts[#parts + 1] = "checked " .. updates.age() end
    return table.concat(parts, " · ")
  end
  if view == "installed" then
    return P.term() ~= "" and (count:get() .. " of " .. #P.installed()) or ""
  end
  local f = P.found()
  if f.term == "" or f.note == "short" then
    return P.helper ~= "" and ("Installed with " .. P.helper .. ", in a terminal")
      or "No AUR helper — the repositories only"
  end
  local parts = { f.repos .. " in the repositories" }
  if f.note == "offline" then parts[#parts + 1] = "the AUR did not answer"
  elseif f.note == "prefix" then parts[#parts + 1] = "the first " .. f.aur .. " in the AUR that start with it"
  else parts[#parts + 1] = f.aur .. " in the AUR" end
  return table.concat(parts, " · ")
end

-- ------------------------------------------------------------------ panel --

local function build()
  local input
  input = ui.TextInput {
    anchors = { left = true, vertical_center = true, left_margin = 29 },
    width = INNER_W - 29 - 300, height = P.field_height,
    vertical_alignment = "center",
    text = P.query(),
    placeholder = function()
      local view = P.view()
      if view == "find" then return "Find a package…" end
      return view == "updates" and "Filter the updates…" or "Filter what is installed…"
    end,
    placeholder_color = C.textMuted,
    font_family = function() return theme.font() end, font_size = 16,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    focus = true,
    on_text_changed = function(text) P.set_query(text) end,
    on_accepted = activate_selected,
    on_escape = function() island.close() end,
    on_key_pressed = function(keysym)
      if keysym == KEY.UP then move(-1)
      elseif keysym == KEY.DOWN then move(1)
      elseif keysym == KEY.PAGE_UP then move(-ROWS)
      elseif keysym == KEY.PAGE_DOWN then move(ROWS)
      elseif keysym == KEY.LEFT_TAB then P.step(-1) end
    end,
    -- Tab is the engine's (it moves the keyboard on); the catcher below is
    -- where it lands, and losing the keyboard to it is a Tab.
    on_focus_changed = function(focused)
      if focused or field ~= input or island.state.open_panel() ~= "packages" then return end
      P.step(1)
      input.focus = true
    end,
  }
  field = input
  local catcher = ui.MouseArea {
    x = 0, y = 0, width = 1, height = 1,
    on_key_pressed = function() input.focus = true end,
  }

  local views = controls.segmented {
    anchors = { right = true, vertical_center = true },
    current = P.view,
    on_selected = function(id) P.set_view(id) input.focus = true end,
    options = {
      { id = "updates", label = function() local n = updates.count() return n > 0 and ("Updates · " .. n) or "Updates" end },
      { id = "installed", label = "Installed" },
      { id = "find", label = "Find" },
    },
  }

  local filter = controls.segmented {
    anchors = { left = true, vertical_center = true },
    visible = function() return P.view() == "installed" end,
    current = P.filter,
    on_selected = function(id) P.set_filter(id) input.focus = true end,
    options = {
      { id = "mine", label = function() return "By you · " .. P.mine_count() end },
      { id = "all", label = function() return "All · " .. #P.installed() end },
      { id = "aur", label = function() return "AUR · " .. P.aur_count() end },
    },
  }

  local check = controls.pill {
    height = 26, icon = "󰑐",
    text = function() return updates.checking() and "Checking…" or "Check" end,
    enabled = function() return not updates.checking() end,
    on_click = function() updates.refresh() end,
  }
  local everything = controls.pill {
    height = 26, icon = "󰚰", text = "Update everything", active = true,
    enabled = function() return updates.count() > 0 and not P.busy() end,
    on_click = function() if updates.count() > 0 then act(P.upgrade) end end,
  }

  return ui.Item {
    width = INNER_W, height = INNER_H,
    ui.Column {
      gap = GAP,
      ui.Item {
        width = INNER_W, height = P.field_height,
        kit.glyph {
          anchors = { left = true, vertical_center = true },
          glyph = function() return VIEW_ICONS[P.view()] or "󰍉" end,
          size = 17, color = C.accent, width = 20,
        },
        input, catcher, views,
      },
      ui.Rect { width = INNER_W, height = 1, color = C.islandBorder },
      ui.Item {
        width = INNER_W, height = LIST_H,
        kit.text {
          anchors = { center_in = true },
          visible = function() return count:get() == 0 end,
          text = empty_text, size = theme.size.regular, color = C.textMuted,
          width = INNER_W - 80, wrap = true, horizontal_alignment = "center",
        },
        ui.Repeater { as = "column", gap = ROW_GAP, model = slots, delegate = row },
        ui.MouseArea {
          anchors = { fill = true }, z = -1,
          on_wheel = function(_, _, _, _, _, steps_y)
            if not steps_y or steps_y == 0 then return end
            local most = math.max(1, count:get() - ROWS + 1)
            first:set(math.max(1, math.min(most, first:get() + steps_y)))
          end,
        },
      },
      ui.Rect { width = INNER_W, height = 1, color = C.islandBorder },
      ui.Item {
        width = INNER_W, height = P.footer_height,
        filter,
        kit.text {
          anchors = { vertical_center = true },
          x = function() return P.view() == "installed" and ((filter.layout_width or 0) + 10) or 0 end,
          width = function()
            local used = P.view() == "installed" and ((filter.layout_width or 0) + 10) or 0
            local pills = P.view() == "updates" and ((check.layout_width or 0) + (everything.layout_width or 0) + 20) or 0
            return math.max(10, INNER_W - used - pills)
          end,
          horizontal_alignment = function() return P.view() == "installed" and "right" or "left" end,
          text = status_text, size = theme.size.small, color = C.textMuted, elide = "right",
        },
        ui.Row {
          anchors = { right = true, vertical_center = true }, gap = 10,
          visible = function() return P.view() == "updates" end,
          check, everything,
        },
      },
    },
  }
end

island.register("packages", {
  size = function() return P.panel_width, P.panel_height end,
  build = build,
})

-- --------------------------------------------------------------------- IPC --

morf.ipc.packages = function()
  island.toggle("packages")
  return island.state.open_panel()
end
morf.ipc["packages.view"] = function(id)
  if id == "updates" or id == "installed" or id == "find" then P.set_view(id) end
  return P.view()
end
morf.ipc["packages.query"] = function(...)
  local text = table.concat({ ... }, " ")
  P.set_query(text)
  if field then pcall(function() field.text = text field.cursor_position = #text end) end
  return text
end
morf.ipc["packages.filter"] = function(id)
  if id == "mine" or id == "all" or id == "aur" then P.set_filter(id) end
  return P.filter()
end
morf.ipc["packages.key"] = function(name)
  if name == "up" then move(-1) elseif name == "down" then move(1)
  elseif name == "tab" then P.step(1) elseif name == "enter" then activate_selected() end
  return tostring(selected:get()) .. "/" .. tostring(count:get())
end
morf.ipc["updates.sample"] = function()
  updates.sample()
  return tostring(updates.count())
end
