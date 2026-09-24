-- Settings: the sections on the left, one page at a time on the right.
--
-- Port of SettingsPanel.qml. Settings are filed by what they change, not by
-- how they are applied. The sidebar is grouped into three categories and a
-- long section is split into parts (a tab strip): two levels of
-- navigation, no more. Where an option is a shape, the control is a live
-- miniature built from the real components, so it follows the palette.
--
-- Navigation is a trail rather than a single id, so a page that links to
-- another can be left with the back button. The page is built when its
-- section is shown and let go when another is, so a page reads the state it
-- needs when it is made.

local ui = require("morf.ui")
local theme = require("theme")
local kit = require("components.kit")
local controls = require("components.controls")
local setting = require("components.setting")
local tab_strip = require("components.tab_strip")
local hero = require("components.settings_hero")
local palette_board = require("components.palette_board")
local tr = require("services.tr")

local C = theme.color
local fast = function() return theme.behave("fast") end

local M = {}

M.WIDTH, M.HEIGHT = 980, 680
M.MARGIN, M.GAP, M.SIDEBAR = 14, 12, 206
M.PAGE_X = M.MARGIN + M.SIDEBAR + M.GAP
M.PAGE_W = M.WIDTH - M.PAGE_X - M.MARGIN
M.HEADER = 28
M.VIEW_Y = M.MARGIN + M.HEADER + M.GAP
M.VIEW_H = M.HEIGHT - M.VIEW_Y - M.MARGIN

M.categories = {
  { id = "shell", label = "THE SHELL" },
  { id = "desk", label = "THE DESK" },
  { id = "session", label = "THE SESSION" },
}

-- One row per page. `keywords` are search terms beyond the label; `module`
-- is the file under settings/ that builds the page.
M.sections = {
  { id = "bar", category = "shell", icon = "󰧨", label = "Bar & Island",
    blurb = "The top bar, the island and notifications.",
    tabs = { { id = "island", label = "The island" }, { id = "modules", label = "The bar" },
      { id = "workspaces", label = "Workspaces" }, { id = "notifications", label = "Notifications" } },
    keywords = "bar island notch height margin width full span workspaces unified layout island bar band notification toast do not disturb silence timeout clock time date seconds format beside running modules layout left right split drag catalogue chip icon ring figure hover shape",
    module = "settings.bar" },
  { id = "widgets", category = "shell", icon = "󰕮", label = "Desktop",
    blurb = "What sits on the wallpaper, under the windows.",
    tabs = { { id = "modules", label = "Module settings" }, { id = "widgets", label = "The widgets" } },
    keywords = "widgets desktop wallpaper widget place drag size shape capsule bare outline accent style palette ink arrange clock calendar notes note sticky theme analogue modern opacity weather location github username handwriting edges deck pet creature plush paper pixel spectrum",
    module = "settings.widgets" },
  { id = "controls", category = "shell", icon = "󰕰", label = "Control Centre",
    blurb = "What the island opens onto when you click it.", tabs = {},
    keywords = "controls control centre center panel doors buttons row pet games notes board tasks stats blocks grid toggles tiles arrange edit",
    module = "settings.controls" },
  { id = "dock", category = "shell", icon = "󱂩", label = "Dock",
    blurb = "The dock and the applications kept on it.", tabs = {},
    keywords = "dock apps applications launcher pinned kept favourite favorite running edge bottom left right align icon size autohide taskbar every screen",
    module = "settings.dock" },
  { id = "launcher", category = "shell", icon = "󰍉", label = "Launcher",
    blurb = "Search, sigils and the clipboard history.",
    tabs = { { id = "results", label = "Results" }, { id = "sigils", label = "Sigils" },
      { id = "clipboard", label = "Clipboard" } },
    keywords = "launcher search apps results calculate run window timer prefix sigil order recent frequency favourite pinned kept clipboard history copy paste images wipe lock",
    module = "settings.launcher" },
  { id = "appearance", category = "desk", icon = "󰏘", label = "Appearance",
    blurb = "Palette, windows, fonts and animations.",
    tabs = { { id = "theme", label = "Theme" }, { id = "windows", label = "Windows" },
      { id = "type", label = "Type" }, { id = "motion", label = "Motion" } },
    keywords = "appearance theme colour color palette wallpaper transition fade wipe wave circle random greeting shadow glass font family sans mono nerd typeface animation speed curve easing preset motion",
    module = "settings.appearance" },
  { id = "monitors", category = "desk", icon = "󰍹", label = "Displays",
    blurb = "Screen layout, modes and the laptop lid.",
    tabs = { { id = "arrangement", label = "Arrangement" }, { id = "screen", label = "The screen" },
      { id = "lid", label = "The lid" }, { id = "night", label = "Night light" } },
    keywords = "displays monitor screen resolution refresh scale rotate transform vrr mirror extend primary night light warm temperature lid clamshell laptop",
    module = "settings.monitors" },
  { id = "input", category = "desk", icon = "󰍽", label = "Input",
    blurb = "The keyboard, the pointer and the cursor.",
    tabs = { { id = "keyboard", label = "Keyboard" }, { id = "pointer", label = "Pointer" } },
    keywords = "input keyboard layout repeat mouse sensitivity pointer cursor size colour shake find",
    module = "settings.input" },
  { id = "keys", category = "desk", icon = "󰌌", label = "Keys",
    blurb = "Every keybinding, the shell's and Hyprland's.",
    tabs = { { id = "shell", label = "The shell's own" }, { id = "compositor", label = "The compositor's" } },
    keywords = "keys shortcut binding hotkey super ipc verb",
    module = "settings.keys" },
  { id = "session", category = "session", icon = "󰌾", label = "Session",
    blurb = "Your account, the lock screen and idle behaviour.",
    tabs = { { id = "lock", label = "Lock screen" }, { id = "idle", label = "When you leave" } },
    keywords = "session lock blur password suspend idle timeout sleep screen off away avatar picture name account clock stacked inline",
    module = "settings.session" },
  { id = "system", category = "session", icon = "󰍛", label = "System",
    blurb = "Profiles, this machine and reset.",
    tabs = { { id = "profiles", label = "Profiles" }, { id = "machine", label = "This machine" } },
    keywords = "system about version reset defaults profile profiles switch rename duplicate delete import export backup file json night light",
    module = "settings.system" },
}

local by_id = {}
for _, entry in ipairs(M.sections) do by_id[entry.id] = entry end
function M.section_of(id) return by_id[id] end

-- Where a setting is changed, by its key: the section and the part.
local KEY_PAGES = {
  { "^bar", "bar", "island" }, { "^island", "bar", "island" }, { "^clock", "bar", "island" },
  { "^chip", "bar", "modules" }, { "^workspace", "bar", "workspaces" },
  { "^notification", "bar", "notifications" }, { "^doNotDisturb", "bar", "notifications" },
  { "^desktop", "widgets", "widgets" }, { "^weather", "widgets", "modules" },
  { "^github", "widgets", "modules" }, { "^pet", "widgets", "modules" },
  { "^notes", "widgets", "modules" }, { "^deck", "widgets", "modules" },
  { "^spectrum", "widgets", "modules" }, { "^centre", "controls", "" },
  { "^dock", "dock", "" }, { "^launcher", "launcher", "results" },
  { "^clipboard", "launcher", "clipboard" }, { "^theme", "appearance", "theme" },
  { "^wallpaper", "appearance", "theme" }, { "^greeting", "appearance", "theme" },
  { "^window", "appearance", "windows" }, { "^font", "appearance", "type" },
  { "^motion", "appearance", "motion" }, { "^animation", "appearance", "motion" },
  { "^lid", "monitors", "lid" }, { "^displays", "monitors", "arrangement" },
  { "^keys$", "keys", "shell" }, { "^night", "monitors", "night" },
  { "^keyboard", "input", "keyboard" }, { "^keyRepeat", "input", "keyboard" },
  { "^pointer", "input", "pointer" }, { "^cursor", "input", "pointer" },
  { "^shake", "input", "pointer" }, { "^lock", "session", "lock" }, { "^user", "session", "lock" },
  { "^idle", "session", "idle" }, { "^language", "system", "machine" },
}

function M.section_for_key(key)
  for _, rule in ipairs(KEY_PAGES) do
    if tostring(key):match(rule[1]) then return rule[2], rule[3] end
  end
  return "system", "profiles"
end

-- ------------------------------------------------------------ navigation --

-- The trail and where on it the window is. Kept across openings of the
-- window, as the original's panel kept its state while hidden.
local trail = { { section = "bar", tab = "island" } }
local visited = {}
local s = {
  cursor = morf.signal("impasto.settings.cursor", 1),
  length = morf.signal("impasto.settings.length", 1),
  section = morf.signal("impasto.settings.section", "bar"),
  tab = morf.signal("impasto.settings.tab", "island"),
  filter = morf.signal("impasto.settings.filter", ""),
  scroll = morf.signal("impasto.settings.scroll", 0),
  -- True for a moment after the language changes: the page shown lets go
  -- and is built again, in the new language.
  relabel = morf.signal("impasto.settings.relabel", false),
}
M.state = s

do
  local seen = tr.language()
  morf.effect("impasto.settings.language", function()
    local now = tr.language()
    if now == seen then return end
    seen = now
    morf.timer(1, function()
      -- A page's module translates some of its text while it loads (its
      -- option lists); loaded again, it does so in the new language.
      for name in pairs(package.loaded) do
        if type(name) == "string" and name:match("^settings%.")
            and name ~= "settings.panel" and name ~= "settings.window" and name ~= "settings.appearance" then
          package.loaded[name] = nil
        end
      end
      s.relabel:set(true)
      morf.timer(1, function() s.relabel:set(false) end, false)
    end, false)
  end)
end

local function first_tab(id)
  local entry = by_id[id]
  return entry and entry.tabs[1] and entry.tabs[1].id or ""
end

local function sync()
  local here = trail[s.cursor:get()]
  s.section:set(here.section)
  s.tab:set(here.tab)
  s.length:set(#trail)
  s.scroll:set(0)
end

function M.section() return s.section:get() end
function M.tab() return s.tab:get() end

--- Opens a section, on `wanted` part or the one it was last left on.
-- A page by its id, its label ("Displays" is `monitors`), or a setting key
-- it holds; the part the key names comes with it.
local function resolve(name)
  if by_id[name] then return name end
  local lower = tostring(name or ""):lower()
  for _, entry in ipairs(M.sections) do
    if entry.module and (entry.label or ""):lower() == lower then return entry.id end
  end
  for _, rule in ipairs(KEY_PAGES) do
    if tostring(name):match(rule[1]) then return rule[2], rule[3] end
  end
end

function M.go(name, wanted)
  local id, part = resolve(name)
  if not id then return end
  if (not wanted or wanted == "") and part and part ~= "" then wanted = part end
  local part = (wanted and wanted ~= "") and wanted or visited[id] or first_tab(id)
  if id == s.section:get() and part == s.tab:get() then return end
  local cursor = s.cursor:get()
  for i = #trail, cursor + 1, -1 do trail[i] = nil end
  trail[#trail + 1] = { section = id, tab = part }
  s.cursor:set(#trail)
  sync()
end

--- Shows another part of the section that is open.
function M.show(part)
  if part == s.tab:get() then return end
  visited[s.section:get()] = part
  trail[s.cursor:get()] = { section = s.section:get(), tab = part }
  sync()
end

function M.back()
  if s.cursor:get() > 1 then s.cursor:set(s.cursor:get() - 1) sync() end
end

function M.forward()
  if s.cursor:get() < #trail then s.cursor:set(s.cursor:get() + 1) sync() end
end

--- Whether a section matches the search.
function M.matches(entry, term)
  term = (term or s.filter:get()):match("^%s*(.-)%s*$"):lower()
  if term == "" then return true end
  -- In English and in the language shown.
  local function has(text)
    return text:lower():find(term, 1, true) or tr(text):lower():find(term, 1, true)
  end
  if has(entry.label) or entry.keywords:find(term, 1, true) or has(entry.blurb) then
    return true
  end
  for _, part in ipairs(entry.tabs) do
    if has(part.label) then return true end
  end
  return false
end

function M.shown()
  local out = {}
  for _, entry in ipairs(M.sections) do if M.matches(entry) then out[#out + 1] = entry end end
  return out
end

-- --------------------------------------------------------------- sidebar --

local function sidebar_entry(entry)
  local hovered = controls.signal("settings.entry", false)
  local active = function() return s.section:get() == entry.id end
  return ui.Rect {
    width = M.SIDEBAR - 16, height = 30, radius = theme.radius_small,
    visible = function() return M.matches(entry) end,
    color = function()
      if active() then return C.islandSurfaceHover end
      return hovered:get() and C.island or "#00000000"
    end,
    border_width = 1,
    border_color = function() return active() and C.accent() or "#00000000" end,
    behavior = { color = fast(), border_color = fast() },
    kit.glyph {
      anchors = { left = true, left_margin = 11, vertical_center = true },
      glyph = entry.icon, size = 13, width = 16,
      color = function() return active() and C.accent() or C.textMuted() end,
    },
    kit.text {
      anchors = { left = true, left_margin = 38, vertical_center = true },
      text = function() return tr(entry.label) end,
      size = theme.size.small, width = M.SIDEBAR - 16 - 50, elide = "right",
      weight = function() return active() and 600 or 400 end,
      color = function() return active() and C.accent() or C.text() end,
    },
    setting.hit { hovered = hovered, on_click = function() M.go(entry.id) end },
  }
end

local function sidebar(field_node)
  local rows = { direction = "column", gap = 1, align = "start", width = M.SIDEBAR - 16 }
  for _, group in ipairs(M.categories) do
    local members = {}
    for _, entry in ipairs(M.sections) do
      if entry.category == group.id then members[#members + 1] = entry end
    end
    rows[#rows + 1] = ui.Item {
      width = M.SIDEBAR - 16, height = 24,
      visible = function()
        for _, entry in ipairs(members) do if M.matches(entry) then return true end end
        return false
      end,
      kit.text {
        anchors = { left = true, left_margin = 12, bottom = true, bottom_margin = 3 },
        text = function() return tr(group.label) end,
        size = theme.size.label, weight = 600, letter_spacing = 0.8,
        color = C.textMuted, opacity = 0.7,
      },
    }
    for _, entry in ipairs(members) do rows[#rows + 1] = sidebar_entry(entry) end
  end

  local signature = ui.Row {
    gap = 8, align = "center",
    palette_board { size = 34 },
    kit.text {
      text = "impasto", size = 28,
      font_family = function() return theme.font_signature() end,
      font_source = morf.fs.is_file(theme.hand_file) and theme.hand_file or "",
    },
  }

  return ui.Rect {
    x = M.MARGIN, y = M.MARGIN, width = M.SIDEBAR, height = M.HEIGHT - 2 * M.MARGIN,
    radius = theme.radius_large, color = C.islandSurface,
    border_width = 1, border_color = C.islandBorder,
    ui.Item {
      x = 8, y = 12, width = M.SIDEBAR - 16, height = 38,
      ui.Item { anchors = { center_in = true },
        width = function() return signature.layout_width or 0 end, height = 38, signature },
    },
    ui.Rect {
      x = 8, y = 58, width = M.SIDEBAR - 16, height = 30, radius = theme.radius_small,
      color = C.island, border_width = 1,
      border_color = function() return field_node.focused_signal:get() and C.accent() or C.islandBorder end,
      behavior = { border_color = fast() },
      kit.glyph { anchors = { left = true, left_margin = 9, vertical_center = true },
        glyph = "󰍉", size = 12, color = C.textMuted },
      ui.Item {
        x = 28, y = 0, width = M.SIDEBAR - 16 - 28 - 9, height = 30,
        field_node.node,
      },
    },
    ui.ClipRect {
      x = 8, y = 96, width = M.SIDEBAR - 16, height = M.HEIGHT - 2 * M.MARGIN - 96 - 8,
      color = "#00000000",
      ui.Flex(rows),
    },
  }
end

-- ------------------------------------------------------------------ page --

--- The window's contents. `close()` closes the window.
function M.build(close)
  local focused = controls.signal("settings.search.focus", false)
  local search
  search = ui.TextInput {
    anchors = { fill = true },
    vertical_alignment = "center",
    text = s.filter:get(),
    placeholder = function() return tr("Search settings") end, placeholder_color = C.textMuted,
    font_family = function() return theme.font() end,
    font_size = theme.size.small,
    color = C.text, caret_color = C.accent,
    selection_color = C.accent, selected_text_color = C.accentText,
    focus = true,
    on_focus_changed = function(on) focused:set(on) end,
    on_text_changed = function(text) s.filter:set(text) end,
    -- Enter opens the first match.
    on_accepted = function()
      local first = M.shown()[1]
      if first then M.go(first.id) end
    end,
    -- Escape clears the field first, and closes the window once it is empty.
    on_escape = function()
      if search.text == "" then close() return end
      search.text = ""
      s.filter:set("")
    end,
  }

  local page_parts = { direction = "column", gap = M.GAP, align = "start", width = M.PAGE_W }
  page_parts[#page_parts + 1] = hero {
    icon = function() return by_id[s.section:get()].icon end,
    title = function() return tr(by_id[s.section:get()].label) end,
  }
  for _, entry in ipairs(M.sections) do
    if #entry.tabs > 1 then
      page_parts[#page_parts + 1] = ui.Item {
        width = M.PAGE_W, height = 30,
        visible = function() return s.section:get() == entry.id end,
        -- Built again with the page when the language changes.
        ui.Loader {
          active = function() return not s.relabel:get() end,
          source = function()
            local shown = {}
            for index, part in ipairs(entry.tabs) do shown[index] = { id = part.id, label = tr(part.label) } end
            return tab_strip { width = M.PAGE_W, tabs = shown,
              current = function() return s.tab:get() end, on_picked = M.show }
          end,
        },
      }
    end
  end
  -- One loader per section: the page is built while it is shown.
  local loaders = {}
  for _, entry in ipairs(M.sections) do
    loaders[#loaders + 1] = ui.Loader {
      active = function() return s.section:get() == entry.id and not s.relabel:get() end,
      source = function()
        local t0 = morf.time.now_ms()
        local ok, built = pcall(function()
          if (morf.env("IMPASTO_SETTINGS_TIMING") or "") ~= "" then
            morf.timer(1, function()
              morf.log("warn", "settings timing: " .. entry.id .. " built and laid out in "
                .. (morf.time.now_ms() - t0) .. " ms")
            end, false)
          end
          return require(entry.module).build {
            width = M.PAGE_W,
            tab = function() return s.tab:get() end,
            go = M.go, close = close,
          }
        end)
        if (morf.env("IMPASTO_SETTINGS_TIMING") or "") ~= "" then
          morf.log("warn", "settings timing: " .. entry.id .. " built in " .. (morf.time.now_ms() - t0) .. " ms")
        end
        if ok and built then return built end
        morf.log("error", "impasto: settings page " .. entry.id .. ": " .. tostring(built))
        return kit.text { text = "This page did not load: " .. tostring(built), color = C.red,
          width = M.PAGE_W, wrap = true, size = theme.size.small }
      end,
    }
  end
  loaders.width = M.PAGE_W
  page_parts[#page_parts + 1] = ui.Item(loaders)
  -- Room under the last group, so it is not flush with the window's edge.
  page_parts[#page_parts + 1] = ui.Item { width = M.PAGE_W, height = 8 }
  local body = ui.Flex(page_parts)

  local function room() return math.max(0, (body.layout_height or 0) - M.VIEW_H) end
  setting.scroll = function(steps, pixels)
    local delta = (steps ~= 0 and steps * 48) or pixels or 0
    s.scroll:set(math.max(0, math.min(room(), s.scroll:get() + delta)))
  end
  -- Keeps the offset inside the page when the page gets shorter.
  local clamp = function() return math.min(s.scroll:get(), room()) end

  local header = ui.Item {
    x = M.PAGE_X, y = M.MARGIN, width = M.PAGE_W, height = M.HEADER,
    ui.Row {
      anchors = { left = true, vertical_center = true }, gap = 6,
      controls.icon_button { icon = "󰅁", icon_size = 13, on_click = M.back,
        enabled = function() return s.cursor:get() > 1 end, dim_opacity = 0.3 },
      controls.icon_button { icon = "󰅂", icon_size = 13, on_click = M.forward,
        enabled = function() return s.cursor:get() < s.length:get() end, dim_opacity = 0.3 },
    },
    ui.Row {
      anchors = { right = true, vertical_center = true }, gap = 6,
      controls.icon_button { icon = "󰋗", icon_size = 13, on_click = function()
        local ok, act = pcall(require, "services.act")
        if ok then act.spawn("open the guide", "xdg-open", { "https://andreumassanet.github.io/impasto-docs/" }) end
      end },
      controls.icon_button { icon = "󰅖", icon_size = 13, on_click = close },
    },
  }

  -- A scroll bar that only shows when the page is taller than the view.
  local bar = ui.Rect {
    x = M.PAGE_X + M.PAGE_W + 4, width = 4, radius = 2, color = C.islandBorder,
    visible = function() return room() > 0 end,
    height = function()
      local total = math.max(1, body.layout_height or 1)
      return math.max(24, M.VIEW_H * M.VIEW_H / total)
    end,
    y = function()
      local total = math.max(1, body.layout_height or 1)
      local h = math.max(24, M.VIEW_H * M.VIEW_H / total)
      local r = room()
      return M.VIEW_Y + (r > 0 and (M.VIEW_H - h) * clamp() / r or 0)
    end,
  }

  return ui.Rect {
    width = M.WIDTH, height = M.HEIGHT, color = C.island,
    sidebar { node = search, focused_signal = focused },
    header,
    ui.ClipRect {
      x = M.PAGE_X, y = M.VIEW_Y, width = M.PAGE_W, height = M.VIEW_H, color = "#00000000",
      setting.wheel_area(),
      ui.Item {
        width = M.PAGE_W,
        height = function() return body.layout_height or 0 end,
        -- A transform, not `y`: scrolling moves pixels and lays nothing out.
        translate_y = function() return -clamp() end,
        body,
      },
    },
    bar,
  }
end

return M
