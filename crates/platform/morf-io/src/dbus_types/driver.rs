//! One thread drives every D-Bus connection.
//!
//! zbus gives each connection an executor and, by default, a thread of its
//! own to tick it: one per bus, per served name, per call bound. Connections
//! are built here with that thread switched off and their executors handed to
//! one driver, which ticks each of them for as long as it has work.

use std::sync::OnceLock;

use async_executor::Executor;

/// The driver's executor, or none when its thread could not start.
fn driver() -> Option<&'static Executor<'static>> {
    static DRIVER: OnceLock<Option<&'static Executor<'static>>> = OnceLock::new();
    *DRIVER.get_or_init(|| {
        let executor: &'static Executor<'static> = Box::leak(Box::new(Executor::new()));
        std::thread::Builder::new()
            .name("morf-dbus".into())
            .spawn(move || zbus::block_on(executor.run(std::future::pending::<()>())))
            .ok()
            .map(|_| executor)
    })
}

/// Builds `builder` with no thread of its own and has the driver tick it;
/// with no driver, zbus keeps its own thread.
pub(crate) fn build(
    builder: zbus::connection::Builder<'static>,
) -> zbus::Result<zbus::blocking::Connection> {
    let Some(driver) = driver() else {
        return zbus::block_on(builder.build()).map(Into::into);
    };
    let connection = zbus::block_on(builder.internal_executor(false).build())?;
    let executor = connection.executor().clone();
    driver
        .spawn(async move {
            while !executor.is_empty() {
                executor.tick().await;
            }
        })
        .detach();
    Ok(connection.into())
}
