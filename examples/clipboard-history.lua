-- A clipboard history, and a place to drop things.
--
-- Everything copied anywhere appears at the top of the list: text as a line,
-- images as a thumbnail. It needs no focus to see a copy -- the compositor
-- announces every change through data control (`ext-data-control-v1`, or
-- wlroots' older spelling of it), the protocol `wl-paste --watch` uses.
-- Clicking an entry copies it again; dragging one drags it out, as text or
-- as the image file.
--
-- The strip at the top is a `ui.DropArea`: drag files, text or an image onto
-- it from any application and they join the history too.

local morf = require("morf")
local ui = require("morf.ui")

morf.surface.width = 420
morf.surface.height = 560
morf.surface.anchors = { top = true, right = true }
morf.surface.margin_top = 12
morf.surface.margin_right = 12
morf.surface.keyboard_focus = "none"

local KEEP = 8

-- Images are written where `ui.Image` can find them. The runtime directory
-- is memory-backed and private to the user, which suits a clipboard.
local store = (morf.fs.dir("runtime") or "/tmp") .. "/morf-clipboard"

-- The rows the list draws, newest first, and what each copies back as.
local rows = morf.list_model({})
local entries = {}
local next_id = 0
local status = morf.signal("clipboard.status", "")
local count = morf.signal("clipboard.count", 0)
local hovering = morf.signal("clipboard.drop", "")

local function publish()
  local list = {}
  for index, entry in ipairs(entries) do
    list[index] = {
      id = entry.id, kind = entry.kind, label = entry.label, source = entry.source or "",
    }
  end
  rows:replace(list, "id")
  count:set(#entries)
end

-- One line of a text, trimmed to fit a row.
local function first_line(text)
  local line = text:match("^%s*([^\n]*)") or ""
  if #line > 80 then line = line:sub(1, 80) .. "..." end
  if line == "" then line = "(" .. #text .. " bytes of whitespace)" end
  return line
end

local function remember(entry)
  -- A copy of what is already newest -- often this history copying an entry
  -- back -- moves nothing.
  local newest = entries[1]
  if newest and newest.kind == entry.kind and newest.data == entry.data then return end
  next_id = next_id + 1
  entry.id = next_id
  if entry.kind == "image" then
    entry.source = store .. "/" .. entry.id .. "." .. (entry.mime:match("image/([%w%+%-]+)") or "img")
    morf.fs.write(entry.source, entry.data)
    entry.label = entry.mime .. ", " .. math.floor(#entry.data / 1024) .. " KiB"
  else
    entry.label = first_line(entry.data)
  end
  table.insert(entries, 1, entry)
  while #entries > KEEP do
    local gone = table.remove(entries)
    if gone.source then morf.fs.remove(gone.source) end
  end
  publish()
end

local function has_image(mime_types)
  for _, mime in ipairs(mime_types) do
    if mime:sub(1, 6) == "image/" then return mime end
  end
end

-- Every copy, anywhere. Nothing is read until the offer's types say what it
-- is: an image is read as the best image type, anything else as text.
morf.clipboard.watch(function(offer)
  if not offer then return end
  local image = has_image(offer.mime_types)
  offer:read(image and "image" or "text", function(bytes, err)
    if not bytes then
      status:set("could not read the clipboard: " .. err)
      return
    end
    if #bytes == 0 then return end
    remember { kind = image and "image" or "text", mime = image or "text/plain", data = bytes }
  end)
end)

-- Whether data control is here is known only once the shell has connected,
-- after this file has run; until something is copied the line says what to do.
local function describe()
  if status:get() ~= "" then return status:get() end
  local n = count:get()
  if n == 0 then return "copy something anywhere, or drop it below" end
  return n .. (n == 1 and " copy" or " copies") .. ", newest first"
end

local function drop_zone()
  return ui.Rect {
    width = 396, height = 64, radius = 10,
    color = function() return hovering:get() ~= "" and "#23384d" or "#161c22" end,
    border_width = 1,
    border_color = function() return hovering:get() ~= "" and "#5aa2e6" or "#2a333d" end,
    behavior = { color = { duration = 120 } },
    ui.Text {
      anchors = { center_in = true },
      color = "#8a94a0", font_size = 14,
      text = function()
        local accepted = hovering:get()
        return accepted ~= "" and ("drop to keep it (" .. accepted .. ")") or "drop files, text or images here"
      end,
    },
    ui.DropArea {
      anchors = { fill = true },
      keys = { "image", "files", "text" },
      on_entered = function(info) hovering:set(info.accepted or "") end,
      on_exited = function() hovering:set("") end,
      on_dropped = function(drop)
        if drop.accepted and drop.accepted:sub(1, 6) == "image/" then
          -- Only the files and the text come fetched; anything else is read
          -- here, inside the handler, while the drop is still open.
          local mime = drop.accepted
          drop:read(mime, function(bytes)
            if bytes and #bytes > 0 then remember { kind = "image", mime = mime, data = bytes } end
          end)
        elseif drop.paths and #drop.paths > 0 then
          remember { kind = "text", mime = "text/plain", data = table.concat(drop.paths, "\n") }
        elseif drop.text then
          remember { kind = "text", mime = "text/plain", data = drop.text }
        end
      end,
    },
  }
end

local function row(entry)
  return ui.MouseArea {
    width = 396, height = entry.kind == "image" and 88 or 40,
    cursor = "pointer",
    on_clicked = function()
      for _, stored in ipairs(entries) do
        if stored.id == entry.id then
          morf.clipboard.set(stored.data, stored.kind == "image" and stored.mime or nil)
        end
      end
    end,
    -- A drag out: the text itself, or the image as the file it was saved to.
    on_drag_started = function()
      for _, stored in ipairs(entries) do
        if stored.id == entry.id then
          if stored.kind == "image" then
            morf.drag.start { paths = { stored.source } }
          else
            morf.drag.start { text = stored.data }
          end
        end
      end
    end,
    enter = { opacity = 0 },
    behavior = { opacity = { duration = 180 } },
    ui.Rect {
      anchors = { fill = true },
      radius = 8, color = "#1b2128",
      ui.Row {
        anchors = { fill = true, margins = 8 },
        align = "center",
        gap = 10,
        entry.kind == "image" and ui.Image {
          width = 96, height = 72, source = entry.source, fill_mode = "preserve_aspect_fit",
        } or ui.Item { width = 0, height = 0 },
        ui.Text {
          text = entry.label, color = "#e6edf3", font_size = 14,
          width = entry.kind == "image" and 270 or 376, elide = "right",
        },
      },
    },
  }
end

ui.Rect {
  width = 420, height = 560, radius = 14, color = "#101418",
  ui.Column {
    anchors = { fill = true, margins = 12 },
    gap = 10,
    ui.Text { text = "Clipboard", color = "#ffffff", font_size = 18, font_weight = 600 },
    ui.Text { text = describe, color = "#8a94a0", font_size = 13 },
    drop_zone(),
    ui.Repeater {
      model = rows,
      as = "column",
      gap = 6,
      delegate = row,
    },
  },
}
