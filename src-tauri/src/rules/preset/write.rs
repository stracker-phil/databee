//! Writes a preset back as a preset file.

use serde::Serialize;

use super::Header;
use crate::rules::actions::Action;
use crate::rules::icons::IconRule;
use crate::rules::kinds::{KindRule, StyleRule};
use crate::rules::queries::{Anomaly, Pattern, Pinned};
use crate::rules::types::{Entity, FindSource, MetaTable, Preset, Reference};

#[derive(Serialize)]
struct FileOut<'a> {
    preset: Header,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    meta: &'a [MetaTable],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    reference: &'a [Reference],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    entity: &'a [Entity],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    kind: &'a [KindRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    style: &'a [StyleRule],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pattern: &'a [Pattern],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pinned: &'a [Pinned],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    anomaly: &'a [Anomaly],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    action: &'a [Action],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    find: &'a [FindSource],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    icon: &'a [IconRule],
}

/// `preset` as a preset file; `parse` reads it back to an equal preset.
pub fn to_toml(preset: &Preset) -> Result<String, toml::ser::Error> {
    toml::to_string(&FileOut {
        preset: Header {
            name: preset.name.clone(),
            requires: preset.requires.clone(),
            marker_tables: preset.marker_tables.clone(),
            network_tables: preset.network_tables.clone(),
        },
        meta: &preset.meta,
        reference: &preset.references,
        entity: &preset.entities,
        kind: &preset.kinds,
        style: &preset.styles,
        pattern: &preset.patterns,
        pinned: &preset.pinned,
        anomaly: &preset.anomalies,
        action: &preset.actions,
        find: &preset.finds,
        icon: &preset.icons,
    })
}
