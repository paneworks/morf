//! `ui.Repeater`, `ui.ListView`, `ui.GridView` and `ui.each`: view nodes
//! built from their properties.

use super::*;

pub(crate) fn view_constructor<'gc>(
    ctx: Context<'gc>,
    state: Rc<RefCell<ReactiveState>>,
    limits: Limits,
    kind: ViewKind,
) -> Callback<'gc> {
    Callback::from_fn(&ctx, move |ctx, _, mut stack| {
        let properties: Table = stack.consume(ctx)?;
        let node = construct_view(ctx, &state, limits, kind, properties)?;
        stack.replace(ctx, node);
        Ok(CallbackReturn::Return)
    })
}

/// Builds a view node from its properties table; shared by `ui.Repeater`,
/// `ui.ListView`, `ui.GridView` and `ui.each`.
pub(crate) fn construct_view<'gc>(
    ctx: Context<'gc>,
    state: &Rc<RefCell<ReactiveState>>,
    limits: Limits,
    kind: ViewKind,
    properties: Table<'gc>,
) -> Result<LuaValue<'gc>, luna::Error<'gc>> {
    {
        let virtualized = !matches!(kind, ViewKind::Repeater);
        let model = match properties.get_value(ctx, "model") {
            LuaValue::UserData(model) => model
                .downcast_static::<ListModelToken>()
                .map_err(|_| HostError("view model must be a morf list model".to_owned()))?,
            _ => return Err(HostError("view model must be a morf list model".to_owned()).into()),
        };
        let delegate = match properties.get_value(ctx, "delegate") {
            LuaValue::Function(Function::Closure(delegate)) => {
                crate::vm::handler_store::register(ctx.stash(delegate))
            }
            _ => return Err(HostError("view delegate must be a function".to_owned()).into()),
        };
        // A Repeater lays its delegates out as whatever it is asked to be:
        // `as = "column"` makes it a Column of them, `as = "grid"` a Grid
        // with the `columns` and spacings a Grid takes. Without it the
        // delegates keep their own positions, as before.
        let element = match kind {
            ViewKind::Repeater => match properties.get_value(ctx, "as") {
                LuaValue::Nil => Element::Item,
                LuaValue::String(name) => match name.display_lossy().to_string().as_str() {
                    "item" => Element::Item,
                    "row" => Element::Row,
                    "column" => Element::Column,
                    "grid" => Element::Grid,
                    "flex" => Element::Flex,
                    other => {
                        return Err(HostError(format!(
                            "Repeater `as` must be item, row, column, grid or flex, not `{other}`"
                        ))
                        .into());
                    }
                },
                _ => return Err(HostError("Repeater `as` must be a string".to_owned()).into()),
            },
            _ => Element::Item,
        };
        let repeater_keeps_columns = element == Element::Grid;
        let clean = Table::new(&ctx);
        for (key, value) in properties.iter(ctx) {
            let special = matches!(
                key,
                LuaValue::String(name)
                    if matches!(
                        name.display_lossy().to_string().as_str(),
                        "model"
                            | "delegate"
                            | "as"
                            | "item_extent"
                            | "overscan"
                            | "content_y"
                            | "cell_width"
                            | "cell_height"
                            | "size_field"
                            | "kind_field"
                    ) || (name.display_lossy().to_string() == "columns" && !repeater_keeps_columns)
            );
            if !special {
                clean
                    .set(ctx, key, value)
                    .map_err(|error| HostError(error.to_string()))?;
            }
        }
        if virtualized {
            clean.set_field(ctx, "clip", true);
        }
        let node = create_node(state, element);
        configure_element(state, ctx, limits, node, clean).map_err(HostError)?;
        let model_handle = Rc::clone(&model.model);
        let model = model_handle.borrow();
        let configured_view;
        let (range, _item_extent, offset, _columns, column_extent) = match kind {
            ViewKind::Repeater => {
                configured_view = Some(VirtualList::new_unbounded());
                (0..model.len(), 0.0, 0.0, 1, 0.0)
            }
            ViewKind::List => {
                let item_extent =
                    table_number(ctx, properties, "item_extent", 1.0).map_err(HostError)?;
                let height = view_size(ctx, properties, "height")?;
                let offset = table_number(ctx, properties, "content_y", 0.0).map_err(HostError)?;
                let overscan = table_number(ctx, properties, "overscan", 1.0).map_err(HostError)?;
                if item_extent <= 0.0 || height < 0.0 || offset < 0.0 || overscan < 0.0 {
                    return Err(HostError("invalid ListView dimensions".to_owned()).into());
                }
                let mut view = VirtualList::new(item_extent, height, overscan as usize)
                    .ok_or_else(|| HostError("invalid ListView dimensions".to_owned()))?;
                view.set_offset(offset);
                if let LuaValue::String(field) = properties.get_value(ctx, "size_field") {
                    let field = field.display_lossy().to_string();
                    view.set_extents(&crate::views::row_extents(&model, &field, item_extent));
                }
                let range = view.visible_range(model.len());
                configured_view = Some(view);
                (range, item_extent, offset, 1, 0.0)
            }
            ViewKind::Grid => {
                let cell_width =
                    table_number(ctx, properties, "cell_width", 1.0).map_err(HostError)?;
                let cell_height =
                    table_number(ctx, properties, "cell_height", 1.0).map_err(HostError)?;
                let width = view_size(ctx, properties, "width")?;
                let height = view_size(ctx, properties, "height")?;
                let offset = table_number(ctx, properties, "content_y", 0.0).map_err(HostError)?;
                let overscan = table_number(ctx, properties, "overscan", 1.0).map_err(HostError)?;
                let default_columns = (width / cell_width).floor().max(1.0);
                let columns =
                    table_number(ctx, properties, "columns", default_columns).map_err(HostError)?;
                if cell_width <= 0.0
                    || cell_height <= 0.0
                    || width < 0.0
                    || height < 0.0
                    || offset < 0.0
                    || overscan < 0.0
                    || columns < 1.0
                    || columns.fract() != 0.0
                {
                    return Err(HostError("invalid GridView dimensions".to_owned()).into());
                }
                let columns = columns as usize;
                let mut view =
                    VirtualList::new_grid(cell_height, height, overscan as usize, columns)
                        .ok_or_else(|| HostError("invalid GridView dimensions".to_owned()))?;
                view.set_offset(offset);
                let range = view.visible_range(model.len());
                configured_view = Some(view);
                (range, cell_height, offset, columns, cell_width)
            }
        };
        let reuse_limit = range.len().max(1) * 2;
        let mut active = HashMap::new();
        for index in range {
            let (id, item) = model
                .get(index)
                .expect("view range contains live model indexes");
            let child = execute_delegate(ctx, &delegate, item, index, limits).map_err(HostError)?;
            if virtualized {
                let placing = configured_view
                    .as_ref()
                    .expect("a virtual view has its list");
                position_view_child(
                    &mut state.borrow_mut().scene,
                    child.node,
                    index,
                    placing,
                    offset,
                    column_extent,
                )
                .map_err(HostError)?;
            }
            state
                .borrow_mut()
                .scene
                .reparent(child.node, Some(node))
                .map_err(|error| HostError(error.to_string()))?;
            active.insert(id, child);
        }
        drop(model);
        if let Some(mut view) = configured_view {
            let _ = view.sync(&model_handle.borrow(), &[]);
            state.borrow_mut().views.insert(
                node,
                LuaVirtualView {
                    model: model_handle,
                    view,
                    delegate,
                    active,
                    reusable: HashMap::new(),
                    reuse_order: VecDeque::new(),
                    reuse_limit,
                    pool_root: None,
                    exiting: Vec::new(),
                    column_extent,
                    positioned: virtualized,
                    size_field: text_field(properties.get_value(ctx, "size_field")),
                    kind_field: text_field(properties.get_value(ctx, "kind_field")),
                },
            );
        }
        Ok(LuaValue::UserData(node_userdata(
            ctx,
            Rc::clone(state),
            node,
        )))
    }
}

/// A string property read as owned text, if it is one.
fn text_field(value: LuaValue<'_>) -> Option<String> {
    match value {
        LuaValue::String(text) => Some(text.display_lossy().to_string()),
        _ => None,
    }
}

/// A view's size as it is made: a number, or 0 for one given as a binding --
/// the view reads its node's size again as it lays out, so a list bound to
/// its panel's height shows the rows that fit once the panel is laid out.
fn view_size<'gc>(
    ctx: luna::Context<'gc>,
    properties: luna::Table<'gc>,
    field: &str,
) -> Result<f64, luna::Error<'gc>> {
    match properties.get_value(ctx, field) {
        luna::Value::Function(_) => Ok(0.0),
        _ => Ok(table_number(ctx, properties, field, 0.0).map_err(HostError)?),
    }
}
