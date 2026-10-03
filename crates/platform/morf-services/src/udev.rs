use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_PROPERTIES: usize = 256;

/// What one read off the multicast socket produced.
///
/// The distinction that matters is between a packet this monitor does not want
/// and no packet at all: the first should be skipped over, the second ends the
/// drain.
enum ReadOutcome {
    Event(UdevEvent),
    Filtered,
    Empty,
}

/// One bounded kernel uevent received from udev's netlink channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdevEvent {
    pub action: String,
    pub devpath: String,
    pub subsystem: Option<String>,
    pub devname: Option<String>,
    pub properties: BTreeMap<String, String>,
}

/// Native udev monitor failure.
#[derive(Debug)]
pub struct UdevError(String);

impl fmt::Display for UdevError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UdevError {}

/// Nonblocking kernel uevent monitor with an optional subsystem filter.
pub struct UdevMonitor {
    socket: OwnedFd,
    subsystem: Option<String>,
    /// Scratch space for one datagram, kept between reads.
    ///
    /// It is 64 KiB, and it used to be a stack array declared inside the read.
    /// Because it is then handed to an FFI call the compiler cannot see into,
    /// the zeroing could not be elided — so a monitor drained up to thirty-two
    /// times a frame paid two megabytes of memset per frame per output to
    /// receive packets that are a few hundred bytes long.
    scratch: Vec<u8>,
    /// Rings the shell's loop when the socket has something, so a monitor
    /// costs nothing while the machine is quiet and a device plugged in is
    /// seen at once rather than at whatever wakes the shell next.
    alarm: Option<Alarm>,
}

/// Where the watcher thread stands: watching, fired and waiting for the
/// socket to be drained, or told to stop.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AlarmState {
    Armed,
    Fired,
    Stop,
}

/// A thread that waits on the monitor's socket and rings the loop once per
/// burst: after it fires it waits to be re-armed by a drain that found the
/// socket empty, because a readable socket stays readable until it is read.
struct Alarm {
    state: Arc<(Mutex<AlarmState>, Condvar)>,
    /// An eventfd that ends the thread's `poll`.
    stop: OwnedFd,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Alarm {
    fn start(socket: OwnedFd) -> io::Result<Self> {
        let raw = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        let stop = unsafe { OwnedFd::from_raw_fd(raw) };
        let stop_raw = stop.as_raw_fd();
        let state = Arc::new((Mutex::new(AlarmState::Armed), Condvar::new()));
        let shared = Arc::clone(&state);
        let join = std::thread::Builder::new()
            .name("morf-udev".to_owned())
            .spawn(move || {
                let (lock, armed) = &*shared;
                loop {
                    {
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        while *state == AlarmState::Fired {
                            state = armed.wait(state).unwrap_or_else(|error| error.into_inner());
                        }
                        if *state == AlarmState::Stop {
                            return;
                        }
                    }
                    let mut descriptors = [
                        libc::pollfd {
                            fd: socket.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        },
                        libc::pollfd {
                            fd: stop_raw,
                            events: libc::POLLIN,
                            revents: 0,
                        },
                    ];
                    let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) };
                    if ready < 0 {
                        if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return;
                    }
                    if descriptors[1].revents != 0 {
                        return;
                    }
                    if descriptors[0].revents == 0 {
                        continue;
                    }
                    {
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        if *state == AlarmState::Stop {
                            return;
                        }
                        *state = AlarmState::Fired;
                    }
                    morf_io::wake_all();
                    // A socket in error stays readable: one ring is enough,
                    // and the reader reports the error.
                    if descriptors[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)
                        != 0
                    {
                        return;
                    }
                }
            })?;
        Ok(Self {
            state,
            stop,
            join: Some(join),
        })
    }

    /// Watches again after a drain that emptied the socket.
    fn rearm(&self) {
        let (lock, armed) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if *state == AlarmState::Fired {
            *state = AlarmState::Armed;
            armed.notify_one();
        }
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        {
            let (lock, armed) = &*self.state;
            *lock.lock().unwrap_or_else(|error| error.into_inner()) = AlarmState::Stop;
            armed.notify_one();
        }
        let one = 1u64.to_ne_bytes();
        let _ = unsafe { libc::write(self.stop.as_raw_fd(), one.as_ptr().cast(), one.len()) };
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl UdevMonitor {
    /// Opens the kernel uevent multicast channel.
    pub fn new(subsystem: Option<String>) -> Result<Self, UdevError> {
        let raw = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                libc::NETLINK_KOBJECT_UEVENT,
            )
        };
        if raw < 0 {
            return Err(last_error("could not open udev monitor"));
        }
        let socket = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        address.nl_family = libc::AF_NETLINK as u16;
        address.nl_groups = 1;
        let result = unsafe {
            libc::bind(
                socket.as_raw_fd(),
                (&raw const address).cast(),
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };
        if result < 0 {
            return Err(last_error("could not bind udev monitor"));
        }
        // Without the watcher the monitor still works when polled; it only
        // loses the promptness, so failing to start one is not an error.
        let alarm = socket.try_clone().and_then(Alarm::start).ok();
        Ok(Self {
            socket,
            subsystem: subsystem.filter(|value| !value.is_empty()),
            scratch: vec![0; MAX_EVENT_BYTES],
            alarm,
        })
    }

    /// Waits up to the supplied timeout for one matching event.
    ///
    /// `None` means the socket is empty, and only that. The multicast group
    /// this monitor binds carries every uevent on the machine, so a subscriber
    /// interested in one subsystem sees a great many packets it does not want —
    /// and reporting those as "nothing here" told the caller its queue had run
    /// dry. A single unrelated packet then stopped a drain that had a burst of
    /// wanted events sitting behind it, and they arrived a frame late, or after
    /// the next unrelated packet, or not at all.
    pub fn next_event(&mut self, timeout: Duration) -> Result<Option<UdevEvent>, UdevError> {
        loop {
            match self.read_event(timeout)? {
                ReadOutcome::Event(event) => return Ok(Some(event)),
                ReadOutcome::Empty => {
                    if let Some(alarm) = &self.alarm {
                        alarm.rearm();
                    }
                    return Ok(None);
                }
                // Not the subsystem asked for. Go back for the next packet
                // rather than claiming the socket is empty.
                ReadOutcome::Filtered => {}
            }
        }
    }

    fn read_event(&mut self, timeout: Duration) -> Result<ReadOutcome, UdevError> {
        let mut descriptor = libc::pollfd {
            fd: self.socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let milliseconds = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        let ready = unsafe { libc::poll(&raw mut descriptor, 1, milliseconds) };
        if ready < 0 {
            return Err(last_error("could not poll udev monitor"));
        }
        if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(UdevError("udev monitor reported a socket error".into()));
        }
        if ready == 0 || descriptor.revents & libc::POLLIN == 0 {
            return Ok(ReadOutcome::Empty);
        }
        let bytes = &mut self.scratch;
        let length = unsafe {
            libc::recv(
                self.socket.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT | libc::MSG_TRUNC,
            )
        };
        if length < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(ReadOutcome::Empty);
            }
            return Err(UdevError(format!("could not read udev event: {error}")));
        }
        let length =
            usize::try_from(length).map_err(|_| UdevError("invalid udev event size".into()))?;
        if length > bytes.len() {
            return Err(UdevError(format!(
                "udev event exceeds {MAX_EVENT_BYTES} bytes"
            )));
        }
        let event = parse_event(&bytes[..length])?;
        if self
            .subsystem
            .as_deref()
            .is_some_and(|wanted| event.subsystem.as_deref() != Some(wanted))
        {
            return Ok(ReadOutcome::Filtered);
        }
        Ok(ReadOutcome::Event(event))
    }
}

fn parse_event(bytes: &[u8]) -> Result<UdevEvent, UdevError> {
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let header = fields
        .next()
        .ok_or_else(|| UdevError("udev event has no header".into()))?;
    let header = std::str::from_utf8(header)
        .map_err(|_| UdevError("udev event header is not UTF-8".into()))?;
    let (header_action, header_devpath) = header
        .split_once('@')
        .ok_or_else(|| UdevError("udev event header has no device path".into()))?;
    let mut properties = BTreeMap::new();
    for (index, field) in fields.enumerate() {
        if index == MAX_PROPERTIES {
            return Err(UdevError(format!(
                "udev event exceeds {MAX_PROPERTIES} properties"
            )));
        }
        let field = std::str::from_utf8(field)
            .map_err(|_| UdevError("udev event property is not UTF-8".into()))?;
        if let Some((key, value)) = field.split_once('=') {
            properties.insert(key.to_owned(), value.to_owned());
        }
    }
    let action = properties
        .get("ACTION")
        .cloned()
        .unwrap_or_else(|| header_action.to_owned());
    let devpath = properties
        .get("DEVPATH")
        .cloned()
        .unwrap_or_else(|| header_devpath.to_owned());
    Ok(UdevEvent {
        action,
        devpath,
        subsystem: properties.get("SUBSYSTEM").cloned(),
        devname: properties.get("DEVNAME").cloned(),
        properties,
    })
}

fn last_error(context: &str) -> UdevError {
    UdevError(format!("{context}: {}", io::Error::last_os_error()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_alarm_rings_once_per_burst_and_again_after_a_drain() {
        // A socket pair stands in for the netlink one: the alarm only needs
        // something that becomes readable.
        let (reader, writer) = std::os::unix::net::UnixDatagram::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let wake = morf_io::Wake::new().unwrap();
        wake.drain();
        let alarm = Alarm::start(reader.try_clone().unwrap().into()).unwrap();
        writer.send(b"one").unwrap();
        assert!(wake.wait(Duration::from_secs(5)), "a packet rang the loop");
        wake.drain();
        // Unread, the socket stays readable; the alarm waits to be re-armed
        // rather than ringing on and on. (Asked of its state, since other
        // tests ring every alarm in the process.)
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            *alarm.state.0.lock().unwrap() == AlarmState::Fired,
            "once per burst"
        );
        let mut buffer = [0u8; 16];
        while reader.recv(&mut buffer).is_ok() {}
        alarm.rearm();
        writer.send(b"two").unwrap();
        assert!(
            wake.wait(Duration::from_secs(5)),
            "the next burst rings again"
        );
        drop(alarm);
    }

    use super::*;

    #[test]
    fn parses_kernel_uevent_properties() {
        let event = parse_event(
            b"add@/devices/virtual/input/input1\0ACTION=add\0DEVPATH=/devices/virtual/input/input1\0SUBSYSTEM=input\0DEVNAME=input1\0",
        )
        .unwrap();

        assert_eq!(event.action, "add");
        assert_eq!(event.devpath, "/devices/virtual/input/input1");
        assert_eq!(event.subsystem.as_deref(), Some("input"));
        assert_eq!(event.devname.as_deref(), Some("input1"));
    }

    #[test]
    fn rejects_a_header_without_a_device_path() {
        assert!(parse_event(b"add\0ACTION=add\0").is_err());
    }
}
