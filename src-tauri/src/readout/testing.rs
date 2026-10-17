use std::fs;
use std::sync::Arc;

use tempfile::{TempDir, tempdir};

use crate::db::FakeDatabase;
use crate::query::Ctx;
use crate::query::testing::{BASE, TIMEOUT};
use crate::rules::ProjectsStore;
use crate::rules::catalog::PresetCatalog;
use crate::rules::effective::RuleSources;
use crate::session::Session;
use crate::test_support::spec;

/// A session over a fake `wp` database (`query::testing::shop`) with presets in a tempdir:
/// `doc` is `BASE` plus the given rules, `others` are written as they are.
pub struct Rig {
    _dir: TempDir,
    pub db: Arc<FakeDatabase>,
    pub session: Session,
    pub catalog: PresetCatalog,
    pub projects: ProjectsStore,
}

impl Rig {
    pub fn new(db: FakeDatabase, rules: &str, others: &[(&str, &str)]) -> Self {
        let dir = tempdir().unwrap();
        let presets = dir.path().join("presets");
        fs::create_dir_all(&presets).unwrap();
        fs::write(presets.join("doc.toml"), format!("{BASE}\n{rules}")).unwrap();
        for (file, src) in others {
            fs::write(presets.join(file), src).unwrap();
        }
        let db = Arc::new(db);
        let session = Session::new(
            "s".into(),
            spec("127.0.0.1", 3306, "wp"),
            "local".into(),
            Box::new(db.clone()),
        );
        Self {
            catalog: PresetCatalog::with_built_in(presets, &[]),
            projects: ProjectsStore::open(dir.path()),
            _dir: dir,
            db,
            session,
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
