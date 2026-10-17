//! Builders shared by the unit tests of the WordPress screens: a schema of prefixed tables and
//! the context and catalog the built-in presets make of it.

use std::path::PathBuf;
use std::sync::Arc;

use super::catalog::Catalog;
use super::site::WpContext;
use crate::db::{TableInfo, TableKeys};
use crate::rules::PresetCatalog;
use crate::rules::catalog::BUILT_IN;
use crate::rules::effective::EffectiveRules;
use crate::rules::project::Project;
use crate::schema::DatabaseSchema;

/// `(table without prefix, columns, primary key)` of the tables whose entities tests open.
const KNOWN: &[(&str, &[&str], &str)] = &[
    (
        "posts",
        &[
            "ID",
            "post_author",
            "post_date",
            "post_date_gmt",
            "post_title",
            "post_status",
            "post_type",
            "post_parent",
        ],
        "ID",
    ),
    (
        "postmeta",
        &["meta_id", "post_id", "meta_key", "meta_value"],
        "meta_id",
    ),
    (
        "users",
        &[
            "ID",
            "user_login",
            "user_pass",
            "user_email",
            "user_registered",
            "display_name",
        ],
        "ID",
    ),
    (
        "usermeta",
        &["umeta_id", "user_id", "meta_key", "meta_value"],
        "umeta_id",
    ),
    (
        "options",
        &["option_id", "option_name", "option_value", "autoload"],
        "option_id",
    ),
    (
        "comments",
        &[
            "comment_ID",
            "comment_post_ID",
            "comment_type",
            "comment_author",
            "comment_date_gmt",
            "comment_content",
            "comment_parent",
            "user_id",
        ],
        "comment_ID",
    ),
    (
        "wc_orders",
        &[
            "id",
            "type",
            "status",
            "currency",
            "total_amount",
            "customer_id",
            "billing_email",
            "date_created_gmt",
            "parent_order_id",
            "payment_method",
        ],
        "id",
    ),
    (
        "wc_orders_meta",
        &["id", "order_id", "meta_key", "meta_value"],
        "id",
    ),
    (
        "wc_order_addresses",
        &["id", "order_id", "address_type", "first_name", "last_name"],
        "id",
    ),
    (
        "woocommerce_order_items",
        &[
            "order_item_id",
            "order_item_name",
            "order_item_type",
            "order_id",
        ],
        "order_item_id",
    ),
    (
        "woocommerce_order_itemmeta",
        &["meta_id", "order_item_id", "meta_key", "meta_value"],
        "meta_id",
    ),
];

/// Tables of a WooCommerce site with HPOS storage: orders live in `wc_orders`.
pub const HPOS: &[&str] = &[
    "options",
    "posts",
    "postmeta",
    "users",
    "usermeta",
    "comments",
    "wc_orders",
    "wc_orders_meta",
    "wc_order_addresses",
    "woocommerce_order_items",
    "woocommerce_order_itemmeta",
];

/// Tables of a WooCommerce site with legacy storage: orders are `shop_order*` posts.
pub const LEGACY: &[&str] = &[
    "options",
    "posts",
    "postmeta",
    "users",
    "usermeta",
    "comments",
    "woocommerce_order_items",
    "woocommerce_order_itemmeta",
];

/// A schema of `names` under `prefix`; the tables in `KNOWN` have their columns and key, others
/// have neither.
pub fn tables(prefix: &str, names: &[&str]) -> DatabaseSchema {
    let mut schema = DatabaseSchema {
        tables: Vec::new(),
        keys: Default::default(),
        columns: Default::default(),
        incoming: Default::default(),
    };
    for name in names {
        let table = format!("{prefix}{name}");
        let known = KNOWN.iter().find(|(known, _, _)| known == name);
        let columns: Vec<String> = known
            .map(|(_, columns, _)| columns.iter().map(|c| c.to_string()).collect())
            .unwrap_or_default();
        let primary_key: Vec<String> = known
            .map(|(_, _, key)| key.to_string())
            .into_iter()
            .collect();
        schema.tables.push(TableInfo {
            name: table.clone(),
            is_view: false,
            rows_estimate: None,
            size_bytes: None,
            comment: String::new(),
        });
        schema.keys.insert(
            table.clone(),
            TableKeys {
                primary_key,
                ..TableKeys::default()
            },
        );
        schema.columns.insert(table, columns);
    }
    schema
}

/// The effective rules of the built-in presets and `project` over `schema`.
pub fn rules(project: &Project, schema: &DatabaseSchema) -> Arc<EffectiveRules> {
    let presets = PresetCatalog::with_built_in(PathBuf::new(), BUILT_IN).presets();
    Arc::new(EffectiveRules::build(project, schema, &presets))
}

/// A project that prefixes tables with `prefix` and applies exactly the presets `names`.
pub fn project(prefix: &str, names: &[&str]) -> Project {
    Project {
        prefix: Some(prefix.into()),
        presets: Some(names.iter().map(|n| n.to_string()).collect()),
        ..Project::new("local", "wp")
    }
}

/// The context and catalog of a site with tables `names` under `prefix`, applying `presets`.
pub fn site_with(prefix: &str, names: &[&str], presets: &[&str]) -> (WpContext, Catalog) {
    let schema = tables(prefix, names);
    let rules = rules(&project(prefix, presets), &schema);
    let catalog = Catalog::new(&rules, &schema);
    let ctx = WpContext::new(rules).expect("the wordpress preset is among the presets");
    (ctx, catalog)
}

/// A `wp_` site with the `wordpress` and `woocommerce` presets and tables `names`.
pub fn site(names: &[&str]) -> (WpContext, Catalog) {
    site_with("wp_", names, &["wordpress", "woocommerce"])
}
