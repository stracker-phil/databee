use super::Session;
use crate::connection::ConnectionSpec;
use crate::db::FakeDatabase;

pub(crate) fn session(id: &str) -> Session {
    Session::new(
        id.into(),
        local_spec(),
        "local".into(),
        Box::new(FakeDatabase::new().with_database("wp")),
    )
}

pub(crate) fn local_spec() -> ConnectionSpec {
    ConnectionSpec {
        name: None,
        host: "127.0.0.1".into(),
        port: 3306,
        socket: None,
        user: "root".into(),
        database: None,
    }
}

pub(crate) fn socket_spec() -> ConnectionSpec {
    ConnectionSpec {
        name: Some("Local".into()),
        host: "localhost".into(),
        port: 3306,
        socket: Some("/tmp/mysql.sock".into()),
        user: "wp".into(),
        database: Some("shop".into()),
    }
}
