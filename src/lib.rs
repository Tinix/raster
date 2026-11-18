//! Raster: a blazing-fast, Lua-extensible text editor built in Rust.
//!
//! The binary (`ras`) is a thin wrapper around this library, which keeps
//! the editor core testable without a terminal.

pub mod app;
pub mod buffer;
pub mod command;
pub mod config;
pub mod history;
pub mod keymap;
pub mod keys;
pub mod mode;
pub mod options;
pub mod theme;
pub mod ui;
pub mod vim;
