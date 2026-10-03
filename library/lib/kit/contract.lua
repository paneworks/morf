-- The widget contract: what every theme's kit provides, as data.
--
-- A layout draws only through the kit; a theme implements the kit in its
-- own style. This file says what the kit is: the functions every kit
-- exports, the archetypes (behaviours with no look; each theme
-- skins them), the display widgets (no input, a kit function each), the
-- composites (archetypes combined), and the other owners the catalogue
-- (catalogue.lua) assigns elements to.
--
-- Every entry carries `stage`: the step of the widget plan that delivers
-- it. `M.stage` is how far the kits have come; `lib.kit.check` requires
-- every entry at or below it, so the contract tightens as stages land.

local M = {}

--- The plan stage the kits have reached.
M.stage = 21

-- ------------------------------------------------------------- functions --
--
-- What a kit exports today, by group. `value = true` marks a field that is
-- a value rather than a function (and may be nil where a theme has none).

M.functions = {
  -- colour and ink
  { name = "signal", group = "colour", stage = 1 },
  { name = "ink", group = "colour", stage = 1 },
  { name = "stroke", group = "colour", stage = 1 },
  { name = "level", group = "colour", stage = 1 },
  { name = "state_ink", group = "colour", stage = 1 },
  -- text
  { name = "text", group = "text", stage = 1 },
  { name = "heading", group = "text", stage = 1 },
  { name = "subtitle", group = "text", stage = 1 },
  { name = "section_label", group = "text", stage = 1 },
  { name = "menu_label", group = "text", stage = 1 },
  { name = "label", group = "text", stage = 1 },
  { name = "readout", group = "text", stage = 1 },
  { name = "caption", group = "text", stage = 1 },
  { name = "facts", group = "text", stage = 1 },
  { name = "code", group = "text", stage = 1 },
  { name = "term", group = "text", stage = 1 },
  { name = "icon", group = "text", stage = 1 },
  -- containers
  { name = "surface", group = "container", stage = 1 },
  { name = "card", group = "container", stage = 1 },
  { name = "panel", group = "container", stage = 1 },
  { name = "header", group = "container", stage = 1 },
  { name = "chip", group = "container", stage = 1 },
  { name = "decor", group = "container", stage = 1 },
  { name = "collect", group = "container", stage = 1 },
  { name = "centred", group = "container", stage = 1 },
  { name = "with_viewport", group = "container", stage = 1 },
  -- controls (until their archetypes replace them)
  { name = "action", group = "control", stage = 1 },
  { name = "hover", group = "control", stage = 1 },
  -- focus: Tab reaches an area and the theme marks it (`visual_focus`)
  { name = "focusable", group = "control", stage = 2 },
  { name = "pill", group = "control", stage = 1 },
  { name = "switch", group = "control", stage = 1 },
  { name = "slider", group = "control", stage = 1 },
  { name = "tabs", group = "control", stage = 1 },
  { name = "tabbed", group = "control", stage = 1 },
  { name = "selection", group = "control", stage = 1 },
  { name = "state_surface", group = "control", stage = 1 },
  { name = "keycap", group = "control", stage = 1 },
  { name = "field", group = "control", stage = 1 },
  { name = "icon_button", group = "control", stage = 1 },
  { name = "keyboard_look", group = "control", stage = 1 },
  { name = "loading", group = "control", stage = 1 },
  { name = "media_progress", group = "control", stage = 1 },
  { name = "round", group = "control", stage = 1 },
  -- shapes
  { name = "shape", group = "shape", stage = 1 },
  { name = "shape_path", group = "shape", stage = 1 },
  { name = "svg", group = "shape", stage = 1 },
  { name = "sdf_shape", group = "shape", stage = 1 },
  { name = "arc_path", group = "shape", stage = 1 },
  { name = "morph_number", group = "shape", stage = 1 },
  -- readings
  { name = "gauge", group = "reading", stage = 1 },
  { name = "ring", group = "reading", stage = 1 },
  { name = "mini_ring", group = "reading", stage = 1 },
  { name = "bar", group = "reading", stage = 1 },
  { name = "meter", group = "reading", stage = 1 },
  { name = "fill", group = "reading", stage = 1 },
  { name = "vmeter", group = "reading", stage = 1 },
  { name = "chart", group = "reading", stage = 1 },
  { name = "spectrum", group = "reading", stage = 1 },
  { name = "radar", group = "reading", stage = 1 },
  { name = "dial", group = "reading", stage = 1 },
  { name = "cell", group = "reading", stage = 1 },
  { name = "stat", group = "reading", stage = 1 },
  { name = "triplet", group = "reading", stage = 1 },
  -- status
  { name = "emblem", group = "status", stage = 1 },
  { name = "status_line", group = "status", stage = 1 },
  { name = "status", group = "status", stage = 1 },
  -- motion
  { name = "spring", group = "motion", stage = 1 },
  { name = "elastic", group = "motion", stage = 1 },
  { name = "bud", group = "motion", stage = 1 },
  { name = "ride", group = "motion", stage = 1 },
  { name = "STRETCH", group = "motion", stage = 1, value = true },
  -- helpers
  { name = "bytes", group = "helper", stage = 1 },
}

-- ------------------------------------------------------------ archetypes --
--
-- Behaviours with no look (morf-kit, Rust). A theme skins each: a function
-- of the archetype's state returning its slot trees. Every archetype is a
-- Control and has the base's state and slots besides its own.

M.base = {
  state = { "hovered", "down", "focused", "visual_focus", "enabled", "mirrored", "highlighted" },
  geometry = { "padding", "left_padding", "right_padding", "top_padding", "bottom_padding",
    "insets", "left_inset", "right_inset", "top_inset", "bottom_inset" },
  slots = { "background", "content" },
  focus = { "none", "click", "tab", "strong" },
  accessible = { "accessible_role", "accessible_name", "accessible_description" },
}

M.archetypes = {
  Press = { stage = 6, role = { "button", "toggle_button", "check_box", "radio_button", "switch", "link", "menu_item" },
    state = { "checkable", "checked", "tristate", "partial", "auto_repeat", "delay", "interval", "pressed_at", "group" },
    signals = { "on_clicked", "on_pressed", "on_released", "on_toggled", "on_long_pressed", "on_double_clicked" },
    keys = { "space", "return", "arrows_in_group" },
    slots = { "indicator", "icon", "label", "badge" },
    widgets = { "push", "suggested", "destructive", "flat", "raised", "outlined", "text", "tonal", "elevated", "pill",
      "circular", "icon", "link", "close", "copy", "loading", "toggle", "toggle_group_member", "switch", "checkbox",
      "radio", "chip_assist", "chip_filter", "chip_input", "chip_suggestion", "tag", "tile", "card_action",
      "row_activation", "menu_item", "check_menu_item", "radio_menu_item", "keycap", "fab", "extended_fab",
      "speed_dial_item", "segment", "rating_star", "help", "disclosure_button", "repeat_button", "back", "forward" } },
  Range = { stage = 6, role = { "slider", "spin_button", "scroll_bar", "progress" },
    state = { "from", "to", "value", "step", "page_step", "snap", "live", "orientation", "inverted", "logarithmic",
      "position", "visual_position", "wrap", "range", "first", "second" },
    signals = { "on_moved", "on_value_changed", "increase", "decrease" },
    keys = { "arrows", "page_up", "page_down", "home", "end" },
    slots = { "track", "fill", "handle", "second_handle", "ticks", "value_label", "increase", "decrease" },
    widgets = { "slider", "vertical_slider", "range_slider", "discrete_slider", "log_slider", "angle_slider", "knob",
      "bipolar_knob", "stepped_knob", "fader", "spin_button", "scrubber", "scroll_bar", "seek_bar", "volume",
      "brightness", "zoom", "rating", "level_control", "osd_level" } },
  Plane = { stage = 7, role = { "slider" },
    state = { "x_from", "x_to", "y_from", "y_to", "x", "y", "step_x", "step_y", "visual_x", "visual_y", "constraint" },
    signals = { "on_moved", "on_value_changed" },
    keys = { "arrows", "page_up", "page_down" },
    slots = { "field", "handle", "crosshair" },
    widgets = { "colour_plane", "hue_wheel", "xy_pad", "pan_pad", "envelope_point", "joystick", "minimap_viewport",
      "crop_handle" } },
  Selection = { stage = 7, role = { "tab_list", "list_box", "radio_group", "tree", "grid" },
    state = { "model", "current", "selected", "mode", "wrap", "orientation", "follow_focus" },
    signals = { "on_current_changed", "on_selection_changed", "on_activated" },
    keys = { "arrows", "home", "end", "typeahead", "space_toggles", "ctrl_a" },
    slots = { "item", "indicator", "separator" },
    widgets = { "tabs", "segmented", "view_switcher", "inline_view_switcher", "radio_group", "toggle_group",
      "list_selection", "grid_selection", "carousel_dots", "pagination", "stepper_header", "sidebar_list",
      "breadcrumbs", "day_grid", "swatch_grid", "emoji_grid", "icon_chooser", "transfer_side", "rating_items" } },
  Popup = { stage = 8, role = { "dialog", "alert_dialog", "menu", "tooltip", "list_box_popup" },
    state = { "open", "modal", "dim", "close_policy", "placement", "anchor", "side", "align", "flip", "shift",
      "focus_on_open", "restore_focus" },
    signals = { "on_opened", "on_closed", "on_about_to_close" },
    keys = { "escape", "tab_trap" },
    slots = { "dim", "enter", "exit" },
    widgets = { "menu", "context_menu", "menu_bar_menu", "submenu", "popover", "tooltip", "rich_tooltip", "hover_card",
      "dropdown_list", "autocomplete_list", "command_palette", "dialog", "alert_dialog", "message_dialog",
      "preferences_dialog", "about_dialog", "shortcuts_dialog", "bottom_sheet", "side_sheet", "drawer", "toast",
      "snackbar", "banner", "notification_popup", "lightbox", "tour_step" } },
  TextField = { stage = 9, role = { "text_field", "password_text", "text_area", "search_field" },
    state = { "text", "placeholder", "echo", "read_only", "max_length", "validator", "acceptable", "multiline", "wrap",
      "selected_text" },
    signals = { "on_edited", "on_accepted", "on_text_changed", "on_invalid" },
    keys = { "editing", "return_accepts", "escape_reverts" },
    slots = { "field", "leading", "trailing", "placeholder", "counter", "error" },
    widgets = { "entry", "password", "search", "text_area", "url", "email", "numeric_entry", "otp", "tag_input",
      "mentions", "inline_rename", "entry_row", "filter_field", "code_input" } },
  Scroll = { stage = 9, role = { "scroll_pane" },
    state = { "content_x", "content_y", "content_width", "content_height", "scroll_policy", "snap", "at_start",
      "at_end" },
    signals = { "on_scrolled", "on_reached_start", "on_reached_end" },
    keys = { "arrows", "page_up", "page_down", "home", "end", "space" },
    slots = { "scroll_bar_x", "scroll_bar_y", "edge_fade", "overscroll" },
    widgets = { "scroll_view", "scroll_area", "pager", "shelf", "infinite_scroll" } },
  Collection = { stage = 10, role = { "list", "grid", "table", "tree", "tree_grid" },
    state = { "model", "delegate", "layout", "columns", "expanded", "section", "item_size" },
    signals = { "on_row_activated", "on_sort_changed", "on_expanded_changed", "on_end_reached" },
    keys = { "selection_keys", "left_right_tree" },
    slots = { "row", "header", "section_header", "footer", "empty", "loading", "placeholder_row" },
    widgets = { "list", "boxed_list", "list_box", "virtual_list", "grid_view", "flow_box", "data_table", "tree_view",
      "tree_table", "file_list", "timeline", "feed", "chat_log", "kanban_column", "transfer_list" } },
  Disclosure = { stage = 11, role = { "button_expanded", "group" },
    state = { "expanded", "group", "animated" },
    signals = { "on_expanded", "on_collapsed" },
    keys = { "space", "return", "left_right_tree" },
    slots = { "header", "indicator", "content" },
    widgets = { "expander", "expander_row", "accordion", "collapsible_header", "collapsible_section", "details",
      "tree_node", "show_more", "collapsible_card", "fold_out" } },
  Drag = { stage = 11, role = { "splitter", "grip", "draggable" },
    state = { "axis", "bounds", "threshold", "active", "delta", "target", "mode" },
    modes = { "move", "resize", "split", "reorder", "swipe", "transfer" },
    signals = { "on_drag_started", "on_dragged", "on_dropped", "on_swiped" },
    keys = { "arrows", "alt_arrows_reorder" },
    slots = { "handle", "ghost", "drop_indicator" },
    widgets = { "split_pane", "resizable_panel", "resize_grip", "reorderable_rows", "reorderable_tabs",
      "sortable_grid", "swipe_dismiss", "swipe_actions", "pull_to_refresh", "sheet_handle", "window_move",
      "drag_source", "drop_zone" } },
  Navigation = { stage = 11, role = { "tab_panel", "group" },
    state = { "pages", "current", "mode", "can_go_back", "history" },
    signals = { "on_pushed", "on_popped", "on_current_changed" },
    keys = { "alt_left", "back", "ctrl_tab", "arrows_carousel" },
    slots = { "page", "transition", "back", "indicator" },
    widgets = { "navigation_view", "view_stack", "tab_pages", "carousel", "onboarding", "wizard",
      "settings_subpages", "master_detail" } },
  Shell = { stage = 16, role = { "application", "navigation", "main", "complementary" },
    state = { "breakpoints", "collapsed", "layout", "sidebar_width", "content_width", "toolbar_style" },
    signals = { "on_breakpoint", "on_collapsed" },
    keys = { "f9", "ctrl_b", "f6" },
    slots = { "header_bar", "sidebar", "content", "inspector", "bottom_bar", "toolbar_top", "toolbar_bottom", "banner",
      "toasts" },
    widgets = { "window_layout", "header_bar", "toolbar_view", "split_view", "overlay_split_view",
      "navigation_split_view", "multi_pane", "breakpoint_bin", "clamp", "bottom_bar" } },
  -- A world a viewport looks into: panned, zoomed, its items picked,
  -- selected, moved, connected and drawn (mara's graph, canvas, board,
  -- map and image views; a zoomable chart; a timeline).
  Canvas = { stage = 21, role = { "group", "image" },
    state = { "view_x", "view_y", "zoom", "tool", "selection", "hovered", "pointer_x", "pointer_y", "gesture",
      "band", "draft", "connect_from", "connect_to" },
    tools = { "select", "pan", "point", "line", "rect", "ellipse", "polyline", "polygon", "freehand", "connect",
      "brush", "zoom" },
    signals = { "on_view_changed", "on_selection_changed", "on_moved", "on_drawn", "on_connected", "on_activated",
      "on_context", "on_brushed", "on_deleted" },
    keys = { "arrows_nudge", "plus_minus_zoom", "zero_reset", "home_fit", "ctrl_a", "delete", "escape", "tab_items" },
    slots = { "grid", "item", "wires", "selection", "draft", "band", "crosshair", "overlay" },
    widgets = { "zoomable_canvas", "node_graph", "whiteboard", "diagram", "map_view", "image_viewer",
      "chart_inspector", "timeline_track", "drawing_board" } },
  -- Panels in splits and tab stacks the user rearranges (mara's shelves,
  -- panes and tabbed containers; an IDE's tool windows).
  Dock = { stage = 21, role = { "group", "tab_list", "tab", "tab_panel", "splitter" },
    state = { "focused", "focused_panel", "maximized", "dragging", "drop_target", "drop_zone", "panel_count" },
    signals = { "on_layout_changed", "on_activated", "on_closed", "on_maximized", "on_focus_changed" },
    keys = { "ctrl_page", "ctrl_w", "ctrl_shift_m", "f6", "escape" },
    slots = { "tab", "stack", "divider", "floating", "drop_indicator" },
    widgets = { "dock_area", "shelf_dock", "tabbed_container", "document_tabs", "tool_windows" } },
}

-- --------------------------------------------------------------- display --
--
-- Widgets that take no input: a kit function each, in every theme. `fn`
-- is the kit function that draws it; entries due in a later stage name
-- the function they will add.

M.display = {
  text = {
    { name = "label", fn = "label", stage = 1 }, { name = "heading", fn = "heading", stage = 1 },
    { name = "subtitle", fn = "subtitle", stage = 1 }, { name = "caption", fn = "caption", stage = 1 },
    { name = "readout", fn = "readout", stage = 1 }, { name = "facts", fn = "facts", stage = 1 },
    { name = "body", fn = "text", stage = 1 }, { name = "kbd", fn = "keycap", stage = 1 },
    { name = "markup", fn = "markup", stage = 14 }, { name = "code_block", fn = "code_block", stage = 14 },
    { name = "quote", fn = "quote", stage = 14 }, { name = "mono", fn = "mono", stage = 14 },
    { name = "link_text", fn = "link_text", stage = 14 },
  },
  media = {
    { name = "icon", fn = "icon", stage = 1 }, { name = "image", fn = "image", stage = 14 },
    { name = "avatar", fn = "avatar", stage = 14 }, { name = "thumbnail", fn = "thumbnail", stage = 14 },
    { name = "video", fn = "video", stage = 14 },
  },
  status = {
    { name = "emblem", fn = "emblem", stage = 1 }, { name = "status_line", fn = "status_line", stage = 1 },
    { name = "status", fn = "status", stage = 1 }, { name = "chip", fn = "chip", stage = 1 },
    { name = "spinner", fn = "loading", stage = 1 }, { name = "badge", fn = "badge", stage = 14 },
    { name = "dot", fn = "dot", stage = 14 }, { name = "progress_bar", fn = "progress", stage = 14 },
    { name = "progress_ring", fn = "progress_ring", stage = 14 }, { name = "battery", fn = "battery", stage = 14 },
    { name = "signal_bars", fn = "signal_bars", stage = 14 }, { name = "empty_state", fn = "empty_state", stage = 14 },
    { name = "skeleton", fn = "skeleton", stage = 14 }, { name = "banner_content", fn = "banner", stage = 14 },
    { name = "status_led", fn = "led", stage = 14 }, { name = "tag", fn = "tag", stage = 14 },
    { name = "segmented_progress", fn = "segmented_progress", stage = 14 },
    { name = "semicircle_progress", fn = "semicircle", stage = 14 },
    { name = "status_card", fn = "status_card", stage = 14 }, { name = "result_page", fn = "result_page", stage = 14 },
    { name = "toast_content", fn = "toast", stage = 14 },
  },
  readings = {
    { name = "gauge", fn = "gauge", stage = 1 }, { name = "ring", fn = "ring", stage = 1 },
    { name = "mini_ring", fn = "mini_ring", stage = 1 }, { name = "bar", fn = "bar", stage = 1 },
    { name = "meter", fn = "meter", stage = 1 }, { name = "fill", fn = "fill", stage = 1 },
    { name = "vmeter", fn = "vmeter", stage = 1 }, { name = "dial", fn = "dial", stage = 1 },
    { name = "radar", fn = "radar", stage = 1 }, { name = "cell", fn = "cell", stage = 1 },
    { name = "stat", fn = "stat", stage = 1 }, { name = "triplet", fn = "triplet", stage = 1 },
    { name = "thermometer", fn = "thermometer", stage = 14 }, { name = "tank", fn = "tank", stage = 14 },
    { name = "led_bar", fn = "led_bar", stage = 14 }, { name = "seven_segment", fn = "seven_segment", stage = 14 },
    { name = "vu_meter", fn = "vu_meter", stage = 14 }, { name = "peak_meter", fn = "peak_meter", stage = 14 },
    { name = "compass", fn = "compass", stage = 14 }, { name = "sparkline", fn = "sparkline", stage = 14 },
    { name = "segmented_meter", fn = "segmented_meter", stage = 14 },
  },
  charts = {
    { name = "chart", fn = "chart", stage = 1 }, { name = "spectrum", fn = "spectrum", stage = 1 },
    { name = "bars", fn = "bars", stage = 14 }, { name = "stacked", fn = "stacked", stage = 14 },
    { name = "histogram", fn = "histogram", stage = 14 }, { name = "scatter", fn = "scatter", stage = 14 },
    { name = "pie", fn = "pie", stage = 14 }, { name = "donut", fn = "donut", stage = 14 },
    { name = "heatmap", fn = "heatmap", stage = 14 }, { name = "calendar_heatmap", fn = "calendar_heatmap", stage = 14 },
    { name = "waveform", fn = "waveform", stage = 14 }, { name = "spectrogram", fn = "spectrogram", stage = 14 },
    { name = "candlestick", fn = "candlestick", stage = 14 }, { name = "box_plot", fn = "box_plot", stage = 14 },
    { name = "state_timeline", fn = "state_timeline", stage = 14 }, { name = "gantt", fn = "gantt", stage = 14 },
    { name = "treemap", fn = "treemap", stage = 14 }, { name = "sunburst", fn = "sunburst", stage = 14 },
    { name = "sankey", fn = "sankey", stage = 14 }, { name = "funnel", fn = "funnel", stage = 14 },
    { name = "flame_graph", fn = "flame_graph", stage = 14 },
    { name = "stacked_area", fn = "stacked_area", stage = 14 }, { name = "radial_bar", fn = "radial_bar", stage = 14 },
    { name = "status_history", fn = "status_history", stage = 14 },
  },
  structure = {
    { name = "card", fn = "card", stage = 1 }, { name = "panel", fn = "panel", stage = 1 },
    { name = "header", fn = "header", stage = 1 }, { name = "surface", fn = "surface", stage = 1 },
    { name = "separator", fn = "separator", stage = 14 }, { name = "spacer", fn = "spacer", stage = 14 },
    { name = "group_box", fn = "group_box", stage = 14 }, { name = "labelled_divider", fn = "labelled_divider", stage = 14 },
    { name = "frame", fn = "frame", stage = 14 }, { name = "inset", fn = "inset", stage = 14 },
  },
}

-- ------------------------------------------------------------ composites --
--
-- Archetypes combined (library/kit/composites/), shared by every theme.

M.composites = {
  combo_box = { stage = 15, parts = { "Press", "Popup", "Selection" } },
  menu_button = { stage = 15, parts = { "Press", "Popup" } },
  rows = { stage = 15, parts = { "Press", "Range", "TextField", "Disclosure" },
    variants = { "action_row", "switch_row", "check_row", "combo_row", "entry_row", "spin_row", "expander_row",
      "button_row", "property_row", "preferences_group", "preferences_page" } },
  date_picker = { stage = 15, parts = { "Press", "Popup", "Selection", "Navigation" } },
  time_picker = { stage = 15, parts = { "Range", "Popup" } },
  colour_picker = { stage = 15, parts = { "Plane", "Range", "TextField", "Selection", "Popup" } },
  font_picker = { stage = 15, parts = { "TextField", "Collection" } },
  file_chooser = { stage = 15, parts = { "Shell", "Collection", "Navigation", "TextField" } },
  emoji_picker = { stage = 15, parts = { "TextField", "Selection" } },
  picker = { stage = 15, parts = { "Press", "Popup", "Selection" } },
  search_bar = { stage = 15, parts = { "TextField", "Disclosure" } },
  tag_input = { stage = 15, parts = { "TextField", "Press" } },
  command_palette = { stage = 15, parts = { "Popup", "TextField", "Collection" } },
  notification_stack = { stage = 15, parts = { "Popup", "Collection", "Drag" } },
  tour = { stage = 15, parts = { "Popup", "Navigation" } },
  about_dialog = { stage = 15, parts = { "Popup", "Navigation" } },
  shortcuts_window = { stage = 15, parts = { "Popup", "Collection", "TextField" } },
  calendar = { stage = 15, parts = { "Selection", "Navigation" } },
  media_controls = { stage = 15, parts = { "Press", "Range" } },
  tab_view = { stage = 15, parts = { "Selection", "Navigation", "Drag", "Collection" } },
  sidebar = { stage = 15, parts = { "Selection", "Disclosure" } },
  header_bar = { stage = 16, parts = { "Shell", "Press" } },
  toolbar = { stage = 15, parts = { "Press", "Popup" } },
  status_bar = { stage = 15, parts = { "Press" } },
  carousel = { stage = 15, parts = { "Navigation", "Selection", "Drag" } },
  wizard = { stage = 15, parts = { "Navigation", "Selection" } },
  transfer_list = { stage = 15, parts = { "Collection", "Press" } },
  kanban = { stage = 15, parts = { "Collection", "Drag" } },
  input_group = { stage = 15, parts = { "TextField", "Press" } },
  dashboard = { stage = 15, parts = { "Collection", "Drag" } },
}

-- ----------------------------------------------------------------- other --
--
-- Owners that are not widgets: engine primitives already provided,
-- platform services, and domain instruments (display widgets or
-- composites over Plane, Drag and Collection, marked for what they are).

M.engine = { "layout", "animation", "theme", "utility", "model", "text", "image", "path", "shader", "terminal" }
M.platform = { "tray", "portal", "clipboard", "notifications" }
M.domain = {
  aviation = { stage = 14, over = { "display" }, module = "aviation",
    widgets = { "airspeed_tape", "altimeter_tape", "attitude_indicator", "heading_indicator", "course_deviation",
      "eicas_strip", "flight_path_marker", "heading_tape", "hsi", "nav_display", "pitch_ladder", "radar_altimeter",
      "range_rings", "bank_scale", "rolling_digits", "turn_coordinator", "vertical_speed", "weather_radar" } },
  hud = { stage = 14, over = { "display" }, modules = { "hud_game", "hud_fui" },
    widgets = { "health_bar", "charge_ring", "pie_menu", "cooldown_sweep", "damage_trail_bar", "minimap",
      "compass_strip", "hotbar", "kill_feed", "objective_tracker", "resource_orb", "stamina_ring", "xp_bar",
      "achievement_banner", "crosshair", "shield_bar", "ammo_counter", "damage_direction", "damage_numbers",
      "pip_container", "nameplate", "buff_row", "combo_counter", "offscreen_arrow", "waypoint_marker", "hit_marker",
      "lap_tracker", "racing_hud", "boss_bar", "interaction_prompt", "inventory_grid", "scoreboard", "subtitle_box",
      "tick_ruler", "radar_sweep", "segmented_arc_ring", "target_lock", "scan_sweep", "waveform_rings",
      "concentric_rings", "decode_text", "glitch_text", "hex_grid", "countdown_ring", "dot_matrix_progress",
      "biometric_scan", "data_stream", "striped_loading", "signal_noise", "crt_scanlines", "crosshair_grid", "callout",
      "wireframe", "telemetry_block", "motion_tracker", "proximity_ring", "bracket_tag", "orbit_diagram", "starfield",
      "assistant_orb" } },
  audio = { stage = 15, over = { "Range", "Plane", "display" }, module = "audio",
    widgets = { "automation_lane", "audio_visualiser", "compressor_curve", "eq_bars", "lissajous", "mixer_strip",
      "piano_keyboard", "parametric_eq", "tuner" } },
  editor = { stage = 15, over = { "Plane", "Drag", "Collection" }, module = "editor",
    widgets = { "envelope_editor", "node_editor", "piano_roll", "step_sequencer" } },
}

return M
