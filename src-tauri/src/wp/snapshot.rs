//! Snapshots and comparisons work on flat paths (`meta/_ppcp_fees/gross_amount/value`),
//! which makes diffs trivial to compute and to render as a list.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::context::WpContext;
use super::detail::args_reference;
use super::entity::{Catalog, EntityDoc, cell_str};
use super::lists::{epoch, text};
use super::{expand, fetch, load_docs};
use crate::db::Database;
use crate::decode::value::ValueNode;
use crate::error::{AppError, AppResult};

pub type Paths = BTreeMap<String, String>;

fn flatten_node(prefix: &str, node: &ValueNode, out: &mut Paths) {
    if node.children.is_empty() {
        out.insert(
            prefix.to_string(),
            node.text
                .clone()
                .unwrap_or_else(|| node.kind.as_str().to_string()),
        );
        return;
    }
    if let Some(class) = &node.class {
        out.insert(format!("{prefix}/@class"), class.clone());
    }
    for child in &node.children {
        flatten_node(&format!("{prefix}/{}", child.key), &child.node, out);
    }
}

fn flatten_value(
    path: &str,
    raw: Option<&str>,
    decoded: Option<&crate::decode::value::DecodedValue>,
    out: &mut Paths,
) {
    match decoded.and_then(|d| d.root.as_ref()) {
        Some(root) => flatten_node(path, root, out),
        None => {
            out.insert(path.to_string(), raw.unwrap_or("NULL").to_string());
        }
    }
}

/// Fields as `field/<name>`, meta as `meta/<key>` (decoded values expand into sub-paths).
pub fn flatten_doc(prefix: &str, doc: &EntityDoc, out: &mut Paths) {
    for f in &doc.fields {
        flatten_value(
            &format!("{prefix}field/{}", f.name),
            cell_str(&f.value),
            f.decoded.as_ref(),
            out,
        );
    }
    // WordPress allows repeated meta keys; number repeats so none of them is hidden.
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for f in doc.meta.iter().flat_map(|g| &g.entries) {
        let n = seen.entry(f.name.as_str()).or_default();
        *n += 1;
        let key = if *n == 1 {
            f.name.clone()
        } else {
            format!("{}[{n}]", f.name)
        };
        flatten_value(
            &format!("{prefix}meta/{key}"),
            cell_str(&f.value),
            f.decoded.as_ref(),
            out,
        );
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub fn diff(before: &Paths, after: &Paths) -> Vec<DiffEntry> {
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .map(|k| DiffEntry {
            path: k.clone(),
            before: before.get(k).cloned(),
            after: after.get(k).cloned(),
        })
        .collect()
}

/// What a snapshot watches.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub entity: Option<String>,
    pub id: Option<String>,
    /// Also capture directly related documents (items, notes, refunds, …).
    #[serde(default)]
    pub include_related: bool,
    /// Options whose name matches this SQL LIKE pattern, e.g. `woocommerce_ppcp%`.
    #[serde(default)]
    pub option_pattern: Option<String>,
    /// Scheduled actions referencing the entity ID.
    #[serde(default)]
    pub include_actions: bool,
}

pub async fn capture(
    session: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    scope: &Scope,
) -> AppResult<Paths> {
    let dialect = session.dialect();
    let mut out = Paths::new();
    if let (Some(entity), Some(id)) = (&scope.entity, &scope.id) {
        let doc = load_docs(session, ctx, catalog, db, entity, std::slice::from_ref(id))
            .await?
            .pop()
            .ok_or_else(|| AppError::InvalidInput(format!("{entity} #{id} was not found")))?;
        flatten_doc(&format!("{entity}#{id}/"), &doc, &mut out);
        if scope.include_related {
            for rel in &doc.relations {
                for related in expand(session, ctx, catalog, db, entity, id, &rel.name).await? {
                    flatten_doc(
                        &format!("{}/{}#{}/", rel.name, related.entity, related.id),
                        &related,
                        &mut out,
                    );
                }
            }
        }
        if scope.include_actions && ctx.has_table("actionscheduler_actions") {
            let rs = fetch(
                session,
                &format!(
                    "SELECT action_id, hook, status, {}, args FROM {}.{} WHERE args LIKE {}",
                    epoch("scheduled_date_gmt"),
                    dialect.quote_ident(db),
                    dialect.quote_ident(&ctx.table("actionscheduler_actions")),
                    dialect.like(&format!("%{id}%"))
                ),
            )
            .await?;
            for r in rs
                .rows
                .iter()
                .filter(|r| text(&r[4]).is_some_and(|a| args_reference(&a, id)))
            {
                let aid = text(&r[0]).unwrap_or_default();
                out.insert(
                    format!("actions/{aid}/hook"),
                    text(&r[1]).unwrap_or_default(),
                );
                out.insert(
                    format!("actions/{aid}/status"),
                    text(&r[2]).unwrap_or_default(),
                );
                out.insert(
                    format!("actions/{aid}/scheduled"),
                    text(&r[3]).unwrap_or_default(),
                );
            }
        }
    }
    if let Some(pattern) = scope
        .option_pattern
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        let rs = fetch(
            session,
            &format!(
                "SELECT option_name, option_value FROM {}.{} WHERE option_name LIKE {} ORDER BY option_name LIMIT 2000",
                dialect.quote_ident(db),
                dialect.quote_ident(&ctx.table("options")),
                dialect.like(pattern.trim())
            ),
        )
        .await?;
        for r in &rs.rows {
            let Some(name) = text(&r[0]) else { continue };
            let raw = text(&r[1]);
            let decoded = raw.as_deref().and_then(crate::decode::value::decode);
            flatten_value(
                &format!("options/{name}"),
                raw.as_deref(),
                decoded.as_ref(),
                &mut out,
            );
        }
    }
    Ok(out)
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompareRow {
    pub path: String,
    pub a: Option<String>,
    pub b: Option<String>,
}

/// Side-by-side comparison of two entities of the same kind (paths relative to each).
pub async fn compare(
    session: &dyn Database,
    ctx: &WpContext,
    catalog: &Catalog,
    db: &str,
    entity: &str,
    a: &str,
    b: &str,
) -> AppResult<Vec<CompareRow>> {
    let docs = load_docs(
        session,
        ctx,
        catalog,
        db,
        entity,
        &[a.to_string(), b.to_string()],
    )
    .await?;
    let find = |id: &str| {
        docs.iter()
            .find(|d| d.id == id)
            .ok_or_else(|| AppError::InvalidInput(format!("{entity} #{id} was not found")))
    };
    let (mut pa, mut pb) = (Paths::new(), Paths::new());
    flatten_doc("", find(a)?, &mut pa);
    flatten_doc("", find(b)?, &mut pb);
    let mut keys: Vec<&String> = pa.keys().chain(pb.keys()).collect();
    keys.sort();
    keys.dedup();
    Ok(keys
        .into_iter()
        .map(|k| CompareRow {
            path: k.clone(),
            a: pa.get(k).cloned(),
            b: pb.get(k).cloned(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::value::decode;

    #[test]
    fn decoded_values_flatten_into_sub_paths() {
        let mut out = Paths::new();
        let raw = r#"a:2:{s:5:"gross";a:1:{s:5:"value";s:5:"95.40";}s:3:"fee";s:4:"3.82";}"#;
        flatten_value(
            "meta/_ppcp_paypal_fees",
            Some(raw),
            decode(raw).as_ref(),
            &mut out,
        );
        assert_eq!(out["meta/_ppcp_paypal_fees/gross/value"], "95.40");
        assert_eq!(out["meta/_ppcp_paypal_fees/fee"], "3.82");
        flatten_value("field/status", Some("wc-processing"), None, &mut out);
        assert_eq!(out["field/status"], "wc-processing");
    }

    #[test]
    fn diff_lists_added_removed_and_changed_paths() {
        let before: Paths = [("a", "1"), ("b", "2"), ("c", "3")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let after: Paths = [("a", "1"), ("b", "20"), ("d", "4")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        let d = diff(&before, &after);
        let shape: Vec<_> = d
            .iter()
            .map(|e| (e.path.as_str(), e.before.as_deref(), e.after.as_deref()))
            .collect();
        assert_eq!(
            shape,
            [
                ("b", Some("2"), Some("20")),
                ("c", Some("3"), None),
                ("d", None, Some("4"))
            ]
        );
        assert!(diff(&before, &before).is_empty());
    }
}
