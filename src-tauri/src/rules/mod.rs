//! Rules from presets (TOML files) and projects (per connection and database): references, kinds, value styles,
//! entities, patterns, pinned queries, anomalies, actions and find sources. Everything here is
//! generic; application knowledge lives only in preset files (docs/presets.md).

pub mod actions;
pub mod catalog;
pub mod commands;
pub mod kinds;
pub mod placeholder;
pub mod prefix;
pub mod preset;
pub mod project;
pub mod project_change;
pub mod queries;
pub mod types;

pub use catalog::{PresetCatalog, PresetInfo, PresetSource};
pub use project::ProjectsStore;
pub use types::Preset;
