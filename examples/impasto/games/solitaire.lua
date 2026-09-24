-- Klondike: seven columns, one card more in each, top cards face up, the
-- rest in the stock. The stock deals one card at a time to the waste, and a
-- click on an empty stock turns the waste over. No dragging: a click picks
-- up a face-up card and everything below it, the next click puts them down
-- (on a column one rank higher in the other colour, on an empty column if
-- it is a King, or on a foundation if it is a single card next in its
-- suit). Clicking a single held card again sends it to a foundation if
-- possible. A face-down top card turns over automatically.
--
-- The score is the move count, lower is better; dealing from the stock and
-- turning the waste count as moves. The round ends when all four
-- foundations are complete. A stuck game stays on the table until redealt.
--
-- Port of Solitaire.qml. There, one Repeater drew whatever was placed, by
-- position in a list rebuilt on every move. Here each of the 52 cards is a
-- node of its own, built once, and a move writes where each one now lies;
-- a card keeps its node wherever it goes.

local ui = require("morf.ui")
local theme = require("theme")
local common = require("games.common")

local C = theme.color

-- Pile indices, so one tap handler covers them all: the seven columns,
-- then the waste, the stock and the four foundations.
local WASTE, STOCK, FOUNDATION = 7, 8, 9

local SUITS = { "♠", "♥", "♦", "♣" }
local RANKS = { "A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K" }

-- Cards are 72 x 100 at the frame's 760 x 560 and shrink with the width;
-- every other measurement derives from the card.
local MARGIN, STREET = 24, 26
local DOWN_STEP, UP_STEP = 24, 26

local function hex(c) return morf.color(c):hex():sub(1, 7) end

return function(ctx)
  local card_w = math.max(1, math.floor(math.min(72, (ctx.width - 2 * MARGIN - 6 * STREET) / 7)))
  local card_h = math.floor(card_w * 25 / 18)
  local inset = math.floor(card_w / 12 + 0.5)
  local pitch = card_w + STREET
  local table_left = math.floor((ctx.width - 7 * pitch + STREET) / 2)
  local top_row = MARGIN
  local tableau_top = MARGIN + card_h + MARGIN
  local id = tostring({})

  local function slot_x(column) return table_left + column * pitch end

  -- A card is `{ suit, rank, up, id }` (suit 0..3, rank 1..13, id 1..52)
  -- and a pile is an array, bottom first.
  local stock, waste, foundations, tableau = {}, {}, {}, {}
  local selected_pile, selected_index = -1, -1

  local function red(card) return card.suit == 1 or card.suit == 2 end

  -- -------------------------------------------------------------- cards --

  -- What each card's node shows, written by `layout` and read when a node
  -- is built (the cards are built a moment after the table).
  local views = {}
  local nodes = {}

  local function apply(card_id)
    local v, n = views[card_id], nodes[card_id]
    if not v or not n then return end
    n.node.visible = v.shown
    if not v.shown then return end
    n.node.x, n.node.y, n.node.z = v.x, v.y, v.z
    -- The light across the face (see `common.lit`).
    n.face.gradient = common.lit(v.up and C.scrimText or ctx.tint(), 0.14, 0.0, 0.1)
    n.face.border_width = v.selected and 2 or 1
    n.face.border_color = v.selected and C.accent()
      or v.up and common.over(C.island, 0.35, C.scrimText) or C.islandBorder
    n.back.visible = not v.up
    for _, t in ipairs(n.texts) do t.visible = v.up end
  end

  -- Every card with its position (only the top card for waste, stock and
  -- foundations), in drawing order. A column that would overflow tightens
  -- its overlap.
  local function layout()
    local order = 0
    for i = 1, 52 do views[i] = views[i] or {} views[i].shown = false end
    local function place(card, pile, index, x, y)
      order = order + 1
      local v = views[card.id]
      v.shown, v.x, v.y, v.z, v.up = true, x, y, order, card.up
      v.selected = pile == selected_pile and index >= selected_index
    end
    local function top(pile, which, x)
      if #pile > 0 then place(pile[#pile], which, #pile - 1, x, top_row) end
    end
    top(stock, STOCK, slot_x(0))
    top(waste, WASTE, slot_x(1))
    for slot = 0, 3 do top(foundations[slot + 1], FOUNDATION + slot, slot_x(3 + slot)) end
    local room = math.max(0, ctx.height - tableau_top - MARGIN - card_h)
    for column = 0, 6 do
      local pile = tableau[column + 1]
      local downs = 0
      for _, card in ipairs(pile) do if not card.up then downs = downs + 1 end end
      local natural = downs * DOWN_STEP + math.max(0, #pile - downs - 1) * UP_STEP
      local squeeze = natural > room and room / natural or 1
      local y = tableau_top
      for index, card in ipairs(pile) do
        place(card, column, index - 1, slot_x(column), math.floor(y + 0.5))
        y = y + (card.up and UP_STEP or DOWN_STEP) * squeeze
      end
    end
    for i = 1, 52 do apply(i) end
  end

  -- -------------------------------------------------------------- rules --

  local function select(pile, index) selected_pile, selected_index = pile, index end

  local function in_hand()
    if selected_pile < 0 then return {} end
    if selected_pile == WASTE then return { waste[#waste] } end
    local out = {}
    local pile = tableau[selected_pile + 1]
    for i = selected_index + 1, #pile do out[#out + 1] = pile[i] end
    return out
  end

  local function fits_column(cards, column)
    if #cards == 0 then return false end
    local pile = tableau[column + 1]
    local head = cards[1]
    if #pile == 0 then return head.rank == 13 end
    local top = pile[#pile]
    return top.up and red(top) ~= red(head) and top.rank == head.rank + 1
  end

  local function fits_foundation(cards, slot)
    if #cards ~= 1 then return false end
    local pile = foundations[slot + 1]
    if #pile == 0 then return cards[1].rank == 1 end
    local top = pile[#pile]
    return top.suit == cards[1].suit and top.rank == cards[1].rank - 1
  end

  -- Removes the held cards from their pile, turning over the card left on
  -- top if it is face down, and returns them.
  local function lift()
    local cards = in_hand()
    if selected_pile == WASTE then
      table.remove(waste)
    else
      local pile = tableau[selected_pile + 1]
      for _ = selected_index + 1, #pile do table.remove(pile) end
      if #pile > 0 and not pile[#pile].up then pile[#pile].up = true end
    end
    return cards
  end

  -- Places cards on a column or foundation, counts the move, and ends the
  -- round if it completed the foundations.
  local function drop(cards, pile)
    local target = pile >= FOUNDATION and foundations[pile - FOUNDATION + 1] or tableau[pile + 1]
    for _, card in ipairs(cards) do target[#target + 1] = card end
    ctx.score:set(ctx.score:get() + 1)
    select(-1, -1)
    for slot = 1, 4 do
      if #foundations[slot] ~= 13 then return end
    end
    ctx.over:set(true)
    ctx.finished(ctx.score:get())
  end

  local function send_home()
    local cards = in_hand()
    for slot = 0, 3 do
      if fits_foundation(cards, slot) then
        drop(lift(), FOUNDATION + slot)
        return
      end
    end
  end

  -- Deals one card from the stock, or turns the waste over when the stock
  -- is empty.
  local function turn_stock()
    if ctx.over:get() then return end
    select(-1, -1)
    if #stock > 0 then
      local top = table.remove(stock)
      top.up = true
      waste[#waste + 1] = top
    elseif #waste > 0 then
      for i = #waste, 1, -1 do
        local card = waste[i]
        card.up = false
        stock[#stock + 1] = card
      end
      waste = {}
    else
      layout()
      return
    end
    ctx.score:set(ctx.score:get() + 1)
    layout()
  end

  -- All table clicks land here (`index` -1 for an empty pile). A valid
  -- destination takes the held cards; otherwise a pickable card is picked
  -- up; otherwise the selection is cleared.
  local function tap(pile, index)
    if ctx.over:get() then return end
    if pile == STOCK then
      turn_stock()
      return
    end
    local held = in_hand()
    if pile == selected_pile and index == selected_index then
      -- A single held card clicked again goes to a foundation if it can.
      if #held == 1 then send_home() end
    elseif pile >= FOUNDATION then
      if fits_foundation(held, pile - FOUNDATION) then drop(lift(), pile) end
    elseif pile == WASTE then
      if index >= 0 then select(pile, index) else select(-1, -1) end
    elseif fits_column(held, pile) then
      drop(lift(), pile)
    else
      local card = index >= 0 and tableau[pile + 1][index + 1] or nil
      if card and card.up then select(pile, index) else select(-1, -1) end
    end
    layout()
  end

  local function restart()
    local deck = {}
    for suit = 0, 3 do
      for rank = 1, 13 do
        deck[#deck + 1] = { suit = suit, rank = rank, up = false, id = suit * 13 + rank }
      end
    end
    for i = #deck, 2, -1 do
      local other = common.random(i) + 1
      deck[i], deck[other] = deck[other], deck[i]
    end
    tableau = {}
    for column = 1, 7 do
      local pile = {}
      for _ = 1, column do pile[#pile + 1] = table.remove(deck, 1) end
      pile[column].up = true
      tableau[column] = pile
    end
    stock, waste, foundations = deck, {}, { {}, {}, {}, {} }
    select(-1, -1)
    ctx.score:set(0)
    ctx.over:set(false)
    layout()
  end

  -- Where each card is, by id, for the tap on it.
  local function locate(card_id)
    local function find(pile, which, top_only)
      for i = top_only and #pile or 1, #pile do
        if pile[i] and pile[i].id == card_id then return which, i - 1 end
      end
    end
    local p, i = find(stock, STOCK, true)
    if p then return p, i end
    p, i = find(waste, WASTE, true)
    if p then return p, i end
    for slot = 0, 3 do
      p, i = find(foundations[slot + 1], FOUNDATION + slot, true)
      if p then return p, i end
    end
    for column = 0, 6 do
      p, i = find(tableau[column + 1], column, false)
      if p then return p, i end
    end
  end

  -- ---------------------------------------------------------- the table --

  -- The card back: the tint with a faint inner frame and a lattice of
  -- diamonds in it, one picture shared by every face-down card.
  local function back_svg()
    local w, h = card_w - 2 * inset, card_h - 2 * inset
    local radius = math.max(2, theme.radius_small - inset / 2)
    local tint = ctx.tint()
    local frame = hex(common.mix(tint, C.text(), 0.14))
    local diamond = hex(common.mix(common.mix(tint, C.text(), 0.14), C.text(), 0.3 * 0.35))
    local d = math.floor(card_w * 0.12 + 0.5)
    local gap = math.floor(card_w * 0.14 + 0.5)
    local cell = d * math.sqrt(2)
    local grid_w, grid_h = 3 * d + 2 * gap, 4 * d + 3 * gap
    local ox, oy = (w - grid_w) / 2, (h - grid_h) / 2
    local shapes = {}
    for i = 0, 11 do
      local cx = ox + (i % 3) * (d + gap) + d / 2
      local cy = oy + (i // 3) * (d + gap) + d / 2
      shapes[#shapes + 1] = string.format('<polygon points="%g,%g %g,%g %g,%g %g,%g"/>',
        cx, cy - cell / 2, cx + cell / 2, cy, cx, cy + cell / 2, cx - cell / 2, cy)
    end
    return string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d">'
      .. '<clipPath id="k"><rect width="%g" height="%g" rx="%g"/></clipPath>'
      .. '<g clip-path="url(#k)"><rect width="%g" height="%g" fill="%s"/><g fill="%s">%s</g></g></svg>',
      w, h, w, h, radius, w, h, frame, diamond, table.concat(shapes))
  end


  local function card_node(card_id)
    local suit, rank = (card_id - 1) // 13, (card_id - 1) % 13 + 1
    local ink = (suit == 1 or suit == 2) and C.red or C.island
    local label = RANKS[rank] .. SUITS[suit + 1]
    local function text(values)
      values.font_family = function() return theme.font() end
      values.color = ink
      values.visible = false
      return ui.Text(values)
    end
    -- Rank and suit in the corner, visible under the overlap, the suit in
    -- the middle, and the corner again upside down in the other one, the
    -- way a card is printed.
    local texts = {
      text { x = inset, y = math.floor(inset / 2 + 0.5), text = label,
        font_size = theme.size.medium, font_weight = 700 },
      text { x = 0, y = 0, width = card_w, height = card_h, text = SUITS[suit + 1],
        horizontal_alignment = "center", vertical_alignment = "center",
        font_size = math.floor(card_h / 3 + 0.5), opacity = 0.9 },
      text { x = card_w - inset - 40, y = card_h - math.floor(inset / 2 + 0.5) - 20,
        width = 40, height = 20, text = label, rotation = 180, horizontal_alignment = "left",
        vertical_alignment = "top", font_size = theme.size.medium, font_weight = 700 },
    }
    local back = ui.Image {
      x = inset, y = inset, width = card_w - 2 * inset, height = card_h - 2 * inset,
      source = back_svg, visible = false,
    }
    local face = ui.Rect {
      x = 0, y = 0, width = card_w, height = card_h, radius = theme.radius_small,
      color = "#00000000", border_width = 1,
      back,
      texts[1], texts[2], texts[3],
    }
    local node = ui.Item {
      x = 0, y = 0, width = card_w, height = card_h, visible = false,
      -- What the card casts on the one under it: overlapping cards are a
      -- stack rather than a printed column.
      ui.Rect { x = 1, y = 3, width = card_w, height = card_h, radius = theme.radius_small,
        color = morf.color.rgb(0, 0, 0, 0.45) },
      face,
      ui.MouseArea {
        anchors = { fill = true }, cursor = "pointer",
        on_clicked = function()
          local pile, index = locate(card_id)
          if pile then tap(pile, index) end
        end,
      },
    }
    nodes[card_id] = { node = node, face = face, back = back, texts = texts }
    apply(card_id)
    return node
  end

  -- The thirteen slots, drawn under the cards so empty piles are still
  -- clickable.
  local slots = { { pile = STOCK, x = slot_x(0), y = top_row }, { pile = WASTE, x = slot_x(1), y = top_row } }
  for slot = 0, 3 do slots[#slots + 1] = { pile = FOUNDATION + slot, x = slot_x(3 + slot), y = top_row } end
  for column = 0, 6 do slots[#slots + 1] = { pile = column, x = slot_x(column), y = tableau_top } end
  local slot_nodes = {}
  for _, entry in ipairs(slots) do
    local ring = math.floor(card_w / 3 + 0.5)
    slot_nodes[#slot_nodes + 1] = ui.Rect {
      x = entry.x, y = entry.y, width = card_w, height = card_h, radius = theme.radius_small,
      color = function() return common.over(C.text(), 0.04) end,
      border_color = function() return common.over("#ffffff", 0x20 / 255) end,
      border_width = 1,
      -- A ring on the empty stock, where the waste turns over.
      ui.Rect {
        x = (card_w - ring) / 2, y = (card_h - ring) / 2, width = ring, height = ring, radius = ring / 2,
        visible = entry.pile == STOCK, color = "#00000000",
        border_color = function() return common.over(C.textMuted(), 0.5, common.over(C.text(), 0.04)) end,
        border_width = 2,
      },
      -- An ace marker on each foundation.
      ui.Text {
        x = 0, y = 0, width = card_w, height = card_h,
        horizontal_alignment = "center", vertical_alignment = "center",
        visible = entry.pile >= FOUNDATION, text = RANKS[1],
        color = function() return common.over(C.textMuted(), 0.5, common.over(C.text(), 0.04)) end,
        font_family = function() return theme.font() end, font_size = theme.size.large, font_weight = 700,
      },
      ui.MouseArea { anchors = { fill = true }, on_clicked = function() tap(entry.pile, -1) end },
    }
  end

  local node = common.ground {
    x = 0, y = 0, width = ctx.width, height = ctx.height,
    ui.Item { x = 0, y = 0, width = ctx.width, height = ctx.height, table.unpack(slot_nodes) },
    common.cells(52, card_node, { x = 0, y = 0 }),
  }

  local function key(keysym)
    if keysym ~= common.K.SPACE then return false end
    turn_stock()
    return true
  end

  -- The faces are written by `apply`, not bound, so a new palette (the
  -- game's tint, the accent) repaints them here.
  morf.effect("impasto.solitaire.palette." .. id, function()
    local _ = ctx.tint()
    local _accent = C.accent()
    for i = 1, 52 do apply(i) end
  end, { owner = node })

  restart()
  return {
    node = node, key = key, restart = restart,
    debug = function()
      local sizes = {}
      for column = 1, 7 do sizes[column] = #tableau[column] end
      return "stock " .. #stock .. " waste " .. #waste .. " columns " .. table.concat(sizes, ",")
    end,
    -- For testing: taps a pile (and card index in it) as a click would.
    tap = tap,
  }
end
