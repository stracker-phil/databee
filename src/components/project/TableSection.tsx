import { ChevronDown, ChevronRight } from "lucide-react";
import type { MouseEvent } from "react";
import type { TableRules } from "../../api";
import { plural } from "../../lib/format";
import type { useCollapsedGroups } from "../object/useCollapsedGroups";
import { TableIcon } from "../TableIcon";
import type { ProjectContext } from "./context";
import type { CoverageState } from "./coverage";
import { IconControl } from "./IconControl";
import { ReferencedByLine, ReferenceLines } from "./ReferenceLines";
import { EntityActionLines, KindLines, StyleLines } from "./RuleSections";

const NO_TABLE = "no table";

export const groupName = (group: TableRules) => group.table ?? NO_TABLE;

function countsText(group: TableRules): string {
  const parts = [
    group.references.length && plural(group.references.length, "reference", "references"),
    group.referencedBy.length && `${group.referencedBy.length} incoming`,
    group.styles.length && plural(group.styles.length, "value style", "value styles"),
    group.kinds.length && plural(group.kinds.length, "kind", "kinds"),
    group.entities.length && plural(group.entities.length, "entity", "entities"),
    group.actions.length && plural(group.actions.length, "action", "actions"),
  ];
  return parts.filter(Boolean).join(" · ");
}

function Caption({ text }: { text: string }) {
  return (
    <div className="row hdr caption" role="row">
      <span>{text}</span>
    </div>
  );
}

interface Props {
  group: TableRules;
  coverage: Record<string, CoverageState>;
  collapse: ReturnType<typeof useCollapsedGroups>;
  ctx: ProjectContext;
}

/** A band per table, collapsed per database like a document group; its rules below, by type. */
export function TableSection({ group, coverage, collapse, ctx }: Props) {
  const name = groupName(group);
  const id = `project:${name}`;
  const collapsed = collapse.isCollapsed(id);
  const toggleLabel = `${collapsed ? "Expand" : "Collapse"} ${name}`;
  const onBandClick = (e: MouseEvent) => {
    if (!(e.target as HTMLElement).closest("button")) collapse.toggle(id);
  };
  return (
    <section aria-label={name} className="project-table">
      <div className="sh" data-table={group.table ?? undefined} onClick={onBandClick}>
        <button className="sh-toggle" aria-expanded={!collapsed} aria-label={toggleLabel} data-stage={`project:collapse:${name}`} data-stage-label={toggleLabel} onClick={() => collapse.toggle(id)}>
          {collapsed ? <ChevronRight size="0.923rem" /> : <ChevronDown size="0.923rem" />}
        </button>
        <span className={`tn${group.table ? "" : " sans"}`} data-stage={`project:table:${name}`} data-stage-label={name} data-stage-list="project-tables">
          {group.table && <TableIcon database={ctx.tab.database} table={group.table} sessionId={ctx.tab.sessionId} />}
          {name}
        </span>
        {!group.table && <span className="via">value styles and kinds whose scope covers no table here</span>}
        <span className="spacer" />
        <span className="cnt">{countsText(group)}</span>
      </div>
      {!collapsed && (
        <>
          {group.table && (
            <div className="row facts icon-line">
              <IconControl table={group.table} ruleName={ctx.rules.tables.find((t) => t.table === group.table)?.name ?? null} icon={group.icon} ctx={ctx} />
            </div>
          )}
          {group.references.length > 0 && <Caption text="references" />}
          <ReferenceLines rows={group.references} coverage={coverage} ctx={ctx} />
          <ReferencedByLine incoming={group.referencedBy} />
          {group.styles.length > 0 && <Caption text="value styles" />}
          <StyleLines rows={group.styles} ctx={ctx} />
          {group.kinds.length > 0 && <Caption text="kinds" />}
          <KindLines rows={group.kinds} ctx={ctx} />
          <EntityActionLines entities={group.entities} actions={group.actions} />
        </>
      )}
    </section>
  );
}
