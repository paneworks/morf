//! D-Bus serving: owning a name, answering calls, emitting signals, and
//! leaving the bus when dropped.

use std::thread;
use std::time::{Duration, Instant};

use crate::*;

#[test]
fn a_service_owns_a_name_answers_a_call_and_emits_a_signal() {
    // The whole serving half in one pass, because the halves are only useful
    // together: a name nobody can call is not a service, and a reply nobody
    // asked for is not an answer.
    //
    // This is the first time this engine is a service rather than a client, and
    // it is what everything that requires *being* something on the bus needs —
    // a notification server, an MPRIS player, a portal backend. Each is a name
    // plus a handful of methods.
    const NAME: &str = "org.morf.ServeSmoke";
    const PATH: &str = "/org/morf/ServeSmoke";
    const INTERFACE: &str = "org.morf.ServeSmoke";

    let Ok((mut service, outcome)) = DbusService::own(Bus::Session, NAME, PATH, true) else {
        // No session bus here; the thing under test cannot run.
        return;
    };
    assert_eq!(outcome, NameOutcome::Owned, "the name is ours");
    assert_eq!(service.name(), NAME);

    // A second connection, calling us the way anybody else would.
    let caller = thread::spawn(|| {
        let proxy = DbusProxy::connect_with_timeout(
            Bus::Session,
            NAME,
            PATH,
            INTERFACE,
            Duration::from_secs(5),
        )
        .expect("a caller can connect");
        proxy.call_value("Echo")
    });

    // Answer it. The call has to be read before the reply can be addressed to
    // it, which is why the caller runs on its own thread — both halves are
    // blocking, and doing them in one order on one thread is a deadlock.
    let call = service
        .next_call(Duration::from_secs(5))
        .expect("the call arrived");
    assert_eq!(call.member, "Echo");
    assert_eq!(call.interface, INTERFACE);
    assert_eq!(call.path, PATH);
    assert!(!call.sender.is_empty(), "and it says who called");
    service
        .reply(call.id, &DbusValue::String("answered".to_owned()))
        .expect("the reply is sent");

    assert_eq!(
        caller.join().expect("the caller finished").unwrap(),
        // One argument, bare. It used to arrive wrapped in a variant -- a
        // `Value` handed to zbus serialises as `v` -- and a caller that asked
        // for `s` and was given `v` rejected it. Now the body is built the
        // same way for one value as for several, and one value is one
        // argument on the wire, which the decoder hands back as itself.
        DbusValue::String("answered".to_owned()),
        "and the caller got it",
    );

    // Answering twice is refused rather than silently sending a second reply,
    // which the caller would have no way to interpret.
    assert!(
        service.reply(call.id, &DbusValue::Nil).is_err(),
        "a call can only be answered once",
    );

    // And a signal reaches somebody listening for it.
    let listener =
        DbusProxy::connect(Bus::Session, NAME, PATH, INTERFACE).expect("a listener can connect");
    let signals = listener
        .subscribe("Rang")
        .expect("the subscription is made");
    service
        .emit(PATH, INTERFACE, "Rang", &DbusValue::Integer(7))
        .expect("the signal is emitted");
    let received = signals.next_value(Duration::from_secs(5));
    assert_eq!(
        received.map(Result::unwrap),
        // Bare, for the same reason a reply is: one value is one argument.
        Some(DbusValue::Integer(7)),
        "the signal arrived with its body",
    );
}

#[test]
fn a_service_with_no_name_answers_on_its_unique_one() {
    // An agent registers a path with an authority and is called back on its
    // unique name; it owns nothing well-known, and on the system bus could
    // not. An empty name is that service.
    const PATH: &str = "/org/morf/Nameless";
    let Ok((mut service, outcome)) = DbusService::own(Bus::Session, "", PATH, false) else {
        return;
    };
    assert_eq!(outcome, NameOutcome::Owned);
    let unique = service.name().to_owned();
    assert!(
        unique.starts_with(':'),
        "the name is the unique one: {unique}"
    );
    let caller = thread::spawn(move || {
        let proxy = DbusProxy::connect_with_timeout(
            Bus::Session,
            unique,
            PATH,
            "org.morf.Nameless",
            Duration::from_secs(5),
        )
        .expect("a caller can connect to a unique name");
        proxy.call_value("Ping")
    });
    let call = service
        .next_call(Duration::from_secs(5))
        .expect("the call arrived");
    assert_eq!(call.member, "Ping");
    service.reply(call.id, &DbusValue::Nil).unwrap();
    assert!(caller.join().unwrap().is_ok());
}

#[test]
fn a_service_dropped_leaves_the_bus() {
    // Its reader thread held the connection, blocked on the socket, after
    // the service had gone: a thread, a socket and a bus connection for
    // every runtime that came and went. Gone now means gone from the bus.
    const PATH: &str = "/org/morf/Dropped";
    let Ok((service, _)) = DbusService::own(Bus::Session, "", PATH, false) else {
        return;
    };
    let unique = service.name().to_owned();
    drop(service);
    let bus = DbusProxy::connect(
        Bus::Session,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .expect("the bus answers");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let owned = bus
            .call_value_with("NameHasOwner", &DbusValue::String(unique.clone()))
            .expect("the bus says whether a name has an owner");
        if owned == DbusValue::Bool(false) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{unique} is still on the bus after its service was dropped"
        );
        thread::sleep(Duration::from_millis(20));
    }
}
