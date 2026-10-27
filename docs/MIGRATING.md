# Migrating to 0.6

0.6 renames rather than adds: every library module, widget, API function
and environment variable has one name, and the old ones are gone with no
alias. This is the map, old → new, for a shell or theme outside this
repository. "—" means deleted.

## Library modules

| 0.5 | 0.6 |
| --- | --- |
| `lib.services.dbus_client` | `lib.services.dbus` |
| `lib.services.tray_watcher` | `lib.services.tray` |
| `lib.services.dbusmenu` | `lib.services.tray_menu` |
| `lib.integrations.hyprland` | `lib.compositor.hyprland` |
| `lib.integrations.hyprland_config` | `lib.compositor.hyprland_config` |
| `lib.integrations.claude_usage`, `lib.integrations.github` | — (impasto's own `services/`) |
| `lib.util.osk`, `lib.util.board` | `lib.shell.osk` |
| `lib.util.material`, `lib.util.m3shapes` | `lib.theme.material`, `lib.theme.shapes` |
| `lib.util.palette` | — (impasto's own `services/palette.lua`) |
| `lib.util.align`, `lib.util.component` | — (inlined where they were used) |
| `lib.util.auth`, `authsteps`, `pattern`, `capture`, `annotation`, `settings`, `settings_pages`, `emoji`, `emoji_data`, `frecency`, `units`, `equalizer`, `spectrum`, `ringer`, `remote`, `channel` | — (no caller here; a shell that used them keeps its own copy) |
| `lib.services.accounts`, `keyboards`, `keyring_agent`, `modem`, `sessions`, `vpns` | — (likewise) |
| `lib.integrations.clipse`, `lule`, `lyrics`, `taskwarrior`, `tor` | — (likewise) |
| `lib.testing.fake_hyprland` | `lib.testing.hyprland` |
| `lib.kit.domain.*`, `lib.kit.catalogue`, `lib.kit.samples.*` | — |
| — | `lib.util.clock` (`clock.format(pattern)` at the pattern's grain) |

## The kit

The widget contract is libadwaita's, named in snake case
(`AdwActionRow` → `action_row`), with its style classes as variants. A theme
now exports tokens and skins only; every widget is made by
`lib.kit.widgets` (`widgets.<name> { ... }`).

| 0.5 | 0.6 |
| --- | --- |
| `lib.kit.<archetype>` (`selection`, `popup`, `shell`, ...) | `lib.kit.archetypes.<archetype>` |
| the Transform, Sheet, Form, Roving and Overflow archetypes | — |
| `lib.kit.caption`, `lib.kit.press_style` | `lib.kit.archetypes.press` |
| `lib.kit.display.*`, the kit functions (`kit.text`, `kit.card`, `kit.surface`, ...) | display widgets (`label`, `image`, `box`, `frame`, `progress_bar`, `level_bar`, `separator`, `status_page`, ...) and `lib.kit.tokens` |
| the charts, readings and status pieces | — (draw them with `ui.Path`, `ui.Sdf` and `morf.geometry`) |
| `composites.header_bar` | `widgets.header_bar` (Shell) |
| `composites.toolbar` | `widgets.toolbar_view` with a `toolbar` box |
| `composites.carousel`, `composites.tab_view` | `widgets.carousel`, `widgets.tab_view` (Navigation), `tab_bar`, `tab_overview` |
| `composites.colour_picker` | `color_chooser` |
| `composites.combo_box` | `drop_down` |
| `composites.emoji_picker`, `font_picker` | `emoji_chooser`, `font_chooser` |
| `composites.shortcuts_window` | `shortcuts_dialog` |
| `composites.notification_stack` | `notification`, `toast_overlay` |
| `composites.rows` and its kinds | `action_row` (`property` variant), `switch_row`, `combo_row`, `entry_row`, `password_entry_row`, `spin_row`, `expander_row`, `button_row`, `preferences_group`, `preferences_page`; `check_row` → `switch_row` |
| `composites.date_picker` | a `menu_button` holding a `calendar` |
| `composites.input_group` | a `box` with the `linked` variant |
| `command_palette`, `dashboard`, `kanban`, `picker`, `status_bar`, `tag_input`, `time_picker`, `tour`, `transfer_list`, `wizard` | — |
| `widgets.tree_view`, `widgets.list` | `tree_expander`, `list_view` |
| `widgets.slider`, `widgets.icon`, `widgets.area` | `scale`, `button { icon = ... }`, a `ui.MouseArea` |
| `shell.make("window_layout", ...)` | `widgets.application_window`, `navigation_split_view`, `overlay_split_view` |
| `popup.make("dialog", ...)` for a confirmation | `widgets.alert_dialog` |

## The Lua API

`require("morf.<namespace>")` is `morf.<namespace>`; `morf.core`,
`morf.io` and `morf.io.json` are gone.

| 0.5 | 0.6 |
| --- | --- |
| `morf.spawn`, `run`, `kill`, `process`, `process_view`, `exec_detached` | `morf.process.spawn`, `run`, `kill`, `open`, `view`, `detach` |
| `morf.connect`, `request_socket`, `socket`, `socket_server` | `morf.socket.connect`, `request`, `open`, `server` |
| `morf.file`, `file_view` | `morf.fs.file`, `morf.fs.view` |
| `morf.fs.append(path, data)` | `morf.fs.write(path, data, { append = true })` |
| `morf.shell_dir()`, `shell_path(rel)`, `config_path(rel)` | `morf.paths.shell(rel?)` |
| `morf.state_dir()`, `state_path(rel)`, `cache_dir()`, `cache_path(rel)` | `morf.paths.state(rel?)`, `morf.paths.cache(rel?)` |
| `morf.session_paths()` | `morf.paths.session()` |
| `morf.timer`, `elapsed_timer` | `morf.time.timer`, `morf.time.elapsed` |
| `morf.clock`, `minute_clock`, `hour_clock` | `morf.time.clock`, `minute_clock`, `hour_clock` |
| `morf.system_clock { ... }:format(pattern)` | `require("lib.util.clock").format(pattern)`; a snapshot is `morf.time.date()` |
| `morf.menu`, `density`, `primary`, `on_primary`, `screens_revision`, `on_backdrop_click`, `on_keyboard_focus` | `morf.window.<same>` |
| `morf.window.floating` | `morf.window.toplevel` |
| `morf.toplevel.<action>` | `morf.toplevels.<action>` |
| `morf.desktop_entries` | `morf.apps.entries` |
| `morf.lock_surface`, `on_session_lock_state`, `on_session_locked` | `morf.lock.surface`, `on_state`, `on_locked` |
| `morf.session_lock`, `session_lock_state()` | `morf.lock.state` (a signal) |
| `morf.on_reload_completed(fn)`, `on_reload_failed(fn(err))` | `morf.on_reload(fn(ok, err))`; returning true takes a failure |
| `morf.shader(name, ...)`, `shader_data` | `morf.shader.define(name, ...)`, `morf.shader.data` |
| `morf.icon_path`, `has_icon` | `morf.image.icon`, `morf.image.has_icon` |
| `morf.font_families`, `font_axes` | `morf.text.families`, `morf.text.axes` |
| `morf.easing_curve` | `morf.easing.curve` |
| `morf.view_extent`, `view_item_start` | `morf.ui.view_extent`, `morf.ui.view_item_start` |
| `morf.http.url_encode`, `url_decode` | `morf.encoding.url_encode`, `url_decode` (`{ plus = true }` decodes `+`) |
| `ui.each(list, delegate, options)` | `ui.Repeater { model = list, delegate = ..., as = ... }` |
| `ui.smoothed { ... }` | `{ kind = "smoothed", ... }` |
| `morf.persistent`, `scope`, `retainable`, `retain_lock`, `variants`, `virtual_list`, `transition_parent`, `color_quantizer`, `data_dir`, `data_path`, `working_directory`, `watch_files`, `split_parser`, `stream_collector`, `has_version` | — |
| `morf.time.days_between`, `is_leap_year`, `relative`, `timezone` | — (`morf.time.date()` has `leap_year` and `timezone`) |
| `morf.fs.basename`, `dirname`, `extension`, `stem`, `normalize`, `realpath`, `is_link` | — |
| `morf.json.read_file`, `write_file` | `morf.json.decode(file:text())`, `file:set_text(morf.json.encode(...))` |
| `morf.encoding.compression`, `decompress`, `crc32`, `hex_encode`, `hex_decode`, `random_bytes` | — |
| `morf.archive`, `morf.gamma`, `morf.workspace`, `morf.workspaces`, `morf.text_input` | — |
| `morf.image.encode_png`, `from_rgba` | — (`morf.image.from_dbus` stays) |
| `morf.text.fuzzy_score`, `highlight` | — (`morf.text.fuzzy` returns the positions) |
| `morf.color.hsl`, `lab`, `lch`, `oklab`, `xyz`, `lms`, `cmyk`, `gray`, `named`, `names`, `ansi8` | — (`morf.color "hsl(...)"` and the other CSS forms still parse) |
| `morf.geometry.graph_series`, `plot` | — (`ui.Path`'s `series` and `plot`) |
| `morf.clipboard.subscribe`, `drag.supported`, `idle.inhibited`, `input_method.preedit`, `pam.authenticate_unlock`, `xkb.compile`, `audio.backend` | — |

## Environment variables

Four lists replace thirty variables; each is a comma list of words and
`key=value` items.

| 0.5 | 0.6 |
| --- | --- |
| `MORF_A11Y_LOG`, `MORF_AUDIO_LOG`, `MORF_DAMAGE_LOG`, `MORF_DENSITY_LOG`, `MORF_FRAME_LOG`, `MORF_GPU_LOG`, `MORF_GPU_WAIT`, `MORF_JIT_LOG`, `MORF_PALETTE_LOG`, `MORF_PRESENT_LOG`, `MORF_WAKE_LOG` | `MORF_LOG=a11y,audio,damage,density,frame,gpu,gpu_wait,jit,palette,present,wake` |
| `MORF_PROFILE=1`, `MORF_GPU_PROFILE`, `MORF_GPU_PROFILE_COMMANDS`, `MORF_SLOW_MS=N` | `MORF_PROFILE=lua,gpu,gpu_commands,slow=N` |
| `MORF_FRAME_LOG=2`, `MORF_FRAME_SPLIT_MS=N`, `MORF_RENDER_SPLIT_MS=N` | `MORF_PROFILE=frame_split=N,render_split=N` |
| `MORF_PRESENT=swapchain`, `MORF_PRESENT_MODE`, `MORF_PRESENT_BUFFERS`, `MORF_PRESENT_MODIFIER`, `MORF_PRESENT_WAIT_MS` | `MORF_PRESENT=swapchain,mode=M,buffers=N,modifier=M,wait_ms=N` |
| `MORF_NO_DUAL_SOURCE`, `MORF_CAPTURE_READBACK`, `MORF_LAYOUT_FULL`, `MORF_GPU_SHARED=0`, `MORF_POPUP_AUTO`, `MORF_NO_A11Y`, `MORF_DISABLE_CRASH_HANDLER`, `MORF_CRASH_CORE` | `MORF_DEBUG=no_dual_source,capture_readback,layout_full,gpu_unshared,popup_auto,no_a11y,no_crash_handler,crash_core` |
| `MORF_HDR=pq`, `MORF_HDR_WHITE=N` | `MORF_HDR=pq,white=N` |

The paths, names and settings are unchanged: `MORF_CONFIG`,
`MORF_RUNTIME_PATH`, `MORF_FONT_PATH`, `MORF_ICON_THEME`,
`MORF_PAM_LIBRARY`, `MORF_OUTPUT`, `MORF_APP_ID`, `MORF_DIRECTION`,
`MORF_LIMITS`, `MORF_JIT`, `MORF_DEPTH`, `MORF_HDR`, `MORF_CRASH_SCREEN`.
