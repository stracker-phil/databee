//! Which presets apply to a database, and under which prefixes their table names resolve.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use super::catalog::requires_order;
use super::prefix::{markers_exist, suggest_prefix};
use super::project::Project;
use super::types::Preset;
use crate::schema::DatabaseSchema;

#[derive(Debug, Clone)]
pub struct Applied {
    /// The project's prefix, else the suggestion, else none.
    pub prefix: String,
    /// The project's network prefix, else the suggestion, else the prefix.
    pub network_prefix: String,
    /// The shortest prefix under which a candidate preset's marker tables all exist.
    pub suggested_prefix: Option<String>,
    /// The applied presets, each after the presets it requires.
    pub presets: Vec<Arc<Preset>>,
    /// Table names (without prefix) that resolve under the network prefix.
    network_tables: BTreeSet<String>,
    tables: HashSet<String>,
}

impl Applied {
    /// With `project.presets` set, those presets and what they require apply, whatever tables
    /// exist (the user chose them). Otherwise every preset with marker tables applies when they
    /// all exist under the prefix, and brings the presets it requires. A preset without marker
    /// tables applies only when chosen or required.
    pub fn new(project: &Project, schema: &DatabaseSchema, loaded: &[Arc<Preset>]) -> Self {
        let tables: HashSet<String> = schema.tables.iter().map(|t| t.name.clone()).collect();
        let present: HashSet<&str> = tables.iter().map(String::as_str).collect();
        let chosen = project.presets.as_ref();
        let suggested_prefix = suggest_marker_prefix(
            &present,
            loaded
                .iter()
                .filter(|p| chosen.is_none_or(|names| names.contains(&p.name))),
        );
        let prefix = project
            .prefix
            .clone()
            .or_else(|| suggested_prefix.clone())
            .unwrap_or_default();
        let wanted: Vec<String> = match chosen {
            Some(names) => names.clone(),
            None => detected(&present, &prefix, loaded),
        };
        let presets = ordered(loaded, &wanted);
        let network_tables: BTreeSet<String> = presets
            .iter()
            .flat_map(|p| p.network_tables.iter().cloned())
            .collect();
        let network_list: Vec<String> = network_tables.iter().cloned().collect();
        let network_prefix = project
            .network_prefix
            .clone()
            .or_else(|| suggest_prefix(present.iter().copied(), &network_list))
            .unwrap_or_else(|| prefix.clone());
        Self {
            prefix,
            network_prefix,
            suggested_prefix,
            presets,
            network_tables,
            tables,
        }
    }

    pub fn preset_names(&self) -> Vec<String> {
        self.presets.iter().map(|p| p.name.clone()).collect()
    }

    /// Where `name` (written without prefix) would be: under the network prefix for a network
    /// table, else under the prefix.
    pub fn table(&self, name: &str) -> String {
        if self.network_tables.contains(name) {
            return format!("{}{name}", self.network_prefix);
        }
        format!("{}{name}", self.prefix)
    }

    /// The existing table `name` stands for: `table`, else the other prefix, so a project rule
    /// that names a network table no preset declares still resolves.
    pub fn existing_table(&self, name: &str) -> Option<String> {
        let preferred = self.table(name);
        if self.tables.contains(&preferred) {
            return Some(preferred);
        }
        [&self.prefix, &self.network_prefix]
            .into_iter()
            .map(|prefix| format!("{prefix}{name}"))
            .find(|table| self.tables.contains(table))
    }

    /// The names (without prefix) a real table may be written as in rules; empty for a table
    /// outside both prefixes.
    pub fn rule_names<'t>(&self, table: &'t str) -> Vec<&'t str> {
        let mut names = Vec::new();
        if let Some(name) = table.strip_prefix(self.network_prefix.as_str())
            && self.network_tables.contains(name)
        {
            names.push(name);
        }
        if let Some(name) = table.strip_prefix(self.prefix.as_str())
            && !names.contains(&name)
        {
            names.push(name);
        }
        names
    }
}

/// The shortest prefix under which all marker tables of one of `candidates` exist; ties go to
/// the alphabetically first.
fn suggest_marker_prefix<'p>(
    present: &HashSet<&str>,
    candidates: impl Iterator<Item = &'p Arc<Preset>>,
) -> Option<String> {
    candidates
        .filter_map(|p| suggest_prefix(present.iter().copied(), &p.marker_tables))
        .min_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
}

fn detected(present: &HashSet<&str>, prefix: &str, loaded: &[Arc<Preset>]) -> Vec<String> {
    loaded
        .iter()
        .filter(|p| !p.marker_tables.is_empty())
        .filter(|p| markers_exist(present, prefix, &p.marker_tables))
        .map(|p| p.name.clone())
        .collect()
}

/// `wanted` with their requirements first. A name that no longer loads is skipped with a
/// warning, so one broken preset does not switch off the others.
fn ordered(loaded: &[Arc<Preset>], wanted: &[String]) -> Vec<Arc<Preset>> {
    let by_name: HashMap<String, Arc<Preset>> =
        loaded.iter().map(|p| (p.name.clone(), p.clone())).collect();
    let mut out: Vec<Arc<Preset>> = Vec::new();
    for name in wanted {
        match requires_order(&by_name, std::slice::from_ref(name)) {
            Ok(presets) => {
                for preset in presets {
                    if !out.iter().any(|p| p.name == preset.name) {
                        out.push(preset);
                    }
                }
            }
            Err(e) => log::warn!("Preset {name} does not apply: {e}"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::testing::{preset, project, tables};

    fn base() -> Arc<Preset> {
        preset("[preset]\nname = \"base\"\n")
    }

    fn kit() -> Arc<Preset> {
        preset("[preset]\nname = \"kit\"\nrequires = [\"base\"]\nmarker_tables = [\"items\"]\n")
    }

    fn plain() -> Arc<Preset> {
        preset("[preset]\nname = \"plain\"\n")
    }

    fn net() -> Arc<Preset> {
        preset(
            "[preset]\nname = \"net\"\nmarker_tables = [\"posts\"]\nnetwork_tables = [\"users\"]\n",
        )
    }

    fn names(project: &Project, tables_: &[&str], loaded: &[Arc<Preset>]) -> Vec<String> {
        Applied::new(project, &tables(tables_), loaded).preset_names()
    }

    #[test]
    fn a_preset_comes_after_the_presets_it_requires_whatever_the_load_order() {
        let loaded = [kit(), plain(), base()];

        assert_eq!(names(&project(), &["f_items"], &loaded), ["base", "kit"]);
    }

    #[test]
    fn a_preset_applies_only_when_all_its_marker_tables_exist_under_the_prefix() {
        let pair = preset("[preset]\nname = \"pair\"\nmarker_tables = [\"items\", \"users\"]\n");

        assert!(names(&project(), &["f_items"], std::slice::from_ref(&pair)).is_empty());
        assert_eq!(
            names(
                &project(),
                &["f_items", "f_users"],
                std::slice::from_ref(&pair)
            ),
            ["pair"]
        );
    }

    #[test]
    fn the_prefix_is_suggested_from_marker_tables_and_the_projects_wins() {
        let loaded = [base(), kit()];
        let tables_ = ["f_items", "x_items"];

        let suggested = Applied::new(&project(), &tables(&tables_), &loaded);
        let own = Applied::new(
            &Project {
                prefix: Some("x_".into()),
                ..project()
            },
            &tables(&tables_),
            &loaded,
        );
        let typo = Applied::new(
            &Project {
                prefix: Some("zz_".into()),
                ..project()
            },
            &tables(&tables_),
            &loaded,
        );

        assert_eq!(suggested.prefix, "f_");
        assert_eq!(suggested.suggested_prefix.as_deref(), Some("f_"));
        assert_eq!(own.prefix, "x_");
        assert_eq!(own.suggested_prefix.as_deref(), Some("f_"));
        assert_eq!(own.preset_names(), ["base", "kit"]);
        assert_eq!(typo.prefix, "zz_");
        assert!(typo.presets.is_empty());
    }

    #[test]
    fn a_preset_without_marker_tables_applies_only_when_required_or_listed() {
        let loaded = [base(), plain(), kit()];
        let listed = Project {
            presets: Some(vec!["plain".into()]),
            ..project()
        };

        assert_eq!(names(&project(), &["f_items"], &loaded), ["base", "kit"]);
        assert_eq!(names(&listed, &["f_items"], &loaded), ["plain"]);
    }

    #[test]
    fn listed_presets_apply_without_marker_tables_and_unknown_names_are_skipped() {
        let loaded = [base(), kit()];
        let listed = Project {
            presets: Some(vec!["ghost".into(), "kit".into()]),
            ..project()
        };

        let applied = Applied::new(&listed, &tables(&["other"]), &loaded);

        assert_eq!(applied.preset_names(), ["base", "kit"]);
        assert_eq!(applied.prefix, "");
        assert_eq!(applied.suggested_prefix, None);
    }

    #[test]
    fn an_empty_preset_list_applies_nothing() {
        let none = Project {
            presets: Some(Vec::new()),
            ..project()
        };

        assert!(names(&none, &["f_items"], &[base(), kit()]).is_empty());
    }

    #[test]
    fn network_tables_resolve_under_the_network_prefix_and_the_rest_under_the_prefix() {
        let own = Project {
            prefix: Some("x2_".into()),
            network_prefix: Some("x_".into()),
            ..project()
        };

        let applied = Applied::new(&own, &tables(&["x_users", "x2_posts", "x_extra"]), &[net()]);

        assert_eq!(applied.table("users"), "x_users");
        assert_eq!(applied.table("posts"), "x2_posts");
        assert_eq!(applied.existing_table("users").as_deref(), Some("x_users"));
        assert_eq!(applied.existing_table("posts").as_deref(), Some("x2_posts"));
        assert_eq!(applied.existing_table("extra").as_deref(), Some("x_extra"));
        assert_eq!(applied.existing_table("ghost"), None);
        assert_eq!(applied.rule_names("x_users"), ["users"]);
        assert_eq!(applied.rule_names("x2_posts"), ["posts"]);
        assert!(applied.rule_names("other").is_empty());
    }

    #[test]
    fn the_network_prefix_is_suggested_from_the_applied_presets_network_tables() {
        let own = Project {
            prefix: Some("x2_".into()),
            ..project()
        };
        let schema = tables(&["x_users", "x2_posts"]);

        let with_network = Applied::new(&own, &schema, &[net()]);
        let without = Applied::new(&own, &schema, &[base()]);

        assert_eq!(with_network.network_prefix, "x_");
        assert_eq!(without.network_prefix, "x2_");
    }

    #[test]
    fn a_preset_that_does_not_apply_declares_no_network_tables() {
        let own = Project {
            prefix: Some("x2_".into()),
            presets: Some(Vec::new()),
            ..project()
        };

        let applied = Applied::new(&own, &tables(&["x_users", "x2_posts"]), &[net()]);

        assert_eq!(applied.network_prefix, "x2_");
        assert_eq!(applied.table("users"), "x2_users");
    }
}
