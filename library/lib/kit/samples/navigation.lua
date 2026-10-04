-- Gallery samples for the Navigation archetype's widgets (samples/init.lua):
-- each a set of pages the widget moves between, with `chrome = true` so
-- the skin draws the widget's own furniture round them (a back button and
-- title, a carousel's dots, a wizard's progress). A composite that draws
-- its own leaves `chrome` out and gets only the skin's transitions.
local ui = require("morf.ui")

local S = {}
S.span = { master_detail = { 2, 1 }, wizard = { 2, 1 }, tab_pages = { 2, 1 } }

local W, H = 280, 220

-- A page: a card of the kit's own ground with an icon, a title and a line,
-- leaving `top` px for the chrome above it and `bottom` below.
local function page(kit, spec)
  local w, top, bottom = spec.width or W, spec.top or 0, spec.bottom or 0
  local h = (spec.height or H) - top - bottom
  return function()
    return ui.Item { width = w, height = spec.height or H,
      ui.Item { x = 0, y = top, width = w, height = h,
        kit.icon(spec.icon or "circle", 28, kit.ink("accent"), { x = 16, y = 14 }),
        kit.text { x = 16, y = 54, width = w - 32, text = spec.title, color = kit.ink("hi"), elide = "right" },
        kit.text { x = 16, y = 80, width = w - 32, text = spec.note or "", color = kit.ink("lo"), elide = "right" },
      } }
  end
end

local function pages(kit, list, extra)
  local out = {}
  for _, p in ipairs(list) do
    local spec = {}
    for k, v in pairs(extra or {}) do spec[k] = v end
    for k, v in pairs(p) do spec[k] = v end
    out[p.key] = page(kit, spec)
  end
  return out
end

local function titles(list)
  local out = {}
  for _, p in ipairs(list) do out[p.key] = p.title end
  return out
end

local function order(list)
  local out = {}
  for i, p in ipairs(list) do out[i] = p.key end
  return out
end

function S.navigation_view(kit, widgets)
  local list = { { key = "settings", title = "Settings", icon = "settings", note = "Everything in one place" },
    { key = "display", title = "Display", icon = "monitor", note = "Resolution and scale" } }
  local node, nav = widgets.navigation_view { id = "sample-navigation-view", width = W, height = H, chrome = true,
    current = "settings", titles = titles(list), pages = pages(kit, list, { top = 44 }) }
  nav.push("display")
  return node
end

function S.view_stack(kit, widgets)
  local list = { { key = "albums", title = "Albums", icon = "album", note = "128 albums" },
    { key = "artists", title = "Artists", icon = "person", note = "64 artists" },
    { key = "songs", title = "Songs", icon = "music_note", note = "1 482 songs" } }
  local _, nav
  local switcher = widgets.inline_view_switcher { id = "sample-view-stack-switcher", accessible_name = "Views",
    items = { { label = "Albums" }, { label = "Artists" }, { label = "Songs" } }, item_width = 92, item_height = 34,
    current = 1, on_current_changed = function(i) if nav then nav.go(list[i].key) end end }
  local node
  node, nav = widgets.view_stack { id = "sample-view-stack", width = W, height = H - 46, y = 46, chrome = true,
    current = "albums", order = order(list), pages = pages(kit, list, { height = H - 46 }) }
  return ui.Item { width = W, height = H, switcher, node }
end

function S.tab_pages(kit, widgets)
  local w = 600
  local list = { { key = "general", title = "General", icon = "tune", note = "Name, language and region" },
    { key = "privacy", title = "Privacy", icon = "lock", note = "Location, camera, microphone" },
    { key = "sharing", title = "Sharing", icon = "share", note = "Screen and media sharing" } }
  return (widgets.tab_pages { id = "sample-tab-pages", width = w, height = H, chrome = true, current = "privacy",
    order = order(list), titles = titles(list), pages = pages(kit, list, { width = w, top = 44 }) })
end

function S.carousel(kit, widgets)
  local list = { { key = "a", title = "Morning", icon = "wb_twilight", note = "Sunrise at 6:42" },
    { key = "b", title = "Noon", icon = "light_mode", note = "Clear, 24 degrees" },
    { key = "c", title = "Evening", icon = "dark_mode", note = "Sunset at 19:58" } }
  return (widgets.carousel { id = "sample-carousel", width = W, height = H, chrome = true, current = "b",
    order = order(list), pages = pages(kit, list, { bottom = 32 }) })
end

function S.onboarding(kit, widgets)
  local list = { { key = "a", title = "Welcome", icon = "waving_hand", note = "A quick tour of your desk" },
    { key = "b", title = "Workspaces", icon = "view_carousel", note = "Swipe between them" },
    { key = "c", title = "Quick settings", icon = "toggle_on", note = "Everything one click away" },
    { key = "d", title = "Ready", icon = "check_circle", note = "Enjoy" } }
  return (widgets.onboarding { id = "sample-onboarding", width = W, height = H, chrome = true, current = "b",
    order = order(list), pages = pages(kit, list, { top = 20 }) })
end

function S.wizard(kit, widgets)
  local w = 600
  local list = { { key = "a", title = "Choose a disk", icon = "hard_drive", note = "Where the system goes" },
    { key = "b", title = "Create an account", icon = "person_add", note = "Your name and password" },
    { key = "c", title = "Connect", icon = "wifi", note = "Pick a network" },
    { key = "d", title = "Finish", icon = "flag", note = "Install and restart" } }
  return (widgets.wizard { id = "sample-wizard", width = w, height = H, chrome = true, current = "b",
    order = order(list), titles = titles(list), pages = pages(kit, list, { width = w, top = 44 }) })
end

function S.settings_subpages(kit, widgets)
  local list = { { key = "main", title = "Sound", icon = "volume_up", note = "Output and input" },
    { key = "output", title = "Output", icon = "speaker", note = "Built-in speakers" } }
  local node, nav = widgets.settings_subpages { id = "sample-settings-subpages", width = W, height = H, chrome = true,
    current = "main", titles = titles(list), pages = pages(kit, list, { top = 48 }) }
  nav.push("output")
  return node
end

function S.master_detail(kit, widgets)
  local list = { { key = "inbox", title = "Inbox", icon = "inbox", note = "3 unread messages" },
    { key = "sent", title = "Sent", icon = "send", note = "Last sent yesterday" },
    { key = "drafts", title = "Drafts", icon = "draft", note = "One draft" } }
  local _, nav
  local master = widgets.sidebar_list { id = "sample-master-list", accessible_name = "Folders",
    items = { { label = "Inbox", icon = "inbox" }, { label = "Sent", icon = "send" }, { label = "Drafts", icon = "draft" } },
    item_width = 200, item_height = 40, gap = 2, current = 1,
    on_current_changed = function(i) if nav then nav.go(list[i].key) end end }
  local node
  node, nav = widgets.master_detail { id = "sample-master-detail", x = 216, width = 384, height = H, chrome = true,
    current = "inbox", order = order(list), titles = titles(list), pages = pages(kit, list, { width = 384 }) }
  return ui.Item { width = 600, height = H, ui.Item { y = 0, width = 200, height = H, master }, node }
end

return S
