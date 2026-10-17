import { afterEach, describe, expect, it } from "vitest";
import { installNativeMenuGuard } from "./nativeMenu";

let uninstall: (() => void) | undefined;

function fire(target: Element): MouseEvent {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event;
}

function mount(html: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  document.body.append(host);
  return host;
}

describe("installNativeMenuGuard", () => {
  afterEach(() => {
    uninstall?.();
    uninstall = undefined;
    document.body.innerHTML = "";
  });

  it("prevents the native menu on a plain element", () => {
    uninstall = installNativeMenuGuard();
    const host = mount("<div id='plain'></div>");

    expect(fire(host.querySelector("#plain")!).defaultPrevented).toBe(true);
  });

  it.each([
    ["an input", "<input id='t' />"],
    ["a textarea", "<textarea id='t'></textarea>"],
    ["a contenteditable child", "<div contenteditable='true'><span id='t'>x</span></div>"],
    ["an element inside the code editor", "<div class='cm-editor'><div><span id='t'>x</span></div></div>"],
  ])("keeps the native menu on %s", (_name, html) => {
    uninstall = installNativeMenuGuard();
    const host = mount(html);

    expect(fire(host.querySelector("#t")!).defaultPrevented).toBe(false);
  });

  it("leaves an event prevented by an app handler prevented", () => {
    uninstall = installNativeMenuGuard();
    const host = mount("<div id='plain'></div>");
    const target = host.querySelector("#plain")!;
    target.addEventListener("contextmenu", (e) => e.preventDefault());

    expect(fire(target).defaultPrevented).toBe(true);
  });

  it("stops preventing the native menu after uninstall", () => {
    installNativeMenuGuard()();
    const host = mount("<div id='plain'></div>");

    expect(fire(host.querySelector("#plain")!).defaultPrevented).toBe(false);
  });
});
