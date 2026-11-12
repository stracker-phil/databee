//! The effective rules per database, cached in the session, and the `rules_info` summary.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use ts_rs::TS;

use super::{AppliedEntity, EffectiveRules, RuleSources};
use crate::error::AppResult;
use crate::lock::MutexExt;
use crate::rules::catalog::{PresetEntry, loaded};
use crate::rules::icons::TableIcon;
use crate::rules::project::Project;
use crate::rules::types::Preset;
use crate::schema::{self, DatabaseSchema};
use crate::session::Session;

/// What `rules_info` returns: the prefixes, the presets and the entities of a database.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct RulesInfo {
    pub prefix: String,
    pub network_prefix: String,
    pub suggested_prefix: Option<String>,
    /// In apply order: each after the presets it requires.
    pub applied_presets: Vec<String>,
    pub available_presets: Vec<String>,
    pub entities: Vec<AppliedEntity>,
    /// Entity names by real table, filtered entities first.
    pub table_entities: BTreeMap<String, Vec<String>>,
    /// The icon by real table, every table included.
    pub table_icons: BTreeMap<String, TableIcon>,
}

impl RulesInfo {
    pub fn new(rules: &EffectiveRules, loaded: &[Arc<Preset>]) -> Self {
        let applied = &rules.applied;
        Self {
            prefix: applied.prefix.clone(),
            network_prefix: applied.network_prefix.clone(),
            suggested_prefix: applied.suggested_prefix.clone(),
            applied_presets: applied.preset_names(),
            available_presets: loaded.iter().map(|p| p.name.clone()).collect(),
            entities: rules.entities.clone(),
            table_entities: rules.table_entities(),
            table_icons: rules.icons.clone(),
        }
    }
}

/// What a cached entry was built from. The schema and the preset list are compared by pointer:
/// DDL, a refresh or a preset file change replaces them. The project is compared by value.
struct Cached {
    schema: Arc<DatabaseSchema>,
    presets: Arc<[PresetEntry]>,
    project: Project,
    rules: Arc<EffectiveRules>,
}

/// Lives in `Session.ext`, by database.
#[derive(Default)]
struct RulesCache(Mutex<BTreeMap<String, Cached>>);

/// The effective rules of `database` under `schema`, built once per schema, preset and project
/// version.
pub fn effective(
    session: &Session,
    database: &str,
    schema: &Arc<DatabaseSchema>,
    sources: RuleSources<'_>,
) -> Arc<EffectiveRules> {
    let cache = session.ext.get_or_insert_with(RulesCache::default);
    let entries = sources.presets.entries();
    let project = sources.projects.get(&session.slug(), database);
    let mut cached = cache.0.lock_unpoisoned();
    if let Some(hit) = cached.get(database)
        && Arc::ptr_eq(&hit.schema, schema)
        && Arc::ptr_eq(&hit.presets, &entries)
        && hit.project == project
    {
        return hit.rules.clone();
    }
    let rules = Arc::new(EffectiveRules::build(&project, schema, &loaded(&entries)));
    let entry = Cached {
        schema: schema.clone(),
        presets: entries,
        project,
        rules: rules.clone(),
    };
    cached.insert(database.to_string(), entry);
    rules
}

/// The schema of `database` and its effective rules.
pub async fn load(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
) -> AppResult<(Arc<DatabaseSchema>, Arc<EffectiveRules>)> {
    let schema = schema::database(session, database).await?;
    let rules = effective(session, database, &schema, sources);
    Ok((schema, rules))
}

pub async fn rules_info(
    session: &Session,
    database: &str,
    sources: RuleSources<'_>,
) -> AppResult<RulesInfo> {
    let (_, rules) = load(session, database, sources).await?;
    Ok(RulesInfo::new(&rules, &sources.presets.presets()))
}
