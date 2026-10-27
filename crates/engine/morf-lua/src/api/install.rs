use luna::{Context, Lua, Table};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::api::media::shader::install_shader_api;
use crate::{
    api::io::file::*, api::io::process::*, api::io::socket::*, api::media::image::*,
    api::module::*, api::motion::animation::*, api::motion::fling::*, api::system::host::*,
    api::system::menu::*, api::system::system::*, api::time::timer::*, api::ui::group::*,
    api::ui::retention::*, api::ui::signal::*, api::ui::transform::*, api::ui::ui::*,
    api::ui::view::*, api::window::surface::*, runtime::state::*, vm::types::*,
};

pub(crate) fn finish_reactive_api<'gc>(
    ctx: Context<'gc>,
    morf: Table<'gc>,
    state: Rc<StateCell<ReactiveState>>,
    module_roots: Rc<RefCell<Vec<PathBuf>>>,
    permissions: Rc<RefCell<crate::vm::permissions::Permissions>>,
    limits: Limits,
) {
    ctx.set_global("morf", morf);

    let loaded = Table::new(&ctx);
    loaded.set_field(ctx, "morf", morf);
    let package = Table::new(&ctx);
    package.set_field(ctx, "loaded", loaded);
    ctx.set_global("package", package);

    crate::vm::loader::install_require(
        ctx,
        crate::vm::loader::Require {
            morf: ctx.stash(morf),
            loaded: ctx.stash(loaded),
            state,
            roots: module_roots,
            permissions,
            limits,
        },
    );
}
pub(crate) fn install_reactive_api(
    lua: &mut Lua,
    state: Rc<StateCell<ReactiveState>>,
    module_roots: Rc<RefCell<Vec<PathBuf>>>,
    permissions: Rc<RefCell<crate::vm::permissions::Permissions>>,
    limits: Limits,
    screen: Option<&Screen>,
) {
    lua.enter(|ctx| {
        let morf = Table::new(&ctx);
        crate::api::color::color::install_color_api(ctx, morf);
        crate::api::geometry::geometry::install(ctx, morf);
        install_signal_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::ui::state::install_state_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::color::theme::install_theme_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::system::prefers::install_prefers_api(ctx, Rc::clone(&state), morf, screen);
        install_retention_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::input::focus::install_focus_api(ctx, Rc::clone(&state), morf);
        crate::api::input::overlay::install_overlay_api(ctx, Rc::clone(&state), morf);
        install_shell_api(ctx, Rc::clone(&state), morf);
        install_time_api(ctx, morf);
        install_image_api(ctx, Rc::clone(&state), morf);
        crate::api::media::image_ops::install_image_ops_api(ctx, Rc::clone(&state), morf);
        install_shader_api(ctx, morf, Rc::clone(&state));
        install_transform_api(ctx, Rc::clone(&state), morf);
        install_animation_api(ctx, Rc::clone(&state), morf);
        install_easing_api(ctx, morf);
        install_group_api(ctx, Rc::clone(&state), morf);
        install_fling_api(ctx, Rc::clone(&state), morf);
        install_host_service_api(ctx, Rc::clone(&state), morf, screen);
        install_view_api(ctx, Rc::clone(&state), morf, limits);
        install_process_api(ctx, Rc::clone(&state), morf);
        install_file_api(ctx, Rc::clone(&state), morf);
        crate::api::io::fs::install_fs_api(ctx, morf, Rc::clone(&state));
        crate::api::io::watch::install_watch_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::time::date::install_date_api(ctx, morf);
        install_keys_api(ctx, morf);
        crate::api::media::terminal_palette::install_palette_api(ctx, Rc::clone(&state), morf);
        crate::api::io::encoding::install_encoding_api(ctx, morf);
        crate::api::geometry::text::install_text_api(ctx, morf);
        crate::api::system::log::install_log_api(ctx, Rc::clone(&state), morf);
        crate::api::io::broadcast::install_broadcast_api(ctx, Rc::clone(&state), morf);
        crate::api::media::audio::install_audio_api(ctx, Rc::clone(&state), morf);
        crate::api::desktop::toplevels::install_toplevels_api(ctx, Rc::clone(&state), morf);
        install_socket_api(ctx, Rc::clone(&state), morf);
        install_system_service_api(ctx, Rc::clone(&state), morf);
        crate::api::system::service::install_service_api(ctx, Rc::clone(&state), morf);
        let (ui, json_kinds) = install_ui_json_api(ctx, Rc::clone(&state), morf, limits);
        crate::api::motion::claims::install_claims_api(ctx, Rc::clone(&state), ui);
        crate::api::io::http::install_http_api(ctx, Rc::clone(&state), morf, json_kinds);
        crate::api::io::io::install_io_api(ctx, Rc::clone(&state), morf);
        install_menu_desktop_api(ctx, Rc::clone(&state), morf, limits);
        install_module_api(ctx, Rc::clone(&state), morf, limits);
        finish_reactive_api(ctx, morf, state, module_roots, permissions, limits);
    });
}

/// `morf.keys`: every named key's keysym by its X name (`morf.keys.Down`),
/// for comparing against the number a handler gets.
fn install_keys_api<'gc>(ctx: luna::Context<'gc>, morf: luna::Table<'gc>) {
    let keys = luna::Table::new(&ctx);
    for (name, keysym) in crate::keys::NAMED {
        keys.set_field(ctx, name, i64::from(*keysym));
    }
    for number in 1..=35u32 {
        let name = format!("F{number}");
        let _ = keys.set(
            ctx,
            ctx.intern(name.as_bytes()),
            i64::from(0xffbe + number - 1),
        );
    }
    morf.set_field(ctx, "keys", keys);
}
