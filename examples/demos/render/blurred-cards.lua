-- Blurred cards: sixty cards on a 1920x1080 desk, each one a layer blurred
-- by the same radius -- the dashboard the frame bench measures compositing
-- layers on.
--
--     cargo run --release -p morf-cli --example frame_bench -- \
--         examples/demos/render/blurred-cards.lua gpu-time 1920x1080 200

local ui = require("morf.ui")

morf.surface.width = 1920
morf.surface.height = 1080

local COLUMNS, ROWS = 10, 6
local WIDTH, HEIGHT = 168, 152

local children = { ui.Rect { width = 1920, height = 1080, color = "#1b1d23" } }
for row = 0, ROWS - 1 do
  for column = 0, COLUMNS - 1 do
    local index = row * COLUMNS + column
    children[#children + 1] = ui.Rect {
      x = 24 + column * (WIDTH + 24), y = 36 + row * (HEIGHT + 24),
      width = WIDTH, height = HEIGHT, radius = 14, color = "#2b2f3a", blur = 4,
      ui.Text { x = 14, y = 12, text = ("Card %02d"):format(index + 1), font_size = 16, color = "#e6e6e6" },
      ui.Text { x = 14, y = 40, text = ("%d%%"):format(index * 37 % 100), font_size = 32, color = "#ffffff" },
      ui.Rect {
        x = 14, y = HEIGHT - 30, width = (WIDTH - 28) * (index * 37 % 100) / 100, height = 8,
        radius = 4, color = "#5fa8d3",
      },
    }
  end
end

ui.Item { width = 1920, height = 1080, table.unpack(children) }
