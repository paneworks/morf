# Library inventory

What `library/lib` holds, what really uses each part, and where the weight
comes from. Measured on 2026-10-25 against morf `develop` and eva `main`.
Line counts include comments and blank lines.

## How this was measured

1. **Static requires.** Every quoted `lib.*` module name was searched for
   in eva (`shell/`, `themes/`, `greet/`, `lock/`), the other example
   shells (chillpill, impasto, panacea), the example apps, the demos, the
   library itself, its specs and the Rust crates. That gave a require
   graph over the library.
2. **Runtime probes.**
   - eva was loaded headless in both styles (material, tsugumori), and
     every panel reachable by IPC was opened: sidebar, utilities and
     every settings page, launcher, leftbar, bottom, drawers, assistant,
     lule, capture, window menu, keyboard, OSD and notifications.
   - The settings and editor example apps were loaded the same way.
   - Each probe recorded which `lib.*` modules were loaded, which
     archetype and widget every `control.make` and `control.headless`
     built, and which display widgets were called.
3. **Name lookups.** Composites (`composites.X`, including through
   aliases), widgets (`widgets.X`) and domain instruments are reached by
   name, not by require. Each was searched for by hand.

Caveats:
- The probes do not reach every sub-state of every panel. For eva's lock
  and greet screens, and for panels the probes missed, the static search
  decides.
- A name built at run time can hide from both methods. Where a module
  sat on the line, it was checked by hand.

Each module is filed under its most real consumer:

| Class | Meaning |
| --- | --- |
| **eva** | the shell you run uses it |
| **example apps** | only `examples/apps/settings` or `examples/apps/editor` use it |
| **other shell / demo** | only impasto, chillpill, panacea or a demo uses it |
| **loaded, never called** | loaded on every start, nothing calls into it |
| **gallery only** | reached only by the gallery samples and the specs that render them, in either repo |
| **tooling** | the catalogue and `lib.kit.check` |
| **nothing** | no reference at all |

## The numbers

`library/lib` is **57,535 lines in 189 modules**, plus 11 permission
manifests (51 lines).

| Class | Lines | Modules | Share |
| --- | ---: | ---: | ---: |
| eva | 18,955 | 64 | 32.9% |
| example apps | 11,899 | 37 | 20.7% |
| other shell / demo | 4,076 | 11 | 7.1% |
| loaded, never called | 3,996 | 9 | 6.9% |
| gallery only | 17,446 | 63 | 30.3% |
| tooling | 1,158 | 4 | 2.0% |
| nothing | 5 | 1 | 0.0% |

**About 22,600 lines (39%) serve nothing a person runs.** That is the last
four rows. The shell you actually use needs a third of the library.

By folder:

| Folder | Lines | eva | apps | other | loaded, never called | gallery | tooling | nothing |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `kit/skins` | 9,326 | | 9,019 | | | 299 | 8 | |
| `kit` (core files) | 8,129 | 2,995 | 1,473 | | | 2,588 | 1,073 | |
| `kit/domain` | 7,745 | | | | | 7,745 | | |
| `util` | 7,553 | 5,520 | | 2,028 | | | | 5 |
| `kit/composites` | 6,856 | 1,881 | 1,031 | | | 3,944 | | |
| `services` | 6,240 | 5,568 | | 672 | | | | |
| `kit/display` | 4,455 | | | | 3,996 | 459 | | |
| `integrations` | 4,148 | 2,991 | | 1,157 | | | | |
| `kit/samples` | 2,864 | | 376 | | | 2,411 | 77 | |
| `testing` | 219 | | | 219 | | | | |

The split is clear:
- `services`, `integrations` and `util` are almost all real use.
- The kit, 39,375 lines, is where the weight is.

---

## Where the bloat comes from

Most of it traces back to one decision: the kit was planned from a
catalogue of every UI element in thirteen toolkits, not from what a shell
needs.

- **`kit/catalogue.lua`** (969 lines, generated) lists 960 elements drawn
  from libadwaita, GTK4, KDE, WinUI, AppKit, egui, ImGui, iced, Slint,
  Flutter, Compose, web design systems and instrument/HUD kits. Its source,
  `xtra/ui-elements.csv`, and its generator are gitignored. The output is
  shipped, but the input is not in the repository.
- **`kit/contract.lua`** turns that catalogue into what every theme must
  draw:
  - 235 named archetype widgets;
  - 62 display widgets;
  - 91 domain instruments (aviation, game HUD, sci-fi HUD);
  - 30 composites plus 11 row variants;
  - 72 kit functions each theme must export.
- **`kit/widgets.lua`** generates a constructor for every one of those
  names.

Against that, what really gets built:

| Contract part | Named | Built by eva | Built by the apps | Built by anything real |
| --- | ---: | ---: | ---: | ---: |
| archetype widgets | 235 | 18 | 17 | ~30 (13%) |
| display widgets in `lib.kit.display` | 62 | 0 | 1 (`separator`) | 1 (2%) |
| domain instruments | 91 | 0 | 0 | 0 |
| composites | 30 | 7 | 7 | 12 (40%) |

The 18 widgets eva builds are `area`, `icon`, `pill`, `segment`,
`shelf_arrow`, `switch`, `scroll_bar`, `slider`, `scroll_view`, `shelf`,
`swatch_grid`, `tabs`, `file_list`, `list`, `colour_plane`, `entry`,
`password` and `search`. It also runs Collection, Drag, Navigation and
Popup headless.

### The cost does not stop at the library

The contract binds every theme. eva's specs render the whole gallery
(`tests/kit_gallery_spec.lua`, `tests/composites_gallery_spec.lua`), so
each eva theme has to draw everything:

- **Material:** 155 widget looks defined, in 5,744 lines of `widgets/`
  skins.
- **Tsugumori:** 155 widget looks defined, in 6,105 lines of `widgets/`
  skins.

eva builds about 20 of those widgets. In each theme, roughly 1,300–1,500
lines skin archetypes eva never instantiates: Sheet, Canvas, Transform,
Overflow, Dock, Roving, Shell, Form and Disclosure. More lines go to
variants of the archetypes it does use, such as 44 kinds of Press for the
6 it uses.

Those skins also pull library code into eva at start-up:
- `lib.kit.canvas`, `lib.kit.form` and `lib.kit.roving`;
- the default skin's `canvas_dock`, `widgets.canvas`, `widgets.dock` and
  `widgets.transform`.

eva loads these only so its skins for those archetypes can build. It
never shows any of them.

---

## Findings by area

### `kit/domain`: 7,745 lines, gallery only

Five instrument families:
- `aviation` (1,586 lines): an airspeed tape, an attitude indicator, an
  HSI and more;
- `hud_game` (2,030);
- `hud_fui` (1,739);
- `audio` (1,297);
- `editor` (556);
- plus 537 lines of samples.

They are registered into every kit by `lib.kit.display.install`, and only
the gallery draws them. No shell, app or demo does. This is the largest
single block of unused code, and none of it is shell UI.

### Gallery content shipped as library modules: about 4,700 lines

| What | Lines |
| --- | ---: |
| `kit/samples/*` (one file per archetype) | 2,864 |
| `kit/composites/samples_*` | 375 |
| `kit/display/samples_*` | 459 |
| `kit/catalogue.lua` | 969 |

Domain samples are counted with `kit/domain` above. All of this is
fixtures for specs and the gallery, installed as part of the library.
The editor example app builds its main view from one of them,
`kit.samples.canvas` (376 lines).

### Composites nothing real uses: 18 of 30, 3,569 lines

| Composite | Lines |
| --- | ---: |
| `emoji_picker` | 429 |
| `command_palette` | 330 |
| `notification_stack` | 308 |
| `tab_view` | 259 |
| `shortcuts_window` | 257 |
| `kanban` | 229 |
| `tour` | 222 |
| `dashboard` | 209 |
| `transfer_list` | 184 |
| `time_picker` | 176 |
| `wizard` | 169 |
| `carousel` | 140 |
| `status_bar` | 128 |
| `picker` | 124 |
| `menu_button` | 114 |
| `tag_input` | 111 |
| `search_bar` | 97 |
| `media_controls` | 83 |

Six of them have libadwaita counterparts: `tab_view`, `carousel`,
`shortcuts_window`, `menu_button`, `search_bar` and the toast mode of
`notification_stack`. Under `ADWAITA_PLAN.md` their logic moves to Rust,
so they get rewritten either way. The other twelve have no user and no
plan.

Used composites:
- **eva:** `calendar`, `colour_picker`, `combo_box`, `date_picker`,
  `file_chooser`, `font_picker`, `input_group`.
- **The apps:** `about_dialog`, `header_bar`, `rows`, `sidebar`,
  `toolbar`.

### Display widgets: 3,996 lines loaded, never called

`lib.kit.display.install` runs in every kit, eva's included, and requires
all six groups at start-up. That is 3,409 lines, plus `layout` (446),
`util` (86) and the installer (55): `charts` (1,195), `readings` (832),
`status` (730), `media` (246), `text` (212), `structure` (194).

The probes recorded **no calls from eva** and one from the apps
(`structure.separator`). eva's themes draw their own `gauge`, `spectrum`,
`avatar`, `badge` and so on. So eva has two implementations of these
widgets, and the library's is dead weight.

### Archetype modules (Lua) with no real user: 2,588 lines

- `form` (674), `sheet` (588), `roving` (393), `overflow` (339), `drag`
  (307), `transform` (287).
- The apps' toolbar runs the Overflow and Roving archetypes headless
  without these wrappers, and eva's uses of Drag are headless too.
- Their specs are kept for them alone: `kit_form_spec` 95,
  `kit_sheet_spec` 147, `kit_roving_overflow_spec` 209,
  `kit_transform_spec` 119.

### The default skin: 9,326 lines, used by the apps

This is the look a configuration gets when it brings no theme, so it is
real infrastructure. But it is sized by the contract: it skins every
contract widget, and it loads every archetype's skin at start-up (Sheet,
Plane and Overflow included). The two apps build about 17 widgets.

### Rust: `morf-kit`, 11,423 lines including its tests

| Archetype | Lines | Built by |
| --- | ---: | --- |
| Canvas | 2,001 | editor app only |
| Dock | 1,102 | editor app only |
| Range | 816 | eva |
| Selection | 704 | eva |
| Transform | 692 | **nothing** |
| Sheet (spreadsheet grid) | 500 | **nothing** |
| Collection | 441 | eva |
| Drag | 421 | eva (headless) |
| Plane | 411 | eva, once (`colour_plane` in lule's colour picker) |
| TextField | 368 | eva |
| Press (+ group 37) | 397 | eva |
| Scroll | 345 | eva |
| Shell | 338 | settings app only |
| Navigation | 333 | eva (headless, settings pages), apps |
| Form | 312 | **nothing** |
| Overflow | 281 | apps' toolbar (headless) |
| Roving | 266 | apps' toolbar (headless) |
| Disclosure | 188 | settings app only |
| Popup | 171 | eva |
| base (control, access, slots, module, tokens, value) | 1,221 | everything |

- 1,504 lines (Transform, Sheet, Form) have no user.
- 3,103 lines (Canvas, Dock) serve one example app.
- What eva runs is about 4,400 lines plus the base. That part is lean.

### `util`: 7,553 lines, mostly real

- **eva uses 5,520 lines:** `osk`, `poll`, `settings`, `settings_pages`,
  `capture`, `annotation`, `auth`, `authsteps`, `pattern`, `emoji` (with
  1,788 lines of `emoji_data`), `material`, `m3shapes`, `frecency`,
  `units`, `ringer`, `remote`, `equalizer`, `spectrum`, `channel`.
- **Duplication: two on-screen keyboards.** `util.osk` (824, eva) and
  `util.board` (577, demos only). `board`'s header says the layout and
  keys "exist once", but `osk` does not use it.
- **One shell's code in the shared library:**
  - `util.palette` (1,326): impasto's wallpaper-to-theme pipeline;
  - `util.align` (54) and `util.component` (71): one demo each.
- **Dead:** `util.m3shapes_paths` (5 lines).
- **Three colour sources side by side:** `util.material` (eva),
  `util.palette` (impasto) and `integrations.lule` (eva reads lule's
  output). They are not duplicates, but they overlap.

### `services`: 6,240 lines, real

- **eva uses 5,568 lines**, which is all of them except `logind` (413,
  impasto) and `dbusmenu`/`tray_watcher` (259, panacea and the demos).
- `registry`, `binder` and `catalog` (916) are the declarative binding
  layer from `PLAN.md` §7, and eva runs it.
- Nothing to cut here.

### `integrations`: 4,148 lines, real but personal

- **eva uses 2,991 lines:** hyprland (1,344), hyprland_config (655),
  weather, lyrics, taskwarrior, lule, tor, clipse.
- **impasto alone uses 1,157 lines:** `claude_usage` (543), `github`
  (330), `packages` (284). Each also has Rust tests in
  `crates/engine/morf-lua/tests/engine/lib_sysinfo_web/`.
- The rules allow opinionated integrations here. These three serve one
  example shell.

### Found along the way

Loading eva logs "a Item with N children sits at the top level with no
surface; it is not drawn" three times. eva builds nodes it never places,
in both styles.

---

## What a cut would look like

Nothing below has been done. Each tier is independent.

| Tier | What | Lines out of `library/lib` |
| --- | --- | ---: |
| 1 | Delete `kit/domain`; move every `samples*` file and the catalogue out of the library (into the gallery specs, or `examples/gallery`); move `samples.canvas` into the editor app | ~12,400 |
| 2 | Drop or move the 18 unused composites and the six unused archetype modules; drop Transform, Sheet and Form from morf-kit (another 1,504 Rust lines) | ~6,150 |
| 3 | Stop installing display widgets into every kit; keep only what a theme asks for, or leave them to themes, as eva already does | ~4,000 |
| 4 | Move one shell's code into that shell: `palette`, `claude_usage`, `github`, `packages` into impasto; `align`, `component` into the demos; merge `board` into `osk` or move it to the demos | ~3,200 |

After all four, `library/lib` would be about 32,000 lines. Cutting the
contract down to what remains would then shrink the default skin and
eva's two themes by several thousand lines more. eva could also drop its
gallery specs, or shrink them to the new contract.

**How this fits `ADWAITA_PLAN.md`.** Cut first, then build parity. Step 1
of that plan resets the contract to libadwaita's list instead of thirteen
toolkits'. Doing it against the trimmed kit means no work goes into
widgets that are about to be deleted.

---

## Appendix: every module

| Module (`lib.`) | Lines | Used by | What it is |
| --- | ---: | --- | --- |
| `integrations.claude_usage` | 543 | impasto | Claude Code's token usage, from its own transcripts. |
| `integrations.clipse` | 52 | eva | clipse's clipboard history (github.com/savedra1/clipse), read as it keeps it: `~/.config/clipse/clipbo… |
| `integrations.github` | 330 | impasto | A GitHub user's contribution calendar: a year of days, each with a count and the 0-4 shade GitHub draw… |
| `integrations.hyprland` | 1344 | eva | Hyprland, over its own two sockets. |
| `integrations.hyprland_config` | 655 | eva | Hyprland's configuration, changed at run time: options, animations, monitor rules, workspaces moved be… |
| `integrations.lule` | 93 | eva | lule's colours: what `lule create ... |
| `integrations.lyrics` | 249 | eva | Synced lyrics for what is playing. |
| `integrations.packages` | 284 | impasto | Pending updates and installed counts, from the package managers present. |
| `integrations.taskwarrior` | 135 | eva | Taskwarrior's CLI is the database boundary: no private storage formats, no shell interpolation. |
| `integrations.tor` | 68 | eva | Tor, as a switch: the system's tor service started and stopped -- never enabled, so it is off again af… |
| `integrations.weather` | 395 | eva | The weather, from keyless web APIs. |
| `kit.app` | 120 | example apps | Applications: a configuration run as a program with windows rather than as a shell (`morf app <app.lua… |
| `kit.canvas` | 604 | example apps | Canvases (the Canvas archetype): a world a viewport looks into, panned and zoomed, holding items that … |
| `kit.caption` | 50 | eva | A bounded button legend. |
| `kit.catalogue` | 969 | tooling | The catalogue of UI elements (xtra/ui-elements.csv: libadwaita, GTK4, KDE, WinUI, AppKit, egui, ImGui,… |
| `kit.check` | 104 | tooling | Conformance: a theme's kit against the contract (contract.lua). |
| `kit.collection` | 454 | eva | Collections (the Collection archetype): lists, grids, tables and trees over a model, through the engin… |
| `kit.composites` | 37 | eva | Composites: widgets built from archetypes (library/lib/kit/contract.lua, `composites`), shared by ever… |
| `kit.composites.about_dialog` | 203 | example apps | An about dialog: an application's name, version, icon and links, with pages for its credits and its le… |
| `kit.composites.calendar` | 346 | eva | A calendar (composite: Selection days + Navigation months), inline. |
| `kit.composites.carousel` | 140 | gallery only | A carousel (composite: Navigation carousel + Selection dots + Drag swipe). |
| `kit.composites.colour_picker` | 209 | eva | A colour picker (composite: Plane + Range hue + Range alpha + TextField hex + Selection swatches; in a… |
| `kit.composites.combo_box` | 330 | eva | A combo box: a closed field showing the current item that opens a list of them (Press + Popup + Select… |
| `kit.composites.command_palette` | 330 | gallery only | A command palette: a search field over a grouped list of commands that narrows as it is typed into, in… |
| `kit.composites.dashboard` | 209 | gallery only | A dashboard (composite: a grid Collection of tiles + Drag to rearrange). |
| `kit.composites.date_picker` | 152 | eva | A date picker and a date range picker (composite: Press + Popup + Selection day grid + Navigation mont… |
| `kit.composites.emoji_picker` | 429 | gallery only | An emoji picker (composite: TextField search + Selection category tabs + Selection grid of emoji). |
| `kit.composites.file_chooser` | 499 | eva | A file chooser (composite: Shell + Collection of files + Navigation of folders + TextField path and na… |
| `kit.composites.font_picker` | 184 | eva | A font picker (composite: TextField search + Collection + preview). |
| `kit.composites.header_bar` | 71 | example apps | A header bar (composite: Shell slot + Press): a window's title, its own controls at the start and end,… |
| `kit.composites.input_group` | 124 | eva | An input group: a text field with parts before and after it, on one ground -- "−" [value] "+", "https:… |
| `kit.composites.kanban` | 229 | gallery only | A kanban board (composite: a Collection per column + Drag to move cards). |
| `kit.composites.media_controls` | 83 | gallery only | Media controls: transport presses, a seek bar between the position and the length, and a volume slider… |
| `kit.composites.menu_button` | 114 | gallery only | A menu button: a button that opens a menu under it (Press + Popup); with `split = true`, a split butto… |
| `kit.composites.notification_stack` | 308 | gallery only | A notification stack: cards that come in at the top, newest first, stay for a while and go -- by a swi… |
| `kit.composites.picker` | 124 | gallery only | A picker: choose one of a set from a popup grid or list (Press + Popup + Selection) -- an icon, a size… |
| `kit.composites.rows` | 334 | example apps | Preference rows (a row frame with a Press, Range, TextField or Disclosure in it), the boxed group they… |
| `kit.composites.samples_inputs` | 106 | gallery only | Gallery samples for composites (inputs): name -> function(kit, composites) returning a node that fits … |
| `kit.composites.samples_layouts` | 104 | gallery only | Gallery samples for composites (layouts): name -> function(kit, composites) returning a node that fits… |
| `kit.composites.samples_pickers` | 42 | gallery only | Gallery samples for composites (pickers): name -> function(kit, composites) returning a node that fits… |
| `kit.composites.samples_surfaces` | 123 | gallery only | Gallery samples for composites (surfaces): name -> function(kit, composites) returning a node that fit… |
| `kit.composites.search_bar` | 97 | gallery only | A search bar: a search field that slides into view on Ctrl+F (or its toggle) and away on Escape (TextF… |
| `kit.composites.shortcuts_window` | 257 | gallery only | A shortcuts window: an application's keys, grouped, with a search that narrows them, in a dialog (Popu… |
| `kit.composites.sidebar` | 188 | example apps | A sidebar (composite: Selection items + Disclosure sections). |
| `kit.composites.status_bar` | 128 | gallery only | A status bar: a strip along the foot of a window or a panel with what is going on -- words, icons, bad… |
| `kit.composites.tab_view` | 259 | gallery only | A tab view with an overview (composite: Selection tabs + Navigation pages + Drag reorder + Collection … |
| `kit.composites.tag_input` | 111 | gallery only | A tag input: a text field that turns what is typed into removable chips (TextField + Press). |
| `kit.composites.time_picker` | 176 | gallery only | A time picker (composite: Range spins + Popup). |
| `kit.composites.toolbar` | 235 | example apps | A toolbar: a row of tool buttons, toggles and separators; what does not fit its width goes into an ove… |
| `kit.composites.tour` | 222 | gallery only | A tour (coachmarks): a card beside each thing it points out in turn, with a ring round that thing, Bac… |
| `kit.composites.transfer_list` | 184 | gallery only | A transfer list (composite: two Collections + Press move buttons). |
| `kit.composites.wizard` | 169 | gallery only | A wizard, or stepper (composite: Navigation wizard + Selection header). |
| `kit.contract` | 431 | eva | The widget contract: what every theme's kit provides, as data. |
| `kit.control` | 522 | eva | The base every kit control is built on: a Rust archetype (crates/morf-kit, `morf.kit.native`) owning i… |
| `kit.disclosure` | 43 | example apps | Disclosures (the Disclosure archetype): expanders, accordions, collapsible sections, "show more". |
| `kit.display` | 55 | loaded, never called | Display widgets every theme draws: text, media, status, readings, charts and structure that take no in… |
| `kit.display.charts` | 1195 | loaded, never called | Display widgets: charts. |
| `kit.display.layout` | 446 | loaded, never called | The layout maths of the structured charts (lib.kit.display.charts): where a pie's slices, a treemap's … |
| `kit.display.media` | 246 | loaded, never called | Display widgets: media. |
| `kit.display.readings` | 832 | loaded, never called | Display widgets: readings -- instruments that show one value (or a short run of them) and take no input. |
| `kit.display.samples_charts` | 190 | gallery only | Gallery samples for the charts display widgets: name -> function(kit) returning a node that fits a 260… |
| `kit.display.samples_media` | 35 | gallery only | Gallery samples for the media display widgets: name -> function(kit) returning a node that fits a 260x… |
| `kit.display.samples_readings` | 63 | gallery only | Gallery samples for the readings display widgets: name -> function(kit) returning a node that fits a 2… |
| `kit.display.samples_status` | 87 | gallery only | Gallery samples for the status display widgets: name -> function(kit) returning a node that fits a 260… |
| `kit.display.samples_structure` | 44 | gallery only | Gallery samples for the structure display widgets: name -> function(kit) returning a node that fits a … |
| `kit.display.samples_text` | 40 | gallery only | Gallery samples for the text display widgets: name -> function(kit) returning a node that fits a 260x1… |
| `kit.display.status` | 730 | loaded, never called | Display widgets: status. |
| `kit.display.structure` | 194 | loaded, never called | Display widgets: structure. |
| `kit.display.text` | 212 | loaded, never called | Display widgets: text. |
| `kit.display.util` | 86 | loaded, never called | What the display widgets share: reading a spec's values, placing a widget, and the few drawing moves e… |
| `kit.dock` | 355 | example apps | Docks (the Dock archetype): panels in a tree of splits and tab stacks that the user rearranges -- a ta… |
| `kit.domain.audio` | 1297 | gallery only | Domain instruments: audio -- the controls and scopes of a mixer, a synth and a mastering chain, over t… |
| `kit.domain.aviation` | 1586 | gallery only | Domain instruments: aviation -- the flight deck's tapes, dials and displays, drawn over the display wi… |
| `kit.domain.editor` | 556 | gallery only | Domain instruments: editor -- the editing surfaces of a synth and a sequencer, over the kit's archetyp… |
| `kit.domain.hud_fui` | 1739 | gallery only | Domain instruments: hud_fui -- the "fictional UI" pieces of a sci-fi heads-up display, drawn over the … |
| `kit.domain.hud_game` | 2030 | gallery only | Domain instruments: hud_game -- a game's heads-up display, drawn over the display widgets in a theme's… |
| `kit.domain.samples_audio` | 59 | gallery only | Gallery samples for the audio instruments: name -> function(kit) returning a node that fits a 260x190 … |
| `kit.domain.samples_aviation` | 98 | gallery only | Gallery samples for the aviation instruments: name -> function(kit) returning a node that fits a 260x1… |
| `kit.domain.samples_editor` | 21 | gallery only | Gallery samples for the editor instruments: name -> function(kit) returning a node that fits a 260x190… |
| `kit.domain.samples_hud_fui` | 145 | gallery only | Gallery samples for the hud_fui instruments: name -> function(kit) returning a node that fits a 260x19… |
| `kit.domain.samples_hud_game` | 214 | gallery only | Gallery samples for the hud_game instruments: name -> function(kit) returning a node that fits a 260x1… |
| `kit.drag` | 307 | gallery only | Drags (the Drag archetype): split panes, resize grips, reorderable rows, swipe-to-dismiss, window move. |
| `kit.form` | 674 | gallery only | Forms (the Form archetype): what a group of fields adds up to -- whether it can be sent, whether anyth… |
| `kit.navigation` | 98 | example apps | Navigations (the Navigation archetype): a navigation view's push and pop, a view stack, a carousel, a … |
| `kit.overflow` | 339 | gallery only | Overflows (the Overflow archetype): items sharing a line keep the ones that fit and put the rest behin… |
| `kit.popup` | 382 | eva | Popups (the Popup archetype) on the engine's overlay layer: menus, tooltips, dialogs, toasts, popovers… |
| `kit.press_style` | 22 | eva | Fade the whole disabled button once, so its caption, icon and background stay together. |
| `kit.roving` | 393 | gallery only | Rovings (the Roving archetype): one Tab stop for a group of controls the arrows move between -- a tool… |
| `kit.samples` | 77 | tooling | Gallery samples for every archetype's widgets: samples/<archetype>.lua (snake case: press, range, text… |
| `kit.samples.canvas` | 376 | example apps | Gallery samples for the Canvas archetype's widgets: each a canvas as it is used -- a node graph of a f… |
| `kit.samples.collection` | 215 | gallery only | Gallery samples for the Collection widgets (lib.kit.samples): each a working collection over a small m… |
| `kit.samples.disclosure` | 86 | gallery only | Gallery samples for the Disclosure archetype's widgets (samples/init.lua): each the widget as a config… |
| `kit.samples.dock` | 129 | gallery only | Gallery samples for the Dock archetype's widgets: each a dock as it is used -- an editor's area of fil… |
| `kit.samples.drag` | 273 | gallery only | Gallery samples for the Drag archetype's widgets (lib.kit.drag): each a small working layout -- panes … |
| `kit.samples.form` | 107 | gallery only | Gallery samples for the Form archetype's widgets (samples/init.lua): each a form as an application wou… |
| `kit.samples.navigation` | 134 | gallery only | Gallery samples for the Navigation archetype's widgets (samples/init.lua): each a set of pages the wid… |
| `kit.samples.overflow` | 50 | gallery only | Gallery samples for the Overflow archetype's widgets (samples/init.lua): each a line given more than i… |
| `kit.samples.plane` | 14 | gallery only | Gallery samples for the Plane widgets (lib.kit.samples): each one as a configuration would make it, at… |
| `kit.samples.popup` | 348 | gallery only | Gallery samples for the Popup archetype's widgets (lib.kit.samples): each gives the button that opens … |
| `kit.samples.press` | 103 | gallery only | Gallery samples for the Press archetype's widgets (see samples/init.lua): each as an application would… |
| `kit.samples.range` | 28 | gallery only | Gallery samples for the Range widgets (lib.kit.samples): each one as a configuration would make it, at… |
| `kit.samples.roving` | 63 | gallery only | Gallery samples for the Roving archetype's widgets (samples/init.lua): each a group as a configuration… |
| `kit.samples.scroll` | 113 | gallery only | Gallery samples for the Scroll widgets (see samples/init.lua): a page of settings rows, a sheet that s… |
| `kit.samples.selection` | 204 | gallery only | Gallery samples for the Selection archetype's widgets (samples/init.lua): each the widget as a configu… |
| `kit.samples.sheet` | 114 | gallery only | Gallery samples for the Sheet archetype's widgets: each a grid as it is used -- a budget in a spreadsh… |
| `kit.samples.shell` | 201 | gallery only | Gallery samples for the Shell archetype: each widget as a miniature application window -- a header, a … |
| `kit.samples.text_field` | 121 | gallery only | Gallery samples for the TextField archetype's widgets (lib.kit.samples): each field as an application … |
| `kit.samples.transform` | 108 | gallery only | Gallery samples for the Transform archetype's widgets (lib.kit.transform): each as it is used -- an in… |
| `kit.scroll` | 179 | eva | Scrolled views (the Scroll archetype) around the engine's flickable. |
| `kit.selection` | 491 | eva | Selections (the Selection archetype): tabs, segmented choices, list and grid selection, swatch grids. |
| `kit.sheet` | 588 | gallery only | Sheets (the Sheet archetype): a grid the keyboard walks one cell at a time -- a spreadsheet, a data gr… |
| `kit.shell` | 253 | example apps | Application shells (the Shell archetype): a window's header bar, sidebar, content, inspector, bottom b… |
| `kit.skin` | 116 | eva | Skins: how a theme draws the archetypes (crates/morf-kit). |
| `kit.skins.default` | 115 | example apps | The default kit: a complete, Adwaita-like (GNOME HIG) kit, so an application needs no theme. |
| `kit.skins.default.canvas_dock` | 331 | example apps | The default kit's Canvas and Dock skins, in the Adwaita manner: a view ground with a hairline grid, ca… |
| `kit.skins.default.check` | 8 | tooling | A configuration that loads the default kit as `require("kit")` and draws nothing, for `morf check --kit`. |
| `kit.skins.default.components` | 1274 | example apps | The default kit's components: the contract's functions (lib.kit.contract) drawn in an Adwaita-like sty… |
| `kit.skins.default.display_style` | 41 | example apps | The default kit's style for the shared display widgets (lib.kit.display): flat fills with Adwaita's co… |
| `kit.skins.default.skins` | 907 | example apps | The default kit's skins for every archetype (lib.kit.skin), in the Adwaita manner: grey push buttons t… |
| `kit.skins.default.tabbed` | 62 | example apps | A panel with tabs for the default kit: the view switcher (`kit.tabs`) along the top, the pages under i… |
| `kit.skins.default.tokens` | 121 | example apps | The default kit's tokens: an Adwaita-like (GNOME HIG) palette in three variants that share every name,… |
| `kit.skins.default.widgets.canvas` | 543 | example apps | The default kit's looks for each Canvas widget, over the archetype's skin (canvas_dock.lua), in the Ad… |
| `kit.skins.default.widgets.collection` | 667 | example apps | The default kit's looks for each Collection widget, in the Adwaita manner: plain rows with a wash unde… |
| `kit.skins.default.widgets.disclosure` | 280 | example apps | The default kit's looks for each Disclosure widget, in the Adwaita manner: an expander row's card, an … |
| `kit.skins.default.widgets.dock` | 110 | example apps | The default kit's looks for each Dock widget, over the archetype's skin (canvas_dock.lua), in the Adwa… |
| `kit.skins.default.widgets.drag` | 425 | example apps | The default kit's looks for the Drag archetype's widgets, in the Adwaita manner: hairline dividers who… |
| `kit.skins.default.widgets.form` | 48 | example apps | The default kit's look for the Form widgets, in the Adwaita manner: an error banner tinted with the de… |
| `kit.skins.default.widgets.navigation` | 287 | example apps | The default kit's looks for each Navigation widget, in the Adwaita manner. |
| `kit.skins.default.widgets.overflow` | 113 | example apps | The default kit's looks for each Overflow widget, in the Adwaita manner. |
| `kit.skins.default.widgets.plane` | 305 | example apps | The default kit's looks for the Plane widgets beyond the colour plane (skins.lua), in the libadwaita m… |
| `kit.skins.default.widgets.popup` | 305 | example apps | The default kit's look for each Popup widget, in the Adwaita manner: raised grounds with a hairline an… |
| `kit.skins.default.widgets.press` | 650 | example apps | The default kit's looks for the Press widgets, each the libadwaita style class its name promises: a su… |
| `kit.skins.default.widgets.range` | 592 | example apps | The default kit's looks for the Range widgets beyond the plain scale (skins.lua draws the slider, the … |
| `kit.skins.default.widgets.roving` | 106 | example apps | The default kit's looks for each Roving widget, in the Adwaita manner. |
| `kit.skins.default.widgets.scroll` | 227 | example apps | The default kit's looks for the Scroll widgets, in the Adwaita manner: a scrolled view's undershoot ha… |
| `kit.skins.default.widgets.selection` | 748 | example apps | The default kit's looks for each Selection widget, in the Adwaita manner: linked buttons on a trough, … |
| `kit.skins.default.widgets.sheet` | 266 | example apps | The default kit's looks for each Sheet widget, in the Adwaita manner: a spreadsheet of hairline grid l… |
| `kit.skins.default.widgets.shell` | 78 | example apps | The default kit's Shell looks, in the libadwaita manner: the window's ground, a header in the header t… |
| `kit.skins.default.widgets.text_field` | 418 | example apps | The default kit's look for each TextField widget, in the Adwaita manner: soft grey wells with 6 px cor… |
| `kit.skins.default.widgets.transform` | 299 | gallery only | The default kit's looks for the Transform archetype and each of its widgets, in the Adwaita manner: a … |
| `kit.text_field` | 131 | eva | Text fields (the TextField archetype) around the engine's text input. |
| `kit.transform` | 287 | gallery only | Transforms (the Transform archetype): a box the user moves by its body, resizes from its edges and cor… |
| `kit.widgets` | 217 | eva | Every widget the Press, Range, Plane and Selection archetypes make (contract.lua), as a constructor: `… |
| `services.accounts` | 72 | eva | The machine's people: the accounts a login screen lists, and the picture each keeps of itself. |
| `services.binder` | 608 | eva | The registry's binder: one descriptor's objects, subscriptions, debounce and teardown, kept as reactiv… |
| `services.bluez` | 359 | eva | Bluetooth, from BlueZ, as reactive state a shell can bind to. |
| `services.catalog` | 9 | eva | The system services `morf.service` binds, by name: the module that defines each and what it holds. |
| `services.dbus_client` | 385 | eva | The shared half of every system-service library: calling, reading properties, and hearing about change… |
| `services.dbusmenu` | 114 | panacea, demos | A tray item's menu, read off the bus and handed to `morf.menu`. |
| `services.keyboards` | 56 | eva | Whether a real keyboard is attached: what an on-screen keyboard asks before it offers itself. |
| `services.keyring_agent` | 73 | eva | GCR keyring prompts. |
| `services.logind` | 413 | impasto | systemd-logind: the session, the power buttons, the lid and the backlight. |
| `services.modem` | 173 | eva | The mobile network: a phone's modem, through ModemManager, and mobile data on or off through NetworkMa… |
| `services.mpris` | 373 | eva | Media players, over MPRIS, as reactive state a shell can bind to. |
| `services.networkmanager` | 759 | eva | NetworkManager, as reactive state a shell can bind to. |
| `services.notifications` | 257 | eva | A notification server, in the configuration. |
| `services.polkit_agent` | 324 | eva | A polkit authentication agent, in the configuration. |
| `services.registry` | 299 | eva | System services described as data, and the one binder that runs them. |
| `services.sessions` | 61 | eva | The sessions a login screen can start: the desktop entries in wayland-sessions and xsessions, as greet… |
| `services.sysinfo` | 1362 | eva | The machine, from /proc and /sys. |
| `services.tray_watcher` | 145 | panacea, demos | The tray watcher, in the configuration. |
| `services.upower` | 172 | eva | Batteries and power profiles, as reactive state a shell can bind to. |
| `services.vpns` | 226 | eva | The VPNs a machine runs beside NetworkManager's own, each through its own command -- what it says, and… |
| `testing.fake_hyprland` | 219 | impasto, specs | A fake Hyprland for specs: both sockets under `<runtime>/hypr/<signature>`, answering like a Lua-confi… |
| `util.align` | 54 | one demo | The bar layout: something at the left, something at the right, and something in the middle. |
| `util.annotation` | 183 | eva | Theme-independent screenshot document. |
| `util.auth` | 300 | eva | The two doors a login screen has, behind one interface: a lock's, asked through PAM, and a greeter's, … |
| `util.authsteps` | 107 | eva | Where an authentication somewhere on the machine has got to -- sudo looking for a face, waiting at the… |
| `util.board` | 577 | demos | The on-screen keyboard, as a board rather than as a program. |
| `util.capture` | 357 | eva | Reusable screenshot acquisition, compositor geometry and output actions. |
| `util.channel` | 38 | eva | Data channels for what draws them (`ui.Path { series = ch.id, plot = ... |
| `util.component` | 71 | one demo | A component: a model, the messages that change it, and a view of it. |
| `util.emoji` | 37 | eva | Emoji by name: every one lib/util/emoji_data.lua knows (Unicode's own names), searched fuzzily. |
| `util.emoji_data` | 1788 | eva | Every emoji this list knows, with its Unicode name: { char, name }. |
| `util.equalizer` | 157 | eva | Managed PipeWire smart filter. |
| `util.frecency` | 145 | eva | What gets launched often and lately, remembered, for ranking a launcher. |
| `util.m3shapes` | 56 | eva | UI/animation policy for the engine's native rounded-shape geometry. |
| `util.m3shapes_paths` | 5 | nothing | Compatibility lookup; canonical outlines live in the native geometry module. |
| `util.material` | 326 | eva | Material 3 colour schemes, from a colour or from a picture. |
| `util.osk` | 824 | eva | An on-screen keyboard, whole: for a shell on a desk, a phone and a lock screen alike. |
| `util.palette` | 1326 | impasto | A desk's colours from its painting. |
| `util.pattern` | 21 | eva | Pattern numbering is row-major. |
| `util.poll` | 450 | eva | Polling, shared by the libraries that watch the machine and the web. |
| `util.remote` | 104 | eva | Files on the web, as files on disk: a URL fetched once into the cache, and its local path from then on. |
| `util.ringer` | 83 | eva | Sound, vibrate or silent: how the machine calls for attention -- a phone's ring switch, for any shell. |
| `util.settings` | 246 | eva | A shell's preferences: defaults in the configuration, what the user changed in a JSON file, and every … |
| `util.settings_pages` | 81 | eva | A settings tree: routes are stable keys, Back follows parents, and the breadcrumb is derived from the … |
| `util.spectrum` | 95 | eva | An audio visualiser's bars, from the bands `morf.audio.monitor` measures. |
| `util.units` | 122 | eva | Unit conversion, as a launcher's calculator reads it: "10ft in m", "72 f to c", "5 gb in mib", "90 km/… |

The 11 `*.permissions.lua` manifests beside the integrations (51 lines) are not listed.
