//! The processes around a shell: the supervisor, the workers per output, services, crash reports, sockets.

pub mod crash;
pub mod services;
pub mod socket_path;
pub mod supervisor;
pub mod workers;
