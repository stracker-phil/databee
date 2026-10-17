//! Reads of one database through its schema, effective rules and lens: `Ctx` and the bounded
//! queries on it, and `Rows`, annotated results with provenance. Shared by documents, the
//! palette's search, the readout and value counts.

mod ctx;
mod rows;
#[cfg(test)]
pub(crate) mod testing;

pub use ctx::{Capped, Ctx, KeyOrder};
pub use rows::{CurrencyLookup, Rows};
