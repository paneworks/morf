//! Touch replay with input timestamps independent of frame delivery.

use super::*;

/// A real touch event through the virtual seat, including capture and gestures.
pub(crate) fn touch(host: &mut TestHost, arguments: &[IpcValue]) -> Result<Vec<IpcValue>, String> {
    let phase = optional_text(arguments.first()).unwrap_or_default();
    let id = number(arguments.get(1), "contact id")? as i32;
    let x = number(arguments.get(2), "x")?;
    let y = number(arguments.get(3), "y")?;
    let subject = host.subject()?;
    let surface = role(subject, arguments.get(4))?;
    let time_ms = match arguments.get(5) {
        None | Some(IpcValue::Nil) => None,
        value => {
            let value = number(value, "time_ms")?;
            if !value.is_finite()
                || !(0.0..=u32::MAX as f64).contains(&value)
                || value.fract() != 0.0
            {
                return Err("time_ms must be an unsigned 32-bit millisecond timestamp".into());
            }
            Some(value as u32)
        }
    };
    let event = match phase.as_str() {
        "down" => Event::TouchDown {
            surface,
            id,
            x,
            y,
            time_ms,
        },
        "move" => Event::TouchMotion {
            surface,
            id,
            x,
            y,
            time_ms,
        },
        "up" => Event::TouchUp {
            surface,
            id,
            x,
            y,
            time_ms,
        },
        "cancel" => Event::TouchCancel,
        _ => return Err("touch phase must be down, move, up or cancel".into()),
    };
    subject.pointer(event)?;
    Ok(Vec::new())
}
