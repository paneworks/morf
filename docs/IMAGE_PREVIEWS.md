# Asynchronous image previews

`morf.image.preview(source)` opens a native preview without reading or decoding
the source on the Lua thread. A VM can keep four previews open. Close a preview
when its owner is destroyed or its source changes.

The returned table uses dot calls:

```lua
local preview = morf.image.preview(path)
local queued, why = preview.cover(1080, 1920, 180, function(ok, result)
  if ok then
    -- Assign to a signal used by ui.Image.source.
    image_source:set(result.path)
  end
end)
-- When the owner is destroyed:
preview.close()
```

`preview.cover(width, height, scale_120, callback)` prepares the whole image at
the resolution needed to cover a rectangle in Morf's logical units. Dimensions
and scale must be positive integers; 120 means one device pixel per logical
unit, 180 means 1.5. Use `screen.density_scale * 120`, rounded to an integer,
for an output. SVGs rasterize directly at the requested resolution; raster
images use the renderer's Lanczos resize. Use the resulting source with
`fill_mode="preserve_aspect_crop"` to center and clip it to the rectangle.

`preview.render(operations, callback)` instead applies image editing operations
to a retained, immutable decoded original. Cover preparation does not retain an
editing original. Both methods use the existing two image workers and shared
32-job queue. They return `true` when queued, or `nil, message` when the queue
refuses the job; malformed arguments raise an error. The callback runs on the
owning UI loop as `(true, {path, width, height, format="rgba"})` or
`(false, message)`. The path is a `memory:` source, with no intermediate file.

A preview owns its last published frame. Publishing another replaces and
releases the previous one. Closing releases retained pixels and prevents a
queued or completed worker result from publishing later. Runtime teardown
discards callbacks. Use separate previews, closing the previous one, when
changing source or preparation size: that also prevents stale work from
replacing a newer frame. Keep a generation check in callbacks that update UI
state.

Preview source and frame sizes are limited to 128 MiB each; retained originals
and published frames share the existing 256 MiB process budget. Preparation can
fail for missing, malformed or oversized sources. A UI should show its base
color while pending or failed instead of assigning the file to `ui.Image` and
decoding it synchronously during a gesture. A published image still needs its
first GPU upload; this API moves file decoding and rasterization off the UI
thread, not GPU work.
