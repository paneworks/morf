-- An application launcher that is fzf.
--
-- Not a picker drawn to look like fzf: fzf itself, in a `ui.Terminal`, over
-- the desktop entries morf reads. Its fuzzy matching, its keys (Ctrl+J/K,
-- the arrows, Tab to mark, Esc to leave) and its colours are fzf's own; the
-- panel around it is the shell's. Enter launches the pick; either way the
-- launcher closes when fzf exits.
--
-- The list goes in through a file and the pick comes out through another,
-- because fzf's standard input and output are the terminal it draws on; it
-- draws on `/dev/tty`, which is the terminal all the same.
--
--     morf examples/demos/text/fzf_launcher.lua
--     morf ipc call type fire     -- type into it from outside
--     morf ipc call type $'\r'    -- and pick

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.width = 640
morf.surface.height = 420
morf.surface.anchors = {}
morf.surface.layer = "overlay"
-- A launcher takes the keyboard while it is open, as any launcher does.
morf.surface.keyboard_focus = "exclusive"

local paper = "#14161d"
local ink = "#e2e4ea"
local muted = "#737b8c"
local accent = "#ff9e64"

-- Every application a menu would show, one per line: its id, then what fzf
-- shows and matches (the name, and the comment dimmed after it).
local entries = morf.desktop_entries()
local lines, by_id = {}, {}
for _, app in ipairs(entries:applications()) do
  if not app.no_display and app.name ~= "" and not by_id[app.id] then
    by_id[app.id] = app
    local comment = app.comment ~= "" and ("  \27[2m" .. app.comment .. "\27[0m") or ""
    lines[#lines + 1] = app.id .. "\t" .. app.name .. comment
  end
end
table.sort(lines, function(a, b) return a:match("\t(.*)"):lower() < b:match("\t(.*)"):lower() end)

local dir = (morf.fs.dir("runtime") or "/tmp") .. "/morf-fzf-launcher"
local list, pick = dir .. "/applications", dir .. "/pick"
morf.fs.write(list, table.concat(lines, "\n") .. "\n")
morf.fs.remove(pick)

local fzf = table.concat({
  "fzf",
  "--ansi",
  "--delimiter='\t'",
  "--with-nth=2..",
  "--layout=reverse",
  "--no-scrollbar",
  "--info=inline-right",
  "--prompt='  '",
  "--pointer='▌'",
  "--marker='┃'",
  "--color='bg:-1,bg+:#1f2330,fg:#aab1c0,fg+:#e2e4ea,hl:#ff9e64,hl+:#ff9e64,prompt:#ff9e64,pointer:#ff9e64,info:#737b8c,border:#262a36,gutter:#14161d'",
}, " ")

local count = #lines
local term
term = ui.Terminal {
  -- fzf reads the list and writes the pick; the terminal is where it draws.
  command = { "sh", "-c", fzf .. ' < "$0" > "$1"', list, pick },
  font_size = 14,
  padding = 10,
  focus = true,
  colors = { foreground = ink, background = paper, cursor = accent },
  layout = { grow = 1 },
  on_exit = function(code)
    if code == 0 then
      local chosen = morf.fs.read(pick) or ""
      local id = chosen:match("^([^\t]+)")
      local app = id and by_id[id]
      if app then
        morf.log.info("launching " .. app.name)
        entries:launch(app.id)
      end
    end
    morf.quit()
  end,
}

morf.ipc.type = function(text) return term:write(text) end
morf.ipc.text = function() return term:text() end

ui.Rect {
  anchors = { fill = true },
  radius = 16,
  color = paper,
  border_width = 1,
  border_color = "#2a2e3b",
  ui.Flex {
    anchors = { fill = true },
    direction = "column",
    padding = 8,
    ui.Flex {
      direction = "row",
      align = "center",
      gap = 8,
      layout = { height = 26 },
      ui.Item { width = 6, height = 1 },
      ui.Text { text = "Launch", color = accent, font_size = 13, font_weight = 700 },
      ui.Text {
        text = ("%d applications"):format(count),
        color = muted, font_size = 12,
        layout = { grow = 1 },
      },
      ui.Text { text = "esc to close", color = muted, font_size = 12 },
      ui.Item { width = 6, height = 1 },
    },
    term,
  },
}
