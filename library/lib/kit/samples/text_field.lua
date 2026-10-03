-- Gallery samples for the TextField archetype's widgets (lib.kit.samples):
-- each field as an application would make it -- its size, its insets, a
-- label, some text -- so the theme's skin has something real to draw.
local ui = require("morf.ui")

local M = {}

local CELL_W, CELL_H = 280, 220

local function theme_of(kit)
  if kit.theme then return kit.theme end
  local ok, theme = pcall(require, "theme")
  return ok and type(theme) == "table" and theme or {}
end

--- A field of `name` with the kit's ink and face, centred in its cell.
local function field(name, spec)
  return function(kit, widgets)
    local theme = theme_of(kit)
    spec = (type(spec) == "function") and spec(kit) or spec
    local props = {}
    for k, v in pairs(spec) do if k ~= "after" then props[k] = v end end
    props.id = "sample-" .. name
    props.width = props.width or 260
    props.font_family = props.font_family or theme.font
    props.font_size = props.font_size or (theme.size and theme.size.normal) or 15
    props.color = props.color or kit.ink("hi")
    props.placeholder_color = props.placeholder_color or kit.ink("lo")
    props.caret_color = props.caret_color or kit.ink("accent")
    props.selection_color = props.selection_color or function() return kit.ink("accent")():alpha(0.3) end
    props.x = math.floor((CELL_W - props.width) / 2)
    props.y = math.floor((CELL_H - 20 - props.height) / 2)
    local node, input = widgets[name](props)
    if spec.after then spec.after(kit, input) end
    return ui.Item { width = CELL_W, height = CELL_H - 20, node }
  end
end

M.entry = field("entry", { label = "Full name", text = "Ana Lindqvist", height = 56, inset = { 16, 24, 16, 6 },
  focus = true })

M.password = field("password", { label = "Password", text = "hunter22", icon = "lock", height = 56,
  inset = { 48, 24, 44, 6 } })

M.search = field("search", { placeholder = "Search files", icon = "search", text = "", height = 44,
  inset = { 46, 0, 42, 0 } })

M.text_area = field("text_area", { label = "Message", max_length = 280, height = 132, inset = { 16, 28, 16, 26 },
  text = "Meet at the harbour at seven.\nBring the proofs and the second lens." })

M.url = field("url", { label = "Website", text = "https://morf.dev", icon = "link", height = 56,
  inset = { 48, 24, 44, 6 } })

M.email = field("email", { label = "Email", text = "ana@", icon = "mail", height = 80,
  supporting = "Used to sign you in", inset = { 48, 24, 44, 30 } })

M.numeric_entry = field("numeric_entry", { label = "Width", text = "1280", unit = "px", width = 170, height = 56,
  horizontal_alignment = "right", inset = { 16, 24, 44, 6 } })

M.otp = field("otp", { text = "4821", width = 264, height = 52 })

M.tag_input = field("tag_input", { label = "Tags", tags = { "design", "type", "print" }, placeholder = "Add a tag…",
  height = 86, inset = { 16, 52, 16, 6 } })

M.mentions = field("mentions", { text = "Thanks @ana for the proofs", icon = "alternate_email", height = 48,
  inset = { 46, 0, 50, 0 } })

M.inline_rename = field("inline_rename", { text = "Quarterly report.pdf", width = 240, height = 36,
  icon = "edit", inset = { 10, 0, 36, 0 } })

M.entry_row = field("entry_row", { label = "Server address", text = "morf.local", height = 60,
  icon = "check", inset = { 16, 26, 52, 6 } })

M.filter_field = field("filter_field", { placeholder = "Filter", text = "png", icon = "filter_list", clear = true,
  width = 210, height = 34, inset = { 36, 0, 34, 0 } })

-- Keywords, calls and numbers in the theme's tones, over byte ranges of
-- what is typed (an engine without `highlights` simply shows it plain).
local KEYWORDS = { fn = true, let = true, ["return"] = true, ["if"] = true, ["else"] = true }
local function highlight(kit, input)
  local function tone(kind) return (kit.signal_ink or kit.signal or kit.ink)(kind) end
  local keyword, call, number = kit.ink("accent"), tone("info"), tone("warn")
  pcall(function()
    input.highlights = function()
      local marks, text = {}, input.text or ""
      for start, word, stop in text:gmatch("()([%a_][%w_]*)()") do
        local after = text:sub(stop, stop)
        if KEYWORDS[word] then marks[#marks + 1] = { start = start - 1, stop = stop - 1, color = keyword() }
        elseif after == "(" then marks[#marks + 1] = { start = start - 1, stop = stop - 1, color = call() } end
      end
      for start, stop in text:gmatch("()%d+()") do
        marks[#marks + 1] = { start = start - 1, stop = stop - 1, color = number() }
      end
      return marks
    end
  end)
end

M.code_input = field("code_input", function(kit)
  local theme = theme_of(kit)
  return { text = "fn main() {\n    run(42)\n}", height = 96, multiline = true, vertical_alignment = "top",
    font_family = theme.mono, font_size = 14, inset = { 44, 12, 12, 10 }, after = highlight }
end)

return M
