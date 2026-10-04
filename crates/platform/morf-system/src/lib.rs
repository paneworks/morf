//! Native system services for morf.

/// XDG desktop entries: parsing, discovery, lookup, launching.
pub mod desktop_entries;
/// Hierarchical menu models (a tray's, an application's).
pub mod menu;
mod greetd;
mod greetd_conversation;
mod pam;
mod pam_conversation;
pub mod prefers;
mod status_notifier;
mod udev;
mod xkb;

pub use greetd::{AuthMessageType, GreetdClient, GreetdError, GreetdResponse};
pub use greetd_conversation::{GreetdConversation, GreetdEvent};
pub use pam::{PAM_CANCELLED, PamAuthenticator, PamError, PamSession, PamTask};
pub use pam_conversation::{PamEvent, PamPrompt};
pub use status_notifier::{StatusNotifierAddress, StatusNotifierError, StatusNotifierHost};
pub use udev::{UdevError, UdevEvent, UdevMonitor};
pub use xkb::{XkbError, XkbKey, XkbKeymap, XkbSymbol};
