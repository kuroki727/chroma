#![forbid(unsafe_code)]

pub mod adapters;
pub mod cli;
pub mod config;
pub mod engine;
pub mod integration;
pub mod palette;
pub mod safe_fs;
pub mod wallpaper;

pub use anyhow::{Context, Result, bail, ensure};
