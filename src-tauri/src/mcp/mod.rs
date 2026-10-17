//! `databee mcp`: a read-only MCP server on stdio (ADR 0025), built on `rmcp`. It never starts
//! Tauri or opens a window; stdout carries only JSON-RPC.

mod data;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::schemars::JsonSchema;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::ipc::IpcMessage;
use crate::secrets::{Keychain, SecretStore};
use data::Core;

const INSTRUCTIONS: &str = "Read-only access to the databases saved in DataBee, a SQL client for \
local WordPress and MySQL/MariaDB development. Addresses look like \
databee://<connection>/<database>/<table>/<key> (a row) or databee://<connection>/<database>/<table> \
(a table); `connections` lists the connection slugs. Values carry their provenance (table, column, \
key). Nothing here writes: `query` refuses anything but SELECT, SHOW, DESCRIBE, EXPLAIN and \
WITH ... SELECT. `current`, `open` and `command` need the DataBee app running.";

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AddressArgs {
    /// A databee:// address
    pub address: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RowsArgs {
    /// A databee:// table or row address
    pub address: String,
    /// At most this many rows (default 200, at most 1000)
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct QueryArgs {
    /// A connection slug from `connections`
    pub connection: String,
    /// Read-only SQL: SELECT, SHOW, DESCRIBE, EXPLAIN or WITH ... SELECT
    pub sql: String,
    /// At most this many rows per result (default 200, at most 1000)
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ReferencesArgs {
    /// A connection slug from `connections`
    pub connection: String,
    pub database: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FindArgs {
    /// A connection slug from `connections`
    pub connection: String,
    pub database: String,
    /// `#4711` (rows with that id and the rows that reference them), `@jane` (people), `=value`
    /// (every meta value and searchable column) or free text of 6+ characters (search columns
    /// and external ids)
    pub query: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct OpenArgs {
    /// A databee:// address, or an entity shorthand such as `order 4711` (in the active connection)
    pub address: String,
    /// Bring the DataBee window to the front (default false)
    #[serde(default)]
    pub focus: bool,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CommandArgs {
    /// A command id, as in DataBee's keybindings.json (e.g. `view.sidebar`)
    pub id: String,
}

#[derive(Clone)]
pub struct DataBeeServer {
    core: Arc<Core>,
    socket: PathBuf,
}

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

fn answer<T: Serialize>(result: crate::error::AppResult<T>) -> Result<String, String> {
    result
        .map_err(|e| e.to_string())
        .and_then(|value| json(&value))
}

#[tool_handler]
impl ServerHandler for DataBeeServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("databee", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[tool_router]
impl DataBeeServer {
    pub fn new(core: Arc<Core>, socket: PathBuf) -> Self {
        Self { core, socket }
    }

    #[tool(
        description = "The saved connections: slug (the first part of an address), name, host, port and default database"
    )]
    async fn connections(&self) -> Result<String, String> {
        json(&self.core.connections())
    }

    #[tool(
        description = "The document of a row address: its own columns, meta rows, rows that reference it, patterns and events, each value with its provenance"
    )]
    async fn document(&self, Parameters(args): Parameters<AddressArgs>) -> Result<String, String> {
        answer(self.core.document(&args.address).await)
    }

    #[tool(description = "The rows of a table address (with its column filter) or a row address")]
    async fn rows(&self, Parameters(args): Parameters<RowsArgs>) -> Result<String, String> {
        answer(self.core.rows(&args.address, args.limit).await)
    }

    #[tool(
        description = "Runs read-only SQL on a connection; anything that could write is refused before it reaches the server"
    )]
    async fn query(&self, Parameters(args): Parameters<QueryArgs>) -> Result<String, String> {
        answer(
            self.core
                .query(&args.connection, &args.sql, args.limit)
                .await,
        )
    }

    #[tool(
        description = "Every reference between the tables of a database (foreign keys, preset and project rules, same-name inference), each with its source"
    )]
    async fn references(
        &self,
        Parameters(args): Parameters<ReferencesArgs>,
    ) -> Result<String, String> {
        answer(self.core.references(&args.connection, &args.database).await)
    }

    #[tool(
        description = "Finds rows as DataBee's palette does: by id across entities, people, external ids or a value anywhere; each hit has its address, key and summary values"
    )]
    async fn find(&self, Parameters(args): Parameters<FindArgs>) -> Result<String, String> {
        answer(
            self.core
                .find(&args.connection, &args.database, &args.query)
                .await,
        )
    }

    #[tool(description = "The canonical databee:// link of an address, to open in DataBee")]
    async fn link(&self, Parameters(args): Parameters<AddressArgs>) -> Result<String, String> {
        Address::parse(&args.address)
            .map(|address| address.to_string())
            .map_err(|e| e.to_string())
    }

    #[tool(description = "The address of the active tab in the running DataBee app, or null")]
    async fn current(&self) -> Result<String, String> {
        json(&ui::ask(&self.socket, IpcMessage::Current).await?)
    }

    #[tool(description = "Opens an address in the running DataBee app")]
    async fn open(&self, Parameters(args): Parameters<OpenArgs>) -> Result<String, String> {
        let message = IpcMessage::Open {
            address: args.address,
            connection: None,
            focus: args.focus,
        };
        json(&ui::ask(&self.socket, message).await?)
    }

    #[tool(description = "Runs a command of the running DataBee app by its id")]
    async fn command(&self, Parameters(args): Parameters<CommandArgs>) -> Result<String, String> {
        json(&ui::ask(&self.socket, IpcMessage::Command { id: args.id }).await?)
    }
}

/// Serves MCP on stdin/stdout until the client disconnects; the process exit code.
pub fn run(config_dir: &Path, socket: PathBuf) -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Cannot start the MCP server: {e}");
            return 2;
        }
    };
    let secrets: Arc<dyn SecretStore> = Arc::new(Keychain);
    let core = Arc::new(Core::new(config_dir.to_path_buf(), secrets));
    let server = DataBeeServer::new(core, socket);
    runtime.block_on(async move {
        let service = match server.serve(rmcp::transport::stdio()).await {
            Ok(service) => service,
            Err(e) => {
                eprintln!("MCP handshake failed: {e}");
                return 1;
            }
        };
        match service.waiting().await {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("MCP server stopped: {e}");
                1
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tempfile::{TempDir, tempdir};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const TOOLS: [&str; 10] = [
        "command",
        "connections",
        "current",
        "document",
        "find",
        "link",
        "open",
        "query",
        "references",
        "rows",
    ];

    fn server(dir: &TempDir) -> DataBeeServer {
        let core = Core::new(dir.path().to_path_buf(), Arc::new(MemoryStore::default()));
        DataBeeServer::new(Arc::new(core), dir.path().join("none.sock"))
    }

    #[test]
    fn the_server_offers_exactly_the_ten_tools() {
        let mut names: Vec<String> = DataBeeServer::tool_router()
            .list_all()
            .iter()
            .map(|tool| tool.name.to_string())
            .collect();
        names.sort();

        assert_eq!(names, TOOLS);
    }

    struct Client {
        reader: BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    }

    impl Client {
        async fn send(&mut self, message: Value) {
            let line = format!("{message}\n");
            self.writer.write_all(line.as_bytes()).await.unwrap();
        }

        async fn reply(&mut self) -> Value {
            let mut line = String::new();
            tokio::time::timeout(Duration::from_secs(5), self.reader.read_line(&mut line))
                .await
                .expect("the server answers")
                .unwrap();
            serde_json::from_str(&line).unwrap()
        }

        async fn call(&mut self, id: u32, method: &str, params: Value) -> Value {
            self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
                .await;
            self.reply().await
        }
    }

    async fn connected(dir: &TempDir) -> Client {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (server_read, server_write) = tokio::io::split(server_io);
        let server = server(dir);
        tokio::spawn(async move {
            let service = server.serve((server_read, server_write)).await.unwrap();
            let _ = service.waiting().await;
        });
        let (read, writer) = tokio::io::split(client_io);
        let mut client = Client {
            reader: BufReader::new(read),
            writer,
        };
        let init = client
            .call(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "0"}
                }),
            )
            .await;
        assert_eq!(init["result"]["serverInfo"]["name"], "databee");
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        client
    }

    #[tokio::test]
    async fn a_client_lists_the_tools_over_json_rpc() {
        let dir = tempdir().unwrap();
        let mut client = connected(&dir).await;

        let listed = client.call(2, "tools/list", json!({})).await;

        let mut names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, TOOLS);
    }

    #[tokio::test]
    async fn a_client_calls_the_link_tool_over_json_rpc() {
        let dir = tempdir().unwrap();
        let mut client = connected(&dir).await;
        let address = "databee://shop/db/wp_posts/42";
        let canonical = Address::parse(address).unwrap().to_string();

        let called = client
            .call(
                2,
                "tools/call",
                json!({"name": "link", "arguments": {"address": address}}),
            )
            .await;

        assert_eq!(called["result"]["content"][0]["text"], canonical);
    }

    #[tokio::test]
    async fn the_link_tool_rejects_text_that_is_not_an_address() {
        let dir = tempdir().unwrap();

        let result = server(&dir)
            .link(Parameters(AddressArgs {
                address: "https://example.com".into(),
            }))
            .await;

        assert!(result.is_err(), "{result:?}");
    }

    #[tokio::test]
    async fn ui_tools_say_that_databee_is_not_running_without_the_app() {
        let dir = tempdir().unwrap();
        let server = server(&dir);

        assert_eq!(server.current().await, Err(ui::NOT_RUNNING.to_string()));
        assert_eq!(
            server
                .command(Parameters(CommandArgs {
                    id: "view.sidebar".into()
                }))
                .await,
            Err(ui::NOT_RUNNING.to_string())
        );
    }
}
