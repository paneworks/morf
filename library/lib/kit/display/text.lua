-- Display widgets: text. See lib.kit.display.
--
-- Rich text (markup or runs), a block of code, a quotation, a mono run
-- and text with links. All of it is the engine's `Text`: `markup`,
-- `spans`, `links` and `on_link` (docs/UI.md, "Text in runs, and links").
local ui = require("morf.ui")
local U = require("lib.kit.display.util")

local M = {}
local get = U.get

local function floor_size(style, size) return math.max(size, style.size.small - 3) end

--- Rich text: `markup` (the notification subset of HTML: b, i, u, s, a,
--- br), or `spans` (runs: strings or `{ text, bold, italic, underline,
--- strike, color, size, link }`), or `text` (read as markup when it holds
--- a tag); `width` (wraps at it), `max_lines`, `size`, `color`,
--- `on_link(href)`.
function M.markup(spec, style)
  local props = U.place(spec, {
    width = spec.width, font_size = floor_size(style, spec.size or style.size.small),
    color = spec.color and U.color(spec, style) or style.ink, link_color = style.accent,
    line_height = spec.line_height or 1.35, on_link = spec.on_link,
  })
  if spec.width then props.wrap = true props.max_lines = spec.max_lines end
  if spec.spans then props.spans = spec.spans
  elseif spec.markup then props.markup = spec.markup
  else
    local t = spec.text
    if type(t) == "string" and t:find("<") then props.markup = t else props.text = t end
  end
  return style.text(props)
end

-- Words a small highlighter colours; anything else is left in the ink.
local KEYWORDS = {}
for w in ([[and break do else elseif end false for function goto if in local nil not or repeat return then
  true until while const let var fn def class import from export pub use struct enum impl match self mut
  async await yield new this null None True False]]):gmatch("%S+") do KEYWORDS[w] = true end

-- One line of code as runs: comments, strings, numbers and keywords in
-- the style's tones.
local function highlight(line, tones)
  local runs, i, n = {}, 1, #line
  local function push(text, color, italic)
    if color then runs[#runs + 1] = { text = text, color = color, italic = italic }
    else runs[#runs + 1] = text end
  end
  while i <= n do
    local rest = line:sub(i)
    local c = rest:sub(1, 1)
    if rest:match("^%-%-") or rest:match("^//") or rest:match("^#") then
      push(rest, tones.comment, true)
      break
    elseif c == '"' or c == "'" then
      local j = i + 1
      while j <= n and line:sub(j, j) ~= c do if line:sub(j, j) == "\\" then j = j + 1 end j = j + 1 end
      push(line:sub(i, j), tones.string)
      i = j + 1
    elseif rest:match("^%d") then
      local num = rest:match("^[%d%.xXa-fA-F_]+")
      push(num, tones.number)
      i = i + #num
    elseif rest:match("^[%a_]") then
      local word = rest:match("^[%w_]+")
      if KEYWORDS[word] then push(word, tones.keyword) else push(word) end
      i = i + #word
    else
      local other = rest:match("^[^%w_\"'%-/#]+") or c
      push(other)
      i = i + #other
    end
  end
  return runs
end

--- A block of code: `text`, `width` (260), `height` (fits the lines),
--- `numbers` (line numbers in a gutter), `language` (a caption over it),
--- `highlight` (false: no colouring). Mono, in a tinted well; it clips
--- what does not fit.
function M.code_block(spec, style)
  local w = spec.width or 260
  local fs = floor_size(style, spec.size or (style.size.small - 2))
  local lh = math.ceil(fs * 1.5)
  local src = tostring(get(spec.text) or "")
  local lines = {}
  for line in (src .. "\n"):gmatch("(.-)\n") do lines[#lines + 1] = line end
  if #lines > 1 and lines[#lines] == "" then lines[#lines] = nil end
  local pad = 12
  local head = spec.language and 26 or 0
  local h = spec.height or (head + #lines * lh + pad * 2)
  local node = ui.Item(U.place(spec, { width = w, height = h, clip = true }))
  if style.hatched then
    ui.reparent(ui.Rect { anchors = { fill = true }, color = style.surface, border_width = 1,
      border_color = style.line }, node)
    ui.reparent(ui.Rect { width = 2, height = h, color = style.accent }, node)
  else
    ui.reparent(ui.Rect { anchors = { fill = true }, radius = style.radius(h), color = style.track }, node)
  end
  if spec.language then
    if style.hatched then
      ui.reparent(U.caption(style, { text = spec.language, x = pad, y = 6, height = 16,
        font_size = style.size.small - 3, letter_spacing = 1, color = style.accent }), node)
      ui.reparent(ui.Rect { x = 2, y = head - 1, width = w - 3, height = 1, color = U.alpha(style.line, 0.7) }, node)
    else
      ui.reparent(style.text { text = spec.language, x = pad + 2, y = 8, height = 16,
        font_size = style.size.small - 3, font_weight = 600, color = style.ink_lo }, node)
    end
  end
  local font = style.mono_font
  local x = pad
  if spec.numbers then
    local digits = #tostring(#lines)
    local nw = math.ceil(digits * fs * 0.62)
    local nums = {}
    for i = 1, #lines do nums[i] = tostring(i) end
    ui.reparent(style.text { text = table.concat(nums, "\n"), x = x, y = head + pad, width = nw,
      horizontal_alignment = "right", font_family = font, font_size = fs, line_height = lh .. "px",
      color = U.alpha(style.ink_lo, 0.6) }, node)
    x = x + nw + 10
    if style.hatched then
      ui.reparent(ui.Rect { x = x - 5, y = head, width = 1, height = h - head, color = U.alpha(style.line, 0.7) }, node)
      x = x + 4
    end
  end
  local body = { x = x, y = head + pad, width = w - x - 4, font_family = font, font_size = fs, line_height = lh .. "px",
    color = style.ink }
  if spec.highlight == false then
    body.text = table.concat(lines, "\n")
  else
    -- The runs are rebuilt when the palette changes: a run's colour is a value.
    body.spans = function()
      local tones = { keyword = get(style.accent), string = get(style.ok), number = get(style.warn),
        comment = get(style.ink_lo) }
      local runs = {}
      for i, line in ipairs(lines) do
        for _, r in ipairs(highlight(line, tones)) do runs[#runs + 1] = r end
        if i < #lines then runs[#runs + 1] = "\n" end
      end
      return runs
    end
  end
  ui.reparent(style.text(body), node)
  node.accessible_role = "group"
  node.accessible_name = spec.label or (spec.language and (spec.language .. " code")) or "Code"
  return node
end

--- A quotation: `text`, `cite` (who said it), `width` (260). Material: an
--- accent pill beside an italic body; Tsugumori: a hairline rule with an
--- accent lead, the source in caps.
function M.quote(spec, style)
  local w = spec.width or 260
  local bar
  if style.hatched then
    bar = ui.Item { width = 4,
      ui.Rect { anchors = { left = true, top = true, bottom = true }, width = 1, color = style.line },
      ui.Rect { width = 3, height = 16, color = style.accent } }
  else
    bar = ui.Rect { width = 4, radius = 2, color = style.accent }
  end
  local tw = w - 4 - 14
  local col = { gap = 8,
    style.text { text = spec.text, width = tw, wrap = true, max_lines = spec.max_lines,
      font_size = floor_size(style, spec.size or (style.hatched and style.size.small - 1 or style.size.normal)),
      font_style = style.hatched and "normal" or "italic", line_height = 1.4, color = style.ink },
  }
  if spec.cite then
    col[#col + 1] = style.hatched
      and U.caption(style, { text = function() return "— " .. tostring(get(spec.cite) or "") end,
        font_size = style.size.small - 3, letter_spacing = 1, color = style.accent, width = tw, elide = "right",
        height = 16 })
      or style.text { text = function() return "— " .. tostring(get(spec.cite) or "") end, width = tw,
        elide = "right", font_size = style.size.small - 2, font_weight = 500, color = style.ink_lo }
  end
  local node = ui.Row(U.place(spec, { gap = 14, align = "stretch", bar, ui.Column(col) }))
  node.accessible_role = "group"
  node.accessible_name = spec.cite and ("Quote from " .. tostring(get(spec.cite))) or "Quote"
  return node
end

--- A run in the mono face: `text`, `width` (wraps at it), `size`,
--- `color`, `max_lines`.
function M.mono(spec, style)
  local props = U.place(spec, { text = spec.text, width = spec.width, font_family = style.mono_font,
    font_size = floor_size(style, spec.size or (style.size.small - 1)),
    color = spec.color and U.color(spec, style) or style.ink, line_height = 1.4 })
  if spec.width then props.wrap = true props.max_lines = spec.max_lines end
  return style.text(props)
end

--- Text that is (or holds) a link: `text` (the link's words), `href`,
--- `before`/`after` (plain words round it), `on_link(href)`, `width`,
--- `size`. The engine finds the link under the pointer and gives it the
--- pointer cursor; `on_link` hears the click.
function M.link_text(spec, style)
  local runs = {}
  if spec.before then runs[#runs + 1] = spec.before end
  runs[#runs + 1] = { text = get(spec.text) or get(spec.href) or "", link = get(spec.href) or "",
    bold = style.hatched or nil }
  if spec.after then runs[#runs + 1] = spec.after end
  local props = U.place(spec, { width = spec.width, spans = runs, link_color = style.accent,
    font_size = floor_size(style, spec.size or style.size.small), color = style.ink, line_height = 1.35,
    on_link = spec.on_link })
  if spec.width then props.wrap = true end
  local node = style.text(props)
  node.accessible_role = "link"
  node.accessible_name = get(spec.text) or get(spec.href)
  return node
end

return M
