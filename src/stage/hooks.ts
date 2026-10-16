/**
 * The page's side of the Stage hook (src-tauri/src/stage, docs/stage.md): it answers Rust's
 * `stage:collect`, `stage:info` and `stage:act` events. Nothing here runs unless Rust asks.
 */
import { stageApi } from "./api";
import { StageFailure, click, focusedWindow, nextFrame, press, reveal, scroll, sheet, typeInto } from "./actions";
import { clipsOf, collectAnchors, findAnchor, rectOf, windowOf } from "./anchors";
import { commandForKeys, runStageCommand, stageCommands, stageId } from "./commands";
import { visiblePart } from "./rect";
import { stageState, stageSurfaces, stageUi, stageView } from "./state";
import type { StageActReply, StageActRequest } from "./types";

const PAGE = "main";

type Outcome = Omit<StageActReply, "request" | "page">;
type Act<K extends StageActRequest["kind"]> = Extract<StageActRequest, { kind: K }>;

const NOT_HANDLED: Outcome = { handled: false, ok: false };
const DONE: Outcome = { handled: true, ok: true };

async function onWindow(request: Act<"window">): Promise<Outcome> {
  sheet(request.window, request.action, request.section);
  await nextFrame();
  return DONE;
}

async function onCommand(request: Act<"command">): Promise<Outcome> {
  const failure = runStageCommand(request.command);
  await nextFrame();
  return failure ? { handled: true, ok: false, ...failure } : DONE;
}

async function onKeys(request: Act<"keys">): Promise<Outcome> {
  const k = request.keys;
  const target = document.activeElement instanceof HTMLElement ? document.activeElement : document.body;
  // A command's shortcut runs it, as its menu accelerator or key handler would.
  const command = commandForKeys({ key: k.key, metaKey: k.meta, ctrlKey: k.ctrl, altKey: k.alt, shiftKey: k.shift, target });
  if (command) {
    void command.run();
    await nextFrame();
    return { ...DONE, command: stageId(command.id) };
  }
  if (!request.focused && !document.hasFocus()) return NOT_HANDLED;
  const window = focusedWindow();
  press(k, target);
  await nextFrame();
  return { ...DONE, window };
}

/**
 * Brings the anchor into place (`prepare`), measures it, then acts on it. Measured before acting:
 * a click may close or replace what it was on.
 */
async function onAnchor(
  id: string,
  prepare: (anchor: HTMLElement) => Promise<void>,
  act?: (anchor: HTMLElement) => void,
): Promise<Outcome> {
  const anchor = findAnchor(id);
  if (!anchor) return NOT_HANDLED;
  const window = windowOf(anchor);
  await prepare(anchor);
  const rect = rectOf(anchor);
  const visible = visiblePart(rect, clipsOf(anchor));
  if (act) {
    act(anchor);
    await nextFrame();
  }
  return { ...DONE, window, rect, visible };
}

function handle(request: StageActRequest): Promise<Outcome> {
  switch (request.kind) {
    case "window":
      return onWindow(request);
    case "command":
      return onCommand(request);
    case "keys":
      return onKeys(request);
    case "reveal":
      return onAnchor(request.anchor, reveal);
    case "click":
      return onAnchor(request.anchor, reveal, click);
    case "type":
      return onAnchor(request.anchor, reveal, (anchor) => typeInto(anchor, request.text, request.mode));
    case "scroll":
      return onAnchor(request.anchor, (anchor) => scroll(anchor, request));
  }
}

export async function perform(request: StageActRequest): Promise<StageActReply> {
  const base = { request: request.request, page: PAGE };
  try {
    return { ...base, ...(await handle(request)) };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const status = error instanceof StageFailure ? error.status : undefined;
    const subject = "anchor" in request ? `“${request.anchor}” ` : "";
    return { ...base, handled: true, ok: false, error: `${subject}${message}`, status };
  }
}

/** Call once, when the app starts; returns the listeners to remove. */
export function installStageHooks() {
  return [
    stageApi.onCollect((request) => void stageApi.anchors(request, PAGE, collectAnchors()).catch(() => undefined)),
    stageApi.onInfo(
      (request) =>
        void stageApi
          .info({ request, page: PAGE, state: stageState(), view: stageView(), ui: stageUi(), surfaces: stageSurfaces(), commands: stageCommands() })
          .catch(() => undefined),
    ),
    stageApi.onAct(
      (request) =>
        void perform(request)
          .then((reply) => stageApi.done(reply))
          .catch(() => undefined),
    ),
  ];
}
