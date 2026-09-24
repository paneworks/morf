-- The board: the tasks as a kanban -- to do, doing, done.
--
-- Port of BoardPanel.qml. A card per task, dragged between and within
-- lanes, and New at the top of the first lane. Opening a card fills the
-- panel with the task: its line, its notes, its day (a month opens over the
-- day's button) and its lane.
--
-- Opens with the keyboard ring on New. Arrows move between cards, Enter
-- opens one, Space moves it a lane along, Escape closes. In a task, Escape
-- goes back to the board when the task was opened there, and closes the
-- island when it came from elsewhere. A task closed without a line is
-- dropped.

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local tasks = require("services.tasks")
local notes = require("services.notes")
local kit = require("components.kit")
local pill = require("components.pill")
local day_picker = require("components.day_picker")

local C = theme.color

local KEY = {
  escape = 0xff1b, enter = 0xff0d, kp_enter = 0xff8d, space = 0x20, tab = 0xff09,
  left = 0xff51, up = 0xff52, right = 0xff53, down = 0xff54,
}

local PAD = theme.panel_padding
local GAP = 12
local ROOM_W = tasks.panel_width - 2 * PAD
local ROOM_H = tasks.panel_height - 2 * PAD
local FOOTER = 18
local LANES_H = ROOM_H - GAP - FOOTER
local LANE_W = (ROOM_W - (#tasks.states - 1) * GAP) / #tasks.states
local LIST_Y = 12 + 18 + 10
local LIST_H = LANES_H - LIST_Y - 8
local CARD_W = LANE_W - 16
local NEW_H = 46

local function lane_x(lane) return (lane - 1) * (LANE_W + GAP) end

-- ------------------------------------------------------------ shared state --

-- Whether the month is open over the day's button, for the open task.
local picking = morf.signal("impasto.board.picking", false)

-- Leaving the panel leaves the task (dropping it if it has no line).
local was_open = false
morf.effect("impasto.board.leave", function()
  local open = island.state.open_panel() == "board"
  if was_open and not open then
    morf.timer(1, function()
      if island.state.open_panel() ~= "board" then tasks.leave() end
    end, false)
  end
  was_open = open
end)
morf.effect("impasto.board.picking", function()
  tasks.opened()
  picking:set(false)
end)

local function back()
  if tasks.direct() then island.close() else tasks.leave() end
end

-- One model per lane, for every build of the board, kept current here.
local lane_models = {}
for i, state in ipairs(tasks.states) do
  local function rows()
    local out = {}
    for _, task in ipairs(tasks.in_state(state.id)) do out[#out + 1] = { key = task.key } end
    return out
  end
  lane_models[i] = morf.list_model(rows())
  morf.effect("impasto.board.lane." .. state.id, function() lane_models[i]:replace(rows(), "key") end)
end

local built = 0
local function fresh(name, value)
  built = built + 1
  return morf.signal("impasto.board." .. name .. "." .. built, value)
end

-- ------------------------------------------------------------------ board --

local function build_board()
  local cursor_lane = fresh("cursor_lane", 1)
  local cursor_index = fresh("cursor_index", 0) -- 0 is New, in the first lane
  local walking = fresh("walking", true)
  local dragging = fresh("dragging", "")
  local drop_lane = fresh("drop_lane", 0)
  local drop_index = fresh("drop_index", 0)
  local drop_y = fresh("drop_y", 0)
  local ghost_x = fresh("ghost_x", 0)
  local ghost_y = fresh("ghost_y", 0)
  local scrolls = {}
  for i = 1, #tasks.states do scrolls[i] = fresh("scroll" .. i, 0) end
  local card_nodes = {}

  local function lane_cards(lane)
    local state = tasks.states[lane]
    return state and tasks.in_state(state.id) or {}
  end

  local function lowest(lane) return lane == 1 and 0 or 1 end

  local function walk_lane(delta)
    walking:set(true)
    local lane = math.max(1, math.min(#tasks.states, cursor_lane:get() + delta))
    cursor_lane:set(lane)
    cursor_index:set(math.max(lowest(lane), math.min(#lane_cards(lane), cursor_index:get())))
  end

  local function walk_card(delta)
    walking:set(true)
    local lane = cursor_lane:get()
    cursor_index:set(math.max(lowest(lane), math.min(#lane_cards(lane), cursor_index:get() + delta)))
  end

  local function card_under_cursor()
    return lane_cards(cursor_lane:get())[cursor_index:get()]
  end

  local function open_current()
    if cursor_lane:get() == 1 and cursor_index:get() == 0 then tasks.create(true) return end
    local card = card_under_cursor()
    if card then tasks.open(card.key, true) end
  end

  -- ------------------------------------------------------------- dragging --

  -- Where a lane's cards start in its list: under New in the first lane.
  local function base(lane) return lane == 1 and NEW_H or 0 end

  -- `x`, `y` in the lanes' coordinates.
  local function aim(x, y)
    local lane = math.max(1, math.min(#tasks.states, math.floor(x / (LANE_W + GAP)) + 1))
    local in_list = y - LIST_Y + scrolls[lane]:get()
    local index, line = 1, base(lane)
    for _, other in ipairs(lane_cards(lane)) do
      local node = card_nodes[other.key]
      if other.key ~= dragging:get() and node then
        local top = base(lane) + (node.layout_y or 0)
        local height = node.layout_height or 40
        if top + height / 2 >= in_list then line = top break end
        index = index + 1
        line = top + height + 6
      end
    end
    drop_lane:set(lane)
    drop_index:set(index)
    drop_y:set(LIST_Y + line - 4 - scrolls[lane]:get())
    ghost_x:set(x - CARD_W / 2)
    ghost_y:set(y - 20)
  end

  local function drop()
    local key, lane, index = dragging:get(), drop_lane:get(), drop_index:get()
    dragging:set("")
    drop_lane:set(0)
    local state = tasks.states[lane]
    if key ~= "" and state then tasks.place(key, state.id, index) end
  end

  -- ---------------------------------------------------------------- cards --

  local function new_card()
    local hovered = kit.hover_signal("board.new")
    return ui.Item {
      width = CARD_W, height = NEW_H,
      ui.Rect {
        width = CARD_W, height = NEW_H - 6, radius = theme.radius_small,
        color = function() return hovered:get() and C.islandSurfaceHover or C.island end,
        border_width = 1,
        border_color = function()
          return (walking:get() and cursor_lane:get() == 1 and cursor_index:get() == 0) and C.accent() or C.hairline
        end,
        behavior = { color = theme.behave("fast") },
        ui.Row {
          anchors = { center_in = true }, gap = 8, align = "center",
          kit.glyph { text = "󰐕", size = 13, color = C.accent },
          kit.text { text = "New task", size = theme.size.small, weight = 600 },
        },
        ui.MouseArea {
          anchors = { fill = true }, cursor = "pointer",
          on_entered = function() hovered:set(true) end,
          on_exited = function() hovered:set(false) end,
          on_clicked = function() tasks.create(true) end,
        },
      },
    }
  end

  local function card(lane, key)
    local hovered = kit.hover_signal("board.card")
    local task = function() return tasks.entry(key) end
    local done = function() local t = task() return t and t.state == "done" end
    local held = function() return dragging:get() == key end
    local more = function() local t = task() return t and notes.first_line(t.body) or "" end
    local current = function()
      if not walking:get() or cursor_lane:get() ~= lane then return false end
      local c = lane_cards(lane)[cursor_index:get()]
      return c ~= nil and c.key == key
    end
    local inner_w = CARD_W - 35 - 9
    -- Placed by hand rather than in a Column, which keeps room for a hidden
    -- child: a card without notes or a day is only as tall as its line.
    local has_more = function() return more() ~= "" end
    local has_day = function() local t = task() return t ~= nil and t.due ~= "" end
    local line = kit.text {
      x = 35, y = 9, width = inner_w, wrap = true, max_lines = 2,
      text = function() local t = task() return t and t.text or "" end,
      size = theme.size.small,
      decoration = function() return done() and { line = "through" } or {} end,
      color = function() return done() and C.textMuted() or C.text() end,
    }
    local line_bottom = function() return 9 + (line.layout_height or 14) end
    local detail = kit.text {
      x = 35, y = function() return line_bottom() + 2 end,
      width = inner_w, elide = "right",
      visible = has_more,
      text = more, size = theme.size.label, color = C.textMuted,
    }
    local detail_bottom = function()
      if has_more() then return line_bottom() + 2 + (detail.layout_height or 12) end
      return line_bottom()
    end
    local day = kit.text {
      x = 35, y = function() return detail_bottom() + 2 end,
      visible = has_day,
      text = function() local t = task() return t and ("󰃭 " .. tasks.due_label(t.due)) or "" end,
      mono = true, size = theme.size.label,
      color = function() return tasks.is_overdue(task()) and C.red() or C.textMuted() end,
    }
    local bottom = function()
      if has_day() then return detail_bottom() + 2 + (day.layout_height or 12) end
      return detail_bottom()
    end
    local node
    node = ui.Item {
      width = CARD_W,
      height = function() return math.max(36, bottom() + 9) end,
      ui.Rect {
        anchors = { fill = true }, radius = theme.radius_small,
        color = function() return (hovered:get() and not held()) and C.islandSurfaceHover or C.island end,
        border_width = 1,
        border_color = function() return current() and C.accent() or C.hairline end,
        opacity = function() return held() and 0.35 or 1 end,
        behavior = { color = theme.behave("fast") },
      },
      ui.MouseArea {
        anchors = { fill = true },
        cursor = function() return held() and "grabbing" or "pointer" end,
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() if dragging:get() == "" then tasks.open(key, true) end end,
        on_drag_started = function(_, _, _, _, lx, ly)
          dragging:set(key)
          aim(lane_x(lane) + 8 + lx, LIST_Y - scrolls[lane]:get() + base(lane) + (node.layout_y or 0) + ly)
        end,
        on_dragged = function(_, _, _, _, lx, ly)
          aim(lane_x(lane) + 8 + lx, LIST_Y - scrolls[lane]:get() + base(lane) + (node.layout_y or 0) + ly)
        end,
        on_drag_finished = drop,
      },
      -- The tick: done, or back to the first lane. Over the card's area,
      -- so its press is its own.
      ui.Item {
        x = 9, y = 10, width = 18, height = 18,
        ui.Rect {
          anchors = { center_in = true }, width = 15, height = 15, radius = 7.5,
          color = function() return done() and C.green() or "#00000000" end,
          border_width = 1.5,
          border_color = function() return done() and C.green() or C.textMuted() end,
          behavior = { color = theme.behave("fast") },
          kit.glyph { anchors = { center_in = true }, text = "󰄬", size = 9, visible = done, color = C.island },
        },
        ui.MouseArea {
          anchors = { fill = true }, cursor = "pointer",
          on_clicked = function() tasks.toggle(key) end,
        },
      },
      line, detail, day,
    }
    card_nodes[key] = node
    return node
  end

  local function lane_node(i, state)
    local count = function() return #lane_cards(i) end
    local receiving = function() return dragging:get() ~= "" and drop_lane:get() == i end
    local children = {}
    if i == 1 then children[#children + 1] = new_card() end
    children[#children + 1] = ui.Repeater {
      as = "column", gap = 6,
      model = lane_models[i],
      delegate = function(row) return card(i, row.key) end,
    }
    local list = ui.Column {
      x = 8, gap = 0,
      translate_y = function() return -scrolls[i]:get() end,
      table.unpack(children),
    }
    return ui.Rect {
      x = lane_x(i), width = LANE_W, height = LANES_H,
      radius = theme.radius_medium, color = C.islandSurface,
      border_width = 1,
      border_color = function() return receiving() and C.accent() or C.islandBorder end,
      behavior = { border_color = theme.behave("fast") },
      ui.MouseArea {
        anchors = { fill = true }, z = -1,
        on_wheel = function(_, _, _, pixel_y, _, steps_y)
          local delta = (steps_y and steps_y ~= 0) and steps_y * 40 or (pixel_y or 0)
          local room = math.max(0, (list.layout_height or 0) - LIST_H)
          scrolls[i]:set(math.max(0, math.min(room, scrolls[i]:get() + delta)))
        end,
      },
      ui.Item {
        x = 12, y = 12, width = LANE_W - 24, height = 18,
        ui.Row {
          anchors = { left = true, vertical_center = true }, gap = 8, align = "center",
          kit.glyph {
            text = state.icon, size = 13,
            color = function() return state.id == "done" and C.green() or C.accent() end,
          },
          kit.text { text = state.label, size = theme.size.small, weight = 600 },
        },
        kit.text {
          anchors = { right = true, vertical_center = true },
          text = function() return tostring(count()) end,
          mono = true, size = theme.size.label, color = C.textMuted,
        },
      },
      ui.ClipRect {
        y = LIST_Y, width = LANE_W, height = LIST_H, color = "#00000000",
        list,
      },
    }
  end

  local lanes = {}
  for i, state in ipairs(tasks.states) do lanes[#lanes + 1] = lane_node(i, state) end

  local ghost_task = function() return tasks.entry(dragging:get()) end

  return ui.Item {
    width = ROOM_W, height = ROOM_H,
    ui.MouseArea {
      anchors = { fill = true }, z = -2, focus = true,
      on_key_pressed = function(keysym)
        if keysym == KEY.escape then island.close()
        elseif keysym == KEY.left then walk_lane(-1)
        elseif keysym == KEY.right then walk_lane(1)
        elseif keysym == KEY.down or keysym == KEY.tab then walk_card(1)
        elseif keysym == KEY.up then walk_card(-1)
        elseif keysym == KEY.enter or keysym == KEY.kp_enter then open_current()
        elseif keysym == KEY.space then
          local c = card_under_cursor()
          if c then tasks.set_state(c.key, c.state == "done" and "todo" or tasks.state_after(c.state)) end
        end
      end,
    },
    ui.Item {
      width = ROOM_W, height = LANES_H,
      table.unpack(lanes),
    },
    -- Where the dragged card would land.
    ui.Rect {
      z = 9, height = 2, radius = 1, width = CARD_W, color = C.accent,
      visible = function() return dragging:get() ~= "" and drop_lane:get() > 0 end,
      x = function() return lane_x(math.max(1, drop_lane:get())) + 8 end,
      y = function() return drop_y:get() end,
    },
    -- The card being dragged, free of its lane.
    ui.Rect {
      z = 10, width = CARD_W, height = 40, radius = theme.radius_small,
      visible = function() return dragging:get() ~= "" end,
      x = function() return ghost_x:get() end, y = function() return ghost_y:get() end,
      color = C.island, border_width = 1, border_color = C.accent, opacity = 0.92,
      kit.text {
        x = 9, anchors = { vertical_center = true }, width = CARD_W - 18, elide = "right",
        text = function() local t = ghost_task() return t and t.text or "" end,
        size = theme.size.small,
      },
    },
    kit.text {
      y = ROOM_H - FOOTER, height = FOOTER, vertical_alignment = "center",
      width = ROOM_W, elide = "right", mono = true, size = theme.size.small,
      text = function()
        local pending, over, today = tasks.pending(), #tasks.overdue(), tasks.pending_on(tasks.today())
        if tasks.count() == 0 then return "Nothing on the board yet" end
        local text = pending .. (pending == 1 and " task open" or " tasks open")
        if today > 0 then text = text .. " · " .. today .. " due today" end
        if over > 0 then text = text .. " · " .. over .. " overdue" end
        return text
      end,
      color = function() return #tasks.overdue() > 0 and C.red() or C.textMuted() end,
    },
  }
end

-- ------------------------------------------------------------------- task --

local function build_sheet()
  local key = tasks.opened()
  local first = tasks.entry(key) or { text = "", body = "" }
  local task = function() return tasks.entry(key) end
  local due = function() local t = task() return t and t.due or "" end
  local ROW_Y = ROOM_H - 28

  local line, more
  line = ui.TextInput {
    y = 4, width = ROOM_W - 150, height = 28,
    font_family = function() return theme.font() end,
    font_size = theme.size.large, font_weight = 600,
    color = C.text, caret_color = C.accent,
    placeholder = "What has to be done", placeholder_color = C.textMuted,
    selection_color = C.accent, selected_text_color = C.accentText,
    -- Set once: see the notes sheet.
    text = first.text, focus = true,
    on_text_changed = function(text) tasks.update(key, { text = text:match("^%s*(.-)%s*$") }) end,
    -- Enter moves on to the notes.
    on_accepted = function() more.focus = true end,
    on_escape = back,
  }
  more = ui.TextInput {
    y = 57, width = ROOM_W, height = ROW_Y - 57 - 12,
    multiline = true, wrap = true, vertical_alignment = "top",
    font_family = function() return theme.font() end,
    font_size = theme.size.regular, line_height = 1.4,
    color = C.text, caret_color = C.accent,
    placeholder = "Anything else about it", placeholder_color = C.textMuted,
    selection_color = C.accent, selected_text_color = C.accentText,
    text = first.body,
    on_text_changed = function(text) tasks.update(key, { body = text }) end,
    on_escape = back,
    on_key_pressed = function(keysym)
      if keysym == KEY.up and not more.text:sub(1, more.cursor_position):find("\n") then line.focus = true end
    end,
  }

  -- The caret at the end of the line, once the line has been laid out.
  morf.timer(1, function() line.cursor_position = #line.text end, false)

  local function pick(day)
    tasks.set_due(key, day)
    picking:set(false)
    line.focus = true
  end
  local function dismiss()
    picking:set(false)
    line.focus = true
  end

  local picker_x = 28 + 8 + 6
  return ui.Item {
    width = ROOM_W, height = ROOM_H,
    line,
    kit.text {
      anchors = { right = true, top = true, top_margin = 4 }, height = 28, vertical_alignment = "center",
      mono = true, size = theme.size.label, color = C.textMuted,
      text = function()
        local t = task()
        if not t then return "" end
        local finished = t.state == "done" and t.finished > 0
        local age = notes.age_of(finished and t.finished or t.created)
        return (finished and "done " or "added ") .. (age == "just now" and age or age .. " ago")
      end,
    },
    ui.Rect { y = 44, width = ROOM_W, height = 1, color = C.islandBorder },
    more,
    -- Back, the day as a button, the lane, and Delete. No Done button: the
    -- lane is the state.
    ui.Row {
      y = ROW_Y, height = 28, gap = 8, align = "center",
      kit.icon_button {
        glyph = "󰁍", glyph_size = 14, diameter = 28, color = "#00000000",
        hover_color = C.islandSurfaceHover, on_click = back,
      },
      ui.Item { width = 6, height = 1 },
      pill.button {
        icon = "󰃭",
        text = function() return due() ~= "" and tasks.due_label(due()) or "Pick a day" end,
        active = function() return picking:get() end,
        on_click = function()
          picking:set(not picking:get())
          if not picking:get() then line.focus = true end
        end,
      },
      pill.segmented {
        options = (function()
          local out = {}
          for _, s in ipairs(tasks.states) do out[#out + 1] = { id = s.id, label = s.label } end
          return out
        end)(),
        current = function() local t = task() return t and t.state or "todo" end,
        on_select = function(id) tasks.set_state(key, id) end,
      },
    },
    pill.button {
      anchors = { right = true, top = true, top_margin = ROW_Y },
      icon = "󰆴", text = "Delete",
      on_click = function()
        local was_direct = tasks.direct()
        tasks.remove(key)
        if was_direct then island.close() end
      end,
    },
    -- The month, over the form while picking: a click outside closes it.
    ui.MouseArea {
      anchors = { fill = true }, z = 5,
      visible = function() return picking:get() end,
      on_pressed = dismiss,
    },
    ui.Loader {
      z = 6, x = picker_x, y = ROW_Y - 8 - day_picker.height,
      width = day_picker.width, height = day_picker.height,
      active = function() return picking:get() end,
      source = function()
        return day_picker.build { selected = due(), on_pick = pick, on_dismiss = dismiss }
      end,
    },
  }
end

-- ------------------------------------------------------------------ panel --

island.register("board", {
  size = function() return tasks.panel_width, tasks.panel_height end,
  declared = true,
  build = function()
    return ui.Item {
      anchors = { fill = true },
      ui.Loader {
        width = ROOM_W, height = ROOM_H,
        active = function() return tasks.opened() == "" end,
        source = build_board,
      },
      ui.Loader {
        width = ROOM_W, height = ROOM_H,
        active = function() return tasks.opened() ~= "" end,
        source = build_sheet,
      },
    }
  end,
})

-- Test and keybind verbs: `board.open <key>` opens a task (the first open
-- one without a key); `board.new` a blank one; `board.pick` opens the month
-- over the open task's day.
morf.ipc["board.open"] = function(key)
  if not tasks.entry(key or "") then
    local first = tasks.queue()[1]
    key = first and first.key or ""
  end
  if key == "" then return "no task" end
  tasks.open(key)
  island.open("board")
  return key
end
morf.ipc["board.new"] = function()
  local key = tasks.create(false)
  island.open("board")
  return key
end
morf.ipc["board.pick"] = function()
  if tasks.opened() == "" then return "no task open" end
  picking:set(true)
  return "ok"
end
morf.ipc["board.move"] = function(key, state, index)
  tasks.place(key, state, tonumber(index))
  return tasks.entry(key) and tasks.entry(key).state or "no task"
end
