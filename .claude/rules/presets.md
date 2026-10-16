# Presets and projects

Audience: senior WordPress developers inspecting their own local databases (DDEV/Docker,
dummy data). Not site admins, not shop owners. DataBee complements wp-admin and the code; it
does not replace them.

These rules govern every preset and project rule (references, kinds, value styles, entities,
pinned queries, anomalies, actions), the built-in WordPress and WooCommerce presets included,
and every view that renders their results.

- Show what is stored: real column names, meta keys, option names, status slugs and gateway IDs.
  Human labels or formatting may appear next to the raw value, never instead of it.
- No translated labels for data. Data labels are column names, meta keys and table names.
  Chrome (buttons, settings) may use words.
- Only show facts the database proves. No attribution or classification by naming heuristics
  (e.g. "which plugin owns this key"): without reading plugin code that is a guess.
- Inferred is marked. A reference may be inferred from a column name that equals another table's
  primary key. It is drawn dashed, says "inferred", shows its rule, never looks like a declared
  reference, and can be disabled or promoted to a project reference. Declared references show
  their coverage.
- Never hide stored data because it looks internal; collapse it at most.
- Nothing derived without a source. Every derived value names its source key; relative times,
  currencies and URLs are secondary text with their own provenance. Every object, group or field
  can be opened as its rows in Rows view.
- Color is configuration. No value is colored by default; value styles are project rules scoped
  to columns or meta keys, never global.
- References are virtual. DataBee never creates constraints or other schema objects in a
  database; references live in presets and projects.
- Presets are configuration and the acceptance test. What a preset cannot express becomes a
  generic engine feature, never code for one application. The WordPress and WooCommerce presets
  prove the engine.
- No privacy features (blurring, masking PII): the data is dummy data. Secrets (keys, tokens)
  stay masked until clicked because they may be real sandbox credentials.
