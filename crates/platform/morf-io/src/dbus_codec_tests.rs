//! D-Bus encoding and decoding, with no bus at all.
//!
//! Messages are built and read back in memory, so these run everywhere and
//! touch nothing: typed empties, descriptors through a socket pair, and the
//! `h` that must never come back as a number.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use zbus::zvariant::{Signature, Value};

use crate::dbus_encode::{dbus_argument_value, decode_message_value, typed_dbus_value};
use crate::{DbusFd, DbusValue};

fn typed(signature: &str, value: DbusValue) -> DbusValue {
    DbusValue::Typed {
        signature: signature.to_owned(),
        value: Box::new(value),
    }
}

/// A method call whose body is `values`, as a caller would send it.
fn message_with(values: &[DbusValue]) -> zbus::Message {
    let mut body = zbus::zvariant::StructureBuilder::new();
    for value in values {
        body = body.append_field(dbus_argument_value(value).unwrap());
    }
    let body = body.build().unwrap();
    zbus::Message::method_call("/org/morf/Test", "Test")
        .unwrap()
        .build(&body)
        .unwrap()
}

#[test]
fn an_empty_table_typed_as_a_map_is_an_empty_map() {
    // `{}` from Lua is an empty list, because Lua cannot say otherwise, and
    // `a{sv}` refused it as "not a map" — there was no way to send an empty
    // dictionary at all, and NetworkManager's `RequestScan` wants exactly one.
    let empty = DbusValue::List(Vec::new());
    let value = typed_dbus_value("a{sv}", &empty).unwrap();
    assert_eq!(value.value_signature().to_string(), "a{sv}");
    let Value::Dict(dict) = value else {
        panic!("a dictionary");
    };
    assert_eq!(dict.iter().count(), 0);

    let nested = typed_dbus_value("a{sa{sv}}", &empty).unwrap();
    assert_eq!(nested.value_signature().to_string(), "a{sa{sv}}");
}

#[test]
fn an_empty_table_typed_as_an_array_is_an_empty_array() {
    let empty = DbusValue::List(Vec::new());
    for signature in ["as", "aay", "ao", "a(ss)"] {
        let value = typed_dbus_value(signature, &empty).unwrap();
        assert_eq!(value.value_signature().to_string(), signature);
    }
    // An empty map is as empty as an empty list.
    let empty_map = DbusValue::Map(BTreeMap::new());
    let value = typed_dbus_value("as", &empty_map).unwrap();
    assert_eq!(value.value_signature().to_string(), "as");
}

#[test]
fn an_empty_map_inside_a_map_keeps_its_type() {
    // `{ ssids = typed("aay", {}) }`: the shape NetworkManager's scan options
    // take, with the inner value typed and the outer inferred.
    let options = DbusValue::Map(BTreeMap::from([(
        "ssids".to_owned(),
        typed("aay", DbusValue::List(Vec::new())),
    )]));
    let message = message_with(&[typed("a{sv}", options)]);
    assert_eq!(message.body().signature().to_string_no_parens(), "a{sv}");
}

#[test]
fn typed_empties_reach_the_wire_as_their_own_signature() {
    let message = message_with(&[
        DbusValue::String("wlan0".to_owned()),
        typed("a{sv}", DbusValue::List(Vec::new())),
        typed("aay", DbusValue::List(Vec::new())),
        typed("u", DbusValue::Integer(3)),
    ]);
    assert_eq!(
        message.body().signature().to_string_no_parens(),
        "sa{sv}aayu"
    );
    let decoded = decode_message_value(&message).unwrap();
    let DbusValue::List(values) = decoded else {
        panic!("several arguments come back as a list: {decoded:?}");
    };
    assert_eq!(values.len(), 4);
    assert_eq!(values[1], DbusValue::Map(BTreeMap::new()));
    assert_eq!(values[2], DbusValue::List(Vec::new()));
    assert_eq!(values[3], DbusValue::Unsigned(3));
}

#[test]
fn a_descriptor_goes_into_a_message_and_comes_back_the_same_socket() {
    let (ours, mut theirs) = UnixStream::pair().unwrap();
    let fd = DbusValue::Fd(DbusFd::new(ours.into()));
    // Typed and untyped both mean `h`.
    for value in [fd.clone(), typed("h", fd.clone())] {
        let message = message_with(std::slice::from_ref(&value));
        assert_eq!(message.body().signature().to_string_no_parens(), "h");
        let decoded = decode_message_value(&message).unwrap();
        let DbusValue::Fd(received) = decoded else {
            panic!("an `h` is a descriptor, not {decoded:?}");
        };
        // A different descriptor number — a duplicate, owned by the value —
        // on the same socket.
        let mut stream = UnixStream::from(received.as_fd().try_clone_to_owned().unwrap());
        stream.write_all(b"k").unwrap();
        let mut byte = [0u8; 1];
        theirs.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"k");
    }
}

#[test]
fn a_descriptor_among_other_arguments_is_decoded_in_place() {
    let (ours, _theirs) = UnixStream::pair().unwrap();
    let message = message_with(&[
        DbusValue::String("sleep".to_owned()),
        DbusValue::Fd(DbusFd::new(ours.into())),
        typed("u", DbusValue::Integer(7)),
    ]);
    assert_eq!(message.body().signature().to_string_no_parens(), "shu");
    let DbusValue::List(values) = decode_message_value(&message).unwrap() else {
        panic!("a list");
    };
    assert!(matches!(values[1], DbusValue::Fd(_)), "{values:?}");
    assert_eq!(values[2], DbusValue::Unsigned(7));
}

#[test]
fn only_a_descriptor_can_be_an_h() {
    let error = typed_dbus_value("h", &DbusValue::Integer(0)).unwrap_err();
    assert!(error.contains("file descriptor"), "{error}");
    assert!(Signature::try_from("h").is_ok());
}

#[test]
fn a_descriptor_closes_when_its_last_value_goes() {
    let (ours, mut theirs) = UnixStream::pair().unwrap();
    let fd = DbusFd::new(ours.into());
    let copy = fd.clone();
    assert_eq!(fd, copy, "a clone is the same descriptor");
    drop(fd);
    theirs
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    // Still open through the clone: nothing to read, and no end of file.
    let mut byte = [0u8; 1];
    assert!(theirs.read(&mut byte).is_err(), "still open");
    drop(copy);
    assert_eq!(
        theirs.read(&mut byte).unwrap(),
        0,
        "closed with the last one"
    );
}

#[test]
fn a_byte_array_comes_back_as_bytes() {
    // An SSID that is not text, and an image's pixels: bytes, not numbers.
    let bytes = vec![0x00, 0xff, b'h', b'i', 0x80];
    for sent in [
        typed("ay", DbusValue::Bytes(bytes.clone())),
        typed(
            "ay",
            DbusValue::List(
                bytes
                    .iter()
                    .map(|byte| DbusValue::Integer(i64::from(*byte)))
                    .collect(),
            ),
        ),
    ] {
        let message = message_with(&[sent]);
        assert_eq!(
            decode_message_value(&message).unwrap(),
            DbusValue::List(vec![DbusValue::Bytes(bytes.clone())])
        );
    }
    // Text sent as `ay` is its UTF-8 bytes.
    let message = message_with(&[typed("ay", DbusValue::String("wifi".into()))]);
    assert_eq!(
        decode_message_value(&message).unwrap(),
        DbusValue::List(vec![DbusValue::Bytes(b"wifi".to_vec())])
    );
    // Bytes with no signature are `ay`.
    assert!(matches!(
        dbus_argument_value(&DbusValue::Bytes(vec![1, 2])).unwrap(),
        Value::Array(array) if array.element_signature() == &Signature::U8
    ));
    // Inside a structure, as a notification's `image-data` is.
    let image = typed(
        "(iiibiiay)",
        DbusValue::List(vec![
            DbusValue::Integer(1),
            DbusValue::Integer(1),
            DbusValue::Integer(4),
            DbusValue::Bool(true),
            DbusValue::Integer(8),
            DbusValue::Integer(4),
            DbusValue::Bytes(vec![1, 2, 3, 4]),
        ]),
    );
    let message = message_with(&[image]);
    // A body that is one structure reads as that structure's fields.
    let DbusValue::List(fields) = decode_message_value(&message).unwrap() else {
        panic!("a structure is a list");
    };
    assert_eq!(fields[6], DbusValue::Bytes(vec![1, 2, 3, 4]));
    // A byte out of range is refused, not wrapped.
    assert!(typed_dbus_value("ay", &DbusValue::List(vec![DbusValue::Integer(256)])).is_err());
}
