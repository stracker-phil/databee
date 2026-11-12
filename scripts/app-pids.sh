# Sourced by scripts that start or stop DataBee. `app_pids path/to/databee` lists the app process ids of the
# release or the debug build, whichever the path names, wherever it runs from. The app may carry launch
# arguments (a connection URL, `--hidden`); the short-lived CLI calls (`mcp`, `status`, `quit`, `command`,
# `commands`) are not the app. Only processes named `databee` count, so a shell whose command line merely
# mentions the path does not.
app_pids() {
  local bundle='DataBee'
  [[ "$1" == *"DataBee (Debug).app/"* ]] && bundle='DataBee \(Debug\)'
  local pid cmd found=1
  for pid in $(pgrep -x databee); do
    cmd=$(ps -o command= -p "$pid") || continue
    [[ "$cmd" =~ "^/.*/${bundle}\.app/Contents/MacOS/databee( |\$)" ]] || continue
    [[ "$cmd" =~ "/databee (mcp|status|quit|commands?)( |\$)" ]] && continue
    echo "$pid"
    found=0
  done
  return $found
}
