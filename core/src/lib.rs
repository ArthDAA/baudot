//! Omnikey v0.2 - cœur sans dépendance OS.
//!
//! - [`key`] : positions physiques ISO (Baudot Key ID) et touches logiques v0.1.
//! - [`protocol`] : trames de 32 octets échangées avec le clavier.
//! - [`machine`] : machine à états, transposée de `StateMachine` (main.rs v0.1).
//! - [`engine`] : relie trames, résolution de disposition et machine à états.
//! - [`conf`] : lecture, fusion et écriture des profils `.conf`.

pub mod conf;
pub mod engine;
pub mod key;
pub mod machine;
pub mod protocol;

pub use engine::{Decision, Engine, KeyResolver, Order};
pub use key::{KeyId, LogicalKey};
pub use machine::{CharUnits, HudMsg, Layout};
pub use protocol::{BaudotEvent, DeviceFrame, HostFrame};
