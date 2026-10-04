//! Box-drawing characters by their four arms: which arm is light, heavy or
//! double, and how the arms are drawn into a cell.

use super::cell_drawing::Canvas;

/// How heavy one arm of a box-drawing character is.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Arm {
    None,
    Light,
    Heavy,
    Double,
}

/// The four arms of a box-drawing character: left, right, up, down.
pub(super) fn arms(character: char) -> Option<[Arm; 4]> {
    use Arm::{Double as D, Heavy as H, Light as L, None as N};
    // Left, right, up, down, for U+2500 onwards; dashes, arcs and diagonals
    // are drawn separately and are `None` here.
    const TABLE: [[Arm; 4]; 128] = [
        [L, L, N, N],
        [H, H, N, N],
        [N, N, L, L],
        [N, N, H, H], // ─━│┃
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // dashes
        [N, L, N, L],
        [N, H, N, L],
        [N, L, N, H],
        [N, H, N, H], // ┌┍┎┏
        [L, N, N, L],
        [H, N, N, L],
        [L, N, N, H],
        [H, N, N, H], // ┐┑┒┓
        [N, L, L, N],
        [N, H, L, N],
        [N, L, H, N],
        [N, H, H, N], // └┕┖┗
        [L, N, L, N],
        [H, N, L, N],
        [L, N, H, N],
        [H, N, H, N], // ┘┙┚┛
        [N, L, L, L],
        [N, H, L, L],
        [N, L, H, L],
        [N, L, L, H], // ├┝┞┟
        [N, L, H, H],
        [N, H, H, L],
        [N, H, L, H],
        [N, H, H, H], // ┠┡┢┣
        [L, N, L, L],
        [H, N, L, L],
        [L, N, H, L],
        [L, N, L, H], // ┤┥┦┧
        [L, N, H, H],
        [H, N, H, L],
        [H, N, L, H],
        [H, N, H, H], // ┨┩┪┫
        [L, L, N, L],
        [H, L, N, L],
        [L, H, N, L],
        [H, H, N, L], // ┬┭┮┯
        [L, L, N, H],
        [H, L, N, H],
        [L, H, N, H],
        [H, H, N, H], // ┰┱┲┳
        [L, L, L, N],
        [H, L, L, N],
        [L, H, L, N],
        [H, H, L, N], // ┴┵┶┷
        [L, L, H, N],
        [H, L, H, N],
        [L, H, H, N],
        [H, H, H, N], // ┸┹┺┻
        [L, L, L, L],
        [H, L, L, L],
        [L, H, L, L],
        [H, H, L, L], // ┼┽┾┿
        [L, L, H, L],
        [L, L, L, H],
        [L, L, H, H],
        [H, L, H, L], // ╀╁╂╃
        [L, H, H, L],
        [H, L, L, H],
        [L, H, L, H],
        [H, H, H, L], // ╄╅╆╇
        [H, H, L, H],
        [H, L, H, H],
        [L, H, H, H],
        [H, H, H, H], // ╈╉╊╋
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // ╌╍╎╏
        [D, D, N, N],
        [N, N, D, D], // ═║
        [N, D, N, L],
        [N, L, N, D],
        [N, D, N, D], // ╒╓╔
        [D, N, N, L],
        [L, N, N, D],
        [D, N, N, D], // ╕╖╗
        [N, D, L, N],
        [N, L, D, N],
        [N, D, D, N], // ╘╙╚
        [D, N, L, N],
        [L, N, D, N],
        [D, N, D, N], // ╛╜╝
        [N, D, L, L],
        [N, L, D, D],
        [N, D, D, D], // ╞╟╠
        [D, N, L, L],
        [L, N, D, D],
        [D, N, D, D], // ╡╢╣
        [D, D, N, L],
        [L, L, N, D],
        [D, D, N, D], // ╤╥╦
        [D, D, L, N],
        [L, L, D, N],
        [D, D, D, N], // ╧╨╩
        [D, D, L, L],
        [L, L, D, D],
        [D, D, D, D], // ╪╫╬
        [N; 4],
        [N; 4],
        [N; 4],
        [N; 4], // ╭╮╯╰
        [N; 4],
        [N; 4],
        [N; 4], // ╱╲╳
        [L, N, N, N],
        [N, N, L, N],
        [N, L, N, N],
        [N, N, N, L], // ╴╵╶╷
        [H, N, N, N],
        [N, N, H, N],
        [N, H, N, N],
        [N, N, N, H], // ╸╹╺╻
        [L, H, N, N],
        [N, N, L, H],
        [H, L, N, N],
        [N, N, H, L], // ╼╽╾╿
    ];
    let index = (character as u32).checked_sub(0x2500)? as usize;
    let arms = *TABLE.get(index)?;
    (arms != [N; 4]).then_some(arms)
}

/// A box-drawing character from its four arms, each running from the cell's
/// centre to its edge, joined where they meet.
pub(super) fn draw_arms(canvas: &mut Canvas, arms: [Arm; 4], light: i32, heavy: i32) {
    let (w, h) = (canvas.width as i32, canvas.height as i32);
    let [left, right, up, down] = arms;
    let thickness = |arm: Arm| match arm {
        Arm::None => 0,
        Arm::Light | Arm::Double => light,
        Arm::Heavy => heavy,
    };
    let band = |extent: i32, thickness: i32| {
        let start = (extent - thickness) / 2;
        (start, start + thickness)
    };
    // How far the crossing reaches, so a horizontal arm runs into the widest
    // vertical one rather than stopping at the centre.
    let vertical = thickness(up).max(thickness(down)).max(light);
    let horizontal = thickness(left).max(thickness(right)).max(light);
    let (vx0, vx1) = band(w, vertical);
    let (hy0, hy1) = band(h, horizontal);
    let double_gap = light;
    // A double line is two light lines either side of the centre line.
    let horizontal_bands = |arm: Arm| -> Vec<(i32, i32)> {
        match arm {
            Arm::None => Vec::new(),
            Arm::Double => {
                let (c0, c1) = band(h, light);
                vec![
                    (c0 - double_gap - light, c1 - double_gap - light),
                    (c0 + double_gap + light, c1 + double_gap + light),
                ]
            }
            arm => vec![band(h, thickness(arm))],
        }
    };
    let vertical_bands = |arm: Arm| -> Vec<(i32, i32)> {
        match arm {
            Arm::None => Vec::new(),
            Arm::Double => {
                let (c0, c1) = band(w, light);
                vec![
                    (c0 - double_gap - light, c1 - double_gap - light),
                    (c0 + double_gap + light, c1 + double_gap + light),
                ]
            }
            arm => vec![band(w, thickness(arm))],
        }
    };
    let doubled = arms.contains(&Arm::Double);
    // With doubles, arms meet at the outer edge of the double bands.
    let reach_x = if doubled {
        (vx0 - double_gap - light, vx1 + double_gap + light)
    } else {
        (vx0, vx1)
    };
    let reach_y = if doubled {
        (hy0 - double_gap - light, hy1 + double_gap + light)
    } else {
        (hy0, hy1)
    };
    for (y0, y1) in horizontal_bands(left) {
        canvas.rect(0, y0, reach_x.1, y1, 1.0);
    }
    for (y0, y1) in horizontal_bands(right) {
        canvas.rect(reach_x.0, y0, w, y1, 1.0);
    }
    for (x0, x1) in vertical_bands(up) {
        canvas.rect(x0, 0, x1, reach_y.1, 1.0);
    }
    for (x0, x1) in vertical_bands(down) {
        canvas.rect(x0, reach_y.0, x1, h, 1.0);
    }
}
