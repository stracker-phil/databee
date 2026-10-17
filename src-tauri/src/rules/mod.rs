//! Rules from presets (TOML files) and projects (per connection and database): references, kinds, value styles,
//! entities, patterns, pinned queries, anomalies, actions and find sources. Everything here is
//! generic; application knowledge lives only in preset files (docs/presets.md).

pub mod actions;
pub mod annotate;
pub mod applied;
pub mod catalog;
pub mod commands;
pub mod condition;
pub mod coverage;
pub mod draft;
pub mod effective;
pub mod kinds;
pub mod lens;
pub mod overview;
pub mod placeholder;
pub mod prefix;
pub mod preset;
pub mod project;
pub mod project_change;
pub mod queries;
#[cfg(test)]
mod server_tests;
#[cfg(test)]
mod testing;
pub mod types;

pub use catalog::{PresetCatalog, PresetInfo, PresetSource};
pub use project::ProjectsStore;
pub use types::Preset;
