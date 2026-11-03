# Demo data only

The repo is published as source. Nothing in it may carry the owner's real data.

- Code, comments, docs, ADRs, tests, fixtures, seeds, commit messages and screenshots use made-up values
  only: invented sites, orders, people, companies and dates. Use `example.com` addresses and hosts. Never
  copy a real connection, site, customer, ticket key, path under the owner's home or token into the repo,
  not even as an example.
- The debug build never sees the owner's data: it runs on its own config folder with passwords in memory
  ([ADR 0028](../../docs/adr/0028-debug-build-is-a-separate-app-with-demo-data.md)) and starts with
  connections to the Docker fixtures (`dev/`, `test_support::demo_connections`).
- The owner's own databases (DDEV sites and others) are read-only test material; writes in tests and manual
  checks go to the Docker fixtures (`wp_write`, `databee_test`).
- Screenshots that are committed come from the debug app on the fixtures, produced by
  `scripts/dev/screenshot.sh`, never from the release app.
- When the owner shares real examples to explain a feature, translate them into neutral demo values before
  they reach a file.
