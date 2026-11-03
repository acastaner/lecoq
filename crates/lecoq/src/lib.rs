//! The lecoq app: the startup path (§1.3), the eframe app, the egui chrome and input → commands.
//!
//! The binary (`main.rs`) only calls [`run`]. Everything else lives in this library so the
//! headless App tests (spec §9) drive exactly the code the binary runs.

mod animation;
pub mod app;
mod bench;
pub mod commands;
mod config_save;
mod decode;
mod editor;
mod input;
mod pan;
mod paste;
mod save;
mod siblings;
mod startup;
pub mod strings;
pub mod tools;
mod ui;
mod view;

pub use app::{Launch, LecoqApp};
pub use commands::Command;
pub use startup::{WINDOW_ICON_PNG, run};
