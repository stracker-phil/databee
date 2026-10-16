import { useStore } from "../state/store";

/** Copies `text` and confirms with a toast ("Copied {what}"), or reports that the clipboard failed. */
export async function copyText(text: string, what: string): Promise<void> {
  const { toast } = useStore.getState();
  try {
    await navigator.clipboard.writeText(text);
    toast("success", `Copied ${what}`);
  } catch {
    toast("error", "Clipboard is not available");
  }
}
