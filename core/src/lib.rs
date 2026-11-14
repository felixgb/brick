//! Brick mail engine.
//!
//! Everything here is UI-agnostic. Front ends talk to it through [`Engine::call`]
//! (JSON in, JSON out, never blocks on the network) and receive JSON events
//! through a callback. The C ABI in [`ffi`] is a thin wrapper over that.

pub mod claude;
pub mod engine;
pub mod ffi;
pub mod ics;
pub mod imap;
pub mod items;
pub mod mime;
pub mod model;
pub mod oauth;
pub mod providers;
pub mod secrets;
pub mod smtp;
pub mod store;
pub mod sync;
pub mod threads;

pub use engine::Engine;
