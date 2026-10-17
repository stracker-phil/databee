//! Action parameters: their choices, and the values a plan gets, checked against the
//! declaration (type, choices unless `free`, required unless `optional` or defaulted).

use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

use super::super::context::Ctx;
use crate::error::{AppError, AppResult};
use crate::rules::actions::{Param, ParamType};
use crate::rules::placeholder::with_prefixes;

/// A parameter as the page asks for it.
#[derive(Serialize, Debug, Clone, PartialEq, TS)]
#[ts(export, export_to = "src/api/types.gen.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActionParam {
    pub name: String,
    pub r#type: ParamType,
    /// The first column of `choices_sql`, as stored.
    pub choices: Vec<String>,
    /// A value outside `choices` is accepted.
    pub free: bool,
    pub default: Option<String>,
    pub optional: bool,
    /// `choices_sql` failed.
    pub error: Option<String>,
}

/// A checked value, rendered as an SQL literal or as plain text.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Null,
    Text(String),
    Int(i64),
}

impl ParamValue {
    pub fn literal(&self, ctx: &Ctx<'_>) -> String {
        match self {
            ParamValue::Null => "NULL".into(),
            ParamValue::Text(text) => ctx.literal(text),
            ParamValue::Int(n) => n.to_string(),
        }
    }

    pub fn text(&self) -> String {
        match self {
            ParamValue::Null => String::new(),
            ParamValue::Text(text) => text.clone(),
            ParamValue::Int(n) => n.to_string(),
        }
    }
}

pub async fn describe(ctx: &Ctx<'_>, params: &[Param]) -> Vec<ActionParam> {
    let mut described = Vec::with_capacity(params.len());
    for param in params {
        let (choices, error) = match fetch_choices(ctx, param).await {
            Ok(choices) => (choices, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        described.push(ActionParam {
            name: param.name.clone(),
            r#type: param.r#type,
            choices,
            free: is_free(param),
            default: param.default.clone(),
            optional: param.optional,
            error,
        });
    }
    described
}

fn is_free(param: &Param) -> bool {
    param.free || param.choices_sql.is_none()
}

async fn fetch_choices(ctx: &Ctx<'_>, param: &Param) -> AppResult<Vec<String>> {
    let Some(sql) = &param.choices_sql else {
        return Ok(Vec::new());
    };
    let applied = &ctx.rules.applied;
    let result = ctx
        .fetch(&with_prefixes(
            &ctx.qualified(sql),
            &applied.prefix,
            &applied.network_prefix,
        ))
        .await?;
    Ok(result
        .rows
        .iter()
        .filter_map(|row| row.first()?.as_text().map(str::to_string))
        .collect())
}

/// The value of every declared parameter; an empty given value counts as none.
pub async fn values(
    ctx: &Ctx<'_>,
    action: &str,
    params: &[Param],
    given: &BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, ParamValue>> {
    if let Some(unknown) = given.keys().find(|k| !params.iter().any(|p| &p.name == *k)) {
        return Err(AppError::InvalidInput(format!(
            "`{action}` has no param `{unknown}`"
        )));
    }
    let mut values = BTreeMap::new();
    for param in params {
        let name = &param.name;
        let raw = given
            .get(name)
            .filter(|v| !v.is_empty())
            .or(param.default.as_ref());
        let value = match raw {
            None if param.optional => ParamValue::Null,
            None => {
                return Err(AppError::InvalidInput(format!("`{name}` needs a value")));
            }
            Some(raw) => {
                let choices = if is_free(param) {
                    None
                } else {
                    Some(fetch_choices(ctx, param).await?)
                };
                checked(param, raw, choices.as_deref())?
            }
        };
        values.insert(name.clone(), value);
    }
    Ok(values)
}

/// `choices` is `None` for a free param.
fn checked(param: &Param, raw: &str, choices: Option<&[String]>) -> AppResult<ParamValue> {
    let name = &param.name;
    let value = match param.r#type {
        ParamType::Int => raw.trim().parse().map(ParamValue::Int).map_err(|_| {
            AppError::InvalidInput(format!("`{name}` takes an integer, not `{raw}`"))
        })?,
        ParamType::Text => ParamValue::Text(raw.to_string()),
    };
    let text = value.text();
    if choices.is_some_and(|choices| !choices.contains(&text)) {
        return Err(AppError::InvalidInput(format!(
            "`{text}` is not one of the choices of `{name}`"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::context::testing::{Rig, result, shop, text};

    fn param(name: &str) -> Param {
        Param {
            name: name.into(),
            choices_sql: None,
            free: false,
            r#type: ParamType::Text,
            default: None,
            optional: false,
        }
    }

    fn with_choices(name: &str) -> Param {
        Param {
            choices_sql: Some("SELECT DISTINCT status FROM {p}orders".into()),
            ..param(name)
        }
    }

    fn given(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn rig() -> Rig {
        let choices = result(&["status"], vec![vec![text("open")], vec![text("done")]]);
        Rig::new(shop().on("DISTINCT status", choices), "")
    }

    async fn values_of(
        params: &[Param],
        given: &BTreeMap<String, String>,
    ) -> AppResult<BTreeMap<String, ParamValue>> {
        let rig = rig();
        let ctx = rig.ctx().await;
        values(&ctx, "act", params, given).await
    }

    fn invalid_input(err: AppError) -> String {
        match err {
            AppError::InvalidInput(message) => message,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn an_unknown_param_is_refused() {
        let err = values_of(&[param("a")], &given(&[("a", "1"), ("nope", "2")]))
            .await
            .unwrap_err();

        assert_eq!(invalid_input(err), "`act` has no param `nope`");
    }

    #[tokio::test]
    async fn a_missing_or_empty_required_param_is_refused() {
        for values in [given(&[]), given(&[("a", "")])] {
            let err = values_of(&[param("a")], &values).await.unwrap_err();

            assert_eq!(invalid_input(err), "`a` needs a value", "{values:?}");
        }
    }

    #[tokio::test]
    async fn an_int_param_must_parse_as_an_integer() {
        let int = Param {
            r#type: ParamType::Int,
            ..param("days")
        };

        let err = values_of(std::slice::from_ref(&int), &given(&[("days", "abc")]))
            .await
            .unwrap_err();
        assert_eq!(invalid_input(err), "`days` takes an integer, not `abc`");

        let ok = values_of(&[int], &given(&[("days", " 7 ")])).await.unwrap();
        assert_eq!(ok["days"], ParamValue::Int(7));
    }

    #[tokio::test]
    async fn a_value_outside_the_choices_is_refused_unless_the_param_is_free() {
        let err = values_of(&[with_choices("s")], &given(&[("s", "gone")]))
            .await
            .unwrap_err();
        assert_eq!(
            invalid_input(err),
            "`gone` is not one of the choices of `s`"
        );

        let listed = values_of(&[with_choices("s")], &given(&[("s", "done")]))
            .await
            .unwrap();
        assert_eq!(listed["s"], ParamValue::Text("done".into()));

        let free = Param {
            free: true,
            ..with_choices("s")
        };
        let other = values_of(&[free], &given(&[("s", "gone")])).await.unwrap();
        assert_eq!(other["s"], ParamValue::Text("gone".into()));
    }

    #[tokio::test]
    async fn an_optional_param_without_a_value_is_null() {
        let optional = Param {
            optional: true,
            ..param("a")
        };

        let values = values_of(&[optional], &given(&[])).await.unwrap();

        assert_eq!(values["a"], ParamValue::Null);
    }

    #[tokio::test]
    async fn the_default_is_used_for_a_missing_or_empty_value_and_a_given_value_wins() {
        let defaulted = Param {
            r#type: ParamType::Int,
            default: Some("30".into()),
            ..param("days")
        };

        for (values, expected) in [
            (given(&[]), 30),
            (given(&[("days", "")]), 30),
            (given(&[("days", "5")]), 5),
        ] {
            let found = values_of(std::slice::from_ref(&defaulted), &values)
                .await
                .unwrap();

            assert_eq!(found["days"], ParamValue::Int(expected), "{values:?}");
        }
    }

    #[tokio::test]
    async fn literals_quote_text_and_leave_null_and_numbers_bare() {
        let rig = rig();
        let ctx = rig.ctx().await;

        assert_eq!(ParamValue::Text("it's".into()).literal(&ctx), "'it''s'");
        assert_eq!(ParamValue::Null.literal(&ctx), "NULL");
        assert_eq!(ParamValue::Int(30).literal(&ctx), "30");
    }
}
