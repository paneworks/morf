-- The default kit's Collection skin, in the Adwaita manner: plain rows
-- with a wash under the pointer and the selected wash on the chosen one; a
-- list box's rows split by hairlines, and as a `boxed_list` one rounded
-- card of them; column views with a dim header whose sort arrow turns,
-- stripes and the chosen row across every cell; tree rows with hairline
-- indent guides and a chevron that turns as a node opens; grid tiles and
-- flow box pills. Rows come in with a short rise and leave sliding away.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, draw)
  local P, R = theme.P, theme.radius
  local SZ = theme.size
  local get, quick = draw.get, draw.quick
  local has = require("lib.kit.archetypes.press").has
  -- How a row comes (inserted) and goes (removed).
  local MOTION = {
    enter = { opacity = 0, translate_y = 10, duration = 260, easing = "out_cubic" },
    exit = { opacity = 0, translate_x = -36, duration = 200, easing = "in_cubic" },
  }

  local function label_of(r)
    if type(r) == "table" then return tostring(r.label or r.name or r.title or r.text or r.key or "") end
    return tostring(r or "")
  end
  local function field(s, row, key)
    local r = s.row() or row
    return type(r) == "table" and r[key] or nil
  end
  -- The wash a row wears: chosen, held, under the pointer, or a stripe.
  -- (A table's or a tree's chosen row takes the accent, as a file
  -- manager's does; a list's the selected wash.)
  local function row_tone(s, stripe)
    local p = P()
    if s.selected() or s.current() then
      if s.layout == "table" or s.layout == "tree" then
        return p.accent_bg:alpha((p.dark and 0.32 or 0.16) + (s.hovered() and 0.05 or 0))
      end
      return p.window_fg:alpha(p.wash.selected + (s.hovered() and 0.04 or 0))
    end
    if s.down() then return p.window_fg:alpha(p.wash.active) end
    if s.hovered() then return p.window_fg:alpha(p.wash.hover) end
    if stripe and s.index() % 2 == 0 then return p.window_fg:alpha(p.dark and 0.025 or 0.02) end
    return p.window_fg:alpha(0)
  end
  local function focus_edge(t, s) return function() return t.visual_focus and s.current() and 2 or 0 end end
  local function focus_color() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end
  -- A value as a cell shows it: whole numbers grouped by thousands.
  local function show(v)
    if type(v) ~= "number" then return tostring(v == nil and "" or v) end
    if v == math.floor(v) and math.abs(v) < 1e15 then
      local text = ("%d"):format(v)
      local sign, digits = text:match("^(-?)(%d+)$")
      digits = digits:reverse():gsub("(%d%d%d)", "%1,"):reverse():gsub("^,", "")
      return sign .. digits
    end
    return ("%.1f"):format(v)
  end
  local EXT = { png = "image", jpg = "image", jpeg = "image", svg = "image", gif = "image", webp = "image",
    mp4 = "movie", mkv = "movie", webm = "movie", mov = "movie", mp3 = "music_note", flac = "music_note",
    ogg = "music_note", wav = "music_note", pdf = "picture_as_pdf", zip = "folder_zip", gz = "folder_zip",
    tar = "folder_zip", xz = "folder_zip", lua = "code", rs = "code", py = "code", js = "code", c = "code",
    h = "code", toml = "code", json = "code", md = "article", txt = "article", odt = "article", doc = "article" }
  local ICON_TONE = { folder = "accent", folder_open = "accent", image = "extra", movie = "error",
    music_note = "warning", picture_as_pdf = "error", folder_zip = "warning", code = "info", article = "dim" }
  local function is_folder(r) return type(r) == "table" and (r.is_dir or r.kind == "folder" or r.type == "folder") end
  local function file_icon(r, open)
    if type(r) == "table" and r.icon then return r.icon end
    if is_folder(r) then return open and "folder_open" or "folder" end
    local ext = label_of(r):match("%.([%w]+)$")
    return ext and EXT[ext:lower()] or "draft"
  end
  local function icon_color(name)
    return function()
      local p = P()
      local tone = ICON_TONE[name] or "dim"
      if tone == "dim" then return p.dim_fg end
      return p[tone] or p.accent
    end
  end
  -- A chevron that turns a quarter as its row opens -- animated only when
  -- the same row opens or closes, not when a recycled row is rebound.
  local function chevron(s, size, color, props)
    local holder = ui.Item { x = props.x, anchors = { vertical_center = true }, width = size, height = size,
      visible = props.visible, draw.icon("chevron_right", size, color, { anchors = { center_in = true } }) }
    local key, open
    morf.effect("kit.collection.chevron." .. tostring(holder), function()
      local r = s.row()
      local k, e = r and r.key, r and r.expanded or false
      local turn = e and 90 or 0
      if k == key and e ~= open then
        morf.animation.play { { node = holder, property = "rotation", to = turn, duration = 260, easing = "out_back" } }
      else
        holder.rotation = turn
      end
      key, open = k, e
    end, { owner = holder })
    return holder
  end

  -- What shows when a collection has no rows (a layout's own delegate
  -- brings its own).
  local function empty(t, spec)
    if spec.delegate then return nil end
    return ui.Column { anchors = { center_in = true }, gap = 6, align = "center",
      visible = function() return (t.count or 0) == 0 end,
      draw.icon(spec.empty_icon or "inbox", 32, function() return P().dim_fg:alpha(0.7) end),
      draw.text { text = spec.empty_text or "Nothing here", font_size = SZ.normal, color = function() return P().dim_fg end } }
  end

  -- -------------------------------------------------------------- rows --

  --- A list row: an icon, a title and a subtitle under it when the row is
  --- tall enough, a trailing value, a chevron for a row that leads on.
  --- `o`: `inset`, `radius`, `separator`, `chevron`, `number` (the index,
  --- for a long list), `stripe`.
  local function item_row(t, spec, o)
    local H = spec.row_height or 36
    local two = H >= 46
    return function(row, s)
      local function r(key) return field(s, row, key) end
      local inset = o.inset or 1
      local lead = 12 + inset
      local has_icon = function() return r("icon") ~= nil end
      local tx = function() return lead + (o.number and 44 or 0) + (has_icon() and 30 or 0) end
      local right = function() return 12 + inset + (o.chevron and 22 or 0) + ((r("trailing") or r("value")) and 70 or 0) end
      local node = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = inset }, radius = o.radius or R.small,
          color = function() return row_tone(s, o.stripe) end, behavior = { color = quick() },
          border_width = focus_edge(t, s), border_color = focus_color },
        o.separator and ui.Rect { anchors = { left = true, right = true, bottom = true, left_margin = inset },
          height = 1, color = function() return P().border end,
          visible = function() return s.index() < s.count() end } or nil,
        o.number and draw.text { x = lead, width = 36, anchors = { vertical_center = true }, horizontal_alignment = "right",
          text = function() return tostring(s.index()) end, font_family = theme.mono, font_size = SZ.small,
          color = function() return P().dim_fg end } or nil,
        draw.icon(function() return r("icon") or "" end, 20, function() return P().window_fg end,
          { x = lead + (o.number and 44 or 0), anchors = { vertical_center = true }, visible = has_icon }),
        draw.text { x = tx, y = two and function() return r("subtitle") and (H / 2 - 20) or (H - SZ.normal * 1.3) / 2 end
            or (H - SZ.normal * 1.3) / 2,
          width = function() return math.max(0, s.width() - tx() - right()) end, elide = "right",
          text = function() return label_of(s.row() or row) end, font_size = SZ.normal,
          color = function() return P().window_fg end },
        two and draw.text { x = tx, y = H / 2 + 1, visible = function() return r("subtitle") ~= nil end,
          width = function() return math.max(0, s.width() - tx() - right()) end, elide = "right",
          text = function() return tostring(r("subtitle") or "") end, font_size = SZ.small,
          color = function() return P().dim_fg end } or nil,
        draw.text { anchors = { right = true, vertical_center = true, right_margin = 12 + inset + (o.chevron and 22 or 0) },
          text = function() return tostring(r("trailing") or r("value") or "") end, font_size = SZ.small,
          color = function() return P().dim_fg end },
        o.chevron and draw.icon("chevron_right", 18, function() return P().dim_fg end,
          { anchors = { right = true, vertical_center = true, right_margin = 8 + inset } }) or nil,
      }
      return node, function() end
    end
  end

  --- GtkListView: plain rows.
  S.list_view = { row = function(t, spec) return item_row(t, spec, {}) end, empty = empty }

  --- GtkListBox: rows split by hairlines; as a `boxed_list`, one card the
  --- height of its rows, each leading on with a chevron.
  S.list_box = {
    background = function(t, spec)
      if spec.delegate or not has(spec, "boxed_list") then return nil end
      local RH = spec.row_height or 50
      return ui.Rect { width = function() return t.width end,
        height = function() return math.min(t.height or 0, (t.count or 0) * RH) end,
        radius = R.large, color = function() return P().card_bg end,
        border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
        border_color = function() return P().border end,
        shadow_color = function() local p = P() return p.dark and p.shade:alpha(0) or p.shade:alpha(0.5) end,
        shadow_blur = 3, shadow_offset_y = 1 }
    end,
    row = function(t, spec)
      if has(spec, "boxed_list") then
        return item_row(t, spec, { separator = true, chevron = spec.activatable ~= false, inset = 0, radius = R.large })
      end
      return item_row(t, spec, { separator = true, inset = 0, radius = 0 })
    end,
    empty = empty,
  }

  --- A grid tile: a large icon over its caption, the chosen one washed
  --- with an accent edge.
  S.grid_view = {
    row = function(t, spec)
      local CW, CH = spec.cell_width or 96, spec.cell_height or 96
      return function(row, s)
        local function r(key) return field(s, row, key) end
        local node = ui.Item { width = CW, height = CH,
          ui.Rect { anchors = { fill = true, margins = 4 }, radius = R.large,
            color = function()
              local p = P()
              if s.selected() or s.current() then return p.accent_bg:alpha(p.dark and 0.24 or 0.14) end
              return row_tone(s)
            end,
            border_width = function() return (s.selected() or s.current()) and 2 or 0 end,
            border_color = function() local p = P() return t.visual_focus and s.current() and p.focus or p.accent_bg:alpha(0.7) end,
            behavior = { color = quick() } },
          ui.Item { anchors = { horizontal_center = true }, y = CH * 0.16, width = 40, height = 40,
            scale = function() return s.down() and 0.92 or (s.hovered() and 1.06 or 1) end,
            behavior = { scale = draw.spring(520, 26) },
            draw.icon(function() return r("icon") or "image" end, 36,
              function() local p = P() local tn = r("tone") return tn and p[tn] or p.accent end,
              { anchors = { center_in = true } }) },
          draw.text { x = 8, width = CW - 16, y = CH * 0.16 + 46, horizontal_alignment = "center", elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.small,
            color = function() return P().window_fg end } }
        return node, function() end
      end
    end,
    empty = empty,
  }

  --- A flow box: pills that wrap, the chosen ones filled with the accent.
  S.flow_box = {
    row = function(t, spec)
      local CW, CH = spec.cell_width or 128, spec.cell_height or 44
      return function(row, s)
        local function on() return s.selected() or s.current() end
        local node = ui.Item { width = CW, height = CH,
          ui.Rect { anchors = { fill = true, margins = 5 }, radius = (CH - 10) / 2,
            color = function()
              local p = P()
              if on() then return s.hovered() and p.accent_bg:mix(p.accent_fg, 0.1) or p.accent_bg end
              if s.down() then return p.window_fg:alpha(p.wash.checked) end
              return p.window_fg:alpha(s.hovered() and p.wash.raised_hover or p.wash.button)
            end,
            border_width = function() local p = P() return (t.visual_focus and s.current() and 2) or (p.strong and 1) or 0 end,
            border_color = function() local p = P() return t.visual_focus and s.current() and p.focus or p.border end,
            behavior = { color = quick() } },
          draw.icon("check", 16, function() return P().accent_fg end,
            { x = 16, anchors = { vertical_center = true }, opacity = function() return on() and 1 or 0 end,
              behavior = { opacity = quick() } }),
          draw.text { anchors = { vertical_center = true }, x = function() return on() and 36 or 18 end, width = CW - 52,
            behavior = { x = draw.spring(420, 30) }, elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.small, font_weight = 600,
            color = function() local p = P() return on() and p.accent_fg or p.window_fg end } }
        return node, function() end
      end
    end,
    empty = empty,
  }

  -- ------------------------------------------------------------ tables --

  --- A header cell: its title dim and bold, an arrow that turns between
  --- ascending and descending on the sorted column (and shows faintly on
  --- a sortable one under the pointer), a hairline under it and a grip at
  --- its edge.
  local function header(t, spec)
    return function(column)
      local numeric = column.align == "end" or column.key == "size"
      local sorted = function() return t.sort_column == column.key end
      return ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { left = true, right = true, bottom = true }, height = 1, color = function() return P().border end },
        ui.Rect { anchors = { right = true, top = true, bottom = true, top_margin = 9, bottom_margin = 9 }, width = 1,
          color = function() return P().border end },
        draw.text { anchors = { fill = true, left_margin = 12, right_margin = 30 },
          vertical_alignment = "center", horizontal_alignment = numeric and "right" or "left",
          text = column.title or column.key, elide = "right", font_size = SZ.small, font_weight = 700,
          color = function() local p = P() return sorted() and p.window_fg or p.dim_fg end },
        draw.icon("arrow_upward", 16, function() return P().accent end,
          { anchors = { right = true, right_margin = 10, vertical_center = true },
            rotation = function() return t.sort_ascending and 0 or 180 end,
            opacity = function() return sorted() and 1 or 0 end,
            behavior = { rotation = draw.spring(300, 26), opacity = quick() } }) }
    end
  end

  --- A table cell: the row's ground laid from the first cell across the
  --- whole row (stripes, the pointer's wash, the chosen wash), its value
  --- -- numbers to the right.
  local function cell_text(column, s, row, x, format)
    local numeric = column.align == "end" or column.key == "size"
    return draw.text { anchors = { fill = true, left_margin = x or 12, right_margin = numeric and 30 or 12 },
      vertical_alignment = "center",
      horizontal_alignment = numeric and "right" or "left", elide = "right", font_size = SZ.normal,
      text = function()
        local v = field(s, row, column.key)
        if format then return format(v) end
        return show(v)
      end,
      color = function() local p = P() return column.index == 1 and p.window_fg or p.dim_fg end }
  end
  local function row_ground(t, s)
    return ui.Rect { width = function() return s.width() end, anchors = { top = true, bottom = true },
      color = function() return row_tone(s, true) end, behavior = { color = quick() },
      border_width = focus_edge(t, s), border_color = focus_color }
  end
  local function table_cell(t, spec)
    return function(row, column, s)
      if column.index == 1 then
        return ui.Item { anchors = { fill = true }, row_ground(t, s), cell_text(column, s, row) }, function() end
      end
      return cell_text(column, s, row), function() end
    end
  end
  --- GtkColumnView: a header over the cells.
  S.column_view = { header = header, cell = table_cell, empty = empty }

  -- ------------------------------------------------------------- trees --

  local INDENT = 18
  --- What a tree row draws from `x0`: hairline guides down each level it
  --- sits in, the chevron, the icon and the label (a loading row: a
  --- spinner and its word).
  local function tree_parts(t, s, row, x0, width_of)
    local parts = {}
    for level = 1, 8 do
      parts[#parts + 1] = ui.Rect { x = x0 + (level - 1) * INDENT + 8, width = 1,
        anchors = { top = true, bottom = true }, color = function() return P().border end,
        visible = function() return s.depth() >= level end }
    end
    local function ix() return x0 + s.depth() * INDENT end
    parts[#parts + 1] = chevron(s, 18, function() return P().dim_fg end,
      { x = ix, visible = function() return s.expandable() end })
    parts[#parts + 1] = ui.Item { x = function() return ix() + 1 end, anchors = { vertical_center = true },
      width = 16, height = 16, visible = function() return s.loading() end,
      draw.spinner(16, function() return P().accent_bg end, { active = function() return s.loading() end }) }
    parts[#parts + 1] = draw.icon(function()
        local r = s.row() or row
        if type(r) == "table" and r.icon then return r.icon end
        if s.expandable() then return s.expanded() and "folder_open" or "folder" end
        return file_icon(r)
      end, 18,
      function()
        local r = s.row() or row
        local name = s.expandable() and "folder" or file_icon(r)
        return icon_color(name)()
      end,
      { x = function() return ix() + 20 end, anchors = { vertical_center = true },
        visible = function() return not s.loading() end })
    parts[#parts + 1] = draw.text { anchors = { vertical_center = true },
      x = function() return ix() + (s.loading() and 24 or 44) end,
      width = function() return math.max(0, width_of() - ix() - 52) end, elide = "right",
      text = function() return s.loading() and "Loading…" or label_of(s.row() or row) end, font_size = SZ.normal,
      color = function() local p = P() return s.loading() and p.dim_fg or p.window_fg end }
    return parts
  end
  local function tree_row(t, spec)
    return function(row, s)
      local node = ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = 1 }, radius = R.small,
          color = function() return row_tone(s) end, behavior = { color = quick() },
          border_width = focus_edge(t, s), border_color = focus_color } }
      for _, part in ipairs(tree_parts(t, s, row, 6, function() return s.width() end)) do ui.reparent(part, node) end
      return node, function() end
    end
  end
  --- GtkTreeExpander rows.
  S.tree_expander = { row = tree_row, empty = empty }


  -- Every row builder here carries how its rows come and go
  -- (lib.kit.archetypes.collection reads `motion` off a builder given as a table).
  for _, name in ipairs { "list_view", "list_box", "grid_view", "flow_box", "tree_expander" } do
    local build = S[name].row
    S[name].row = function(t, spec)
      local fn = build(t, spec)
      return setmetatable({ motion = MOTION }, { __call = function(_, row, s) return fn(row, s) end })
    end
  end

  --- Any other collection: a row's label on the pointer's and the chosen
  --- row's washes; a column view's cells and header.
  S.Collection = {
    row = function(t, spec) return item_row(t, spec, {}) end,
    cell = table_cell, header = header, empty = empty,
  }
end
