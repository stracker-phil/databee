# 0020. Everything is rows: one workspace, Document and Rows views of one object

## Status

Accepted (2026-10-17), implemented. Amended 2026-10-20.

The views switch with Mod+1 (`object.viewDocument`) and Mod+2 (`object.viewRows`) in
`src/commands/index.ts`, not with `Tab`.

## Context

DataBee had grown into two apps: a WordPress mode with dashboard cards, lists, entity screens and its own
navigation, and a Database mode with tabs and grids. Each half had its own look, its own value rendering and
its own routes, and every WordPress view was code. The same order looked different on an entity screen and in
`wp_wc_orders`, and a value shown in one half could not be traced or opened the same way in the other.

## Decision

There is one workspace and no mode toggle. Every row with a primary key is an object with two views of the
same data, switched with `Tab`:

- **Document**: the object assembled from its own row, its meta rows, incoming references and pattern groups,
  each group headed by its real table name.
- **Rows**: the raw rows behind it, as grids.

Every value renders through one component, decided by its kind, and names where it is stored. Navigation is
drilling through references (a column stack), the palette and addresses ([0021](0021-addresses.md)). Lists and
the dashboard become pinned queries; live mode and snapshots become inline diffs on the document. A separate
WordPress mode with its own screens was rejected because it duplicates rendering and hides the rows behind it.

## Consequences

- Any database gets documents, with or without presets; WordPress adds configuration, not screens.
- The WordPress screens, routes and `src/components/wp/` are deleted once the object view covers them (U12).
- Everything WordPress shows has to be expressible as rows plus rules ([0022](0022-presets-and-projects.md)).
- The design language is shared and documented once in [`docs/ux.md`](../ux.md).
