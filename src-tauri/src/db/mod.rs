pub mod browse;
pub mod cell;
pub mod classify;
pub mod dialect;
pub mod id_filter;
pub mod lexer;
pub mod mysql;
pub mod safety;
mod types;

pub use cell::Cell;
pub use dialect::Dialect;
pub use types::*;

/// Quotes a MySQL identifier with backticks.
pub fn quote_ident(name: &str) -> String {
    Dialect::MYSQL.quote_ident(name)
}
