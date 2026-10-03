-- An about dialog: an application's name, version, icon and links, with
-- pages for its credits and its legal text (Popup, dialog + Navigation).
--
--     local node, about = composites.about_dialog {
--       id = "about", root = surface_root,
--       app_name = "Morf", version = "0.15", icon = "deployed_code",
--       comments = "A UI engine for shells and apps.",
--       links = { { label = "Website", url = "https://morf.dev" }, { label = "Report an issue", url = "..." } },
--       credits = { { title = "Developers", names = { "Ada", "Linus" } }, { title = "Design", names = { "Grace" } } },
--       copyright = "© 2026 The Morf authors", license = "MIT",
--       legal = "Permission is hereby granted, ...",
--       on_link = function(url) end,
--     }
--     about.open() about.close() about.show("credits") about.page()
--
-- The pages -- "about", "credits", "legal" -- are a Navigation switcher
-- under tabs: a press on a tab, Ctrl+Tab / Ctrl+Shift+Tab, or
-- `show(page)` changes the page, which slides in from its side. A page
-- with nothing to say (no credits, no legal text or licence) is left
-- out. Escape or the close button closes the dialog and focus goes back to
-- what opened it. `inline = true` returns the dialog as a node to place.
-- Other fields: `width` (420), `height` (460), `developers`, `designers`,
-- `artists`, `translators` (lists of names, folded into the credits),
-- `website` (a first link), `on_closed(reason)`. Ids: `<id>-tabs`,
-- `<id>-tab-<n>`, `<id>-pages`, `<id>-link-<n>`, `<id>-close`,
-- `<id>-popup`.
local ui = require("morf.ui")
local popup = require("lib.kit.popup")
local control = require("lib.kit.control")
local selection = require("lib.kit.selection")
local navigation = require("lib.kit.navigation")
local scroll = require("lib.kit.scroll")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end

local TITLES = { about = "About", credits = "Credits", legal = "Legal" }

local function make(spec)
  spec = spec or {}
  local kit = K()
  local id = spec.id
  local W, H = spec.width or 420, spec.height or 460
  local PAD = 16
  local TAB_H = 64
  local PAGE_Y = PAD + TAB_H + 4
  local PW, PH = W - 2 * PAD, H - PAGE_Y - PAD
  local menu, nav, content
  local function is_open() return menu ~= nil and menu.is_open() end
  local function close(reason) if menu then menu.close(reason or "closed") end end

  -- The credits: given sections, then the named roles.
  local credits = {}
  for _, section in ipairs(spec.credits or {}) do credits[#credits + 1] = section end
  for _, role in ipairs { { "developers", "Developers" }, { "designers", "Design" }, { "artists", "Artwork" },
    { "translators", "Translation" } } do
    if spec[role[1]] and #spec[role[1]] > 0 then credits[#credits + 1] = { title = role[2], names = spec[role[1]] } end
  end
  local links = {}
  if spec.website then links[#links + 1] = { label = "Website", url = spec.website } end
  for _, link in ipairs(spec.links or {}) do links[#links + 1] = link end
  local order = { "about" }
  if #credits > 0 then order[#order + 1] = "credits" end
  if spec.legal or spec.license or spec.copyright then order[#order + 1] = "legal" end
  local current = morf.signal("kit.composites.about." .. tostring(id) .. "." .. tostring(spec), 1)

  local function scrolled(column)
    local node = scroll.make("scroll_view", { width = PW, height = PH, clip = true, column })
    return node
  end

  local function about_page()
    local mark = ui.Item { width = 72, height = 72,
      kit.surface { anchors = { fill = true }, radius = kit.round and kit.round(24) or 24,
        color = function() return kit.signal("accent")():alpha(0.16) end },
      kit.icon(spec.icon or "apps", 42, kit.signal("accent"), { anchors = { center_in = true } }) }
    local column = { width = PW, gap = 8, align = "center", mark,
      kit.text { text = spec.app_name or "Application", width = PW, horizontal_alignment = "center",
        accessible_role = "heading", font_weight = 600, color = kit.ink("hi"),
        elide = "right", font_size = 22 },
    }
    if spec.version then
      column[#column + 1] = kit.tag and kit.tag { text = spec.version } or kit.label { text = spec.version }
    end
    if spec.comments then
      column[#column + 1] = kit.text { text = spec.comments, width = PW - 24, wrap = true,
        horizontal_alignment = "center", color = kit.ink("lo") }
    end
    if #links > 0 then
      local row = { gap = 8, align = "center" }
      for n, link in ipairs(links) do
        local label = tostring(link.label or link.url)
        row[#row + 1] = control.make("Press", "menu_item", { widget = "menu_item",
          id = id and (id .. "-link-" .. n) or nil, label = label, icon = link.icon or "open_in_new",
          width = math.min(PW, 56 + math.ceil(utf8.len(label) * 8.2)), height = 36, accessible_name = label,
          accessible_role = "link",
          on_clicked = function()
            if link.on_clicked then link.on_clicked(link) end
            if spec.on_link then spec.on_link(link.url, link) end
          end })
      end
      column[#column + 1] = ui.Item { width = 1, height = 4 }
      column[#column + 1] = ui.Flex { direction = "row", wrap = true, justify = "center", gap = 8, width = PW,
        table.unpack(row) }
    end
    return scrolled(ui.Column(column))
  end

  local function credits_page()
    local column = { width = PW, gap = 6 }
    for i, section in ipairs(credits) do
      if i > 1 then column[#column + 1] = ui.Item { width = 1, height = 8 } end
      column[#column + 1] = kit.label { text = section.title or "" }
      for _, name in ipairs(section.names or {}) do
        column[#column + 1] = kit.text { text = name, width = PW, elide = "right", color = kit.ink("hi") }
      end
    end
    return scrolled(ui.Column(column))
  end

  local function legal_page()
    local column = { width = PW, gap = 8 }
    if spec.copyright then column[#column + 1] = kit.text { text = spec.copyright, width = PW, wrap = true } end
    if spec.license then
      column[#column + 1] = ui.Row { gap = 8, align = "center",
        kit.label { text = "Licence" }, kit.tag and kit.tag { text = spec.license } or kit.text { text = spec.license } }
    end
    if spec.legal then
      column[#column + 1] = kit.text { text = spec.legal, width = PW, wrap = true, color = kit.ink("lo") }
    end
    return scrolled(ui.Column(column))
  end

  local BUILD = { about = about_page, credits = credits_page, legal = legal_page }

  local function build()
    if content then return content end
    local pages = {}
    for _, name in ipairs(order) do pages[name] = BUILD[name] end
    local pages_node
    pages_node, nav = navigation.make("view_stack", {
      id = id and (id .. "-pages") or nil, x = PAD, y = PAGE_Y, width = PW, height = PH, mode = "switcher",
      order = order, pages = pages, current = order[1],
      on_current_changed = function(name)
        for n, page in ipairs(order) do if page == name then current:set(n) end end
      end,
    })
    local names = {}
    for n, page in ipairs(order) do names[n] = { name = TITLES[page], label = TITLES[page] } end
    local TW = W - 2 * PAD - (spec.inline and 0 or 44)
    local tabs = selection.make("tabs", {
      id = id and (id .. "-tabs") or nil, x = PAD, y = PAD, items = names, orientation = "horizontal",
      width = TW, height = TAB_H,
      item_width = math.floor(TW / #names), item_height = TAB_H, press_activates = true,
      item_id = id and function(n) return id .. "-tab-" .. n end or nil,
      current = function() return current:get() end,
      on_current_changed = function(n) if order[n] and nav then nav.go(order[n]) end end,
      accessible_name = "Pages",
    })
    local close_button = not spec.inline and control.make("Press", "icon", { widget = "icon",
      id = id and (id .. "-close") or nil, icon_off = "close", icon_on = "close", width = 36, height = 36, size = 20,
      x = W - PAD - 36, y = PAD + 2, accessible_name = "Close", on_clicked = function() close("closed") end }) or nil
    content = ui.Item { width = W, height = H, z = 1, tabs, close_button, pages_node }
    return content
  end

  local handle = {}
  local node
  if spec.inline then
    build()
    node = ui.Item { id = id, x = spec.x, y = spec.y, width = W, height = H, anchors = spec.anchors,
      kit.card { anchors = { fill = true } }, content }
    handle.open, handle.close = function() end, function() end
    handle.is_open = function() return true end
  else
    local function ensure()
      if menu then return menu end
      build()
      menu = popup.make("about_dialog", {
        id = id and (id .. "-popup") or nil, content = content, width = W, height = H, root = spec.root,
        placement = "center", close_policy = "escape", modal = true, dim = true,
        on_opened = spec.on_opened,
        on_closed = function(reason) if spec.on_closed then spec.on_closed(reason) end end,
      })
      handle.popup = menu
      return menu
    end
    function handle.open(anchor)
      if is_open() then return end
      ensure()
      if nav then nav.go(order[1]) end
      menu.open(anchor)
    end
    handle.close = close
    handle.is_open = is_open
  end
  handle.node = node
  handle.pages = function() return order end
  handle.page = function() return order[current:get()] end
  handle.show = function(name) build() if nav then nav.go(name) end end
  return node, handle
end

return { make = make }
