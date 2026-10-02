-- Compact dashboard layout: wallpaper and palette above one control strip.
local morf = require("morf")
local ui = require("morf.ui")
local kit = require("kit")
local theme = require("theme")
local C = theme.color
local M = { WIDTH = 960, HEIGHT = 545 }
local function text(value, props)
  props = props or {} props.text = value
  return kit.text(props)
end
local function label(value, props)
  props = props or {} props.font_size = props.font_size or 12
  props.color = props.color or function() return C.onSurfaceVariant end
  props.text = value
  return kit.subtitle(props)
end
local function basename(path) return path:match("([^/]+)$") or path end
function M.build(state)
  local appearance=state.appearance
  local w,h=M.WIDTH,function() return M.HEIGHT end
  local function button(id, title, icon, width, action, selected, height)
    local node = kit.pill { id = id, label = title, icon = icon, width = width, height = height or 32,
      on_clicked = function() if not state.busy:get() and not appearance.busy:get() then action() end end,
      color = function() return selected and selected() and C.primary or C.surfaceContainerHighest end,
      ink = function() return selected and selected() and C.onPrimary or C.onSurface end }
    node.opacity = function() return (state.busy:get() or appearance.busy:get()) and 0.45 or 1 end
    return node
  end
  local GAP, PAD, TOP = 12, 16, 354
  local left = math.floor((w - GAP) * 0.60)
  local right = w - left - GAP
  local inner = right - 2 * PAD
  local color = state.color
  local function swatch(id, caption, value, width, height)
    local area
    local function ink()
      return morf.color(value()):text_color()
    end
    area = kit.action { id = id, width = width, height = height, cursor = "pointer",
      on_clicked = function() state.copy(value()) end,
      kit.surface { anchors = { fill = true }, radius = function() return area and area.hovered and 9 or 14 end,
        color = function() return morf.color(value()) end,
        behavior = { radius = kit.spring(420, 24), color = { duration = 220 } },
        text(caption, { x = 8, y = 5, font_size = 10, font_weight = 600, color = ink }),
        text(value, { x = 8, y = 22, font_size = 11, color = ink }),
      },
      (function()
        local mark = kit.decor("corners", { length = 5, color = function() return ink():alpha(0.8) end })
        if mark then mark.visible = function() return area and area.hovered or false end return mark end
        return ui.Item {}
      end)(),
    }
    return area
  end
  local colors = { x = PAD, y = 47, gap = 8, width = inner }
  for row = 0, 3 do
    local line = { gap = 6 }
    for col = 0, 3 do
      local n = row * 4 + col
      line[#line + 1] = swatch("lule-color-" .. n, string.format("%02d", n), function() return color(n) end,
        (inner - 18) / 4, 50)
    end
    colors[#colors + 1] = ui.Row(line)
  end
  local special = { x = PAD, y = 285, gap = 6 }
  for _, entry in ipairs { { "background", "Background" }, { "foreground", "Text" }, { "cursor", "Cursor" } } do
    local key, name = entry[1], entry[2]
    special[#special + 1] = swatch("lule-" .. key, name, function() return color(key) end, (inner - 12) / 3, 49)
  end
  local folder_w = left - 2 * PAD - 212
  local field = ui.TextInput { id = "lule-folder", width = folder_w - 20, height = 30, x = 10,
    font_family = theme.font, font_size = 12, placeholder = "~/Pictures/Wallpapers",
    color = function() return C.onSurface end, placeholder_color = function() return C.onSurfaceVariant end,
    caret_color = function() return C.primary end, selection_color = function() return C.primary:alpha(0.25) end,
    on_text_changed = function(value) if state.folder_draft then state.folder_draft:set(value) end end,
    on_accepted = function(value) state.set_folder(value) end,
    on_escape = state.escape }
  morf.effect("caelestia.lule.folder-field", function() field.text = (state.folder_draft and state.folder_draft:get()) or state.folder:get() end)
  local use_folder = button("lule-use-folder", "Use folder", nil, 96,
    function() state.set_folder(field.text) end, nil, 30)
  use_folder.x, use_folder.y = left - PAD - 96, 275
  local browser_rows = { gap = 3 }
  local PAGE_SIZE = 4
  for i = 1, PAGE_SIZE do
    local function file() return state.files:get()[(state.page:get() - 1) * PAGE_SIZE + i] end
    local area
    area = kit.action { id = "lule-file-" .. i, width = left - 2 * PAD - 16, height = 26, cursor = "pointer",
      visible = function() return file() ~= nil end,
      on_clicked = function() local item = file() if item then state.select(item.path) end end,
      kit.icon("image", 16, kit.ink("accent"), { x = 8, y = 5 }),
      kit.menu_label { text = function() local item = file() return item and item.name or "" end,
        x = 30, y = 5, width = left - 2 * PAD - 55, elide = "right", font_size = 12 },
    }
    kit.hover(area, function(hovered) return hovered and C.primaryContainer or C.surfaceContainerHigh end, 8)
    browser_rows[#browser_rows + 1] = area
  end
  local library = kit.card { id = "lule-browser", x = PAD, y = 44, width = left - 2 * PAD, height = 227,
    radius = 18, color = function() return C.surfaceContainer end, visible = function() return state.browsing:get() end,
    ui.Column { x = 8, y = 8, width = left - 2 * PAD - 16, gap = 5,
      label(function() return #state.files:get() == 0 and "No images in this folder"
        or #state.files:get() .. " wallpapers · Choose a preview" end,
        { width = left - 2 * PAD - 16, height = 15, elide = "middle", font_size = 11 }),
      ui.Item { width = left - 2 * PAD - 16, height = 113, ui.Column(browser_rows) },
      ui.Row { gap = 8,
        button("lule-files-prev", "Back", "chevron_left", 80,
          function() state.page_by(-1,PAGE_SIZE) end, nil, 26),
        button("lule-files-next", "More", "chevron_right", 80,
          function() state.page_by(1,PAGE_SIZE) end, nil, 26),
        label(function() return state.page:get() .. " / " .. math.max(1, math.ceil(#state.files:get() / PAGE_SIZE)) end, { y = 6 }),
      },
    },
  }
  local left_card = kit.card { id = "lule-wallpaper-card", width = left, height = TOP, radius = 24,
    kit.heading { id = "lule-wallpaper-heading", text = "Wallpaper", x = PAD, y = 11, width = 190,
      font_size = 18, font_weight = 600, active = function() return state.active:get() end },
    kit.chip { anchors = { right = true, right_margin = PAD }, y = 17, width = 64,
      text = function()
        local scheme = state.scheme:get() or {}
        return state.selected:get() == scheme.wallpaper and "CURRENT" or "PREVIEW"
      end },
    kit.surface { x = PAD, y = 44, width = left - 2 * PAD, height = 199, radius = 18, clip = true,
      color = function() return C.surfaceContainerLowest end,
      ui.Image { id = "lule-preview", anchors = { fill = true }, fill_mode = "preserve_aspect_crop",
        source = function() return state.preview:get() end },
      kit.decor("corners", { length = 10, color = kit.stroke("mark") }) or ui.Item {},
      ui.Column { anchors = { center_in = true }, align = "center", gap = 8,
        visible = function() return state.preview:get() == "" end,
        kit.icon("wallpaper", 32, kit.ink("accent")),
        label(function() return state.preview_error:get() ~= "" and "Preview unavailable"
          or state.selected:get() == "" and "Choose a wallpaper" or "Preparing preview…" end),
      },
    },
    kit.menu_label { text = function() return basename(state.selected:get()) ~= "" and basename(state.selected:get()) or "Your wallpaper collection" end,
      x = PAD, y = 252, width = left - 2 * PAD, elide = "middle", font_size = 13, font_weight = 600 },
    kit.label { text = "Folder", x = PAD, y = 281, width = 104, height = 18, elide = "right",
      font_size = 11, vertical_alignment = "center", color = kit.ink("lo") },
    kit.surface { x = PAD + 110, y = 275, width = folder_w, height = 30, radius = 10,
      color = function() return C.surfaceContainerHighest end, field,
      kit.decor("corners", { length = 5, color = kit.stroke("mark") }) },
    use_folder,
    ui.Row { x = PAD, y = 313, gap = 6,
      button("lule-browse", "Images", "folder_open", 100, state.browse),
      button("lule-shuffle", "Shuffle", "shuffle", 100, state.shuffle),
      button("lule-random-apply", "Random & apply", "auto_awesome", left - 2 * PAD - 288, state.random_apply, function() return true end),
      button("lule-prev", "", "chevron_left", 32, function() state.step(-1) end),
      button("lule-next", "", "chevron_right", 32, function() state.step(1) end),
    },
    library,
  }
  local palette_card = kit.card { id = "lule-colors-card", x = left + GAP, width = right, height = TOP, radius = 24,
    kit.heading { id = "lule-colors-heading", text = "Colors", x = PAD, y = 11, width = 110,
      font_size = 18, font_weight = 600, active = function() return state.active:get() end },
    label("Click a swatch to copy", { anchors = { right = true, right_margin = PAD }, y = 16, font_size = 11 }),
    ui.Column(colors), ui.Row(special),
  }
  local mode_w, apply_w = 160, 200
  local methods_w = w - 2 * PAD - mode_w - apply_w - 24
  local modes = { x = PAD, y = 35, gap = 6 }
  for _, mode in ipairs { "dark", "light" } do
    modes[#modes + 1] = button("lule-mode-" .. mode, mode == "dark" and "Dark" or "Light",
      mode == "dark" and "dark_mode" or "light_mode", (mode_w - 6) / 2,
      function() state.set_mode(mode) end, function() return state.mode:get() == mode end)
  end
  local methods = { x = PAD + mode_w + 12, y = 35, gap = 6 }
  for _, method in ipairs { "pigment", "median", "histogram", "tonal" } do
    methods[#methods + 1] = button("lule-method-" .. method, method:gsub("^%l", string.upper), nil,
      (methods_w - 18) / 4, function() state.set_method(method) end, function() return state.method:get() == method end)
  end
  local apply = button("lule-apply", function() return state.busy:get() and "Applying…" or "Apply wallpaper & colors" end,
    nil, apply_w, state.apply, function() return true end)
  apply.x, apply.y = w - PAD - apply_w, 35
  -- Three themes between the row's title and the font picker at 510.
  local styles={x=PAD+160+12,y=118,gap=8}
  local style_w=math.floor((510-12-styles.x-2*styles.gap)/3)
  for _,style in ipairs {{"material","Material"},{"tsugumori","Tsugumori"}} do
    styles[#styles+1]=button("lule-theme-"..style[1],style[2],nil,style_w,
      function() appearance.request(style[1]) end,
      function() return require("themes").current.id==style[1] end,34)
  end
  local controls = kit.card { id = "lule-controls", y = TOP + GAP, width = w, height = function() return h() - TOP - GAP end, radius = 24,
    kit.heading { id = "lule-appearance-title", text = "Appearance", level = "caption", active = function() return state.active:get() end, x = PAD, y = 12, font_size = 12, color = function() return C.onSurfaceVariant end },
    kit.heading { id = "lule-method-title", text = "Palette method", level = "caption", active = function() return state.active:get() end, x = PAD + mode_w + 12, y = 12, font_size = 12, color = function() return C.onSurfaceVariant end },
    kit.subtitle { text = "Preview first, then apply", x = w - PAD - apply_w, y = 12, font_size = 12, color = function() return C.onSurfaceVariant end },
    ui.Row(modes), ui.Row(methods), apply,
    kit.heading {id="lule-theme-title",text="Shell theme",level="caption",x=PAD,y=129,font_size=12,
      active=function() return state.active:get() end,color=function() return C.onSurfaceVariant end},
    ui.Row(styles),
    kit.heading {id="lule-font-title",text="Font",level="caption",x=510,y=129,font_size=12,
      active=function() return state.active:get() end,color=function() return C.onSurfaceVariant end},
    ui.Item {x=582,y=118,width=362,height=34,
      button("lule-font",function() local name=require("themes").font return name and name~="" and name or "Theme default" end,
        "expand_more",362,function() require("themes.fonts").open() end,nil,34)},
    kit.subtitle { text = function() return appearance.message:get() ~= "" and appearance.message:get() or state.message:get() end, id = "lule-status", x = PAD, y = 79, width = w - 2 * PAD, height = 32,
      wrap = true, font_size = 12, color = function() return state.failed:get() and C.error or C.onSurfaceVariant end },
  }
  return {page=ui.Item { id = "lule-page", width = w, height = h, left_card, palette_card, controls,
    require("themes.layouts.font_picker")(state) }}
end
return M
