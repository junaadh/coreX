pub mod bindgen;
pub mod diagnostics;
pub mod dump;
pub mod project;
pub mod ui;

pub type DynError = Box<dyn std::error::Error>;
