-- The default kit's looks for each Collection widget, in the Adwaita
-- manner: plain rows with a wash under the pointer and the selected wash
-- on the chosen one; boxed lists as one rounded card of rows split by
-- hairlines; tables with a dim header whose sort arrow turns, stripes and
-- the chosen row across every cell; trees with hairline indent guides and
-- a chevron that turns as a node opens; file rows with a coloured type
-- icon; a timeline's rail and dots; feed cards; chat bubbles left and
-- right; kanban cards with a tag; transfer rows with a check. Rows come in
-- with a short rise and leave sliding away. Every layout here is shared
-- with the caelestia themes' (material, tsugumori): only the style differs.
local morf = require("morf")
local ui = require("morf.ui")

return function(S, theme, M)
  local P, R = theme.P, theme.radius
  local SZ = theme.size
  local function get(v) if type(v) == "function" then return v() end return v end
  local function quick() return { duration = theme.duration.small, easing = theme.ease.standard } end
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
        return p.accent:alpha((p.dark and 0.32 or 0.16) + (s.hovered() and 0.05 or 0))
      end
      return p.ink:alpha(p.wash.selected + (s.hovered() and 0.04 or 0))
    end
    if s.down() then return p.ink:alpha(p.wash.active) end
    if s.hovered() then return p.ink:alpha(p.wash.hover) end
    if stripe and s.index() % 2 == 0 then return p.ink:alpha(p.dark and 0.025 or 0.02) end
    return p.ink:alpha(0)
  end
  local function focus_edge(t, s) return function() return t.visual_focus and s.current() and 2 or 0 end end
  local function focus_color() local p = P() return p.strong and p.focus or p.focus:alpha(0.6) end
  -- A tone of the palette picked by a name, the same name always the same.
  local TONES = { "accent", "success", "warning", "error", "extra", "info" }
  local function tone_of(name)
    local h = 0
    for i = 1, #name do h = (h * 31 + name:byte(i)) % 997 end
    return TONES[h % #TONES + 1]
  end
  local function initial(name) return (tostring(name or "?"):match("[%w]") or "?"):upper() end
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
  local function bytes(n)
    if type(n) ~= "number" then return n and tostring(n) or "" end
    local units, i = { "B", "kB", "MB", "GB", "TB" }, 1
    while n >= 1000 and i < #units do n, i = n / 1000, i + 1 end
    return i == 1 and ("%d %s"):format(n, units[i]) or ("%.1f %s"):format(n, units[i])
  end
  local function date(v)
    if type(v) == "number" then return os.date("%d %b %H:%M", v) end
    return v and tostring(v) or ""
  end
  local EXT = { png = "image", jpg = "image", jpeg = "image", svg = "image", gif = "image", webp = "image",
    mp4 = "movie", mkv = "movie", webm = "movie", mov = "movie", mp3 = "music_note", flac = "music_note",
    ogg = "music_note", wav = "music_note", pdf = "picture_as_pdf", zip = "folder_zip", gz = "folder_zip",
    tar = "folder_zip", xz = "folder_zip", lua = "code", rs = "code", py = "code", js = "code", c = "code",
    h = "code", toml = "code", json = "code", md = "article", txt = "article", odt = "article", doc = "article" }
  local ICON_TONE = { folder = "accent", folder_open = "accent", image = "extra", movie = "error",
    music_note = "warning", picture_as_pdf = "error", folder_zip = "warning", code = "info", article = "ink_dim" }
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
      local tone = ICON_TONE[name] or "ink_dim"
      if tone == "ink_dim" then return p.ink_dim end
      return p[tone .. "_ink"] or p.accent_ink
    end
  end
  -- A chevron that turns a quarter as its row opens -- animated only when
  -- the same row opens or closes, not when a recycled row is rebound.
  local function chevron(s, size, color, props)
    local holder = ui.Item { x = props.x, anchors = { vertical_center = true }, width = size, height = size,
      visible = props.visible, M.icon("chevron_right", size, color, { anchors = { center_in = true } }) }
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
      M.icon(spec.empty_icon or "inbox", 32, function() return P().ink_dim:alpha(0.7) end),
      M.text { text = spec.empty_text or "Nothing here", font_size = SZ.normal, color = function() return P().ink_dim end } }
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
        o.number and M.text { x = lead, width = 36, anchors = { vertical_center = true }, horizontal_alignment = "right",
          text = function() return tostring(s.index()) end, font_family = theme.mono, font_size = SZ.small,
          color = function() return P().ink_dim end } or nil,
        M.icon(function() return r("icon") or "" end, 20, function() return P().ink end,
          { x = lead + (o.number and 44 or 0), anchors = { vertical_center = true }, visible = has_icon }),
        M.text { x = tx, y = two and function() return r("subtitle") and (H / 2 - 20) or (H - SZ.normal * 1.3) / 2 end
            or (H - SZ.normal * 1.3) / 2,
          width = function() return math.max(0, s.width() - tx() - right()) end, elide = "right",
          text = function() return label_of(s.row() or row) end, font_size = SZ.normal,
          color = function() return P().ink end },
        two and M.text { x = tx, y = H / 2 + 1, visible = function() return r("subtitle") ~= nil end,
          width = function() return math.max(0, s.width() - tx() - right()) end, elide = "right",
          text = function() return tostring(r("subtitle") or "") end, font_size = SZ.small,
          color = function() return P().ink_dim end } or nil,
        M.text { anchors = { right = true, vertical_center = true, right_margin = 12 + inset + (o.chevron and 22 or 0) },
          text = function() return tostring(r("trailing") or r("value") or "") end, font_size = SZ.small,
          color = function() return P().ink_dim end },
        o.chevron and M.icon("chevron_right", 18, function() return P().ink_dim end,
          { anchors = { right = true, vertical_center = true, right_margin = 8 + inset } }) or nil,
      }
      return node, function() end
    end
  end

  S.list = { row = function(t, spec) return item_row(t, spec, {}) end, empty = empty }
  S.virtual_list = { row = function(t, spec) return item_row(t, spec, { number = true, stripe = true, inset = 0, radius = 0 }) end,
    empty = empty }
  S.list_box = { row = function(t, spec) return item_row(t, spec, { separator = true, inset = 0, radius = 0 }) end,
    empty = empty }

  --- A boxed list: one card the height of its rows, the rows split by
  --- hairlines, each leading on with a chevron.
  S.boxed_list = {
    background = function(t, spec)
      if spec.delegate then return nil end
      local H = spec.row_height or 50
      return ui.Rect { width = function() return t.width end,
        height = function() return math.min(t.height or 0, (t.count or 0) * H) end,
        radius = R.large, color = function() return P().card end,
        border_width = function() local p = P() return (p.strong and 2) or (p.dark and 0) or 1 end,
        border_color = function() return P().border end,
        shadow_color = function() local p = P() return p.dark and p.shade:alpha(0) or p.shade:alpha(0.5) end,
        shadow_blur = 3, shadow_offset_y = 1 }
    end,
    row = function(t, spec) return item_row(t, spec, { separator = true, chevron = true, inset = 0, radius = R.large }) end,
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
              if s.selected() or s.current() then return p.accent:alpha(p.dark and 0.24 or 0.14) end
              return row_tone(s)
            end,
            border_width = function() return (s.selected() or s.current()) and 2 or 0 end,
            border_color = function() local p = P() return t.visual_focus and s.current() and p.focus or p.accent:alpha(0.7) end,
            behavior = { color = quick() } },
          ui.Item { anchors = { horizontal_center = true }, y = CH * 0.16, width = 40, height = 40,
            scale = function() return s.down() and 0.92 or (s.hovered() and 1.06 or 1) end,
            behavior = { scale = M.spring(520, 26) },
            M.icon(function() return r("icon") or "image" end, 36,
              function() local p = P() local tn = r("tone") return tn and p[tn .. "_ink"] or p.accent_ink end,
              { anchors = { center_in = true } }) },
          M.text { x = 8, width = CW - 16, y = CH * 0.16 + 46, horizontal_alignment = "center", elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.small,
            color = function() return P().ink end } }
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
              if on() then return s.hovered() and p.accent:mix(p.on_accent, 0.1) or p.accent end
              if s.down() then return p.ink:alpha(p.wash.checked) end
              return p.ink:alpha(s.hovered() and p.wash.raised_hover or p.wash.button)
            end,
            border_width = function() local p = P() return (t.visual_focus and s.current() and 2) or (p.strong and 1) or 0 end,
            border_color = function() local p = P() return t.visual_focus and s.current() and p.focus or p.border end,
            behavior = { color = quick() } },
          M.icon("check", 16, function() return P().on_accent end,
            { x = 16, anchors = { vertical_center = true }, opacity = function() return on() and 1 or 0 end,
              behavior = { opacity = quick() } }),
          M.text { anchors = { vertical_center = true }, x = function() return on() and 36 or 18 end, width = CW - 52,
            behavior = { x = M.spring(420, 30) }, elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.small, font_weight = 600,
            color = function() local p = P() return on() and p.on_accent or p.ink end } }
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
        M.text { anchors = { fill = true, left_margin = 12, right_margin = 30 },
          vertical_alignment = "center", horizontal_alignment = numeric and "right" or "left",
          text = column.title or column.key, elide = "right", font_size = SZ.small, font_weight = 700,
          color = function() local p = P() return sorted() and p.ink or p.ink_dim end },
        M.icon("arrow_upward", 16, function() return P().accent_ink end,
          { anchors = { right = true, right_margin = 10, vertical_center = true },
            rotation = function() return t.sort_ascending and 0 or 180 end,
            opacity = function() return sorted() and 1 or 0 end,
            behavior = { rotation = M.spring(300, 26), opacity = quick() } }) }
    end
  end

  --- A table cell: the row's ground laid from the first cell across the
  --- whole row (stripes, the pointer's wash, the chosen wash), its value
  --- -- numbers to the right.
  local function cell_text(column, s, row, x, format)
    local numeric = column.align == "end" or column.key == "size"
    return M.text { anchors = { fill = true, left_margin = x or 12, right_margin = numeric and 30 or 12 },
      vertical_alignment = "center",
      horizontal_alignment = numeric and "right" or "left", elide = "right", font_size = SZ.normal,
      text = function()
        local v = field(s, row, column.key)
        if format then return format(v) end
        return show(v)
      end,
      color = function() local p = P() return column.index == 1 and p.ink or p.ink_dim end }
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
  S.data_table = { header = header, cell = table_cell, empty = empty }

  --- A file list: a table whose name carries the file's type icon, its
  --- size in units and its date.
  local function file_cell(t, spec)
    return function(row, column, s)
      if column.key == "name" or column.index == 1 then
        return ui.Item { anchors = { fill = true },
          column.index == 1 and row_ground(t, s) or nil,
          ui.Item { x = 12, anchors = { vertical_center = true }, width = 22, height = 22,
            M.icon(function() return file_icon(s.row() or row) end, 20,
              function() return icon_color(file_icon(s.row() or row))() end, { anchors = { center_in = true } }) },
          cell_text(column, s, row, 42) }, function() end
      end
      local format = column.key == "size" and function(v)
        if is_folder(s.row() or row) then return field(s, row, "items") and (show(field(s, row, "items")) .. " items") or "" end
        return bytes(v)
      end or (column.key == "modified" and date) or nil
      return cell_text(column, s, row, nil, format), function() end
    end
  end
  S.file_list = { header = header, cell = file_cell, row = function(t, spec)
    -- (A file list without columns: a row of the icon and the name.)
    return function(row, s)
      local H = spec.row_height or 36
      return ui.Item { anchors = { fill = true },
        ui.Rect { anchors = { fill = true, margins = 1 }, radius = R.small, color = function() return row_tone(s) end,
          border_width = focus_edge(t, s), border_color = focus_color },
        M.icon(function() return file_icon(s.row() or row) end, 20,
          function() return icon_color(file_icon(s.row() or row))() end, { x = 12, anchors = { vertical_center = true } }),
        M.text { x = 42, y = (H - SZ.normal * 1.3) / 2, width = function() return s.width() - 54 end, elide = "right",
          text = function() return label_of(s.row() or row) end, font_size = SZ.normal,
          color = function() return P().ink end } }, function() end
    end
  end, empty = empty }

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
    parts[#parts + 1] = chevron(s, 18, function() return P().ink_dim end,
      { x = ix, visible = function() return s.expandable() end })
    parts[#parts + 1] = ui.Item { x = function() return ix() + 1 end, anchors = { vertical_center = true },
      width = 16, height = 16, visible = function() return s.loading() end,
      M.loading(16, function() return P().accent end, { active = function() return s.loading() end }) }
    parts[#parts + 1] = M.icon(function()
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
    parts[#parts + 1] = M.text { anchors = { vertical_center = true },
      x = function() return ix() + (s.loading() and 24 or 44) end,
      width = function() return math.max(0, width_of() - ix() - 52) end, elide = "right",
      text = function() return s.loading() and "Loading…" or label_of(s.row() or row) end, font_size = SZ.normal,
      color = function() local p = P() return s.loading() and p.ink_dim or p.ink end }
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
  S.tree_view = { row = tree_row, empty = empty }

  --- A tree table: the tree in its first column, the rest a table's cells.
  S.tree_table = {
    header = header,
    cell = function(t, spec)
      return function(row, column, s)
        if column.index == 1 then
          local node = ui.Item { anchors = { fill = true }, row_ground(t, s) }
          local width = function() return (column.width or 160) end
          for _, part in ipairs(tree_parts(t, s, row, 6, width)) do ui.reparent(part, node) end
          return node, function() end
        end
        return cell_text(column, s, row, nil, column.key == "size" and bytes or nil), function() end
      end
    end,
    empty = empty,
  }

  -- --------------------------------------------------------- timelines --

  local RAIL = 24
  --- A timeline: a rail down the left with a dot at each event -- the
  --- current one filled and haloed --, its time and title, a line under.
  S.timeline = {
    row = function(t, spec)
      local H = spec.row_height or 56
      local DOT = 12
      local cy = 18
      return function(row, s)
        local function r(key) return field(s, row, key) end
        local function on() return s.current() or s.selected() end
        local node = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 1 }, radius = R.small,
            color = function() local p = P() return s.hovered() and p.ink:alpha(p.wash.hover) or p.ink:alpha(0) end,
            border_width = focus_edge(t, s), border_color = focus_color },
          ui.Rect { x = RAIL - 1, y = 0, width = 2, height = cy, color = function() return P().border end,
            visible = function() return s.index() > 1 end },
          ui.Rect { x = RAIL - 1, y = cy, width = 2, height = H - cy, color = function() return P().border end,
            visible = function() return s.index() < s.count() end },
          ui.Rect { x = RAIL - DOT, y = cy - DOT, width = DOT * 2, height = DOT * 2, radius = DOT,
            color = function() return P().accent:alpha(0.22) end,
            scale = function() return on() and 1 or 0.4 end, opacity = function() return on() and 1 or 0 end,
            behavior = { scale = M.spring(380, 22), opacity = quick() } },
          ui.Rect { x = RAIL - DOT / 2, y = cy - DOT / 2, width = DOT, height = DOT, radius = DOT / 2,
            color = function()
              local p = P()
              if on() then return p.accent end
              return r("done") and p.ink_dim or p.view
            end,
            border_width = 2, border_color = function() local p = P() return on() and p.accent or p.ink_dim end,
            behavior = { color = quick() } },
          M.text { x = RAIL + 22, y = cy - SZ.normal * 0.68, width = function() return s.width() - RAIL - 110 end,
            elide = "right", text = function() return label_of(s.row() or row) end, font_size = SZ.normal,
            font_weight = function() return on() and 700 or 400 end, color = function() return P().ink end },
          M.text { anchors = { right = true, right_margin = 12 }, y = cy - SZ.small * 0.68,
            text = function() return tostring(r("time") or "") end, font_size = SZ.small,
            color = function() local p = P() return on() and p.accent_ink or p.ink_dim end },
          M.text { x = RAIL + 22, y = cy + 10, width = function() return s.width() - RAIL - 34 end, elide = "right",
            text = function() return tostring(r("detail") or r("subtitle") or "") end, font_size = SZ.small,
            color = function() return P().ink_dim end } }
        return node, function() end
      end
    end,
    empty = empty,
  }

  -- -------------------------------------------------------------- feed --

  --- A feed: a card per post -- the author's initial in a tinted disc,
  --- their name and the time, two lines of the post.
  S.feed = {
    row = function(t, spec)
      local H = spec.row_height or 92
      return function(row, s)
        local function r(key) return field(s, row, key) end
        local function author() return tostring(r("author") or r("from") or "") end
        local node = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, left_margin = 2, right_margin = 2, top_margin = 4, bottom_margin = 4 },
            radius = R.large, color = function()
              local p = P()
              if s.down() then return p.card:mix(p.ink, p.wash.active) end
              return s.hovered() and p.card:mix(p.ink, p.wash.hover) or p.card
            end,
            border_width = function()
              local p = P()
              if t.visual_focus and s.current() then return 2 end
              return (p.strong and 2) or (p.dark and 0) or 1
            end,
            border_color = function() local p = P() return t.visual_focus and s.current() and focus_color() or p.border end,
            shadow_color = function() local p = P() return p.dark and p.shade:alpha(0) or p.shade:alpha(0.45) end,
            shadow_blur = 3, shadow_offset_y = 1, behavior = { color = quick() } },
          ui.Rect { x = 14, y = 16, width = 36, height = 36, radius = 18,
            color = function() local p = P() return p[tone_of(author())]:alpha(p.dark and 0.35 or 0.2) end,
            M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
              text = function() return initial(author()) end, font_size = SZ.normal, font_weight = 700,
              color = function() local p = P() return p[tone_of(author()) .. "_ink"] or p.accent_ink end } },
          M.text { x = 60, y = 14, width = function() return s.width() - 140 end, elide = "right",
            text = author, font_size = SZ.normal, font_weight = 700, color = function() return P().ink end },
          M.text { anchors = { right = true, right_margin = 14 }, y = 16, text = function() return tostring(r("time") or "") end,
            font_size = SZ.small, color = function() return P().ink_dim end },
          M.text { x = 60, y = 36, width = function() return s.width() - 76 end, height = H - 46, wrap = true,
            max_lines = 2, text = function() return tostring(r("text") or r("body") or label_of(s.row() or row)) end,
            font_size = SZ.small, color = function() return P().ink end } }
        return node, function() end
      end
    end,
    empty = empty,
  }

  -- ---------------------------------------------------------- chat log --

  --- A chat log: bubbles, mine on the right in the accent, theirs on the
  --- left in the card tone, each with a squared corner toward its sender
  --- and its time under the text.
  S.chat_log = {
    row = function(t, spec)
      return function(row, s)
        local function r(key) return field(s, row, key) end
        local function mine() local f = r("from") return f == "me" or f == true or r("mine") == true end
        local function text() return tostring(r("text") or label_of(s.row() or row)) end
        -- Wrapped at the characters a line the list's estimate of the
        -- message's height counts on (lib.kit.collection: 7.4 px each over
        -- 72 % of the width), in this face's own advance, so the lines it
        -- reserved are the lines drawn; the bubble is as tall as they are.
        local CHAR = 7.4
        local function per_line() return math.max(8, math.floor((s.width() * 0.72 - 28) / 7.4)) end
        local function bw()
          local longest = 0
          for part in (text() .. "\n"):gmatch("([^\n]*)\n") do longest = math.max(longest, #part) end
          local time = tostring(r("time") or "")
          return math.max(64, #time * 8 + 28, math.ceil(math.min(per_line(), longest) * CHAR) + 26 + 6)
        end
        local function bx() return mine() and (s.width() - bw() - 8) or 8 end
        local body
        local function bh()
          local lines = body and body.layout_height or 0
          return math.min((s.area.height or 48) - 8, math.max(lines, 18) + 8 + 24)
        end
        local function fill()
          local p = P()
          if mine() then return p.accent end
          return p.dark and p.card or p.view:mix(p.ink, 0.07)
        end
        local function ink() local p = P() return mine() and p.on_accent or p.ink end
        body = M.text { x = 14, y = 8, width = function() return bw() - 26 end, wrap = true, text = text,
          font_size = SZ.small, color = ink }
        local node = ui.Item { anchors = { fill = true },
          ui.Item { x = bx, y = 4, width = bw, height = bh,
            scale = function() return s.down() and 0.98 or 1 end, behavior = { scale = M.spring(520, 30) },
            ui.Rect { anchors = { fill = true }, radius = 16, color = fill,
              border_width = function() return t.visual_focus and s.current() and 2 or 0 end, border_color = focus_color },
            -- The corner toward the sender, squared.
            ui.Rect { width = 14, height = 14, anchors = { bottom = true }, color = fill,
              x = function() return mine() and (bw() - 14) or 0 end },
            body,
            M.text { anchors = { right = true, bottom = true, right_margin = 12, bottom_margin = 6 },
              text = function() return tostring(r("time") or "") end, font_size = SZ.small,
              color = function() return get(ink()):alpha(0.7) end } } }
        return node, function() end
      end
    end,
    empty = empty,
  }

  -- ------------------------------------------------------------ kanban --

  --- A kanban column's cards: a raised card per task, its title and a
  --- tinted tag, a grip that shows under the pointer.
  S.kanban_column = {
    background = function(t, spec)
      if spec.delegate then return nil end
      return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() local p = P() return p.ink:alpha(0.04) end }
    end,
    row = function(t, spec)
      local H = spec.row_height or 64
      return function(row, s)
        local function r(key) return field(s, row, key) end
        local function tag() return r("tag") and tostring(r("tag")) or nil end
        local node = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 5 }, radius = R.medium,
            color = function()
              local p = P()
              return s.hovered() and p.card:mix(p.ink, p.wash.hover) or p.card
            end,
            border_width = function()
              local p = P()
              if (t.visual_focus and s.current()) or s.selected() then return 2 end
              return (p.strong and 2) or (p.dark and 0) or 1
            end,
            border_color = function()
              local p = P()
              if s.selected() or s.current() then return t.visual_focus and focus_color() or p.accent end
              return p.border
            end,
            shadow_color = function() local p = P() return p.shade:alpha(p.dark and 0.6 or 0.5) end,
            shadow_blur = function() return s.hovered() and 8 or 2 end, shadow_offset_y = function() return s.hovered() and 3 or 1 end,
            translate_y = function() return s.hovered() and -1 or 0 end,
            behavior = { color = quick(), shadow_blur = quick(), translate_y = quick() } },
          M.text { x = 16, y = 12, width = function() return s.width() - 48 end, elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.normal, color = function() return P().ink end },
          ui.Rect { x = 16, y = H - 30, height = 20, radius = 10,
            width = function() return #(tag() or "") * 7 + 18 end, visible = function() return tag() ~= nil end,
            color = function() local p = P() return p[tone_of(tag() or "")]:alpha(p.dark and 0.3 or 0.16) end,
            M.text { anchors = { fill = true }, horizontal_alignment = "center", vertical_alignment = "center",
              text = function() return tag() or "" end, font_size = SZ.small,
              color = function() local p = P() return p[tone_of(tag() or "") .. "_ink"] or p.accent_ink end } },
          M.icon("drag_indicator", 18, function() return P().ink_dim end,
            { anchors = { right = true, right_margin = 12, vertical_center = true },
              opacity = function() return s.hovered() and 1 or 0 end, behavior = { opacity = quick() } }) }
        return node, function() end
      end
    end,
    empty = empty,
  }

  -- ------------------------------------------------------ transfer list --

  --- One side of a transfer list: a check before each row, filled with the
  --- accent when the row is picked.
  S.transfer_list = {
    background = function(t, spec)
      if spec.delegate then return nil end
      return ui.Rect { anchors = { fill = true }, radius = R.large, color = function() return P().view end,
        border_width = function() return P().strong and 2 or 1 end, border_color = function() return P().border end }
    end,
    row = function(t, spec)
      local H = spec.row_height or 36
      return function(row, s)
        local function on() return s.selected() end
        local node = ui.Item { anchors = { fill = true },
          ui.Rect { anchors = { fill = true, margins = 1 }, radius = R.small,
            color = function() return row_tone({ selected = function() return false end, current = function() return false end,
              hovered = s.hovered, down = s.down, index = s.index }) end,
            border_width = focus_edge(t, s), border_color = focus_color },
          ui.Rect { x = 12, y = (H - 18) / 2, width = 18, height = 18, radius = 5,
            color = function() local p = P() return on() and p.accent or p.ink:alpha(0) end,
            border_width = function() return on() and 0 or 2 end, border_color = function() return P().ink_dim:alpha(0.7) end,
            behavior = { color = quick() },
            M.icon("check", 16, function() return P().on_accent end,
              { anchors = { center_in = true }, scale = function() return on() and 1 or 0.4 end,
                opacity = function() return on() and 1 or 0 end, behavior = { scale = M.spring(520, 24), opacity = quick() } }) },
          M.text { x = 42, y = (H - SZ.normal * 1.3) / 2, width = function() return s.width() - 54 end, elide = "right",
            text = function() return label_of(s.row() or row) end, font_size = SZ.normal,
            color = function() return P().ink end } }
        return node, function() end
      end
    end,
    empty = empty,
  }
  -- Every row builder here carries how its rows come and go
  -- (lib.kit.collection reads `motion` off a builder given as a table).
  local CHAT = { enter = { opacity = 0, translate_y = 14, scale = 0.96, duration = 300, easing = "out_back" },
    exit = MOTION.exit }
  for _, name in ipairs { "list", "virtual_list", "list_box", "boxed_list", "grid_view", "flow_box", "file_list",
    "tree_view", "timeline", "feed", "chat_log", "kanban_column", "transfer_list" } do
    local build = S[name].row
    S[name].row = function(t, spec)
      local fn = build(t, spec)
      return setmetatable({ motion = name == "chat_log" and CHAT or MOTION },
        { __call = function(_, row, s) return fn(row, s) end })
    end
  end
end
