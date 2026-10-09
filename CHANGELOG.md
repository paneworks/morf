# Changelog

## [0.2.7] - 2026-10-10

### <!-- 0 -->⛰️ Features

- Add a Roaming quick-settings tile that allows or blocks mobile data roaming on every saved mobile profile.
- Report each saved mobile profile's home-only setting from the NetworkManager service.

## [0.2.6] - 2026-10-09

### <!-- 1 -->🐛 Bug Fixes

- Remember manually hidden keyboards per application for the current login session; showing the keyboard again restores automatic opening for that application.
- Restore typing directly into laptop lock screens and configure lock and display idle timeouts independently.
- Give NixOS authentication prompts access to the graphical session.
- Keep clipped dashboard pages from intercepting input outside their visible area.

### <!-- 4 -->⚡ Performance

- Process reactive updates in dependency order without repeatedly scanning every pending binding.
- Keep inactive dashboard pages idle when another page changes.
- Skip rendering fully clipped pages and restrict text reflow checks to labels that need them.
- Prepare hidden text in short idle slices so opening a panel does less glyph work.

## [0.2.5] - 2026-10-09

### <!-- 1 -->🐛 Bug Fixes

- Avoid the Adreno shader compiler crash in phone lockscreen and greeter rendering.
- Restore velocity-based touch scrolling, preserve fling distance across slow frames, and stop inertia when touched.
- Make phone panels, pages and workspace previews follow the finger and settle using release velocity.
- Open Apps from the left edge and Web from the right, with cancellation and authentication guards.
- Require a deliberate double tap or the power key to wake sleeping phone authentication screens.
- Validate neighbouring pattern dots and show pattern login only for enrolled accounts.
- Use a fading phone scrollbar and hide the application list in the phone bar.
- Correct compact dialog captions, disabled controls, and shared layout spacing.

### <!-- 0 -->⛰️ Features

- Provide upstream NixOS modules for desktop, greeter, lockscreen, phone gestures and protected pattern enrollment.
- Let NixOS select the packaged UI while preserving appearance preferences.
- Include the keyring dialog bridge in Nix packages and report PAM authentication progress without handling passwords.

## [0.2.4] - 2026-10-06

### <!-- 0 -->⛰️  Features

- Redesigned launcher
- One core scale for every window, chosen by the configuration

### <!-- 5 -->🎨 Styling

- Bool::then_some where the value is already made (clippy)

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Theme

## [0.2.3] - 2026-10-05

### <!-- 0 -->⛰️  Features

- MORF_PRESENT_MODIFIER picks the layout of presented buffers; MORF_GPU_LOG names them

### <!-- 1 -->🐛 Bug Fixes

- Presented buffers are linear whenever the compositor takes linear

### <!-- 4 -->⚡ Performance

- A theme colour set before anything is shown, or to one that looks the same, does not fade
- A path is rasterized over its outline, not its whole node

## [0.2.2] - 2026-10-04

### <!-- 1 -->🐛 Bug Fixes

- Verify cached outputs and styled text

## [0.2.1] - 2026-10-04

### <!-- 0 -->⛰️  Features

- Cache engine and Lua releases
- MORF_DAMAGE_LOG also says when the whole surface is redrawn because the engine forgot it
- MORF_DAMAGE_LOG=1 names what made a large damage area

### <!-- 4 -->⚡ Performance

- The machine's fonts are scanned once per process, not once per renderer
- Commands pushed along by an insertion are not damage
- The frame pacer judges a surface by what painting costs now
- A shape joining or leaving a field damages where it is, not the whole field

## [0.2.0] - 2026-10-04

### <!-- 0 -->⛰️  Features

- Morf-host, with everything that runs a configuration
- Morf-runtime, with handlers and timers
- Morf-desktop, on its own queue; gamma control moves there
- Morf.window.toplevel; floating kept as its old name
- The Backend trait, implemented on Wayland and headless
- Morf-value -- colours and the boundary value at the bottom of the graph
- Morf as a flake package
- Nine workspaces a rail, Tsugumori rulers that bracket groups of three, a calmer media cover
- A canvas's selected box resizes by its handles
- A modifier tapped alone is a shortcut; Alt reaches the menu bar
- Hold buttons, slide to confirm, radial and pie menus, tumblers, a shortcut recorder
- Roving and Overflow in every look; the toolbar uses both
- Transform in every look; a list made before its size keeps its pool
- Sheet -- grids walked cell by cell, in every look
- Form -- fields that add up, sent and reset, in every look
- The contract reaches stage 22 -- Transform, Sheet, Roving, Form, Overflow
- Transform, Sheet, Roving, Form and Overflow archetypes; hold, slide, radial and capture
- Lists and grids take their size as a binding; the editor keeps its layout
- Popups play their exit; Material flat and text buttons take their ink
- Every Popup and TextField widget its own look; per-widget defaults
- Canvas and Dock in every look, and an editor app
- Every Selection, Disclosure and Navigation widget its own look
- Every Scroll, Collection, Drag and Shell widget its own look and behaviour
- Every Press widget its own look in all three looks
- Every Range and Plane widget its own look in all three looks
- A text input's highlights -- a code editor's colours over what is typed
- Canvas and Dock glue and default skins; knobs turn, joysticks spring, hue wheels are polar
- Canvas and Dock archetypes; pointer events carry the held modifiers
- Right to left
- Applications and the default look
- Every composite, the audio and editor instruments, and glue fixes
- Aviation and HUD instruments, in both themes
- Every display widget, in both themes
- Data channels, and geometry for the marks a style draws
- Accessibility -- an AT-SPI tree, built only while a screen reader asks
- Disclosure, Drag and Navigation, and caelestia's on them
- Collection and recycling
- Popup, TextField and Scroll, and caelestia's on them
- Plane and Selection, and caelestia's choices on them
- Press and Range, and caelestia's controls on them
- Morf-kit, the Control base and the skin machinery
- Focus, shortcuts, gestures and the overlay layer
- The widget contract as data, checked
- Luna's native tier behind `jit`, and the costs measured
- Themes as style kits over one shared layout
- Refine capture and authentication with native rendering
- Add Tsugumori interaction flashes
- Add full Material and Tsugumori theming
- Add task planner and refine capture, panels, and history
- Sudo's steps as a pill from the top edge
- The lock hears a finger at rest and a face with the sheet
- Logre, the greeter and the lock as one bundle
- The lock per screen, whole on the main one
- A pattern for the lock and the greeter, and nothing else
- The greeter in two stages, as the lock
- The lock in two stages; the reader no longer locks people out
- Morf.broadcast, and the polkit dialog on the screen in use
- The shell as the polkit agent, its dialog from the top edge
- Tor as a quick setting, the bar's status one click
- VPNs as Mesh and Tunnel
- Wired, VPN and airplane mode in quick settings
- Mobile network and ring mode, for a phone
- The bar with the author's logo, the time in the middle
- A bar, on any of the four edges
- Battery and keep awake as quick-settings tiles
- A whole on-screen keyboard, with modes and a pattern pad
- An on-screen keyboard
- The Battery tab, its own design
- The Battery tab is the battery alone
- A Battery tab, and a Power page behind the battery row
- Ui.follow, and pills that stay with the panel
- A monitor can wait for the device it listens to
- A lock screen and a greeter
- Media tab visualiser bars, player volume, honest shuffle
- NVIDIA readings, and a roomier performance tab
- The performance tab laid out as Mission Center's
- The wallpaper is the compositor's unless asked
- Frame callbacks and slow layouts in the frame log
- The launcher laid out as Raycast lays its out
- A launcher after Raycast and Alfred
- The author's appy and browsy, in the launcher

### <!-- 1 -->🐛 Bug Fixes

- Make recipes name the crates as they are now; the greetd command test and doc match the installer
- A headless host's clock means its deltas exactly
- Morf check --kit default finds a library installed under XDG_DATA_DIRS
- An application's own surface gets an empty root
- With nothing focused, a key is offered to each node that takes keys in turn
- Overflow's keys in the contract
- Swipe actions open toward the trailing edge right to left; Material's spinner stops its clock
- Say whether every timer repeats
- An application runs beside the shell
- Graph_series takes any finite reading
- Authsteps follows one sudo, not every sudo at once
- Authsteps ignores a sudo that asked nobody
- Sudo's steps reach the shell while they happen
- The auth-step marker brings its own PATH
- The lock starts under a real session lock
- The polkit agent reads its helper again
- The bar's status icons only open quick settings
- Full has no number row unless asked
- Dev without the symbol and number rows
- No mode bar, a keyboard for code, rows inside the board
- Seven days where wttr.in placed you, and metric
- Dispatch on a Lua-configured Hyprland
- Motion nothing drives is painted

### <!-- 2 -->🚜 Refactor

- The engine steps stretching nodes from a frame's layout
- The engine answers a loop's questions and keeps its own scene bookkeeping
- Morf_runtime::Engine holds every engine subsystem; ReactiveState is the engine plus what only Lua can hold
- Morf-lua's Lua-driven tests are crate-level tests (tests/engine)
- Disposing an effect is Reactive::dispose_effect
- Morf.shared's values are morf-runtime's
- What a removed subtree takes with it from the graph and the windows is morf-runtime's
- Dropping a retainable and forgetting a removed node are Retained's
- What a frame's layout tells the runtime is morf-runtime's
- The clocks are morf-runtime's
- The session lock, the primary duty and the screens revision are morf-runtime's Session
- A configuration's lifecycle, retained nodes and scene revisions are morf-runtime subsystems
- List-model revisions and declarative states are morf-runtime's
- The log, screens, workspaces, toplevels and resource stats are morf-runtime's
- Platform requests and window declarations are morf-runtime's Requests and Declarations
- Motion lives in morf-runtime's animation: loops, follows, theme fades, exits, on_finished, behaviour and fling settings
- Events' routing, key targets, hover and press, and keys bubbling up in morf-runtime
- Morf check, render and test run on the Host, over the headless backend
- Caret, selection, undo, the input method and keyboard focus of text inputs in morf-runtime's editing
- The virtual list's window, pool, recycling and placement in morf-runtime's views
- The overlay stack, placement, dismissal and focus give-back in morf-runtime's overlays
- Morf-runtime's Handlers trait, so its subsystems call handlers without Lua
- The fs checks, file documents, watch and I/O hubs, D-Bus handlers and broadcast live in morf-io
- Http options, archive helpers, process views and socket views live in morf-io
- The terminals' feeding, fitting, input and bookkeeping live in morf-terminal
- The settings portal and desktop entry rescans and launches live in morf-system
- Morf.audio's rows, listeners, monitors and polling live in morf-audio
- One Host runs an output's windows, on any backend, with or without a GPU
- The host talks to dyn Backend, never LayerClient, past connecting
- The lock screen's surfaces are Windows like the rest
- Morf-host's loop, surfaces, paint and process code and morf-cli's config in files under 500 lines
- Morf-kit's canvas, dock, range, selection and transform in files under 500 lines
- Morf-terminal's emulator, morf-app's layer client and morf-desktop's data control in files under 500 lines
- Morf-audio's PipeWire backend and beat tests in files under 500 lines
- Morf-io's D-Bus, reactor, fs and watch code in files under 500 lines
- Morf-lua's vm execute, loader and config in files under 500 lines
- Morf-lua's system, time, module, window and geometry bindings in files under 500 lines
- Morf-lua's overlay, audio and ui bindings in files under 500 lines
- Morf-lua's terminals, services and state in files under 500 lines
- Morf-lua's fs bindings in files under 500 lines
- Morf-lua's io bindings in files under 500 lines
- Morf-lua's tests in files under 500 lines
- Morf-render's GPU backend in files under 500 lines
- Morf-render's backdrops, targets, dmabuf and LCD tests in files under 500 lines
- Morf-render's draw commands, paint and fields in files under 500 lines
- Morf-image's decoders in a file of their own
- Morf-text's terminal, fuzzy and font lookup in files under 500 lines
- Morf-value's HCT and region tests in files under 500 lines
- Morf-layout's layout pass in files under 500 lines
- Morf-scene in files under 500 lines
- Library/lib in services, integrations and util; docs
- Morf-cli names only morf-host and morf-value
- Morf.kit.native is morf-lua's; morf-kit names no Lua
- One Windows map for every declared window
- Morf-host's modules in the plan's directories
- The command line's own code apart from the host's
- Window declarations and platform requests move to morf-runtime
- Wake causes and clock grains move to morf-runtime
- Focus state and movement move to morf-runtime
- Shortcuts, key names and modifiers move to morf-runtime
- The gesture recogniser moves to morf-runtime
- The event vocabulary moves to morf-runtime
- The reactive scheduler moves to morf-runtime
- Handlers are Handlers, not stashed Lua closures
- Capture and foreign toplevels move to morf-desktop
- The clipboard over data control moves to morf-desktop
- Workspaces and idle notification move to morf-desktop
- Output power moves to morf-desktop
- Morf-cli opens, frames, commits and draws windows through Backend
- Morf-app speaks the plan's words
- Morf-app's neutral types out of the wayland backend
- The accessible tree is neutral; morf-app no longer needs the scene
- Render presents through a BufferSink; its Wayland half moves to morf-app
- Morf-wayland becomes morf-app, its code the wayland backend
- Morf-lua's flat files into vm, value, runtime and api modules
- Morf-system from services, desktop entries and menus; retention into morf-scene
- Morf-shader to the engine group; the uniform header to morf-value
- Morf-outline and morf-svg into morf-vector; the XDG directories into morf-image
- Morf-region into morf-value as its region module
- Morf-reactive into morf-scene as its reactive module

### <!-- 3 -->📚 Documentation

- The widgets reference with Transform, Sheet, Roving, Form and Overflow
- The widgets reference regenerated with Canvas and Dock; per-widget looks and galleries
- Pointer events' button and modifiers; the guide knows Canvas and Dock
- The widgets guide, and themes without interaction code (Stage 20)
- Compiled bindings measured and not built (Stage 19)
- BUNDLE.md, how a bundle is made and how logre is built

### <!-- 4 -->⚡ Performance

- Stop idle repaint loops, share sampling across screens; add equalizer
- Lock and greeter redraw only the band the swell grows in
- A held-up compositor skips a frame, not a quarter second
- A removed node does not force a whole layout
- A slow frame does not halve the rate
- An idle shell stays idle

### <!-- 5 -->🎨 Styling

- Clippy-clean morf-lua and morf-runtime
- Series_kinds and frame_bench split under the line gate
- Morf-kit's sheet tests in their own file, under the line gate

### <!-- 6 -->🧪 Testing

- Widget gallery snapshots are taken at the gallery's size
- Widget galleries for every archetype in every look

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Tools/layers.py checks section 3's house rules and runs in CI
- The layer smoke example lives in morf-desktop
- The layer checker fails on anything not pending a later phase
- Crates into their groups; the layer checker; the baseline

## [0.1.5] - 2026-09-26

### <!-- 0 -->⛰️  Features

- Letters at a weight, and no seam where contours overlap
- The speaker and the microphone, each a page of its own
- Thickness makes a letter or a drawing heavier
- The wallpaper is lule's
- Numbers that morph, a backlight heard at once, covers from the web

### <!-- 1 -->🐛 Bug Fixes

- An animation nothing shows is not motion
- The media progress stops following the track while hidden
- The pills' digits without the grown outline
- Smaller numbers in the pills' discs
- No GL in the GPU instance when there is Vulkan
- Say so when there is no Vulkan driver for the GPU

### <!-- 2 -->🚜 Refactor

- Shells and demos, each where it belongs

### <!-- 5 -->🎨 Styling

- Rustfmt

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Merge main into develop
- Merge develop to main
- Merge develop to main

### Build

- Release builds keep function names; tools/morf-stuck.sh

## [0.1.4] - 2026-09-26

### <!-- 0 -->⛰️  Features

- A named shell is a folder of parts; make apply sets one as yours
- One capture page: what, when, where, then Screenshot or Record
- A capture drawer at the bottom; the launcher floats in the middle
- Pills on both edges, only to look at, riding out with their panels
- Tabbed side panels, settings with pages, a left panel
- A Sound tab on the dashboard
- Set each channel's volume on its own
- Volume and brightness sliders atop the utilities
- The workspaces as a rail down the left edge, in place of the bar
- A workspace rail down the right edge instead of the sidebar
- The launcher opens from the bottom edge, the sidebar from the right
- Morf types writes the engine's API for the Lua language server
- Theme.lule, the colour tool's palette, and a lule source
- Morf.terminal hears the colours a colour tool sets on terminals
- Wait for a buffer like a swapchain, and MORF_GPU_PROFILE
- Present through buffers of the engine's own, copying only what changed
- The idle inhibitor reads back, and says whether it can hold
- An opened group's notifications and the empty state come in evenly
- The recorder's modes on a sliding selection; the switch's thumb rolls
- The sidebar and the utilities drawers
- A theme can ease its colours to new values
- A function assigned to a property is a binding
- A module loads on a budget of its own
- A key's name beside its keysym, and morf.keys
- Notifications drop in as one field, OSD levels on a spring, bar icons swell
- The dashboard's cards are one liquid field
- M3 expressive shape morphs through the shell
- The launcher's selection slides as a distance field
- The workspaces and the tab indicator as distance fields
- M3shapes ships its named outlines ready-made
- A drawer's background fades in with its contents
- Opacity on a field layer, mixed between absent and whole
- The dashboard shuts when the pointer leaves the panel, by contains_pointer
- Contains_pointer on every node, whatever is drawn over it
- Notification popups and the volume/brightness OSD
- Text at the reference's optical size, and NEEDS item 2 done
- Every variable-font axis is shaped, and opsz follows the size
- The bar's popouts for the network, Bluetooth and power
- The session menu behind the power button
- The launcher's scheme, variant and wallpaper pickers, and a calculator
- The dashboard's Media, Performance and Weather tabs, each at its own size
- A cubic Bezier easing may be four numbers in order
- Drawer contents fade in as they slide, as the reference's do
- Masks, FILL and wght axes, row enter/exit from the merged engine
- Exit example and spec, GPU test of a leaving node through damage, docs
- Exit = { ... } on any node, played when a Loader, a list or ui.destroy lets it go
- A node on its way out keeps its box and gives up its room
- Exit animations: a node leaves by animating to declared values before it is removed
- Variable font axes on text nodes, animatable, drawn at their point in design space
- Mask = node | { gradient } | nil, mask_invert; reads back as the node
- Alpha masks: a subtree or a gradient composited as a layer's alpha
- Mask property, mask nodes kept as laid-out children never painted or hit
- Drawer motion fitted to films of the reference
- Measured type sizes, user card and media arc like the reference
- Frame, bar and drawers of a clean-room caelestia port
- Settings.lua and frecency.lua
- Lyrics.lua, synced lyrics for what is playing
- Spectrum.lua, visualiser bars from audio bands
- M3shapes.lua, Material 3 shapes that morph into one another
- Material.lua, Material 3 schemes over HCT
- Tracking field layers, layer matrix, circular seams, blend groups, per-layer damage
- Transform_matrix, squash-and-stretch springs, layer tracking relation
- Multi-segment bezier spline easing, overshoot allowed
- One GPU device per process, shared by every surface
- Loader preload and keep; hidden text gets its glyphs while idle
- Incremental layout: redo only the subtrees that moved
- A lua profiler for slow turns, and the island's stalls taken out
- Beat and tempo detection on monitors
- Hct colours and tonal palettes
- Fuzzy matching, ranked and highlighted, as morf.text
- Notifications and clipboard history in the primary runtime
- One primary runtime for what must be done once
- Subpixel text inside opaque rounded panels
- Subpixel (lcd) text where it is safe
- Light a screen again when none is, with or without an output
- A shell that survives with no outputs
- Frosted glass inside a surface
- IMPASTO_LIVE_COMPOSITOR lets a dry run reach a test compositor
- Keyboard layouts are picked from the xkb list, searched, in order (LayoutPicker)
- The wake log says what is moving
- Files followed with morf.fs.watch, and a terminal window per program, destroyed when done
- A byte array arrives in lua as a string of bytes
- Window:destroy() tears a popup, floating or layer window down for good
- File changes pushed from one inotify thread, morf.fs.watch, and no idle polling in ipc or hot reload
- Morf.screens_revision(), and timers named in the wake log
- Keys are rebound from the settings page, written to keys.tsv for Hyprland and reloaded
- The displays service keeps an arrangement per set of screens and pushes it as monitor rules, and the displays page edits it
- The compositor service pushes keyboard, pointer, cursor and shake to Hyprland, and the input page applies them
- Monitors all in lib/hyprland, and hyprland_config for run-time options, animations, monitor rules and workspaces
- A layer_surfaces capability, and impasto's edge surfaces use it
- Text in runs of their own style, markup, and links
- Every surface hears the keyboard and the pointer come and go
- A window says which outputs it is on, and its parent
- A night light through wlr-gamma-control
- An image says what became of its source, and GIFs play
- Pictures from raw pixels, and PNGs written from them
- A loop can hold where it stops
- Decompress gzip, zstd and xz, and read tar archives
- Spanish, from upstream's Tr table, in Settings and the launcher
- The account changed on the account, and smaller service gaps
- Packages read from pacman's database, run in the shell's terminal
- Launcher sigils, one-shot actions and terminal apps
- Capture, recorder and picker act once, and flash what they did
- A lock that is secure when the compositor says so, on every screen
- Settings watched, typed and imported; compositor pushes
- One night light and one ddcutil for the whole desk
- The painted palette board, an avatar component, a paper mouth that morphs
- The real Claude mark, raised contribution tiles, a spectrum's curve
- A photo opens in a viewer of the shell's own, and says when it is lost
- The card's scroll rail can be held, and the picker lists every picture
- The record spins as one endless turn and stops on the spot
- Motion and wallpaper previews move as the real thing
- A setting slider's figure can be typed into
- A pet's name is a text input, its box lit while typing
- Arranging is one mode across screens
- A module with no wide face shows its island detail on the desk
- Notes move between the desk and the screen-edge decks while arranging
- A calendar widget opens a day's tasks, and task rows open the board
- Morf check, morf render and morf test, headless runners
- A virtual clock, per-runtime arguments, host functions and node ids for headless runners
- The control centre's tray and block inspector
- Real faces for the tasks, pet, games, impasto and notes blocks
- The calendar marks days with tasks and lists a picked day
- Pet, github, games, notes and tasks are modules
- The Claude module over the account's own limits
- The weather module and the control centre's weather card
- The timer module, a ring chip and a detail that sets and runs it
- Modules.watch, and chips that keep a polled reading alive
- The overview floats, places and swaps windows
- Notch fillets on the lock screen's island
- Glance guards, the recording's stop button and the countdown at rest
- One escape for every panel, and lists that scroll
- The band, notch fillets and live screen on the bar
- Select with the pointer, and read the selection
- On_key_pressed can keep a key from the program
- Morf.kill signals a process by its id
- Ui.Terminal, a program's screen as a node
- A pseudo-terminal and a vt emulator
- The reactor watches a child it did not start
- The settings window fills its size and scrolls by transform
- A floating window hears its size and its close
- The wheel bubbles to whatever would use it
- The dock follows the compositor, not a timer
- Idle subscribes per action, and settings over IPC
- Smooth = false, and user dirs from the environment
- Packages and updates, read here, changed in a terminal
- The key sheet, every bound key on one page
- System statistics, figures with their history
- Capture, a photo or a take of the screen
- The island's appearance panel, wallpapers and palettes as strips
- Settings, each option drawn as the thing it changes
- The bar draws what the layout says, splits and looks included
- Profiles, whole desks with a name
- The settings vocabulary, rows and tiles and small copies
- A mouse area says whether it is hovered and pressed
- A key target can keep tab, shift+delete needs a selection
- Signals hold tables
- Effects can be disposed
- List reads are dependencies
- The lock screen blends as morf.surface says
- Surfaces that blend the way browsers and Qt do
- Key releases, and repeats that say so
- Arranging the desk, the card, the inspector and the picker
- The spectrum, on a square and along an edge
- The analogue desk, dials, gauges and paper
- The desk, modules on the wallpaper
- A lock process answers ipc on a socket of its own
- Every output locks with a tree of its own
- Lua hears the compositor answer the lock
- Morf.toplevels follows the compositor's windows
- Layer surface handles change their settings at runtime
- Fs.read reads a window of a file
- Claude_usage, tokens in the block and the week
- Packages, pending updates from the managers present
- Github, a user's contribution calendar
- Weather, from open-meteo with wttr.in behind it
- Sysinfo, the machine from /proc and /sys
- Path, shapes that animate
- Groups that alternate and wait, and loop on a node
- On_destroyed, and ui.destroy
- Service libraries that never wait and leave nothing behind
- Calls that answer later, and subscriptions that end
- Say which stage was slow
- The glance and the activities beside the clock
- The control centre
- The bar's modules and their island details
- Quick tiles, switches, sliders, pills and rings
- The quick settings' services
- The overview and the workspaces on the bar
- The launcher, the first character says what for
- A timer, a clipboard history and the workspaces as services
- The tasks and notes modules
- The board, to do, doing, done
- Notes on the screen edges
- Notes are paper
- A panel can make the island paper
- The arcade, eleven games in the island
- The arcade's shelf, frame and records
- The dock, pins first then what is open
- The session panel, five tiles and a second press
- The lock, the island round the padlock
- Morf.spawn, morf.run, morf.connect and morf.request_socket, output as it arrives
- A reactor that watches children and sockets and reports as they talk
- The wallpaper is the shell's, and the palette follows it
- The pets, a family on the bar
- Notifications read their images and hints
- Logind, the session, the power actions and the backlight
- Mpris, players and the one a panel shows
- Upower, batteries and power profiles
- Bluez, adapters and devices from the object tree
- Networkmanager, wifi over the system bus
- Palette, a desk's colours from its painting
- TextInput, text you can edit
- Keys carry the modifiers held with them
- An editing model and caret stops
- One-axis centre anchors, and anchor names checked
- Parts register themselves
- Hyprland, the compositor over its own sockets
- The bar and the island, on morf
- Handlers know the button, and a refusing area is not in the way
- Morf.audio, a mixer's worth of sound for a configuration
- Devices, streams and levels, over pipewire opened at run time
- Data control and drags, the clipboard as it changes
- A DropArea, hit apart from MouseArea
- Morf.log, and a log that stops growing
- Processing off the main loop, svg from a string, captures to files
- Morf.http, requests off the main loop
- Morf.encoding
- Morf.fs and morf.time
- Names under the round buttons, and a calm hover
- The icon becomes what a tap does
- Hover you can see
- Everything answers the pointer
- By touch, a phone's shade, and more that morphs
- Pages scroll, and the wheel counts again
- Every page from one kit
- A shade that dims, a capsule that warms, tiles in a wave
- One look for every page
- Tiles and status icons as registries, and a phone mode
- A press on nothing is the click outside too
- A wake for the loop, so a timer or a child is seen at once
- Back, lule colours, a pixel face, and motion that is not late
- The island morphs, and the rest of the pages
- Panacea on morf
- Slower, bouncier motion everywhere
- Motion, and a smaller symmetric pill
- ChillPill-Shell on morf
- The session the machine names is the one the list opens on
- One file, two doors, in black and white
- The file says what it is; a bundle carries it
- The login as a conversation off the drawing thread
- A first frame to start from, and a cursor for the pointer
- Line height, spacing, slant, width and a decoration
- Tokens, preferences and an inherited colour
- A gradient is a list of stops
- A colour is a value
- An unknown layout key or ui kind is named
- A state that says `when` chooses itself
- Morf.state, ui.each, and a component with a model and messages
- Ui.Layout, a container laid out by two Lua functions
- Flex, and a Grid with tracks, laid out by Taffy
- The containers that existed become the ones worth using
- One flush per handler
- A Repeater follows its model
- The dialog, and the three things that stood between it and polkitd
- The compositor draws the capture into the texture that shows it
- A texture the compositor can draw into, without a copy
- An exclusive zone that follows the surface
- A screen for a crash
- A polkit agent in Lua, and the two engine pieces it needed
- A tray item's menu, read off the bus
- A tray that brings its own watcher
- A notification server in Lua, a lint, and `morf info`
- A workspace can be removed or moved, and a crash can leave a core
- An opaque bar, a shell in the background, and a list of who is running
- Hold the compositor's keys, count only the person, and aim a minimize
- The conversation, as PAM means it
- A popup is scaled by the screen it is on
- A log you can filter, and follow
- A task list you can click
- Workspaces, without knowing whose they are
- Hold the session awake, and give a proxy its own patience
- The shell can be something on the bus, and can leave
- A configuration takes arguments, and a keypad for a numeric password
- A phone is a different shape, not a small desktop
- A cross shell, so the phone does not have to build its own
- The keyboard is part of this screen, not a program beside it
- Nothing on this screen swallows a keystroke
- SVG icons, weather behind frosted glass, and a screen that answers
- A clip path is an intersection, and is taken as one
- A drawing is an outline, and an outline is a shape
- GDM's own structure, not a guess at its stylesheet
- The login screen wears GDM's clothes
- A letter in a field names its face, and morphs across faces
- Keep a point on every corner when resampling a contour
- Read the pixel four times, not once
- Keep corners, by not asking one number to describe two edges
- A shape and the letter cut out of it, both morphing
- Weight for small text, and an edge that knows its own width
- The login screen, rewritten around one slab
- A login screen built out of cut-out shapes
- A letter is a shape a field can compose with
- Morph letters by their outlines, not by averaging fields
- The login screen, and its clock morphs
- Every glyph is drawn from its outline
- Measure glyph fields from outlines, not from bitmaps
- Text morphs from one string into another
- The keyboard morphs between layers instead of swapping
- An on-screen keyboard, as its own process
- A login screen you can use without a keyboard
- Draw on compositors that have no layer shell
- A login screen, and an overview you can click
- Morf can be a service, not only a client
- A working desktop overview, and the piece the plan missed
- Capture one window, not just the whole output
- A configuration can see the compositor's windows
- A greeter can find the sessions it starts
- A twentieth of alpha, and the blur carries the rest
- Let the blur through the blobs
- A blob on the end of the cursor
- Lava made of blurred desktop
- Ask the compositor to blur behind a surface
- A CRT tube and a chromatic split, over one merged field
- Render a configuration on a GPU, and look at what it drew
- The last two boxes
- §5 — textures, vertex displacement, data blocks
- W7 and W8 — the deferrals, and records
- W6 — continue and discard
- W5 — arrays and indexing
- W4 — derivatives and relational
- W3 — integers and bitwise
- W2 — matrices
- W1 — the ordinary arithmetic builtins
- Helper functions, monomorphised
- Port real Shadertoy shaders
- Surface and effect shaders
- Morf.shader, from a configuration to the GPU
- Paint a Lua shader on the GPU
- Compile Lua-syntax shaders to WGSL

### <!-- 1 -->🐛 Bug Fixes

- Apply leaves out an example's link to the library
- A dropped D-Bus service hangs up, and its reader thread ends
- The rail's accent moves from pill to pill instead of fading
- A fading theme keeps the frames coming
- A theme fade asks for the frame that starts it
- One spectrum shader for every spectrum
- Each node draws a shared shader with its own parameters and data
- Lyrics ticks only while synced lyrics play
- Take a buffer the compositor has finished reading first
- One screen at a time, each bar its own workspaces
- A stretch at rest settles; libpipewire from the system's folders
- A click on the desk shuts the sidebar
- An opened notification group as the reference lays it out
- The default wallpaper folder follows XDG_DATA_HOME
- The popouts shut on contains_pointer, not a list of areas
- Keys.rs clean under clippy
- Cards come in evenly, not skewed
- The workspaces and the tab indicator crisp at rest too
- Crisp and apart at rest; liquid only in motion
- The launcher's search is as strict as the reference's
- The lyrics column inside the media page; a sandbox cover of its own
- Action icons at the reference's size
- Headless layers keep an asked size between two anchors, as compositors do
- Render --surface screen stacks layers as a compositor does
- The primary output's values are the ones handed over when every output goes dark
- The island opens and closes the way DynamicIsland.qml does
- An output going away leaves the screen list
- One gpu instance for the process
- The desk frosts its own wallpaper, and mixes as Qt does
- An extra surface repaints when its layout bindings move it
- A panel verb is answered by the screen that opens it
- The cursor page says when the size is left alone
- The island opens on a click, and the overview's right and middle clicks work
- A screen lit again keeps its mode, and its own mode is offered
- The shell starts under an absolute WAYLAND_DISPLAY
- A screen switched off no longer stops the shell
- One settings window, the focused screen's
- Hyprland reads devices while a virtual keyboard is connected
- The wallpaper layer is shown
- Recording a new shortcut holds Hyprland's binds off the shell
- Shortcuts are held off every surface that can take the keyboard
- Each spec file starts from an empty home
- The dock's menu takes Escape, as the desk's does
- The launcher keeps its place after Shift+Delete forgets a clipboard entry
- The picture viewer zooms about the pointer in the window it was given
- A bar capsule clips its chips, so a long figure never spills while it grows
- Arranging the control centre takes the whole bar, so its card is not cut off
- The arranging card opens where it covers the fewest widgets; the desk menu takes Escape; a right click beside the island only closes it
- A desk laid out for a larger screen fits the board without overlapping
- A view delegate gets a budget of its own, not a handler's
- The configuration runs once per output, not once more first and thrown away
- Keys go to the surface last clicked, as a compositor would send them, not always the primary
- A paint owed on an overdue frame callback is made after a stall, not at the next unrelated event
- A held key repeats, at the rate and delay the compositor asks for
- The empty-layout lint passes over hidden subtrees and containers of hidden rows
- The recorder hears another screen's take through a watch, not a 2 s poll
- A subscription's route exists before the bus is asked for its signal
- Every producer rings the loop, and zbus no longer keeps a pool thread awake
- Generic families follow fontconfig, and a morph keeps its point count from the start
- BarStyle "island", upstream's name for one capsule, is read as the port's capsule
- Settings open a page by its label or a setting key
- Keys.lua passes a bind without a description through untouched
- Keys.conf remembers where each moved bind came from, so a second move keeps the first
- Without layer-shell, extra layer surfaces are subsurfaces placed like layer-shell would
- No stray dock or deck squares without layer-shell
- Flatpak's applications and icons, and desktop-theme icons without a theme
- Follow the live screen for the single jobs
- Read the ddc link for a monitor's bus
- Notifications expire, close with their app, and stay trimmed
- The desk's smaller parity gaps
- A task row with no task is not done, rather than nil
- The analogue weather's hours after dark get a moon
- The desk menu's settings row opens the settings window
- A stray top-level node is named, not fatal
- One countdown on the island, and the rest clock as the clock module draws it
- The smaller parity gaps in the modules and blocks
- The clear-clipboard and night light tiles say when they can act
- Arcade keys held and released, palette-true boards
- The dock reserves nothing and sleeps while the desk is arranged
- A bordered clip composites where its ancestors put it
- The tab test says which key action it sends
- Libpipewire is loaded once for the process
- Children never inherit a wrapper's library path
- The desk reads the load as the stats service gives it
- Settings pages scroll by position, and verbs to test them
- A socket path too long to bind moves somewhere short
- Icon lookup follows the spec's fallbacks
- A timer torn down in the same turn does not fire
- Motion that lands on a skipped frame is still painted
- Libpipewire finds its plugins wherever it was loaded from
- The desk reads the services and libraries that landed
- A strike-through is never thinner than a pixel
- A field built with its text keeps to its own rules
- A row skips what it cannot show
- The picture waits for the layout to settle
- A threshold subscribed later reaches the compositor
- A lock screen hears the pointer
- A surface that never gets its frame callback no longer stalls the rest
- Frame_bench honours MORF_RUNTIME_PATH, xdg lookups always reach /usr/share
- Nodes built during a flush no longer panic
- A container growing from nothing is not reported
- Signals can be made inside a binding
- Brightness leaves Hyprland to whoever runs it
- Panels are laid out at their own size, not the capsule's
- The task's caret starts at the end of its line
- Settings are saved again
- Settings are saved again
- A failing Loader is tried once, and limits sized for a desktop
- A removed delegate takes its bindings, handlers and timers with it
- A stale child skips the frame, not the shell
- No edge on the capsule
- The recorder's rows share the look, and bluez may be slow
- The chevron takes its own tap, and the rest of the goal so far
- No backdrop and no reservers without layer-shell
- Issue n cleanup
- Issue n cleanup
- Hover does not open the island unless asked
- A smaller pill, as far from the windows as from the edge
- An export for a capture that failed is let go
- A refreshing thumbnail holds one picture, not one per refresh
- The key handler takes two arguments, and never took three
- The frame bench roots a configuration where the shell does
- An empty layer must not swallow the command after it
- A field's layer index cannot survive being interpolated
- A configuration picks from the faces the machine has
- The morph demo paints no background
- Give large text a field with enough texels in it
- Stop spending the field's precision on distance nobody reads
- An edge cannot fade finer than the field can resolve
- The morph demo is a window, not the whole screen
- Place glyphs where shaping put them, fraction and all
- Large glyphs were boxed in and chewed at the edges
- Drop an import the timeout tests stopped needing
- A D-Bus call cannot hold the paint thread
- The blur region can trail the shape, never lead it
- The blur region keeps up with the shape, not ahead of it
- A skipped frame is not a failed one
- Follow the pointer without swallowing the clicks
- Effect shaders reach their parameters and data blocks
- An undecided literal takes the type it is compared with
- An animating shader goes through the frame pacer
- Rasterise a distance field only when one is wanted
- A rectangle can wear a shader

### <!-- 2 -->🚜 Refactor

- The profile's shading summary lives in profile.rs
- Act runs programs with morf.run and morf.spawn
- Panacea and the hyprland library stop polling
- Every example on the appearance API
- The old words go
- One vocabulary, two layout kinds, no silent words
- One keyboard, two hosts
- The engine has no sound API
- The engine names no desktop environment

### <!-- 3 -->📚 Documentation

- The library's new home
- What a frame costs the GPU, and the variables that say
- The idle inhibitor has no read-back
- Tidy the list of engine fixes
- What phase 2 needs from the engine
- State the fuzzy ranking cost as measured
- What reaches the compositor, the display verb, and the one line keys need
- The desk's decks, verbs and sources as they are now
- A terminal section, btop in a panel, and an fzf launcher
- A set example with a real key
- The capture, stats, keys and packages verbs
- What each library in examples/lib offers
- A clipboard history with a drop strip
- The appearance section
- A comment stops naming the removed kinds
- How to write UI
- Drop a link to a file the repository does not carry
- A gallery of what the language gained
- Track WGSL coverage

### <!-- 4 -->⚡ Performance

- No opacity nudge to make the spectrum repaint
- Warm hidden text within a budget; a rectangle showing nothing costs nothing
- Large fields drawn as the tiles their surface reaches; drawers example
- Letting go of a subtree sweeps its tables once, not once a node
- Offscreen layers sized to what a frame reads of them
- Proxies with the same bus and timeout share one connection
- Fontconfig is asked once per process, before a runner moves the cache
- An idle output sleeps until something is due, and MORF_WAKE_LOG says why it woke
- The runtime says when it next comes due, and at what grain it reads the clock
- A change lays out only the surface whose tree it is in
- A surface the island's size, and one ticker for all
- A pixel opens the runs of a letter that could answer it
- Measure a field against nearby edges, not every edge
- Coarsen the covered-edge grid to eight pixels
- A blur region costs a fifteenth of what it did

### <!-- 5 -->🎨 Styling

- Rustfmt
- Rustfmt the key surface lookup
- Module order that cargo fmt wants

### <!-- 6 -->🧪 Testing

- Every runtime watching one file hears it; caelestia says whether it hears the colour tool
- The sidebar and the utilities open, close and act dry
- Wait for the fake Hyprland to hold the stream before hanging up
- An axis that widens text is drawn as wide as it measures
- Masks in scene, layout (incl. random incremental trees) and draw lists
- Spec -- loads, drawers over IPC and hover, actions, snapshots
- Caelestia gallery and films in the sandbox, readme
- Sandbox seals the system bus for the shell too
- Sandbox pointer that stays, caelestia drawers close over ipc
- Sandbox runs caelestia-dots/shell as a reference
- Nested.sh passes MORF_ENV to the shell under test
- A sandbox that runs a shell, or upstream impasto, in a nested hyprland
- Input scenarios driven by real input kept as specs: backdrop click, launcher escape, arranging drag, arcade escape, layout picker
- The scripted pointer waits for clients to bind the seat's new pointer
- The scripted pointer clicks any button, turns the wheel, drags and hovers
- The watch tests wait for the loop's alarm rather than race it, and ask a forgotten watch only for silence
- The fake hyprland answers the monitors all request
- The fake Hyprland removes its runtime folder when closed
- Hyprland_config's plans, and both config flavours against a fake Hyprland served from the spec
- The descriptor test waits out a sibling's fork
- A bus descriptor closes when dropped, collected or reloaded
- An effect queued inside a flush can be disposed
- The service libraries against fake services
- Measure corner cells against six typefaces, and reject them

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Contains_pointer on every node, opacity on a field layer
- Vendor cosmic-text 0.19.0 unchanged, as a crates.io patch
- MORF_CONFIG picks the morf kind's configuration; caelestia motion steps; NEEDS.md
- Drawers: tracked field layers, squash-and-stretch, circular blends, transform matrices, spline easing
- List reads, disposable effects, tables in signals, hovered and pressed
- Key releases and repeats, browser blending, paint owed, timers after teardown
- Drop pkgs.morf from the devShell, an unrelated tool of the same name
- Cleanup
- Take the key readout off the login screen
- Cleanup
- Cleanup

### Build

- Library/ holds the Lua library; make install puts it where every shell finds it
- A size and a picture for the GPU frame
- Faster builder

### Merge

- Each node draws a shared shader with its own parameters and data; impasto's one spectrum shader
- Present through engine-owned dmabufs, copying and declaring only what changed
- Caelestia sidebar and utilities drawers
- Caelestia port phase 2 — tabs, pickers, calculator, session menu, popouts, notifications, OSD, shape morphs
- Every variable-font axis shaped, automatic optical sizing
- Caelestia port phase 1 — frame, bar, launcher and dashboard, clean-room
- Variable font axes and exit animations
- Masks — a subtree or gradient as a node's alpha, inverted or not
- The sandbox runs the real caelestia, and no longer leaks the system bus
- Incremental layout, preloaded panels and glyph warm-up, one gpu device per process
- Island motion: profiler, cheaper flushes and construction, animation clocks start at the first frame, qt retarget, panels dropped on close
- Fuzzy matching, hct colours and tonal palettes, beat and tempo detection
- Subpixel text in opaque rounded panels, and one primary runtime for process-wide duties
- A shell that survives with no outputs, and subpixel text where it is safe
- Layers render only what is read, pooled and packed; one vulkan instance per process
- Frosted glass inside a surface, and the desk and panels matched to the real upstream
- Real hyprland testing: shortcut inhibit on every surface, outputs off survive, absolute displays, second paint, two-screen fixes
- Config runs once per output, delegate budget, desk fit, arranging, menus take escape, clipped chips
- Real-input testing: key repeat, overdue paints, lint, test keyboard routing, layout picker
- Push file watching, blocking ipc and hot reload, window destroy, dbus bytes as strings
- The event loop sleeps until something is due, and every producer wakes it
- Input, displays and keys applied to hyprland through an opt-in lua library
- Layer surfaces without layer-shell are placed subsurfaces
- Rich text, images from pixels, image status and gifs, loop hold, gamma, toplevel outputs, surface focus, decompression
- Notifications, a real lock state, live-screen capture, launcher terminals, native packages, translations
- Calendar day views, decks while arranging, previews, viewer and the desk's parity gaps
- Morf check, morf render and morf test, headless runners
- Timer, weather, claude and other modules, real blocks, calendar tasks, tray and inspector
- The bar's band, notch, live screen, escape, scrolling lists, games and overview
- Ui.Terminal, a pty and a vt emulator as a node
- Window size and close events, wheel bubbling, clip under transform, per-surface layout
- Capture, recording, the picker, stats, keys and packages
- The settings window, profiles and the appearance panel
- Port-desktop
- Fix-layout-text
- A lock that hears the pointer, trees per output, idle and ipc
- Surfaces that change at runtime, a reactive window list, no stalls
- Sysinfo, weather, github, packages and claude usage libraries
- Paths, destruction hooks, looping animation, require in bindings
- Calls that answer later, and subscriptions that end
- The control centre, the modules and the glance
- The launcher, the overview and the workspaces
- Notes, decks and the board
- The arcade
- Port-dock
- Port-lock
- Feat-async-io
- The pets
- Lib-dbus-services
- Lib-palette
- Feat-text-input
- The hyprland library
- Morf.audio
- Clipboard watching, drops and drags
- Image processing, inline sources, captures to files
- Morf.http

### Tools

- MONITORS lays out any number of headless outputs
- Every shell gets the system's data dirs
- VISIBLE=1 shows the nested session as a window

## [0.1.3] - 2026-08-31

### <!-- 0 -->⛰️  Features

- New things
- New things
- Add interactive motion demo
- Integrate native motion stack
- Add affine transform motion
- Match Quickshell board
- Load declarative font sources
- Add font weight shaping
- Complete clip border controls
- Add mutable color quantizer
- Add mutable socket lifecycle
- Preserve file view policy
- Suspend render updates
- Map item geometry
- Mutate popup placement
- Parent native surfaces
- Support multiple surfaces
- Mutate process contexts
- Check native features
- Interpolate geometry curves
- Grab focused popups
- Mutate floating geometry
- Request system move resize
- Report reload results
- Control config watching
- Mutate floating state
- Refresh desktop entries
- Reparent native scene items
- Locate list model values
- Expose runtime identity
- Constrain floating surfaces
- Request native reloads
- Resolve themed icons
- Scope XDG paths
- Reconcile list models
- Expose screen metadata
- Add native clip rectangle
- Anchor popups to items
- Watch native transforms
- Route pointer axes
- Add native inset primitive
- Route auxiliary input
- Render auxiliary surfaces
- Model general surfaces
- Retain dropped scene objects
- Defer lazy loader creation
- Adapt JSON file views
- Collect bounded streams
- Isolate reload scopes
- Persist typed property scopes
- Add native menu models
- Add native system clock
- Compose input regions
- Quantize image colors
- Expand easing curves
- Generalize popup anchors
- Port board layout
- Add desktop entries
- Add managed processes
- Add stateful file views
- Add native JSON codec
- Expose native shell utilities
- Preload native namespaces
- Configure layer geometry
- Harden plugin lifecycle
- Add inner rectangle shadows
- Mask rounded subtree clips
- Add layer drop shadows
- Add dual-kawase blur
- Composite subtree layers
- Reuse delegates across rows
- Add color overlays
- Reuse virtual delegates
- Add status notifier host
- Drive OSK from XKB
- Add XKB layout facade
- Add output screencopy
- Diagnose animated bindings
- Add text input bridge
- Add input method bridge
- Add virtual keyboard
- Add clipboard bridge
- Manage output power
- Add idle notifications
- Carry reloadable state
- Add compound D-Bus values
- Add udev monitoring
- Clip nested scene content
- Add binding graph logs
- Add typed D-Bus arguments
- Add greetd client
- Compose scene transforms
- Add Unicode eliding
- Preserve source aspect ratio
- Add wrapping and alignment
- Use intrinsic layout sizes
- Add per-corner radii
- Add SDF gradient fills
- Add keyboard focus chain
- Add Loader and Timer
- Expose parsers and socket servers
- Add virtualized GridView
- Add button filters and drag events
- Route independent touch contacts
- Add declared component schemas
- Watch D-Bus service changes
- Add tessellated Shape paths
- Subscribe to D-Bus signals
- Add system service modules
- Expose bounded IO primitives
- Run secure PAM lock screen
- Draw images and theme icons
- Decode and resolve icons
- Refresh system indicators
- Poll native timer callbacks
- Add grids and attached sizing
- Add async PAM callbacks
- Watch the Lua runtimepath
- Add pure Lua phone shell
- Add virtual view elements
- Run PAM off event loop
- Add session lock surfaces
- Add IPC and live reload
- Add bounded IPC transport
- Register bounded IPC handlers
- Add named state transitions
- Queue animated parent changes
- Add SDF blur and shadows
- Animate parent changes
- Expose virtual list models
- Add virtual list model
- Add spring and smoothed motion
- Add native PipeWire graph
- Authenticate through PAM
- Add Lua system indicators
- Add generic Lua proxy
- Add process file socket primitives
- Reconcile screen variants
- Add popup and floating windows
- Target bars by output
- Track compositor outputs
- Apply surface input regions
- Route Lua button events
- Run live clock bar
- Draw rasterized glyphs
- Expose reactive clock service
- Present layer surfaces
- Add wgpu SDF backend
- Add draw list and damage
- Add behavior animations
- Construct scene from config
- Bind reactive signal graph
- Add cosmic shaping cache
- Add arena and layout core
- Add signal graph core
- Add bounded Lua workspace

### <!-- 1 -->🐛 Bug Fixes

- Avoid disjoint damage overflow
- Satisfy Clippy checks
- Hide NoDisplay entries
- Preserve sRGB colors
- Load local file URIs
- Validate antialias shader
- Refresh changed screen metadata
- Recreate hard reload surfaces
- Build every variant instance
- Satisfy strict clippy
- Reclaim stale IPC sockets
- Limit parity to general core
- Isolate plugin loading
- Reconcile Loader and Timer
- Preserve GPU paint order
- Track output hotplug safely

### <!-- 2 -->🚜 Refactor

- One shape vocabulary, one field pipeline
- Convert every include! to a real module, and fix what that revealed
- Split runtime API sources
- Split runtime orchestration
- Split protocol source
- Split oversized sources
- Split oversized Rust sources
- Make variants native
- Remove downstream UI

### <!-- 3 -->📚 Documentation

- Cap Rust files at 500 lines
- Map whole Quickshell surface
- Correct color encoding
- Close surface audit
- Map quickshell types
- Track Quickshell parity
- Define engine boundary

### <!-- 4 -->⚡ Performance

- Keep animation ticks native
- Split glyph atlas formats
- Persist glyph atlas

### <!-- 6 -->🧪 Testing

- Remove stale socket race
- Preserve failed reload state
- Cover registration contract

### <!-- 7 -->⚙️ Miscellaneous Tasks

- Rename the project to morf
- Cleanup
- Docs
- Cleanup

### Build

- Enforce Rust source line limit

