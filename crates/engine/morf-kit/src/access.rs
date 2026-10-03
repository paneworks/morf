//! What a control is to a screen reader: the role its archetype and widget
//! give it, the role of the items it makes (a tab list's tabs, a list's
//! rows), and the accessible states read off its live state.
//!
//! Roles are the engine's names (`morf_scene::accessible`): snake case,
//! `"button"`, `"check_box"`, `"tab_list"`, ... The glue sets them on the
//! control's node; a configuration's own `accessible_role` wins.

use morf_value::IpcValue;

/// The role of a control of `archetype` drawn as `widget`; `checkable` makes
/// a plain press a toggle button.
pub fn role_of(archetype: &str, widget: &str, checkable: bool) -> &'static str {
    match archetype {
        "Press" => match widget {
            "switch" => "switch",
            "checkbox" => "check_box",
            "radio" | "rating_star" => "radio_button",
            "link" => "link",
            "menu_item" => "menu_item",
            "check_menu_item" => "menu_item_check",
            "radio_menu_item" => "menu_item_radio",
            "toggle" | "toggle_group_member" | "chip_filter" => "toggle_button",
            "segment" => "radio_button",
            _ if checkable => "toggle_button",
            _ => "button",
        },
        "Range" => match widget {
            "spin_button" | "numeric_entry" => "spin_button",
            "scroll_bar" => "scroll_bar",
            _ => "slider",
        },
        "Plane" => "slider",
        "Selection" => match widget {
            "tabs" | "view_switcher" | "inline_view_switcher" | "carousel_dots" | "pagination"
            | "stepper_header" => "tab_list",
            "segmented" | "radio_group" | "toggle_group" | "rating_items" => "radio_group",
            "grid_selection" | "day_grid" | "swatch_grid" | "emoji_grid" | "icon_chooser" => "grid",
            "breadcrumbs" => "navigation",
            _ => "list_box",
        },
        "Popup" => match widget {
            "menu" | "context_menu" | "menu_bar_menu" | "submenu" => "menu",
            "tooltip" | "rich_tooltip" | "hover_card" => "tooltip",
            "dropdown_list" | "autocomplete_list" => "list_box",
            "alert_dialog" | "message_dialog" => "alert_dialog",
            "toast" | "snackbar" | "notification_popup" => "status",
            "banner" => "alert",
            _ => "dialog",
        },
        "TextField" => match widget {
            "password" | "otp" => "password_text",
            "search" | "filter_field" => "search_field",
            "text_area" | "mentions" | "code_input" => "text_area",
            _ => "text_field",
        },
        "Scroll" => "scroll_pane",
        "Collection" => match widget {
            "grid_view" | "flow_box" => "grid",
            "data_table" => "table",
            "tree_view" => "tree",
            "tree_table" => "tree_grid",
            _ => "list",
        },
        "Disclosure" => match widget {
            "tree_node" => "tree_item",
            _ => "button",
        },
        "Drag" => match widget {
            "split_pane" | "resizable_panel" => "splitter",
            "resize_grip" | "sheet_handle" | "window_move" => "grip",
            _ => "group",
        },
        "Navigation" => match widget {
            "tab_pages" => "tab_panel",
            _ => "group",
        },
        "Shell" => "application",
        "Dock" => "group",
        "Transform" => match widget {
            "floating_panel" | "pip_window" => "dialog",
            _ => "group",
        },
        "Sheet" => "grid",
        "Roving" => match widget {
            "menubar" => "menu_bar",
            _ => "toolbar",
        },
        "Form" => "form",
        "Overflow" => "toolbar",
        "Canvas" => match widget {
            "image_viewer" => "image",
            _ => "group",
        },
        _ => "group",
    }
}

/// The role of each item a control of `archetype` drawn as `widget` makes,
/// or `None` when it makes none.
pub fn item_role(archetype: &str, widget: &str) -> Option<&'static str> {
    Some(match (archetype, role_of(archetype, widget, false)) {
        ("Selection" | "Collection", "tab_list") => "tab",
        ("Selection" | "Collection", "radio_group") => "radio_button",
        ("Selection" | "Collection", "grid") => "grid_cell",
        ("Selection" | "Collection", "navigation") => "link",
        ("Selection" | "Collection", "list_box") => "list_box_option",
        ("Collection", "list") => "list_item",
        ("Collection", "table" | "tree_grid") => "row",
        ("Collection", "tree") => "tree_item",
        ("Popup", "menu") => "menu_item",
        ("Popup", "list_box") => "list_box_option",
        _ => return None,
    })
}

/// The accessible states a control's live state says, as the entries of
/// its node's `accessible` table: `checked` (true, false or `"mixed"`),
/// `expanded`, `disabled`, `pressed`, `read_only`, `modal`, `value`,
/// `minimum`, `maximum`, `step`, `orientation`, `placeholder`. A state the
/// archetype does not have is left out.
pub fn states_of(archetype: &str, role: &str, state: &[(String, IpcValue)]) -> Vec<(String, IpcValue)> {
    let get = |name: &str| state.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    let truth = |name: &str| matches!(get(name), Some(IpcValue::Boolean(true)));
    let mut out: Vec<(String, IpcValue)> = Vec::new();
    let mut put = |k: &str, v: IpcValue| out.push((k.to_owned(), v));
    if let Some(IpcValue::Boolean(enabled)) = get("enabled") {
        put("disabled", (!enabled).into());
    }
    match archetype {
        "Press" => {
            let checkable = truth("checkable")
                || matches!(role, "switch" | "check_box" | "radio_button" | "toggle_button")
                || matches!(role, "menu_item_check" | "menu_item_radio");
            if checkable {
                if truth("partial") {
                    put("checked", "mixed".into());
                } else {
                    put("checked", truth("checked").into());
                }
            }
            if role == "button" && truth("down") {
                put("pressed", true.into());
            }
        }
        "Range" | "Plane" => {
            let number = |name: &str| match get(name) {
                Some(IpcValue::Number(n)) => Some(*n),
                Some(IpcValue::Integer(n)) => Some(*n as f64),
                _ => None,
            };
            let (value, from, to, step) = if archetype == "Plane" {
                (number("x"), number("x_from"), number("x_to"), number("step_x"))
            } else {
                (number("value"), number("from"), number("to"), number("step"))
            };
            if let Some(v) = value {
                put("value", v.into());
            }
            if let (Some(a), Some(b)) = (from, to) {
                put("minimum", a.min(b).into());
                put("maximum", a.max(b).into());
            }
            if let Some(s) = step.filter(|s| *s > 0.0) {
                put("step", s.into());
            }
            if let Some(IpcValue::String(o)) = get("orientation") {
                put("orientation", o.clone().into());
            }
        }
        "TextField" => {
            if let Some(IpcValue::String(text)) = get("text") {
                // A password's characters are never read out.
                if role != "password_text" {
                    put("value", text.clone().into());
                }
            }
            put("read_only", truth("read_only").into());
            if let Some(IpcValue::String(p)) = get("placeholder") {
                put("placeholder", p.clone().into());
            }
        }
        "Disclosure" => put("expanded", truth("expanded").into()),
        "Popup" => put("modal", truth("modal").into()),
        "Collection" | "Selection" => {
            if let Some(IpcValue::String(o)) = get("orientation") {
                put("orientation", o.clone().into());
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_follow_the_widget() {
        assert_eq!(role_of("Press", "pill", false), "button");
        assert_eq!(role_of("Press", "pill", true), "toggle_button");
        assert_eq!(role_of("Press", "switch", false), "switch");
        assert_eq!(role_of("Selection", "tabs", false), "tab_list");
        assert_eq!(item_role("Selection", "tabs"), Some("tab"));
        assert_eq!(item_role("Collection", "data_table"), Some("row"));
        assert_eq!(item_role("Press", "pill"), None);
        assert_eq!(role_of("TextField", "password", false), "password_text");
    }

    #[test]
    fn states_read_the_live_state() {
        let state = vec![
            ("enabled".to_owned(), IpcValue::Boolean(true)),
            ("checked".to_owned(), IpcValue::Boolean(true)),
        ];
        let out = states_of("Press", "switch", &state);
        assert!(out.contains(&("checked".to_owned(), IpcValue::Boolean(true))));
        assert!(out.contains(&("disabled".to_owned(), IpcValue::Boolean(false))));
        let field = vec![("text".to_owned(), IpcValue::String("hunter2".into()))];
        assert!(states_of("TextField", "password_text", &field).iter().all(|(k, _)| k != "value"));
    }
}
