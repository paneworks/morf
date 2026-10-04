-- An emoji picker (composite: TextField search + Selection category tabs +
-- Selection grid of emoji).
--
--     local node, picker = composites.emoji_picker {
--       id = "emoji", width = 340, height = 320,
--       on_picked = function(emoji, name) insert(emoji) end,
--     }
--
-- A search field (a kit `search` entry) over a row of categories (a kit
-- segmented Selection, each its icon) and a kit `emoji_grid` (a
-- Selection: the arrows walk it, a press or Return picks) in a scrolled
-- view, with the emoji the grid is on named under it. Typing searches
-- every category by name and keywords, fuzzily; Down from the search goes
-- to the grid, Return there picks the first match. `emoji` replaces the
-- built-in table (below): a list of `{ key, name, icon, list = { { emoji,
-- name, keywords }, ... } }`. Ids: `<id>-search`, `<id>-categories`,
-- `<id>-category-<key>`, `<id>-grid`, `<id>-emoji-<i>`, `<id>-name`.
local ui = require("morf.ui")
local widgets = require("lib.kit.widgets")

-- The built-in emoji: categories of "emoji|name|keywords" lines.
local BUILT_IN = {
  { key = "smileys", name = "Smileys", icon = "mood", list = [[
😀|grinning face|smile happy
😃|smiling face with big eyes|happy joy
😄|smiling face with smiling eyes|happy laugh
😁|beaming face|grin teeth
😆|grinning squinting face|laugh
😅|grinning face with sweat|relief nervous
🤣|rolling on the floor laughing|lol rofl
😂|face with tears of joy|lol laugh cry
🙂|slightly smiling face|smile
🙃|upside-down face|silly sarcasm
😉|winking face|wink flirt
😊|smiling face with smiling eyes|blush happy
😇|smiling face with halo|angel innocent
🥰|smiling face with hearts|love adore
😍|smiling face with heart-eyes|love crush
🤩|star-struck|wow excited
😘|face blowing a kiss|kiss love
😋|face savoring food|yum tasty
😜|winking face with tongue|silly joke
🤔|thinking face|think hmm
🤨|face with raised eyebrow|doubt skeptic
😐|neutral face|meh blank
😴|sleeping face|sleep tired zzz
😎|smiling face with sunglasses|cool
🤓|nerd face|geek glasses
😕|confused face|confused
😢|crying face|sad tear
😭|loudly crying face|sob sad
😡|enraged face|angry mad
🤯|exploding head|mind blown shock
🥳|partying face|party celebrate
😱|face screaming in fear|scared shock
]] },
  { key = "people", name = "People", icon = "waving_hand", list = [[
👋|waving hand|hello hi bye
🤚|raised back of hand|stop
✋|raised hand|high five stop
👌|ok hand|okay perfect
🤌|pinched fingers|italian
✌️|victory hand|peace
🤞|crossed fingers|luck hope
🤟|love-you gesture|love
👍|thumbs up|like yes approve
👎|thumbs down|dislike no
👊|oncoming fist|punch bump
👏|clapping hands|applause bravo
🙌|raising hands|celebrate hooray
🙏|folded hands|please thanks pray
💪|flexed biceps|strong muscle
👀|eyes|look see
🧠|brain|smart think
👶|baby|child
🧒|child|kid
🧑|person|adult
👩|woman|female
👨|man|male
🧓|older person|old elder
🧑‍💻|technologist|developer coder computer
🧑‍🍳|cook|chef
🧑‍🚀|astronaut|space
🤷|person shrugging|shrug dunno
🙋|person raising hand|question me
]] },
  { key = "animals", name = "Animals & nature", icon = "pets", list = [[
🐶|dog face|puppy pet
🐱|cat face|kitten pet
🐭|mouse face|mouse
🐹|hamster|pet
🐰|rabbit face|bunny
🦊|fox|fox
🐻|bear|bear
🐼|panda|panda
🐨|koala|koala
🐯|tiger face|tiger
🦁|lion|lion king
🐮|cow face|cow
🐷|pig face|pig
🐸|frog|frog toad
🐵|monkey face|monkey
🐔|chicken|hen bird
🐧|penguin|penguin bird
🐦|bird|bird
🦉|owl|owl night
🐝|honeybee|bee
🦋|butterfly|insect
🐢|turtle|slow
🐙|octopus|sea
🐬|dolphin|sea
🌸|cherry blossom|flower spring
🌻|sunflower|flower
🌲|evergreen tree|tree forest
🍁|maple leaf|autumn fall
🌵|cactus|desert
🍄|mushroom|fungus
]] },
  { key = "food", name = "Food & drink", icon = "restaurant", list = [[
🍎|red apple|fruit
🍐|pear|fruit
🍊|tangerine|orange fruit
🍋|lemon|fruit sour
🍌|banana|fruit
🍉|watermelon|fruit summer
🍇|grapes|fruit wine
🍓|strawberry|fruit berry
🍒|cherries|fruit
🍑|peach|fruit
🥑|avocado|guacamole
🥕|carrot|vegetable
🌽|ear of corn|maize
🍞|bread|toast loaf
🧀|cheese wedge|cheese
🍳|cooking|egg breakfast
🍔|hamburger|burger
🍟|french fries|chips
🍕|pizza|slice
🌮|taco|mexican
🍣|sushi|japanese fish
🍜|steaming bowl|ramen noodles
🍩|doughnut|donut sweet
🍪|cookie|biscuit sweet
🎂|birthday cake|cake party
🍫|chocolate bar|sweet
☕|hot beverage|coffee tea
🍵|teacup without handle|tea green
🍺|beer mug|beer drink
🍷|wine glass|wine drink
]] },
  { key = "travel", name = "Travel & places", icon = "flight", list = [[
🚗|automobile|car
🚕|taxi|cab
🚌|bus|bus
🚲|bicycle|bike
🛵|motor scooter|scooter
🚂|locomotive|train steam
🚆|train|rail
✈️|airplane|plane flight
🚀|rocket|space launch
🛸|flying saucer|ufo
⛵|sailboat|boat sea
🚢|ship|boat cruise
🗺️|world map|map travel
🧭|compass|navigation
🏔️|snow-capped mountain|mountain
🏖️|beach with umbrella|beach summer
🏝️|desert island|island
🏕️|camping|tent outdoors
🏠|house|home
🏢|office building|work office
🏰|castle|castle
🗽|statue of liberty|new york
🗼|tokyo tower|tower
🌋|volcano|eruption
🌍|globe showing europe-africa|earth world
🌙|crescent moon|moon night
☀️|sun|sunny weather
⛅|sun behind cloud|cloudy weather
🌧️|cloud with rain|rain weather
❄️|snowflake|snow cold winter
]] },
  { key = "activities", name = "Activities", icon = "sports_soccer", list = [[
⚽|soccer ball|football sport
🏀|basketball|sport
🏈|american football|sport
⚾|baseball|sport
🎾|tennis|sport
🏐|volleyball|sport
🏓|ping pong|table tennis
🏸|badminton|sport
⛳|flag in hole|golf
🥊|boxing glove|boxing
🎯|bullseye|target dart
🎳|bowling|sport
⛸️|ice skate|skating
🎿|skis|ski snow
🏆|trophy|win award
🥇|1st place medal|gold first
🎮|video game|gaming controller
🕹️|joystick|gaming arcade
🎲|game die|dice
♟️|chess pawn|chess
🧩|puzzle piece|jigsaw
🎨|artist palette|art paint
🎭|performing arts|theatre
🎬|clapper board|film movie
🎤|microphone|sing karaoke
🎧|headphone|music audio
🎸|guitar|music rock
🎹|musical keyboard|piano music
🥁|drum|music
🎉|party popper|celebrate tada
]] },
  { key = "objects", name = "Objects", icon = "lightbulb", list = [[
💡|light bulb|idea
🔦|flashlight|torch light
🕯️|candle|light
📱|mobile phone|phone cell
💻|laptop|computer
⌨️|keyboard|typing
🖥️|desktop computer|monitor
🖨️|printer|print
🖱️|computer mouse|mouse
💾|floppy disk|save
📷|camera|photo
🎥|movie camera|film video
📺|television|tv
📻|radio|music
⏰|alarm clock|time wake
⌛|hourglass done|time wait
🔋|battery|power charge
🔌|electric plug|power
🔧|wrench|tool fix
🔨|hammer|tool build
⚙️|gear|settings cog
🔒|locked|lock secure
🔑|key|password unlock
📚|books|read library
📝|memo|note write
✏️|pencil|write edit
📎|paperclip|attach
📌|pushpin|pin
📅|calendar|date
✉️|envelope|mail email
📦|package|box parcel
🎁|wrapped gift|present gift
]] },
  { key = "symbols", name = "Symbols", icon = "favorite", list = [[
❤️|red heart|love
🧡|orange heart|love
💛|yellow heart|love
💚|green heart|love
💙|blue heart|love
💜|purple heart|love
🖤|black heart|love
💔|broken heart|sad breakup
💯|hundred points|perfect score
✅|check mark button|done yes ok
❌|cross mark|no wrong delete
❓|red question mark|question
❗|red exclamation mark|important
⚠️|warning|caution alert
🚫|prohibited|forbidden no
♻️|recycling symbol|recycle green
✨|sparkles|shine new magic
⭐|star|favourite
🔥|fire|hot lit flame
⚡|high voltage|lightning power
💤|zzz|sleep
💬|speech balloon|chat comment
🔔|bell|notification
🎵|musical note|music
➕|plus|add
➖|minus|subtract
➡️|right arrow|next
⬅️|left arrow|back
🔄|counterclockwise arrows button|refresh reload
🏁|chequered flag|finish race
]] },
}

local function parse(categories)
  local out = {}
  for _, category in ipairs(categories) do
    local entries = {}
    if type(category.list) == "string" then
      for line in category.list:gmatch("[^\n]+") do
        local emoji, name, keywords = line:match("^([^|]+)|([^|]*)|?(.*)$")
        if emoji then entries[#entries + 1] = { emoji = emoji, name = name, keywords = keywords or "" } end
      end
    else
      for _, e in ipairs(category.list or {}) do
        entries[#entries + 1] = { emoji = e.emoji or e[1], name = e.name or e[2] or "", keywords = e.keywords or e[3] or "" }
      end
    end
    out[#out + 1] = { key = category.key, name = category.name, icon = category.icon, list = entries }
  end
  return out
end
local parsed

return function(spec)
  spec = spec or {}
  local kit = require("kit")
  local id = spec.id
  local categories
  if spec.emoji then categories = parse(spec.emoji)
  else
    parsed = parsed or parse(BUILT_IN)
    categories = parsed
  end
  local everything = {}
  for _, category in ipairs(categories) do
    for _, e in ipairs(category.list) do
      everything[#everything + 1] = e
      e.search = e.name .. " " .. e.keywords
    end
  end
  local W, H = spec.width or 340, spec.height or 320
  local SEARCH, TABS, NAME = 40, 40, 24
  local CELL = spec.cell or 40
  local columns = math.max(1, math.floor(W / CELL))
  local GRID = H - SEARCH - TABS - NAME - 3 * 6
  local st = morf.state { query = "", category = 1, cursor = 0 }
  local function shown()
    if st.query ~= "" then
      local out = {}
      for _, hit in ipairs(morf.text.fuzzy(st.query, everything, { key = { "name", { "search", 0.6 } },
        limit = spec.limit or 96 })) do out[#out + 1] = hit.item end
      return out
    end
    local category = categories[st.category]
    return category and category.list or {}
  end
  local function pick(e)
    if not e then return end
    if spec.on_picked then spec.on_picked(e.emoji, e.name) end
  end

  local probe = { text = "" }
  ui.destroy(kit.text(probe), true)
  local focused = morf.state { on = false }
  local grid, flick
  local search_node, search = widgets.search { id = id and (id .. "-search"), accessible_name = "Search emoji",
    x = 10, width = W - 20, height = SEARCH, inset = { 0, 0, 34, 0 },
    placeholder = spec.placeholder or "Search emoji…",
    font_family = probe.font_family, font_source = probe.font_source, font_size = probe.font_size,
    color = kit.ink("hi"), placeholder_color = kit.ink("lo"), caret_color = kit.signal("accent"),
    selection_color = function() return kit.signal("accent")():alpha(0.3) end, vertical_alignment = "center",
    focus = spec.focus,
    on_focus_changed = function(on) focused.on = on end,
    on_text_changed = function(text) st.query = text st.cursor = 0 if flick then flick.content_y = 0 end end,
    on_accepted = function() pick(shown()[1]) end,
    on_escape = spec.on_escape,
    -- Down goes on to the grid, onto its first emoji.
    on_key_pressed = function(_, _, _, _, key)
      if (key ~= "Down" and key ~= "Page_Down") or #shown() == 0 then return false end
      if st.cursor == 0 then st.cursor = 1 end
      morf.focus.set(grid, true)
      return true
    end }
  local field = kit.field { width = W, height = SEARCH, focused = function() return focused.on end, search_node }

  local tab_w = math.floor(W / math.max(1, #categories))
  local tabs = widgets.segmented { id = id and (id .. "-categories"), accessible_name = "Categories",
    y = SEARCH + 6, items = categories, item_width = tab_w, item_height = TABS, gap = 0,
    item_id = function(_, c) return id and (id .. "-category-" .. c.key) or nil end,
    current = function() return st.query == "" and st.category or 0 end,
    on_current_changed = function(i)
      st.category = i
      st.cursor = 0
      if st.query ~= "" then search.text = "" st.query = "" end
      if flick then flick.content_y = 0 end
    end,
    delegate = function(_, c, s)
      return kit.centred(tab_w, TABS, kit.icon(c.icon, 20, function()
        return (s.current() and kit.ink("accent") or kit.ink("lo"))()
      end, { accessible_name = c.name }))
    end }

  local rows_of = function(n) return math.ceil(n / columns) end
  grid = widgets.emoji_grid { id = id and (id .. "-grid"), accessible_name = "Emoji",
    items = function()
      local list = shown()
      local out = {}
      for i, e in ipairs(list) do out[i] = { label = e.emoji, name = e.name, emoji = e.emoji } end
      return out
    end,
    columns = columns, gap = 0, item_width = CELL, item_height = CELL, press_activates = true,
    current = function() return st.cursor end,
    item_id = function(i) return id and (id .. "-emoji-" .. i) or nil end,
    delegate = function(_, e, s)
      return ui.Item { anchors = { fill = true },
        kit.surface { anchors = { fill = true, margins = 3 }, radius = kit.round(CELL / 4),
          color = function() local c = kit.signal("accent")() return c:alpha(s.hovered() and 0.1 or 0) end },
        kit.text { anchors = { center_in = true }, text = e.emoji, font_size = math.floor(CELL * 0.55),
          font_source = "", accessible_name = e.name } }
    end,
    on_current_changed = function(i)
      st.cursor = i
      -- Keep the current row in sight.
      if flick then
        local top = ((i - 1) // columns) * CELL
        local y = flick.content_y or 0
        if top < y then flick.content_y = top
        elseif top + CELL > y + GRID then flick.content_y = top + CELL - GRID end
      end
    end,
    on_activated = function(i) st.cursor = i pick(shown()[i]) end }
  local scroller
  scroller, flick = widgets.scroll_view { id = id and (id .. "-scroll"), y = SEARCH + TABS + 12, width = W,
    height = GRID, clip = true,
    grid }
  local root = ui.Item { width = W, height = H, field, tabs, scroller,
    kit.subtitle { id = id and (id .. "-name"), x = 4, y = H - NAME, width = W - 8, height = NAME, elide = "right",
      text = function()
        local e = shown()[st.cursor]
        if e then return e.emoji .. "  " .. e.name end
        if #shown() == 0 then return "No emoji match" end
        return st.query ~= "" and (#shown() .. " found") or (categories[st.category] or {}).name or ""
      end } }
  for _, k in ipairs { "x", "y", "anchors", "visible", "z" } do if spec[k] ~= nil then root[k] = spec[k] end end
  local handle = { search = search }
  function handle.query() return st.query end
  function handle.shown() return shown() end
  function handle.focus() search.focus = true end
  return root, handle
end
