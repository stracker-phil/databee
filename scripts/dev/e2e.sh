#!/bin/zsh
# Checks that nothing jumps in the running debug app: e2e.sh [database] [table] (default wp_test wp_options).
# Opens the table tab, then clicks the WHERE filter's mode toggle and the table's sidebar row twice each,
# measuring the sidebar, the toolbar, the main area, the first grid row and the clicked control after every
# step. Exits 1 on any drift of 1px or more.
# Start the app on the Docker fixture first:
#   bun run build:app && scripts/dev/launch.sh mysql://root:root@127.0.0.1:33306/wp_test
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$ROOT/src-tauri/target/debug/bundle/macos/DataBee (Debug).app/Contents/MacOS/databee"
LOG="${TMPDIR:-/tmp}/databee-dev.log"
MARK="E2E-$(date +%s)-$$"
DB=${1:-wp_test}
TABLE=${2:-wp_options}
"$BIN" status --json 2>/dev/null | grep -q '"running": *true' || { echo "The debug app isn't running: scripts/dev/launch.sh"; exit 1; }

"$ROOT/scripts/dev/js.sh" "(async () => {
  const report = (text) => window.__TAURI_INTERNALS__.invoke('log_frontend_error', { message: '$MARK ' + text });
  const wait = (ms) => new Promise((done) => setTimeout(done, ms));
  const rect = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return [r.x, r.y, r.width, r.height].map(Math.round).join(',');
  };
  const firstRow = () => document.querySelector('.main [role=row]:not(.grid-head)');
  const parts = (control) => ({
    sidebar: rect(document.querySelector('.sidebar')),
    toolbar: rect(document.querySelector('.toolbar')),
    main: rect(document.querySelector('.main')),
    row: rect(firstRow()),
    control: rect(control),
  });
  const drift = [];
  const compare = (step, before, after) => {
    for (const name of Object.keys(before)) {
      if (name === 'row' && (before.row === null || after.row === null)) continue;
      if (before[name] !== after[name]) drift.push(step + ': ' + name + ' ' + before[name] + ' -> ' + after[name]);
    }
  };
  const clickTwice = async (name, selector) => {
    const control = document.querySelector(selector);
    if (!control) return drift.push(name + ': not found (' + selector + ')');
    const before = parts(control);
    control.click();
    await wait(700);
    compare(name + ' once', before, parts(control));
    control.click();
    await wait(700);
    compare(name + ' twice', before, parts(control));
  };
  try {
    window.databee.getState().openTableTab('$DB', '$TABLE', 'data', undefined, { preview: false });
    await wait(1500);
    await clickTwice('WHERE mode', '[data-command=\"table.filterMode\"]');
    await clickTwice('sidebar table', '.sidebar [data-table=\"$TABLE\"]');
    report(drift.length === 0 ? 'OK' : 'DRIFT ' + drift.join(' | '));
  } catch (error) {
    report('ERROR ' + error);
  }
})()"

for _ in {1..30}; do
  sleep 0.5
  line=$(grep -a -o "$MARK [^\"]*" "$LOG" | head -1) && [[ -n "$line" ]] && break
done
result=${line#"$MARK "}
case "$result" in
  OK) echo "No drift"; exit 0 ;;
  "") echo "No result in $LOG"; exit 1 ;;
  *) echo "${result//' | '/$'\n'}"; exit 1 ;;
esac
