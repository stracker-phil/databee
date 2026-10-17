const EDITABLE = 'input, textarea, [contenteditable=""], [contenteditable="true"], .cm-editor';

function isEditable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE) !== null;
}

/** Suppresses the WebView's own menu (Reload, Inspect Element) outside text fields; app menus are unaffected. */
export function installNativeMenuGuard(root: Document = document): () => void {
  const onMenu = (e: MouseEvent) => {
    if (e.defaultPrevented || isEditable(e.target)) return;
    e.preventDefault();
  };
  root.addEventListener("contextmenu", onMenu);
  return () => root.removeEventListener("contextmenu", onMenu);
}
