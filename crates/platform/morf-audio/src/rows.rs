//! Devices and streams as rows: the records a shell's lists hold and its
//! scripts read, one field per column, in plain values.

use std::collections::BTreeMap;
use std::sync::Arc;

use morf_value::{IpcTable, IpcValue};

use crate::{Device, Stream};

/// A volume for a script: four decimals, so 0.54 reads as 0.54 and not as
/// the float nearest it.
pub fn tidy(volume: f32) -> f64 {
    (f64::from(volume) * 10_000.0).round() / 10_000.0
}

fn text(value: &Option<String>) -> IpcValue {
    value
        .as_ref()
        .map_or(IpcValue::Nil, |value| IpcValue::String(value.clone()))
}

fn number(value: f64) -> IpcValue {
    IpcValue::Number(value)
}

fn map(row: BTreeMap<String, IpcValue>) -> IpcValue {
    IpcValue::Table(Arc::new(IpcTable::Map(row)))
}

/// A device's row: `id`, `name`, `description`, `kind`, `volume`,
/// `volumes`, `muted`, `default`, `channels`, `icon_name`.
pub fn device_row(device: &Device, default: bool) -> IpcValue {
    let mut row = BTreeMap::new();
    row.insert("id".into(), number(f64::from(device.id)));
    row.insert("name".into(), IpcValue::String(device.name.clone()));
    row.insert(
        "description".into(),
        IpcValue::String(device.description.clone()),
    );
    row.insert("kind".into(), IpcValue::String(device.kind.name().into()));
    row.insert("volume".into(), number(tidy(device.volume())));
    row.insert(
        "volumes".into(),
        IpcValue::Table(Arc::new(IpcTable::List(
            device
                .channel_volumes
                .iter()
                .map(|gain| number(tidy(crate::volume::from_linear(*gain))))
                .collect(),
        ))),
    );
    row.insert("muted".into(), IpcValue::Boolean(device.muted));
    row.insert("default".into(), IpcValue::Boolean(default));
    row.insert("channels".into(), number(device.channels() as f64));
    row.insert("icon_name".into(), text(&device.icon_name));
    map(row)
}

/// A stream's row: `id`, `app_name`, `app_id`, `binary`, `icon_name`,
/// `media_name`, `direction`, `device`, `volume`, `muted`, `channels`, `pid`.
pub fn stream_row(stream: &Stream) -> IpcValue {
    let mut row = BTreeMap::new();
    row.insert("id".into(), number(f64::from(stream.id)));
    row.insert("app_name".into(), IpcValue::String(stream.app_name.clone()));
    row.insert("app_id".into(), text(&stream.app_id));
    row.insert("binary".into(), text(&stream.binary));
    row.insert("icon_name".into(), text(&stream.icon_name));
    row.insert("media_name".into(), text(&stream.media_name));
    row.insert(
        "direction".into(),
        IpcValue::String(stream.direction.name().into()),
    );
    row.insert(
        "device".into(),
        stream
            .device
            .map_or(IpcValue::Nil, |device| number(f64::from(device))),
    );
    row.insert("volume".into(), number(tidy(stream.volume())));
    row.insert("muted".into(), IpcValue::Boolean(stream.muted));
    row.insert("channels".into(), number(stream.channels() as f64));
    row.insert(
        "pid".into(),
        stream
            .pid
            .map_or(IpcValue::Nil, |pid| number(f64::from(pid))),
    );
    map(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceKind;

    fn field<'a>(row: &'a IpcValue, key: &str) -> &'a IpcValue {
        match row {
            IpcValue::Table(table) => match &**table {
                IpcTable::Map(fields) => &fields[key],
                IpcTable::List(_) => panic!("a row is a map"),
            },
            _ => panic!("a row is a table"),
        }
    }

    #[test]
    fn volumes_read_as_four_decimals() {
        assert_eq!(tidy(0.54), 0.54);
        assert_eq!(tidy(1.0 / 3.0), 0.3333);
    }

    #[test]
    fn a_device_row_carries_its_columns() {
        let device = Device {
            id: 42,
            name: "alsa_output".into(),
            description: "Speakers".into(),
            kind: DeviceKind::Sink,
            channel_volumes: vec![1.0, 1.0],
            muted: false,
            icon_name: None,
        };
        let row = device_row(&device, true);
        assert_eq!(field(&row, "id"), &IpcValue::Number(42.0));
        assert_eq!(field(&row, "kind"), &IpcValue::String("sink".into()));
        assert_eq!(field(&row, "default"), &IpcValue::Boolean(true));
        assert_eq!(field(&row, "channels"), &IpcValue::Number(2.0));
        assert_eq!(field(&row, "icon_name"), &IpcValue::Nil);
    }
}
