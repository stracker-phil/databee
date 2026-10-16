# 0023. References are virtual foreign keys from four sources

## Status

Accepted (2026-10-17). Being implemented in U4b (schema and inference), U4c (rules, coverage) and U8
(Project page).

## Context

WordPress and most plugin tables declare no foreign keys, yet the relations are what a developer follows:
order to customer, item to order, meta value to user. Writing constraints into the database is not an
option: it writes to the user's database, breaks WordPress and plugin upgrade routines, vanishes on a DDEV
snapshot restore, and fails on the orphaned rows WordPress data always has. Meta tables hold references only
for some keys, and some relations point at external ids rather than primary keys.

## Decision

References live in DataBee, never in the database. A reference has a source column, optional conditions
on the source row (`meta_key = '_customer_user'` or a glob), a target column (usually a primary key, any
column for external ids), optional conditions on the target row, and an optional label. Four sources, by
precedence:

1. schema foreign-key constraints (declared; cannot be overridden or disabled);
2. project references (declared);
3. preset references (declared);
4. same-name inference: `A.c` points to `B` when `B` has a single-column primary key named `c`, `A` is not
   `B`, `c` is not `A`'s primary key, `c` is not `id`, and no declared reference covers `A.c` (inferred).

A project reference with the same source and conditions replaces a preset one; a project can disable a
preset or inferred reference, or promote an inferred one. Inferred references render dashed, say
"inferred" and show their rule. Coverage proves a reference: on demand, a timed `LEFT JOIN` count reports how
many source values resolve, counting `0` and `NULL` apart as "no reference". Value-level inference (any meta
value that equals some id) is deferred to version 2.

## Consequences

- Documents, drilling, peek and the palette work on databases without any declared key.
- Inference can be wrong; it is always marked and one click from being disabled.
- Coverage queries can be slow on large tables, so they run only from the Project page, never per document.
- Conditional references need a per-cell kind decision (`meta_value` depends on `meta_key`).
