-- Material launcher view. Search, navigation and activation belong to its controller.
local morf = require("morf")
local ui = require("morf.ui")
local theme = require("theme")
local kit = require("kit")
local C = theme.color
local V = {}
function V.build(M)
  local row_of = M.row_of
  local mode_name = M.mode_name
  local WIDTH, WIDE = 720, 1270
  local PAD = 8
  local ROW, ROW_GAP = 50, 2
  local HEADER = 30
  local HERO = 116
  local SEARCH = 60
  local FOOTER = 38
  local EMPTY = 90
  local CAROUSEL = 203
  local SLOT, SLOT_ON = 248, 304

  local function entry_height(entry)
    if not entry then return ROW end
    if entry.kind == "header" then return HEADER end
    if entry.kind == "hero" then return HERO end
    return ROW
  end

  --- Where entry `index` starts in the list, and how tall it is.
  local function entry_span(index)
    local y = 0
    for i = 1, index - 1 do y = y + entry_height(M.results:get(i)) + ROW_GAP end
    return y, entry_height(M.results:get(index))
  end

  local function list_height()
    local n = M.count:get()
    if n <= 0 then return EMPTY end
    local y, h = entry_span(n)
    return y + h
  end

  local function wide() return M.mode:get() == "wallpapers" end

  local function width() return wide() and WIDE or WIDTH end

  local function height()
    local body = wide() and CAROUSEL or list_height()
    return SEARCH + PAD + body + PAD + FOOTER
  end

  local function row_icon(row)
    if row.glyph then
      return kit.centred(32, 32, kit.text { text = row.glyph, font_size = 26 })
    end
    if row.swatch then
      local ok, color = pcall(morf.color, row.swatch)
      return kit.centred(32, 32, kit.surface {
        width = 28, height = 28, radius = 14, color = ok and color or row.swatch,
        border_width = 2, border_color = function() return C.outlineVariant end,
      })
    end
    if row.kind == "action" or row.kind == "variant" or row.material then
      return kit.centred(32, 32, kit.icon(row.material or row.icon, 34, function() return C.onSurfaceVariant end))
    end
    if row.kind == "calc" then
      return kit.centred(32, 32, kit.icon("function", 36, function() return C.onSurface end))
    end
    if row.kind == "scheme" then
      if not row.color then
        return kit.centred(32, 32, kit.icon("wallpaper", 30, function() return C.onSurfaceVariant end))
      end
      local ok, color = pcall(morf.color, row.color)
      return kit.centred(32, 32, kit.surface {
        width = 28, height = 28, radius = 14, color = ok and color or row.color,
        border_width = 2, border_color = function() return C.outlineVariant end,
      })
    end
    local hit = M.icon(row.icon)
    if hit and hit.name then
      return ui.Icon { width = 32, height = 32, name = hit.name, source_width = 64, source_height = 64 }
    elseif hit and hit.path then
      return ui.Image { width = 32, height = 32, source = hit.path, fill_mode = "preserve_aspect_fit" }
    end
    return kit.centred(32, 32, kit.icon("apps", 28, function() return C.onSurfaceVariant end))
  end

  local function is_selected(key)
    local sel = M.results:get(M.selected:get())
    return sel and sel.key == key
  end

  local function header(entry)
    return ui.Item {
      id = "launcher-header-" .. entry.key,
      width = WIDTH - 2 * PAD, height = HEADER,
      enter = { opacity = 0, duration = theme.duration.small },
      kit.text {
        x = 14, anchors = { bottom = true, bottom_margin = 6 },
        text = entry.name, font_size = theme.size.small, font_weight = 600,
        color = function() return C.onSurfaceVariant end,
      },
    }
  end

  --- An answer, as Raycast shows one: the question in a box, an arrow, the
  --- answer in large type in another.
  local function hero(entry)
    local row = row_of(entry) or entry
    local box_w = (WIDTH - 2 * PAD - 56) / 2
    local function box(x, big, label, id)
      return kit.surface {
        x = x, y = 6, width = box_w, height = HERO - 12, radius = 16,
        color = function() return C.surfaceContainerHigh end,
        ui.Column {
          anchors = { center_in = true }, gap = 6, align = "center",
          kit.text {
            id = id, text = big, width = box_w - 24, horizontal_alignment = "center", elide = "middle",
            font_size = id and 30 or 19, font_weight = id and 700 or 500,
            color = function() return id and C.onSurface or C.onSurfaceVariant end,
          },
          kit.text {
            text = label, font_size = theme.size.small,
            color = function() return C.onSurfaceVariant end,
          },
        },
      }
    end
    local question = row.question or row.description or ""
    return kit.action {
      id = "launcher-row-" .. entry.key,
      width = WIDTH - 2 * PAD, height = HERO, cursor = "pointer",
      enter = { opacity = 0, scale = 0.97, duration = theme.duration.small, easing = theme.ease.standard_decel },
      on_clicked = function() M.activate(row) end,
      box(0, question, row.material == "currency_exchange" and "Amount" or "Question"),
      kit.icon("arrow_forward", 26, function() return C.onSurfaceVariant end, {
        x = box_w + 15, y = (HERO - 26) / 2,
      }),
      box(box_w + 56, row.name, "Return copies it", "launcher-answer"),
    }
  end

  local function delegate(entry)
    if entry.kind == "header" then return header(entry) end
    if entry.kind == "hero" then return hero(entry) end
    local row = row_of(entry) or entry
    local body
    if row.kind == "calc" then
      -- One line: the expression and its answer, and a button that copies
      -- the answer.
      body = {
        ui.Row {
          x = 12, y = (ROW - 32) / 2, gap = 17, align = "center",
          row_icon(row),
          kit.text {
            id = "launcher-calc", text = row.name, font_size = theme.size.normal + 1,
            width = WIDTH - 2 * PAD - 150, elide = "right",
            color = function() return row.failed and C.onSurfaceVariant or C.onSurface end,
          },
        },
        kit.hover(kit.action {
          id = "launcher-calc-copy",
          anchors = { right = true, right_margin = 12, vertical_center = true },
          width = 54, height = 44, cursor = "pointer",
          visible = not row.failed,
          on_clicked = function() M.activate(row) end,
          kit.icon("open_in_new", 24, function() return C.onTertiaryContainer end, { anchors = { center_in = true } }),
        }, function(hovered)
          return hovered and C.tertiaryContainer:mix(C.onTertiaryContainer, 0.08) or C.tertiaryContainer
        end, 12),
      }
    else
      body = {
        ui.Row {
          x = 12, y = (ROW - 32) / 2, gap = (row.kind == "app") and 13 or 17,
          align = "center",
          row_icon(row),
          ui.Column {
            gap = 3,
            kit.text { id = "launcher-name", text = row.name, font_size = theme.size.larger, color = function() return C.onSurface end },
            kit.text {
              text = row.description, font_size = theme.size.smaller,
              color = function() return C.onSurfaceVariant end,
              width = WIDTH - 2 * PAD - 80 - (row.current and 30 or 0), elide = "right",
            },
          },
        },
        row.current and kit.icon("check", 22, function() return C.primary end, {
          anchors = { right = true, right_margin = 16, vertical_center = true },
        }) or nil,
      }
    end
    local area = kit.action {
      id = "launcher-row-" .. entry.key,
      enter = { opacity = 0, scale = 0.96, duration = theme.duration.small, easing = theme.ease.standard_decel },
      exit = { opacity = 0, scale = 0.96, duration = 150, easing = theme.ease.standard_accel },
      width = WIDTH - 2 * PAD, height = ROW, cursor = "pointer",
      on_entered = function()
        for i = 1, M.results:len() do
          if M.results:get(i).key == entry.key then M.selected:set(i) end
        end
      end,
      on_clicked = function() M.activate(row) end,
      kit.surface {
        anchors = { fill = true },
        radius = 14,
        -- The selection is drawn once, under the rows (the highlight below).
        color = function()
          if row.kind == "calc" then return C.surfaceContainer end
          return C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      },
      table.unpack(body),
    }
    return area
  end

  -- --------------------------------------------------------------- carousel --

  local MAX_SLOTS = 64

  local function carousel()
    local slots = {}
    for i = 1, MAX_SLOTS do
      local function wall() return i <= M.wall_count:get() and M.walls[i] or nil end
      local function on() return M.selected:get() == i end
      -- Only pictures near the chosen one are loaded.
      local function near() return math.abs(M.selected:get() - i) <= 4 end
      local motion = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel }
      local slot
      slot = kit.action {
        id = "launcher-wallpaper-" .. i,
        height = CAROUSEL, cursor = "pointer",
        width = function() return on() and SLOT_ON or SLOT end,
        visible = function() return wall() ~= nil end,
        behavior = { width = motion },
        on_clicked = function()
          if on() then
            local w = wall()
            if w then M.activate(w) end
          else
            M.selected:set(i)
          end
        end,
        ui.Column {
          anchors = { horizontal_center = true }, gap = 6, align = "center",
          y = function() return on() and 14 or 32 end,
          behavior = { y = motion },
          kit.surface {
            radius = 10, clip = true,
            width = function() return on() and 280 or 224 end,
            height = function() return on() and 158 or 126 end,
            behavior = { width = motion, height = motion },
            color = function() return C.surfaceContainerHigh end,
            ui.Image {
              anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
              source = function()
                local w = wall()
                return (w and near()) and w.path or ""
              end,
            },
          },
          kit.text {
            text = function() local w = wall() return w and w.name or "" end,
            font_size = function() return on() and theme.size.normal + 1 or theme.size.small end,
            font_weight = 500,
            width = function() return on() and 280 or 224 end,
            horizontal_alignment = "center", elide = "right",
          },
        },
      }
      local wash = kit.surface {
        anchors = { fill = true, top_margin = 3 }, radius = 12, z = -1,
        color = function()
          if on() then return C.onSurface:alpha(0.07) end
          return slot.hovered and C.onSurface:alpha(0.04) or C.onSurface:alpha(0)
        end,
        behavior = { color = { duration = theme.duration.small } },
      }
      ui.reparent(wash, slot)
      slots[i] = slot
    end
    local row = ui.Row {
      gap = 0,
      translate_x = function()
        local sel = math.max(1, M.selected:get())
        return (WIDE - 2 * PAD) / 2 - ((sel - 1) * SLOT + SLOT_ON / 2)
      end,
      behavior = { translate_x = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel } },
      table.unpack(slots),
    }
    return ui.Item {
      id = "launcher-wallpapers",
      x = PAD, y = SEARCH + PAD, width = WIDE - 2 * PAD, height = CAROUSEL, clip = true,
      visible = wide,
      row,
      kit.text {
        anchors = { center_in = true }, text = "No wallpapers in ~/Pictures/Wallpapers",
        font_size = theme.size.larger, color = function() return C.onSurfaceVariant end,
        visible = function() return M.wall_count:get() == 0 end,
      },
    }
  end

  local field = ui.TextInput {
    id = "launcher-search",
    tab_navigation = false,
    height = SEARCH,
    anchors = { left = true, right = true, left_margin = 56, right_margin = 52 },
    vertical_alignment = "center",
    font_family = theme.font, font_size = 21,
    color = function() return C.onSurface end,
    placeholder = "Search apps, files, the web, windows…",
    placeholder_color = function() return C.onSurfaceVariant end,
    caret_color = function() return C.onSurface end,
    selection_color = function() return C.primary:alpha(0.4) end,
    on_text_changed = function(text) M.query:set(text) end,
    on_accepted = function() M.activate(M.chosen()) end,
    on_escape = M.escape,
    on_key_pressed = M.key,
    on_key_released = M.release,
  }
  local clear
  clear = kit.action {
    id = "launcher-clear",
    width = 36, height = 36, cursor = "pointer",
    anchors = { right = true, right_margin = 12, top = true, top_margin = (SEARCH - 36) / 2 },
    visible = function() return M.query:get() ~= "" end,
    on_clicked = function() field.text = "" M.query:set("") end,
    kit.icon("close", 20, function() return C.onSurfaceVariant end, { anchors = { center_in = true } }),
  }

  local empty = ui.Row {
    id = "launcher-empty",
    anchors = { center_in = true }, gap = 14, align = "center",
    visible = function() return M.count:get() == 0 end,
    kit.icon("manage_search", 40, function() return C.onSurfaceVariant end),
    ui.Column {
      gap = 0,
      kit.text { text = "No results", font_size = theme.size.large, color = function() return C.onSurface end },
      kit.text {
        text = "Try searching for something else", font_size = theme.size.larger,
        color = function() return C.onSurfaceVariant end,
      },
    },
  }

  -- The selection: one rounded box in a distance field under the rows,
  -- tracking an item that springs from row to row -- sliding, squashing and
  -- stretching on the way -- rather than a highlight that jumps.
  local highlight = ui.Item {
    id = "launcher-highlight",
    x = 0, width = WIDTH - 2 * PAD,
    y = function() return (entry_span(math.max(1, M.selected:get()))) end,
    height = function() local _, h = entry_span(math.max(1, M.selected:get())) return h end,
    behavior = { y = kit.spring(380, 26), height = kit.spring(380, 26) },
    stretch = { stiffness = 300, damping = 15, scale = 0.1, max = 0.22 },
    visible = function() return M.count:get() > 0 end,
  }
  local selection = ui.Sdf {
    id = "launcher-selection",
    anchors = { fill = true }, z = -1,
    ui.SdfShape {
      shape = "box", radius = 12, track = highlight,
      fill_color = function() return C.onSurface:alpha(0.15) end,
    },
  }

  -- The bar at the foot: what is being searched, and what Return and Tab do.
  local function key_hint(key, label)
    return ui.Row {
      gap = 6, align = "center",
      kit.text { text = label, font_size = theme.size.small, color = function() return C.onSurfaceVariant end },
      kit.surface {
        height = 22, width = math.max(26, #key * 9 + 12), radius = 6,
        color = function() return C.surfaceContainerHighest end,
        kit.text { anchors = { center_in = true }, text = key, font_size = theme.size.small, font_weight = 600,
          color = function() return C.onSurface end },
      },
    }
  end
  -- Tab's hint, there only while the chosen row has other actions.
  local tab_hint = key_hint("Tab", "Actions")
  tab_hint.visible = function()
    local sel = row_of(M.results:get(M.selected:get()))
    return sel ~= nil and sel.actions ~= nil and M.acting:get() == ""
  end
  local footer = ui.Item {
    id = "launcher-footer",
    anchors = { left = true, right = true, bottom = true }, height = FOOTER,
    kit.surface { anchors = { left = true, right = true, top = true }, height = 1, color = function() return C.outlineVariant:alpha(0.5) end },
    ui.Row {
      x = 16, anchors = { vertical_center = true }, gap = 8, align = "center",
      kit.icon(function() return select(2, mode_name()) end, 18, function() return C.primary end),
      kit.text { text = function() return (mode_name()) end, font_size = theme.size.small, font_weight = 600,
        color = function() return C.onSurface end },
      kit.text {
        text = function()
          if M.query:get() ~= "" or M.acting:get() ~= "" then return "" end
          return "   = calc   / files   ? web   @ windows   ! system   : emoji"
        end,
        font_size = theme.size.small, color = function() return C.onSurfaceVariant end,
      },
    },
    ui.Row {
      anchors = { right = true, right_margin = 12, vertical_center = true }, gap = 14, align = "center",
      key_hint("↵", "Open"),
      tab_hint,
      key_hint("esc", function() return M.acting:get() ~= "" and "Back" or "Close" end),
    },
  }

  local content = ui.Item {
    anchors = { fill = true },
    -- The search, on top, in large type.
    ui.Item {
      id = "launcher-field",
      anchors = { left = true, right = true, top = true }, height = SEARCH,
      kit.icon("search", 24, function() return C.onSurfaceVariant end, { x = 20, y = (SEARCH - 24) / 2 }),
      field,
      clear,
    },
    kit.surface { anchors = { left = true, right = true }, y = SEARCH, height = 1, color = function() return C.outlineVariant:alpha(0.5) end },
    -- The results, down from the search.
    ui.Item {
      y = SEARCH + PAD, width = WIDTH - 2 * PAD,
      anchors = { horizontal_center = true },
      height = function() return list_height() end,
      visible = function() return not wide() end,
      clip = true,
      selection,
      highlight,
      ui.Repeater {
        as = "column", gap = ROW_GAP,
        model = M.results,
        delegate = delegate,
      },
      empty,
    },
    carousel(),
    footer,
  }


  local function top_margin()
    local _, _, _, h = require("bar").desk()
    return math.floor(h * 0.16)
  end
  return {
    content = content, width = width, height = height,
    set_query = function(text) field.text = text field.cursor_position = #text end,
    focus = function(on) field.focus = on end,
    props = {
      anchors = { top = true, horizontal_center = true, top_margin = top_margin() },
      behavior = {
        width = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel },
        height = { duration = theme.duration.normal, easing = theme.ease.emphasized_decel },
      },
    },
  }
end
return V
