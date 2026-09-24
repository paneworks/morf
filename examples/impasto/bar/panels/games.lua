-- The arcade: a shelf of cards, and the game being played.
--
-- Port of GamesPanel.qml (and GameBoard.qml, which it loads games through).
-- The shelf is a carousel, one game per tile; Left and Right slide it, Enter
-- plays. Opening a game morphs the island to that game's size, declared in
-- the catalogue so the capsule gets there before the game exists. Escape
-- goes back to the shelf, then closes.
--
-- The frame around each game lives here: the score and the best, the
-- game-over card, and R to restart. The games themselves know nothing about
-- bests (see `games/common.lua` for what a game is handed).

local ui = require("morf.ui")
local theme = require("theme")
local island = require("bar.island")
local kit = require("components.kit")
local games = require("services.games")
local common = require("games.common")

local C = theme.color
local K = common.K

-- Signals made per build get a fresh name each time: a frame is built
-- again every time its game opens.
local built = 0
local function signal(name, value)
  built = built + 1
  return morf.signal("impasto.games." .. name .. "." .. built, value)
end

-- --------------------------------------------------------------- the size --

-- The shelf's size, or the playing game's: DynamicIsland's `panelSizes.games`.
local function size() return games.panel_size(games.playing()) end

-- Closing the panel leaves the game, so it reopens on the shelf. Only once
-- the island has let the panel go: leaving at once would swap the fading
-- game for the shelf on its way out.
local leave_generation = 0
morf.effect("impasto.games.leave", function()
  if island.state.open_panel() == "games" then
    leave_generation = leave_generation + 1
    return
  end
  if games.playing() == "" then return end
  leave_generation = leave_generation + 1
  local mine = leave_generation
  morf.timer(theme.duration_island_gone() + 20, function()
    if mine == leave_generation and island.state.open_panel() ~= "games" then games.leave() end
  end, false)
end)

-- ------------------------------------------------------------------- pill --

-- impasto's PillButton, filled: the one button the frame has.
local function pill(text, on_click)
  local hovered = kit.hover_signal("games.pill")
  local label = kit.text {
    text = text, size = theme.size.small, weight = 600, color = C.accentText,
  }
  return ui.Rect {
    width = function() return (label.layout_width or 60) + 20 end,
    height = 28,
    radius = 14,
    color = function() return hovered:get() and C.accentHover() or C.accent() end,
    border_color = C.accent,
    border_width = 1,
    behavior = { color = theme.behave("fast") },
    ui.Item { anchors = { center_in = true }, width = function() return label.layout_width or 0 end,
      height = function() return label.layout_height or 0 end, label },
    ui.MouseArea {
      anchors = { fill = true },
      cursor = "pointer",
      on_entered = function() hovered:set(true) end,
      on_exited = function() hovered:set(false) end,
      on_clicked = function() on_click() end,
    },
  }
end

-- ------------------------------------------------------------------ shelf --

-- The carousel (impasto's Carousel and CarouselTile): the current tile
-- centred and enlarged, its neighbours stepping away from it. Each tile's
-- place and scale come from its distance to `current`; the step animates
-- the place, so a held arrow retargets instead of queueing.
local TILE_W, TILE_H, CENTRE_SCALE, TILE_GAP = 160, 100, 1.25, 12
local FIRST_STEP = TILE_W * CENTRE_SCALE / 2 + TILE_GAP + TILE_W / 2
local STRIDE = TILE_W + TILE_GAP

local function offset_of(distance)
  local away = math.abs(distance)
  local offset = away <= 1 and away * FIRST_STEP or FIRST_STEP + (away - 1) * STRIDE
  return distance < 0 and -offset or offset
end

local function scale_of(distance)
  return 1 + (CENTRE_SCALE - 1) * math.max(0, 1 - math.abs(distance))
end

local function shelf()
  local count = #games.catalogue
  local inner_w = games.shelf_width - 2 * theme.panel_padding
  local inner_h = games.shelf_height - 2 * theme.panel_padding
  local strip_h = inner_h - games.strip_gap - 16

  -- The game last opened: the shelf opens there instead of sliding from the
  -- first tile.
  local opening = 1
  local last = games.entry(games.last_opened())
  if last then opening = last.index end
  local current = signal("current", opening)

  local function go_to(index)
    current:set(math.max(1, math.min(count, index)))
  end
  local function play()
    local item = games.catalogue[current:get()]
    if item then games.open(item.id) end
  end
  -- Clicking the centre tile plays it; any other tile scrolls to it.
  local function press(index)
    if index == current:get() then play() else go_to(index) end
  end

  local tiles = {}
  for index, item in ipairs(games.catalogue) do
    local hovered = kit.hover_signal("games.tile")
    local function centred() return current:get() == index end
    local function distance() return index - current:get() end
    local function tint() return games.tint_of(item.id) end
    tiles[#tiles + 1] = ui.Rect {
      width = TILE_W, height = TILE_H,
      x = function() return (inner_w - TILE_W) / 2 + offset_of(distance()) end,
      y = (strip_h - TILE_H) / 2,
      scale = function() return scale_of(distance()) end,
      -- Tiles nearer the centre stack on top, so a sliding neighbour never
      -- overlaps the centred tile.
      z = function() return -math.abs(distance()) end,
      radius = theme.radius_medium,
      color = function() return hovered:get() and C.islandSurfaceHover or C.islandSurface end,
      -- Ringed in the game's own tint, which its board also uses.
      border_color = function() return centred() and tint() or C.islandBorder end,
      border_width = function() return centred() and 2 or 1 end,
      behavior = {
        x = theme.behave("morph"), scale = theme.behave("morph"),
        color = theme.behave("fast"), border_color = theme.behave("fast"),
      },
      ui.Column {
        anchors = { center_in = true },
        gap = 8, align = "center",
        -- The glyph on a tile of its own colour, like a launcher icon: the
        -- tint at a fifth, the glyph at full.
        ui.Rect {
          width = 44, height = 44, radius = 11,
          color = function() return common.alpha(tint(), 0.2) end,
          kit.glyph { glyph = item.icon, size = 22, color = tint, anchors = { center_in = true } },
        },
        kit.text {
          text = item.name, size = theme.size.small,
          font_weight = function() return centred() and 600 or 400 end,
          color = function() return centred() and C.text() or C.textMuted() end,
        },
      },
      ui.MouseArea {
        anchors = { fill = true }, z = 1, cursor = "pointer",
        on_entered = function() hovered:set(true) end,
        on_exited = function() hovered:set(false) end,
        on_clicked = function() press(index) end,
      },
    }
  end

  -- Touchpads send many small deltas; accumulate them to one step per 120.
  local turned = 0
  local strip = ui.ClipRect {
    width = inner_w, height = strip_h,
    color = "#00000000",
    ui.MouseArea {
      anchors = { fill = true }, z = -100,
      on_wheel = function(_, _, px, py, sx, sy)
        local steps = (sx ~= 0 and sx) or sy or 0
        if steps ~= 0 then
          go_to(current:get() + (steps > 0 and 1 or -1))
          return
        end
        turned = turned + ((px ~= 0 and px) or py or 0)
        while math.abs(turned) >= 120 do
          go_to(current:get() + (turned > 0 and 1 or -1))
          turned = turned - (turned > 0 and 120 or -120)
        end
      end,
    },
    table.unpack(tiles),
  }

  local function chosen() return games.catalogue[current:get()] end
  local node = ui.Column {
    gap = games.strip_gap,
    strip,
    -- The middle game's best, and where in the shelf it is.
    ui.Flex {
      width = inner_w, direction = "row", align = "center", gap = games.strip_gap,
      kit.text {
        layout = { grow = 1, minimum_width = 0 },
        elide = "right",
        mono = true, size = theme.size.small, color = C.textMuted,
        text = function()
          local item = chosen()
          if not item then return "" end
          if games.played(item.id) then return "Best " .. games.best_of(item.id) end
          return "Not played yet"
        end,
      },
      kit.text {
        size = theme.size.small, color = C.textMuted,
        text = function() return current:get() .. "/" .. count end,
      },
    },
  }

  local function key(keysym)
    if keysym == K.LEFT then go_to(current:get() - 1)
    elseif keysym == K.RIGHT then go_to(current:get() + 1)
    elseif keysym == K.HOME then go_to(1)
    elseif keysym == K.END then go_to(count)
    elseif common.confirm(keysym) or keysym == K.SPACE then play()
    else return false end
    return true
  end
  return node, key
end

-- ------------------------------------------------------------------ frame --

-- One frame per game, built when that game opens and gone when it closes:
-- the strip, the board and the overlay the round ends under.
local function frame(item)
  local points = signal(item.id .. ".score", 0)
  local over = signal(item.id .. ".over", false)
  local beaten = signal(item.id .. ".beaten", false)
  local by_moves = item.lower
  local tally

  -- The score as the game sees it: a signal whose every change to a new
  -- non-zero value beats the tally.
  local score = {
    get = function() return points:get() end,
    set = function(_, value)
      if value == points:get() then return end
      points:set(value)
      if value ~= 0 and tally then common.kick(tally, "scale", 1.22, 1, 220, "out_back") end
    end,
  }

  local fanfare, play_fanfare = common.burst {
    tint = C.indicatorWarn, spread = 120, sparks = 10, span = 620,
    x = item.width / 2 - 120, y = item.height / 2 - 120,
  }

  local game
  local ctx = {
    width = item.width, height = item.height,
    tint = function() return games.tint_of(item.id) end,
    score = score, over = over,
    -- Whether a tick should run: the round is on and this game is still
    -- the one on screen. A timer node can fire once more after the Loader
    -- that held it let the game go, onto nodes that are gone.
    on_screen = function()
      return games.playing() == item.id and island.state.open_panel() == "games"
    end,
    live = function()
      return not over:get() and games.playing() == item.id
        and island.state.open_panel() == "games"
    end,
    finished = function(final)
      local better = games.record(item.id, final)
      beaten:set(better)
      if better then play_fanfare() end
    end,
  }

  local function again()
    beaten:set(false)
    if game then game.restart() end
  end

  -- The score beats when it changes, so every game has one piece of
  -- feedback the frame gives it for free.
  tally = kit.text {
    mono = true, size = theme.size.small, weight = 600,
    text = function() return (by_moves and "Moves " or "Score ") .. score:get() end,
  }

  -- The game is created only once the island has reached its size: the
  -- island animates up from the shelf, and a game built mid-morph would
  -- start on a board a fraction of its height.
  local roomy = signal(item.id .. ".roomy", false)
  morf.timer(theme.duration_morph() + 10, function() roomy:set(true) end, false)

  local board = ui.Loader {
    x = 0, y = 0, width = item.width, height = item.height,
    active = function() return roomy:get() end,
    source = function()
      local ok, built = pcall(function() return require("games." .. item.id)(ctx) end)
      if not ok then
        morf.log("error", "impasto: game " .. item.id .. " failed: " .. tostring(built))
        return ui.Item {}
      end
      game = built
      return built.node
    end,
  }

  local card_column = ui.Column {
    gap = 10, align = "center",
    kit.text {
      text = by_moves and "Solved" or "Game over",
      size = theme.size.large, weight = 600,
    },
    kit.text {
      mono = true, size = theme.size.medium,
      text = function()
        if beaten:get() then return "★ New best · " .. score:get() end
        return (by_moves and "Moves " or "Score ") .. score:get()
      end,
      color = function() return beaten:get() and C.indicatorWarn or C.textMuted() end,
    },
    pill("Play again", again),
  }

  local overlay = ui.Rect {
    x = 0, y = 0, width = item.width, height = item.height,
    radius = theme.radius_medium,
    color = C.scrim,
    opacity = function() return over:get() and 1 or 0 end,
    visible = function() return over:get() end,
    behavior = { opacity = theme.behave("medium") },
    -- A click on the wash is the same as the button.
    ui.MouseArea {
      anchors = { fill = true }, z = -1,
      on_clicked = function() again() end,
    },
    fanfare,
    -- The end of the round on a card of its own, which lands rather than
    -- appears.
    ui.Rect {
      anchors = { center_in = true },
      width = function() return (card_column.layout_width or 160) + 56 end,
      height = function() return (card_column.layout_height or 80) + 36 end,
      radius = theme.radius_large,
      color = C.islandSurface,
      border_color = function() return beaten:get() and C.indicatorWarn or C.islandBorder end,
      border_width = 1,
      scale = function() return over:get() and 1 or 0.92 end,
      behavior = { scale = { duration = 260, easing = "out_back" } },
      ui.Item {
        anchors = { center_in = true },
        width = function() return card_column.layout_width or 0 end,
        height = function() return card_column.layout_height or 0 end,
        card_column,
      },
    },
  }

  local node = ui.Column {
    gap = games.strip_gap,
    -- Just the score and the best: no name, no key hints, and no buttons
    -- for what a key already does.
    ui.Flex {
      width = item.width, height = games.strip_height,
      direction = "row", align = "center", justify = "end", gap = 10,
      tally,
      kit.text {
        mono = true, size = theme.size.small, color = C.textMuted,
        visible = function() return games.played(item.id) end,
        text = function() return "Best " .. games.best_of(item.id) end,
      },
    },
    ui.Item {
      width = item.width, height = item.height,
      board,
      overlay,
    },
  }

  -- R restarts at any time. Enter and Space do too once the round is over;
  -- while it runs they belong to the game. No game uses R.
  local function key(keysym, text)
    if common.letter(keysym) == "r"
      or (over:get() and (common.confirm(keysym) or keysym == K.SPACE)) then
      again()
      return true
    end
    if game and game.key then return game.key(keysym, text) end
    return false
  end
  return node, key, function() return game end, function() return over:get() end
end

-- ------------------------------------------------------------------ panel --

-- The game on screen right now and the panel's key handler, for the IPC
-- hooks below.
local current_game, current_frame = nil, nil
local panel_key = nil

local function build()
  local shelf_node, shelf_key
  local frames = {}
  local children = {}

  children[#children + 1] = ui.Loader {
    x = 0, y = 0,
    active = function() return games.playing() == "" end,
    source = function()
      local node, key = shelf()
      shelf_key = key
      return node
    end,
  }
  for _, item in ipairs(games.catalogue) do
    children[#children + 1] = ui.Loader {
      x = 0, y = 0,
      active = function() return games.playing() == item.id end,
      source = function()
        local node, key, get, ended = frame(item)
        frames[item.id] = { key = key, get = get, ended = ended }
        return node
      end,
    }
  end

  local function handle_key(keysym, text)
    local playing = games.playing()
    if keysym == K.ESCAPE then
      if playing == "" then island.close() else games.leave() end
      return
    end
    if playing == "" then
      if shelf_key then shelf_key(keysym) end
    elseif frames[playing] then
      frames[playing].key(keysym, text)
    end
  end
  panel_key = handle_key
  current_game = function()
    local f = frames[games.playing()]
    return f and f.get()
  end
  current_frame = function() return frames[games.playing()] end

  -- The panel's keyboard: this area holds focus while the arcade is open,
  -- and every control in it is its child, so a click anywhere inside keeps
  -- the keys here.
  return ui.MouseArea {
    width = function() local w = size() return w - 2 * theme.panel_padding end,
    height = function() local _, h = size() return h - 2 * theme.panel_padding end,
    focus = true,
    on_key_pressed = handle_key,
    table.unpack(children),
  }
end

island.register("games", {
  size = size,
  build = build,
})

-- -------------------------------------------------------------------- IPC --

--- `morf ipc call game <id>` opens the arcade on that game (or the shelf
--- for none): the original's keybinds only open the panel, this reaches a
--- game directly for testing.
morf.ipc.game = function(id)
  if id and id ~= "" and not games.entry(id) then return "no game " .. id end
  if island.state.open_panel() ~= "games" then island.open("games") end
  games.leave()
  if id and id ~= "" then
    morf.timer(30, function() games.open(id) end, false)
  end
  return id or ""
end

--- TESTING ONLY: `morf ipc call game_key <key>` presses a key in the arcade,
--- since a headless compositor has no keyboard to press it with. `<key>` is
--- a keysym name the arcade reads (left, right, up, down, space, return,
--- escape) or one character.
local names = {
  left = K.LEFT, right = K.RIGHT, up = K.UP, down = K.DOWN, space = K.SPACE,
  ["return"] = K.RETURN, escape = K.ESCAPE, home = K.HOME, ["end"] = K.END,
}
morf.ipc.game_key = function(name)
  local keysym = names[name or ""] or (name and #name == 1 and string.byte(name)) or nil
  if not keysym then return "unknown key" end
  if island.state.open_panel() ~= "games" or not panel_key then return "arcade closed" end
  -- The same path a real key takes.
  panel_key(keysym, #name == 1 and name or nil)
  return games.playing()
end

--- TESTING ONLY: `morf ipc call game_state` says what the running game
--- reports about itself, when it has a `debug()`.
morf.ipc.game_state = function()
  local game = current_game and current_game()
  if game and game.debug then
    local f = current_frame and current_frame()
    return (f and f.ended() and "over; " or "") .. game.debug()
  end
  return games.playing()
end
