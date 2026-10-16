# WordPress layer

Audience: senior WordPress developers inspecting their own local databases (DDEV/Docker,
dummy data). Not site admins, not shop owners. It complements wp-admin and the code; it does
not replace them.

- Show what is stored: real column names, meta keys, option names, status slugs and gateway IDs.
  Human labels or formatting may appear next to the raw value, never instead of it.
- Only show facts the database proves. No attribution or classification by naming heuristics
  (e.g. "which plugin owns this key"): without reading plugin code that is a guess.
- Never hide stored data because it looks internal; collapse it at most.
- Every derived value names its source key, and every entity or field can be opened as its
  rows in the Database view.
- No privacy features (blurring, masking PII): the data is dummy data. Secrets (keys, tokens)
  stay masked until clicked because they may be real sandbox credentials.
