//! Builds the effective rules from the project and the applied presets.

use std::collections::HashSet;
use std::sync::Arc;

use super::{
    AppliedEntity, EffectiveRules, GroupKey, MetaRef, RuleAction, RulePattern, RuleReference,
    RuleStyle, inferred_id,
};
use crate::db::ColumnEquals;
use crate::rules::actions::Action;
use crate::rules::applied::Applied;
use crate::rules::condition;
use crate::rules::icons;
use crate::rules::kinds::StyleRule;
use crate::rules::project::Project;
use crate::rules::queries::Pattern;
use crate::rules::types::{ColumnRef, Entity, FindSource, Preset, Reference};
use crate::schema::DatabaseSchema;
use crate::value::ReferenceSource;

impl EffectiveRules {
    pub fn build(project: &Project, schema: &DatabaseSchema, loaded: &[Arc<Preset>]) -> Self {
        let applied = Applied::new(project, schema, loaded);
        let disabled: HashSet<String> = project.disabled.iter().cloned().collect();
        let icons = icons::table_icons(&applied, &project.icons, schema);
        let mut rules = Self {
            applied,
            references: Vec::new(),
            kinds: project.kinds.clone(),
            styles: Vec::new(),
            meta: Vec::new(),
            entities: Vec::new(),
            patterns: Vec::new(),
            actions: Vec::new(),
            finds: Vec::new(),
            disabled,
            icons,
        };
        rules.add_styles(project);
        let (own_on, own_off): (Vec<Reference>, Vec<Reference>) = project
            .references
            .iter()
            .cloned()
            .partition(|r| !rules.disabled.contains(&r.id));
        rules.add_references(&own_on, None);
        rules.keep_inference_off(&own_off);
        let preset_rules: Vec<_> = rules.preset_reference_rules(schema).collect();
        for (preset, references) in preset_rules {
            let kept: Vec<Reference> = references
                .into_iter()
                .filter(|r| !rules.disabled.contains(&r.id) && !is_overridden(project, r))
                .collect();
            rules.add_references(&kept, Some(&preset.name));
            rules.kinds.extend(preset.kinds.iter().cloned());
        }
        let presets = rules.applied.presets.clone();
        for preset in &presets {
            rules.add_preset_tables(preset);
        }
        // Stable, so file order holds among filtered and among unfiltered entities.
        rules.entities.sort_by_key(|e| e.entity.filter.is_none());
        rules
    }

    /// The project's own styles that are on, then the applied presets' styles (the last applied
    /// first) the project switches on, each replaced by its project override.
    fn add_styles(&mut self, project: &Project) {
        let presets = &self.applied.presets;
        let shipped = |id: &str| presets.iter().any(|p| p.styles.iter().any(|s| s.id == id));
        let own = project
            .styles
            .iter()
            .filter(|s| s.enabled && !shipped(&s.id));
        let switched_on = presets
            .iter()
            .rev()
            .flat_map(|p| &p.styles)
            .filter(|s| project.preset_style_on(s))
            .map(|s| project.style_override(&s.id).unwrap_or(s));
        self.styles = own.chain(switched_on).filter_map(to_rule_style).collect();
    }

    /// A switched-off unconditional project reference still replaces the same-name inference on
    /// its column, so switching off an edited inferred reference switches the reference off.
    fn keep_inference_off(&mut self, switched_off: &[Reference]) {
        for rule in switched_off.iter().filter(|r| r.when.is_empty()) {
            if let Some(from) = self.column(&rule.from) {
                self.disabled.insert(inferred_id(&from.table, &from.column));
            }
        }
    }

    /// The references of `preset`, or of the project (`None`). Conditional rules of a source
    /// come before its unconditional ones, which hold for every row and would hide them.
    fn add_references(&mut self, references: &[Reference], preset: Option<&str>) {
        let mut resolved: Vec<RuleReference> = references
            .iter()
            .filter_map(|r| self.resolve_reference(r, preset))
            .collect();
        resolved.sort_by_key(|r| r.when.is_empty());
        self.references.extend(resolved);
    }

    fn resolve_reference(&self, rule: &Reference, preset: Option<&str>) -> Option<RuleReference> {
        let from = self.column(&rule.from)?;
        let to = self.column(&rule.to)?;
        let (when, to_when) = match (
            condition::compile(&rule.when),
            condition::compile(&rule.to_when),
        ) {
            (Ok(when), Ok(to_when)) => (when, to_when),
            (Err(e), _) | (_, Err(e)) => {
                log::warn!("Reference {} skipped: {e}", rule.id);
                return None;
            }
        };
        let to_filter = to_when
            .iter()
            .filter_map(|c| {
                Some(ColumnEquals {
                    column: c.column.clone(),
                    value: Some(c.matcher.equals()?.to_string()),
                })
            })
            .collect();
        let id = rule.id.clone();
        let by = match preset {
            Some(preset) => ReferenceSource::Preset {
                preset: preset.to_string(),
                id,
            },
            None => ReferenceSource::Project { id },
        };
        Some(RuleReference {
            group: GroupKey {
                preset: preset.map(str::to_string),
                rule: rule.default_id(),
            },
            by,
            from,
            when,
            to,
            to_when,
            to_filter,
            label: rule.label.clone(),
            columns: rule.columns.clone(),
        })
    }

    /// The reference rules of each applied preset in precedence order (the last applied first):
    /// its own and those its meta tables imply. Disabled and overridden rules are kept.
    pub(in crate::rules) fn preset_reference_rules<'a>(
        &'a self,
        schema: &'a DatabaseSchema,
    ) -> impl Iterator<Item = (Arc<Preset>, Vec<Reference>)> + 'a {
        self.applied.presets.iter().rev().map(move |preset| {
            let implied = self.meta_references(preset, schema);
            let references = preset.references.iter().chain(&implied).cloned().collect();
            (preset.clone(), references)
        })
    }

    /// The reference `fk → owner` that each owned `[[meta]]` table declares, to the owner's
    /// single-column primary key.
    fn meta_references(&self, preset: &Preset, schema: &DatabaseSchema) -> Vec<Reference> {
        preset
            .meta
            .iter()
            .filter_map(|meta| {
                let (owner, fk) = (meta.owner.as_ref()?, meta.fk.as_ref()?);
                let key = schema.single_key(&self.applied.existing_table(owner)?)?;
                let from = ColumnRef {
                    table: meta.table.clone(),
                    column: fk.clone(),
                };
                let to = ColumnRef {
                    table: owner.clone(),
                    column: key.to_string(),
                };
                Some(Reference::plain(from, to))
            })
            .collect()
    }

    fn column(&self, column: &ColumnRef) -> Option<ColumnRef> {
        Some(ColumnRef {
            table: self.applied.existing_table(&column.table)?,
            column: column.column.clone(),
        })
    }

    fn add_preset_tables(&mut self, preset: &Preset) {
        for meta in &preset.meta {
            let Some(table) = self.applied.existing_table(&meta.table) else {
                continue;
            };
            self.meta.push(MetaRef {
                table,
                name: meta.table.clone(),
                owner: meta
                    .owner
                    .as_ref()
                    .and_then(|owner| self.applied.existing_table(owner)),
                fk: meta.fk.clone(),
                key: meta.key.clone(),
                value: meta.value.clone(),
            });
        }
        for entity in &preset.entities {
            let Some(table) = self.applied.existing_table(&entity.table) else {
                continue;
            };
            self.entities.push(AppliedEntity {
                preset: preset.name.clone(),
                entity: Entity {
                    table,
                    ..entity.clone()
                },
            });
        }
        for pattern in &preset.patterns {
            self.add_pattern(pattern);
        }
        for action in &preset.actions {
            self.add_action(&preset.name, action);
        }
        for find in &preset.finds {
            if let Some(table) = self.applied.existing_table(&find.table) {
                self.finds.push(FindSource {
                    table,
                    ..find.clone()
                });
            }
        }
    }

    fn add_action(&mut self, preset: &str, action: &Action) {
        let Some(table) = self.applied.existing_table(&action.table) else {
            return;
        };
        let when = match condition::compile(&action.when) {
            Ok(conditions) => conditions,
            Err(e) => {
                log::warn!("Action {} skipped: {e}", action.name);
                return;
            }
        };
        self.actions.push(RuleAction {
            preset: preset.to_string(),
            rule: Action {
                table,
                ..action.clone()
            },
            when,
        });
    }

    fn add_pattern(&mut self, pattern: &Pattern) {
        let (Some(owner), Some(table)) = (
            self.applied.existing_table(&pattern.owner),
            self.applied.existing_table(&pattern.table),
        ) else {
            return;
        };
        let owner_when = match condition::compile(&pattern.owner_when) {
            Ok(conditions) => conditions,
            Err(e) => {
                log::warn!("Pattern {} skipped: {e}", pattern.name);
                return;
            }
        };
        self.patterns.push(RulePattern {
            rule: Pattern {
                owner,
                table,
                ..pattern.clone()
            },
            owner_when,
        });
    }
}

/// A project reference with the same id, or the same `from` and `when`, replaces the preset rule.
pub(in crate::rules) fn overrides(own: &Reference, rule: &Reference) -> bool {
    own.id == rule.id || (own.from == rule.from && own.when == rule.when)
}

fn is_overridden(project: &Project, rule: &Reference) -> bool {
    project.references.iter().any(|own| overrides(own, rule))
}

/// A pattern that does not compile was rejected on load, so it is only logged here.
fn to_rule_style(rule: &StyleRule) -> Option<RuleStyle> {
    match rule.matcher() {
        Ok(matcher) => Some(RuleStyle {
            rule: rule.clone(),
            matcher,
        }),
        Err(e) => {
            log::warn!("Value style skipped: {e}");
            None
        }
    }
}
