//! Fakes for tests that read through a `Ctx`: the `shop` database and a `Rig` with one preset.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use tempfile::{TempDir, tempdir};

use super::Ctx;
use crate::db::{Cell, ColumnInfo, ColumnMeta, FakeDatabase, ResultSet, TableStructure};
use crate::rules::ProjectsStore;
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::test_support::spec;

pub const TIMEOUT: Duration = Duration::from_secs(5);

/// The preset every document test starts from: orders owning `ordermeta`, under the prefix `f_`.
pub const BASE: &str = r#"
[preset]
name = "doc"
marker_tables = ["orders"]

[[meta]]
table = "ordermeta"
owner = "orders"
fk = "order_id"
key = "k"
value = "v"
"#;

pub fn text(value: &str) -> Cell {
    Cell::Text(value.into())
}

pub fn num(value: &str) -> Cell {
    Cell::Number(value.into())
}

pub fn result(columns: &[&str], rows: Vec<Vec<Cell>>) -> ResultSet {
    ResultSet {
        columns: columns
            .iter()
            .map(|name| ColumnMeta {
                name: name.to_string(),
                type_name: "TEXT".into(),
                origin: None,
            })
            .collect(),
        rows,
        returns_rows: true,
        ..ResultSet::default()
    }
}

pub fn count(total: u64) -> ResultSet {
    result(&["COUNT(*)"], vec![vec![num(&total.to_string())]])
}

fn structure(columns: &[&str], primary_key: &str) -> TableStructure {
    let column = |name: &&str| ColumnInfo {
        name: name.to_string(),
        column_type: "text".into(),
        nullable: true,
        default: None,
        key: String::new(),
        extra: String::new(),
        comment: String::new(),
        auto_increment: false,
        generated: false,
    };
    TableStructure {
        columns: columns.iter().map(column).collect(),
        indexes: Vec::new(),
        primary_key: vec![primary_key.into()],
        foreign_keys: Vec::new(),
        ddl: None,
    }
}

/// The database `wp` with `f_orders`, `f_ordermeta`, `f_log` and `f_people`.
pub fn shop() -> FakeDatabase {
    FakeDatabase::new()
        .with_database("wp")
        .with_table(
            "wp",
            "f_orders",
            structure(&["id", "status", "total", "currency", "created_gmt"], "id"),
        )
        .with_table(
            "wp",
            "f_ordermeta",
            structure(&["meta_id", "order_id", "k", "v"], "meta_id"),
        )
        .with_table(
            "wp",
            "f_log",
            structure(&["log_id", "order_id", "at_gmt", "msg"], "log_id"),
        )
        .with_table(
            "wp",
            "f_people",
            structure(&["person_id", "name"], "person_id"),
        )
}

/// A session over a fake database, with `BASE` plus `rules` as the one preset.
pub struct Rig {
    _dir: TempDir,
    pub db: Arc<FakeDatabase>,
    pub session: Session,
    catalog: PresetCatalog,
    projects: ProjectsStore,
}

impl Rig {
    pub fn new(db: FakeDatabase, rules: &str) -> Self {
        let dir = tempdir().unwrap();
        let presets = dir.path().join("presets");
        fs::create_dir_all(&presets).unwrap();
        fs::write(presets.join("doc.toml"), format!("{BASE}\n{rules}")).unwrap();
        let db = Arc::new(db);
        let session = Session::new(
            "s".into(),
            spec("127.0.0.1", 3306, "wp"),
            "local".into(),
            Box::new(db.clone()),
        );
        let catalog = PresetCatalog::with_built_in(presets, &[]);
        let projects = ProjectsStore::open(dir.path());
        Self {
            _dir: dir,
            db,
            session,
            catalog,
            projects,
        }
    }

    pub fn sources(&self) -> RuleSources<'_> {
        RuleSources {
            presets: &self.catalog,
            projects: &self.projects,
        }
    }

    pub async fn ctx(&self) -> Ctx<'_> {
        Ctx::load(&self.session, "wp", self.sources(), TIMEOUT)
            .await
            .unwrap()
    }
}
