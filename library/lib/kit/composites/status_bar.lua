-- A status bar: a strip along the foot of a window or a panel with what is
-- going on -- words, icons, badges, a progress bar -- in a left, a centre
-- and a right zone; an item with `on_clicked` is a press (display widgets
-- + Press items).
--
--     local node, bar = composites.status_bar {
--       id = "status", width = 800,
--       left = { { icon = "check_circle", text = "Ready" },
--                { id = "branch", icon = "commit", text = function() return branch:get() end, on_clicked = pick } },
--       center = { { kind = "progress", value = function() return done:get() end, width = 120 } },
--       right = { { text = "Ln 12, Col 4", on_clicked = go_to_line }, { kind = "badge", count = 3, kind_tone = "alert" },
--                 { icon = "notifications", tooltip = "Notifications", on_clicked = show } },
--     }
--
-- An item is `{ text, icon, kind, tooltip, on_clicked, id, width }`;
-- `text` and `icon` may be bindings. `kind` is how it reads: "text" (the
-- default: an icon and words), "label" (the quieter label face), "badge"
-- (`count` or `text`, `tone`), "dot" (a status dot, `tone`, `pulse`),
-- "progress" (`value` 0..1, `width`), "separator", or "node" (`node`, the
-- caller's own). A pressable item takes a hover wash and the keyboard's
-- ring: Tab reaches it, Space and Return press it; one with only an icon
-- shows `tooltip`. The centre zone stays centred whatever the sides hold.
-- Other fields: `x`, `y`, `anchors`, `height` (30), `ground` (false: no
-- tonal strip), `accessible_name`. Ids: an item's `id`, or
-- `<id>-<zone>-<n>`.
local ui = require("morf.ui")
local control = require("lib.kit.control")
local popup = require("lib.kit.popup")

local function K() local ok, kit = pcall(require, "kit") return ok and type(kit) == "table" and kit or {} end
local function get(v) if type(v) == "function" then return v() end return v end

local function make(spec)
  spec = spec or {}
  local kit = K()
  local id = spec.id
  local H = spec.height or 30
  local W = spec.width or 600

  local function look(item)
    local kind = item.kind or "text"
    if kind == "node" then return item.node end
    if kind == "separator" then return kit.separator { vertical = true, length = H - 12 } end
    if kind == "badge" then
      return kit.badge { count = item.count, text = item.text, kind = item.tone or "alert", size = math.min(20, H - 10) }
    end
    if kind == "dot" then return kit.dot { kind = item.tone, color = item.color, size = 8, pulse = item.pulse } end
    if kind == "progress" then
      return kit.progress { value = item.value, width = item.width or 120, label = item.text or "Progress" }
    end
    local ink = kind == "label" and kit.ink("lo") or kit.ink("hi")
    local row = { gap = 6, align = "center" }
    if item.icon then
      row[#row + 1] = kit.icon(item.icon, 16, item.tone and kit.signal(item.tone) or kit.ink("lo"))
    end
    if item.text ~= nil then
      local t = kind == "label" and kit.label { text = item.text, color = ink }
        or kit.text { text = item.text, color = ink, font_size = item.font_size }
      if item.max_width then t.width, t.elide = item.max_width, "right" end
      row[#row + 1] = t
    end
    return ui.Row(row)
  end

  -- A pressable item: the look on a hover wash, sized to it.
  local function press(item, item_id, inner)
    local t
    local wash
    local node
    local pad = 8
    node, t = control.make("Press", "area", { widget = "area", id = item_id, height = H - 4,
      width = item.width or function() return (inner.layout_width or 0) + 2 * pad end, cursor = "pointer",
      accessible_name = item.accessible_name or (type(item.text) == "string" and item.text) or item.tooltip
        or item.icon,
      on_clicked = item.on_clicked }, { children = {} })
    wash = kit.surface { anchors = { fill = true }, radius = kit.round and kit.round(8) or 8,
      color = function()
        local c = kit.ink("hi")()
        if t.down then return c:alpha(0.14) end
        return c:alpha(t.hovered and 0.08 or 0)
      end, behavior = { color = { duration = 120 } } }
    ui.reparent(wash, node)
    inner.anchors = { center_in = true }
    ui.reparent(inner, node)
    if item.tooltip and popup.tooltip then popup.tooltip(node, item.tooltip) end
    return node
  end

  local function zone(name, list)
    local row = { gap = 4, align = "center", height = H }
    for n, item in ipairs(list or {}) do
      local item_id = item.id or (id and ("%s-%s-%d"):format(id, name, n)) or nil
      local inner = look(item)
      local node
      if item.on_clicked then
        node = press(item, item_id, inner)
      else
        node = ui.Item { id = item_id, height = H, width = item.width or function() return inner.layout_width or 0 end,
          visible = item.visible }
        inner.anchors = { vertical_center = true }
        ui.reparent(inner, node)
      end
      if item.visible ~= nil and item.on_clicked then node.visible = item.visible end
      row[#row + 1] = node
    end
    return ui.Row(row)
  end

  local left, center, right = zone("left", spec.left), zone("center", spec.center), zone("right", spec.right)
  left.x, left.anchors = 6, { vertical_center = true }
  center.anchors = { center_in = true }
  right.anchors = { right = true, right_margin = 6, vertical_center = true }
  local children = {}
  if spec.ground ~= false then
    children[#children + 1] = kit.surface { anchors = { fill = true },
      color = function() return kit.ink("hi")():alpha(0.05) end }
    children[#children + 1] = kit.separator { width = W }
  end
  children[#children + 1] = left
  children[#children + 1] = center
  children[#children + 1] = right
  local node = ui.Item { id = id, x = spec.x, y = spec.y, anchors = spec.anchors, width = W, height = H, clip = true,
    accessible_role = "group", accessible_name = spec.accessible_name or "Status",
    table.unpack(children) }
  return node, { node = node }
end

return { make = make }
