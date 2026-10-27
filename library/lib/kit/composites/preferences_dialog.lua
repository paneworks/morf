-- AdwPreferencesDialog: preference pages in a dialog, a view switcher
-- across its top when there is more than one (Popup + Navigation +
-- Selection).
--
--     local node, prefs = composites.preferences_dialog {
--       id = "prefs", root = surface_root, title = "Preferences",
--       pages = {
--         { name = "General", icon = "tune", groups = { { title = "Look", rows = { ... } } } },
--         { name = "Sound", icon = "volume_up", groups = { ... } },
--       },
--     }
--     prefs.open() prefs.close() prefs.show("Sound") prefs.page()
--
-- Each page is a preferences page (lib.kit.composites.rows: `groups` of
-- rows); the switcher (a press, Ctrl+Tab) changes the page, which
-- cross-fades. Escape or the close button closes the dialog and focus goes
-- back to what opened it. Other fields: `width` (560), `height` (520),
-- `on_closed(reason)`. Ids: `<id>-close`, `<id>-switcher`, `<id>-pages`,
-- `<id>-popup`.
local ui = require("morf.ui")
local popup = require("lib.kit.archetypes.popup")
local selection = require("lib.kit.archetypes.selection")
local navigation = require("lib.kit.archetypes.navigation")
local widgets = require("lib.kit.widgets")

local serial = 0

return function(spec)
  spec = spec or {}
  serial = serial + 1
  local id = spec.id
  local function sid(suffix) return id and (id .. "-" .. suffix) or nil end
  local W, H = spec.width or 560, spec.height or 520
  local HEAD = 52
  local pages_spec = spec.pages or {}
  local order, pages = {}, {}
  for i, page in ipairs(pages_spec) do
    local name = page.name or ("Page " .. i)
    order[i] = name
    pages[name] = function()
      return (require("lib.kit.composites").preferences_page { width = W, height = H - HEAD, groups = page.groups })
    end
  end
  local current = morf.signal("kit.composites.preferences_dialog." .. tostring(id) .. "." .. serial, 1)
  local menu, nav, content
  local function close(reason) if menu then menu.close(reason or "closed") end end
  local function build()
    if content then return content end
    local stack
    stack, nav = navigation.make("view_stack", { id = sid("pages"), y = HEAD, width = W, height = H - HEAD,
      order = order, pages = pages, current = order[1],
      on_current_changed = function(name)
        for i, page in ipairs(order) do if page == name then current:set(i) end end
      end })
    local top
    if #order > 1 then
      local items = {}
      for i, page in ipairs(pages_spec) do items[i] = { label = order[i], icon = page.icon } end
      local SW = math.min(W - 120, #order * 110)
      top = selection.make("inline_view_switcher", { id = sid("switcher"), accessible_name = "Pages",
        items = items, item_width = math.floor(SW / #order), item_height = 34, width = SW, height = 34,
        x = (W - SW) / 2, y = (HEAD - 34) / 2, press_activates = true,
        current = function() return current:get() end,
        on_current_changed = function(i) if order[i] and nav then nav.go(order[i]) end end })
    else
      top = widgets.window_title { title = spec.title or order[1] or "Preferences" }
    end
    local close_button = widgets.button { id = sid("close"), icon = "close", variant = "flat circular",
      width = 34, height = 34, x = W - 44, y = (HEAD - 34) / 2, accessible_name = "Close",
      on_clicked = function() close("closed") end }
    content = ui.Item { width = W, height = H, z = 1, ui.Item { width = W, height = HEAD, top, close_button }, stack }
    return content
  end
  local handle = {}
  local function ensure()
    if menu then return menu end
    build()
    menu = popup.make("preferences_dialog", { id = sid("popup"), content = content, width = W, height = H,
      root = spec.root, on_opened = spec.on_opened,
      on_closed = function(reason) if spec.on_closed then spec.on_closed(reason) end end })
    handle.popup = menu
    return menu
  end
  function handle.open(anchor)
    if menu and menu.is_open() then return end
    ensure().open(anchor)
  end
  handle.close = close
  function handle.is_open() return menu ~= nil and menu.is_open() end
  function handle.show(name) build() if nav then nav.go(name) end end
  function handle.page() return order[current:get()] end
  return nil, handle
end
