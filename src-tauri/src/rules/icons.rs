//! Table icons (`[[icon]]`): a table name or glob and one icon of a curated set, shown before the
//! table's name and before every reference into it and every key of its rows.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::applied::Applied;
use super::condition::glob_match;
use super::types::{Check, FieldError, Preset};
use crate::schema::DatabaseSchema;

/// The icons a rule may name (lucide names; the page maps each to its component). Every table
/// has one: `table` unless a rule gives another.
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, TS,
)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "kebab-case")]
pub enum TableIcon {
    #[default]
    Table,
    Database,
    Server,
    List,
    ListChecks,
    Layers,
    Box,
    Archive,
    Hash,
    Key,
    KeyRound,
    Lock,
    LogIn,
    Shield,
    ShieldCheck,
    User,
    UserRound,
    Users,
    UserCog,
    IdCard,
    FileText,
    Files,
    File,
    FileCode,
    FileBraces,
    FileClock,
    Newspaper,
    BookOpen,
    GraduationCap,
    Image,
    Film,
    Music,
    Paperclip,
    Folder,
    FolderTree,
    Menu,
    Tag,
    Tags,
    Bookmark,
    Star,
    Heart,
    Flag,
    MessageSquare,
    MessageCircle,
    Mail,
    Inbox,
    Send,
    Bell,
    Megaphone,
    Phone,
    ShoppingCart,
    ShoppingBag,
    Store,
    Receipt,
    Package,
    Barcode,
    Truck,
    Warehouse,
    CreditCard,
    Wallet,
    Banknote,
    Coins,
    Landmark,
    BadgePercent,
    Gift,
    Ticket,
    Building,
    Briefcase,
    Globe,
    MapPin,
    Map,
    Route,
    Languages,
    Clock,
    Calendar,
    CalendarClock,
    Timer,
    Settings,
    SlidersHorizontal,
    Wrench,
    Activity,
    ChartBar,
    Code,
    Terminal,
    Bug,
    Webhook,
    Plug,
    Puzzle,
    Blocks,
    GitBranch,
    Workflow,
    Link,
}

impl TableIcon {
    /// The name rules write.
    pub fn name(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
}

/// `table` is a name without prefix or a glob (`*meta`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(deny_unknown_fields)]
pub struct IconRule {
    pub table: String,
    pub icon: TableIcon,
}

impl Check for IconRule {
    fn check(&self) -> Result<(), FieldError> {
        if self.table.is_empty() {
            return Err(FieldError::new("table", "names no table"));
        }
        Ok(())
    }
}

impl IconRule {
    pub fn is_glob(&self) -> bool {
        self.table.contains('*')
    }
}

/// The rule of `icons` for a table written as one of `names`: one naming it exactly, else the
/// first glob that covers it.
pub fn rule_for<'r>(icons: &'r [IconRule], names: &[&str]) -> Option<&'r IconRule> {
    icons
        .iter()
        .find(|r| !r.is_glob() && names.contains(&r.table.as_str()))
        .or_else(|| {
            icons
                .iter()
                .find(|r| r.is_glob() && names.iter().any(|n| glob_match(&r.table, n)))
        })
}

/// The preset rule for a table written as one of `names`, the last applied preset first.
pub fn preset_rule_for<'p>(
    presets: &'p [Arc<Preset>],
    names: &[&str],
) -> Option<(&'p Arc<Preset>, &'p IconRule)> {
    presets
        .iter()
        .rev()
        .find_map(|preset| Some((preset, rule_for(&preset.icons, names)?)))
}

/// The icon of every real table: the project's rule, else the applied presets' (the last applied
/// first); exact names before globs within each; else `table`, as for tables outside the prefixes.
pub fn table_icons(
    applied: &Applied,
    project: &[IconRule],
    schema: &DatabaseSchema,
) -> BTreeMap<String, TableIcon> {
    schema
        .tables
        .iter()
        .map(|t| {
            let names = applied.rule_names(&t.name);
            let icon = rule_for(project, &names)
                .or_else(|| preset_rule_for(&applied.presets, &names).map(|(_, r)| r))
                .map_or_else(TableIcon::default, |r| r.icon);
            (t.name.clone(), icon)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::catalog::BUILT_IN;
    use crate::rules::preset;
    use crate::rules::project::Project;
    use crate::rules::testing::{preset, project, tables};

    fn rule(table: &str, icon: TableIcon) -> IconRule {
        IconRule {
            table: table.into(),
            icon,
        }
    }

    fn icon_preset(name: &str, marker: &str, icons: &[(&str, &str)]) -> Arc<Preset> {
        let rules: String = icons
            .iter()
            .map(|(table, icon)| format!("\n[[icon]]\ntable = \"{table}\"\nicon = \"{icon}\"\n"))
            .collect();
        preset(&format!(
            "[preset]\nname = \"{name}\"\nmarker_tables = [\"{marker}\"]\n{rules}"
        ))
    }

    fn icons_of(
        project: &Project,
        schema_tables: &[&str],
        presets: &[Arc<Preset>],
    ) -> BTreeMap<String, TableIcon> {
        let schema = tables(schema_tables);
        let applied = Applied::new(project, &schema, presets);
        table_icons(&applied, &project.icons, &schema)
    }

    fn map<const N: usize>(entries: [(&str, TableIcon); N]) -> BTreeMap<String, TableIcon> {
        entries
            .into_iter()
            .map(|(table, icon)| (table.to_string(), icon))
            .collect()
    }

    #[test]
    fn icon_names_are_kebab_case() {
        assert_eq!(TableIcon::ShoppingCart.name(), "shopping-cart");
        assert_eq!(TableIcon::Table.name(), "table");
        assert_eq!(TableIcon::Menu.name(), "menu");
    }

    #[test]
    fn new_icons_serialize_with_kebab_case_names() {
        for (icon, name) in [
            (TableIcon::FileBraces, "file-braces"),
            (TableIcon::KeyRound, "key-round"),
            (TableIcon::LogIn, "log-in"),
        ] {
            assert_eq!(icon.name(), name);
            assert_eq!(
                serde_json::from_value::<TableIcon>(serde_json::json!(name)).unwrap(),
                icon,
                "{name}"
            );
        }
    }

    #[test]
    fn every_table_gets_an_icon_with_table_as_the_default() {
        let preset = icon_preset("p", "a", &[("a", "user")]);

        let icons = icons_of(&project(), &["x_a", "x_b", "other"], &[preset]);

        assert_eq!(
            icons,
            map([
                ("x_a", TableIcon::User),
                ("x_b", TableIcon::Table),
                ("other", TableIcon::Table),
            ])
        );
    }

    #[test]
    fn an_exact_preset_rule_beats_a_preset_glob_whatever_their_order() {
        let preset = icon_preset(
            "p",
            "a",
            &[("*meta", "menu"), ("postmeta", "tag"), ("a", "user")],
        );

        let icons = icons_of(&project(), &["x_a", "x_postmeta", "x_usermeta"], &[preset]);

        assert_eq!(
            icons,
            map([
                ("x_a", TableIcon::User),
                ("x_postmeta", TableIcon::Tag),
                ("x_usermeta", TableIcon::Menu),
            ])
        );
    }

    #[test]
    fn the_last_applied_preset_beats_an_earlier_one() {
        let first = icon_preset("first", "a", &[("a", "user"), ("b", "tag")]);
        let second = preset(
            "[preset]\nname = \"second\"\nrequires = [\"first\"]\nmarker_tables = [\"a\"]\n\n[[icon]]\ntable = \"a\"\nicon = \"clock\"\n",
        );

        let icons = icons_of(&project(), &["x_a", "x_b"], &[first, second]);

        assert_eq!(
            icons,
            map([("x_a", TableIcon::Clock), ("x_b", TableIcon::Tag)])
        );
    }

    #[test]
    fn any_project_rule_beats_the_presets() {
        let preset = icon_preset("p", "a", &[("a", "user"), ("b", "tag")]);
        let own = Project {
            icons: vec![rule("a", TableIcon::Star), rule("b*", TableIcon::Key)],
            ..project()
        };

        let icons = icons_of(&own, &["x_a", "x_b"], &[preset]);

        assert_eq!(
            icons,
            map([("x_a", TableIcon::Star), ("x_b", TableIcon::Key)])
        );
    }

    #[test]
    fn a_project_table_rule_beats_a_preset_icon() {
        let preset = icon_preset("p", "a", &[("a", "user"), ("b", "tag")]);
        let own = Project {
            icons: vec![rule("a", TableIcon::Table)],
            ..project()
        };

        let icons = icons_of(&own, &["x_a", "x_b"], &[preset]);

        assert_eq!(
            icons,
            map([("x_a", TableIcon::Table), ("x_b", TableIcon::Tag)])
        );
    }

    #[test]
    fn a_preset_icon_none_is_a_field_error_on_icon() {
        let src = "[preset]\nname = \"p\"\nmarker_tables = [\"a\"]\n\n[[icon]]\ntable = \"b\"\nicon = \"none\"\n";

        let error = preset::parse("p.toml", src).unwrap_err();

        assert_eq!(error.field.as_deref(), Some("icon"));
    }

    #[test]
    fn network_tables_resolve_under_the_network_prefix() {
        let net = preset(
            "[preset]\nname = \"net\"\nmarker_tables = [\"posts\"]\nnetwork_tables = [\"users\"]\n\n[[icon]]\ntable = \"users\"\nicon = \"user\"\n\n[[icon]]\ntable = \"posts\"\nicon = \"files\"\n",
        );
        let own = Project {
            prefix: Some("x2_".into()),
            network_prefix: Some("x_".into()),
            ..project()
        };

        let icons = icons_of(&own, &["x_users", "x2_posts"], &[net]);

        assert_eq!(
            icons,
            map([("x_users", TableIcon::User), ("x2_posts", TableIcon::Files)])
        );
    }

    #[test]
    fn tables_outside_the_prefixes_get_the_default_icon() {
        let preset = icon_preset("p", "a", &[("*", "box")]);
        let own = Project {
            icons: vec![rule("*", TableIcon::Star)],
            ..project()
        };

        let icons = icons_of(&own, &["x_a", "other"], &[preset]);

        assert_eq!(
            icons,
            map([("x_a", TableIcon::Star), ("other", TableIcon::Table)])
        );
    }

    #[test]
    fn the_built_in_presets_icon_the_wordpress_woocommerce_and_action_scheduler_tables() {
        let loaded: Vec<Arc<Preset>> = BUILT_IN
            .iter()
            .map(|(file, src)| Arc::new(preset::parse(file, src).unwrap_or_else(|e| panic!("{e}"))))
            .collect();
        let own = Project {
            prefix: Some("wp_".into()),
            presets: Some(vec![
                "wordpress".into(),
                "woocommerce".into(),
                "action-scheduler".into(),
            ]),
            ..project()
        };

        let icons = icons_of(
            &own,
            &[
                "wp_users",
                "wp_postmeta",
                "wp_wc_orders_meta",
                "wp_wc_orders",
                "wp_actionscheduler_actions",
                "wp_actionscheduler_logs",
                "wp_wc_customer_lookup",
                "wp_woocommerce_log",
                "wp_unknown",
            ],
            &loaded,
        );

        assert_eq!(
            icons,
            map([
                ("wp_users", TableIcon::User),
                ("wp_postmeta", TableIcon::Menu),
                ("wp_wc_orders_meta", TableIcon::Menu),
                ("wp_wc_orders", TableIcon::ShoppingCart),
                ("wp_actionscheduler_actions", TableIcon::Clock),
                ("wp_actionscheduler_logs", TableIcon::FileClock),
                ("wp_wc_customer_lookup", TableIcon::UserRound),
                ("wp_woocommerce_log", TableIcon::FileClock),
                ("wp_unknown", TableIcon::Table),
            ])
        );
    }

    #[test]
    fn an_empty_table_name_is_not_a_rule() {
        let error = rule("", TableIcon::User).check().unwrap_err();

        assert_eq!(
            (error.field.as_str(), error.message.as_str()),
            ("table", "names no table")
        );
    }

    #[test]
    fn icon_rules_serialize_with_the_kebab_case_name() {
        assert_eq!(
            serde_json::to_value(rule("orders", TableIcon::ShoppingCart)).unwrap(),
            serde_json::json!({ "table": "orders", "icon": "shopping-cart" })
        );
    }
}
