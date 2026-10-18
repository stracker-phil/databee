import type { RulesInfo, TableIcon } from "../api";
import { projectKey } from "../state/slices/project";
import { useStore } from "../state/store";

/** The `rulesInfo` of a database with the given table icons. */
export function rulesInfoOf(tableIcons: Record<string, TableIcon> = {}): RulesInfo {
  return { prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: [], availablePresets: [], entities: [], tableEntities: {}, tableIcons };
}

/** Puts the table icons of session `s1`, database `db` in the store, as if Rust had answered `rulesInfo`. */
export function showTableIcons(tableIcons: Record<string, TableIcon>) {
  useStore.setState({ rulesInfo: { [projectKey("s1", "db")]: rulesInfoOf(tableIcons) } });
}
