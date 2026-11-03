/** Keeps `window-blurred` on <html> while the window isn't key, so the page dims like an inactive macOS window. */
export function trackWindowFocus(): () => void {
  const root = document.documentElement;
  const update = () => root.classList.toggle("window-blurred", !document.hasFocus());
  update();
  window.addEventListener("focus", update);
  window.addEventListener("blur", update);
  return () => {
    window.removeEventListener("focus", update);
    window.removeEventListener("blur", update);
    root.classList.remove("window-blurred");
  };
}
