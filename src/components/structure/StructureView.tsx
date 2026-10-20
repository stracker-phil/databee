import { useEffect, useState, type ReactNode } from "react";
import { Copy, MoreHorizontal } from "lucide-react";
import { api, type ColumnProfile, type MetaKeyRules, type ProjectChange, type ReferenceRow, type ResolvedReference, type TableProfile } from "../../api";
import { copyText } from "../../lib/clipboard";
import type { ProjectForm } from "../../lib/ruleForms";
import { scopeText } from "../../lib/ruleForms";
import { useApiResult } from "../../lib/useApiResult";
import { projectKey } from "../../state/slices/project";
import { useStore, type ProjectTab, type TableTab as TableTabState } from "../../state/store";
import { useContextMenu, type MenuAnchor, type MenuEntry } from "../ContextMenu";
import { ErrorPanel } from "../ErrorPanel";
import { writesSummary } from "../object/actions";
import { useAutoCoverage, type CoverageState } from "../project/coverage";
import { IconControl } from "../project/IconControl";
import { KindForm, StyleForm } from "../project/KindStyleForms";
import { ReferenceForm } from "../project/ReferenceForm";
import { newKindForm, newReferenceForm, newStyleForm, type ColumnTarget } from "./forms";
import { columnKindItem, columnStyleItem, KindItem, ReferenceItem, referenceKey, StyleItem, type RuleControls } from "./RuleItems";

function useTableProfile(tab: TableTabState) {
  const dataVersion = useStore((s) => s.dataVersion);
  const key = JSON.stringify([tab.sessionId, tab.database, tab.table, dataVersion]);
  return useApiResult(key, () => api.tableRules(tab.sessionId, tab.database, tab.table), { keepData: true });
}

/** The rule forms need the project's rules (the tables rules can name); they load with the view. */
function useRuleControls(tab: TableTabState, form: ProjectForm | null, setForm: (form: ProjectForm | null) => void): RuleControls {
  const { sessionId, database } = tab;
  const rules = useStore((s) => s.projectRules[projectKey(sessionId, database)]?.rules ?? null);
  const dataVersion = useStore((s) => s.dataVersion);
  useEffect(() => {
    void useStore.getState().loadProjectRules(sessionId, database);
  }, [sessionId, database, dataVersion]);
  if (!rules) return null;
  const projectTab: ProjectTab = { id: tab.id, kind: "project", sessionId, title: tab.title, database, address: tab.address, form, nav: tab.nav };
  return {
    tab: projectTab,
    rules,
    change: (change: ProjectChange) => useStore.getState().changeProject(sessionId, database, change),
    setForm,
  };
}

function FormSheet({ form, ctx }: { form: ProjectForm | null; ctx: RuleControls }) {
  if (!form || !ctx) return null;
  if (form.type === "reference") return <ReferenceForm key={`reference:${form.opener}`} form={form} ctx={ctx} />;
  if (form.type === "kind") return <KindForm key={`kind:${form.index}:${form.opener}`} form={form} ctx={ctx} />;
  return <StyleForm key={`style:${form.replacing}:${form.opener}`} form={form} ctx={ctx} />;
}

const applied = (rows: ReferenceRow[]): ResolvedReference[] => rows.flatMap((r) => (r.reference ? [r.reference] : []));

function listedReferences(profile: TableProfile): ResolvedReference[] {
  const rows = [...profile.columns.flatMap((c) => c.references), ...profile.meta.flatMap((m) => m.references), ...profile.referencedBy];
  return applied(rows);
}

/** `PK`, then each index that holds the column: `UNI order_key`, `idx type_status 2` (its position when not first). */
function KeyMarks({ column }: { column: ColumnProfile }) {
  const indexes = column.indexes.filter((i) => i.name !== "PRIMARY");
  return (
    <span className="key-marks">
      {column.primaryKey && <span className="badge">PK</span>}
      {indexes.map((i) => (
        <span key={i.name} className="index-mark" title={`${i.unique ? "unique " : ""}index ${i.name}, column ${i.position}`}>
          <span className="o3">{i.unique ? "UNI" : "idx"}</span> {i.name}
          {i.position > 1 && <span className="o3"> {i.position}</span>}
        </span>
      ))}
    </span>
  );
}

/** `nullable` or `not null`, `default NULL`, the server's extras (`auto_increment`), the comment. */
function columnFacts(info: ColumnProfile["column"]): string[] {
  const facts = [info.nullable ? "nullable" : "not null"];
  if (info.default !== null) facts.push(`default ${info.default}`);
  if (info.extra) facts.push(info.extra);
  if (info.comment) facts.push(info.comment);
  return facts;
}

const RULE_LABELS = { kind: "kind", references: "references", styles: "value styles" };

interface RowProps {
  column: ColumnProfile;
  coverage: Record<string, CoverageState>;
  ctx: RuleControls;
  onMenu: (at: MenuAnchor, column: string) => void;
  onOpenTable: (table: string) => void;
}

function ColumnRow({ column, coverage, ctx, onMenu, onOpenTable }: RowProps) {
  const info = column.column;
  return (
    <div className="structure-row" role="row" aria-label={info.name} data-stage={`structure:column:${info.name}`} data-stage-label={info.name} data-stage-list="structure-columns" onContextMenu={(e) => onMenu(e, info.name)}>
      <span className="col-name" role="cell">
        {info.name}
      </span>
      <span className="col-type mono" role="cell">
        {info.columnType}
        <span className="o3 col-facts">{columnFacts(info).join(" · ")}</span>
      </span>
      <span role="cell">
        <KeyMarks column={column} />
      </span>
      <span className="rule-cell" role="cell" data-label={RULE_LABELS.kind}>
        {column.kinds.map((k) => columnKindItem(k, ctx))}
      </span>
      <span className="rule-cell" role="cell" data-label={RULE_LABELS.references}>
        {column.references.map((r) => (
          <ReferenceItem key={referenceKey(r)} row={r} coverage={coverage} ctx={ctx} shows="target" onOpenTable={onOpenTable} />
        ))}
      </span>
      <span className="rule-cell" role="cell" data-label={RULE_LABELS.styles}>
        {column.styles.map((s) => columnStyleItem(s, ctx))}
      </span>
      <span className="col-menu" role="cell">
        <button className="icon-btn tiny" aria-label={`Column menu ${info.name}`} onClick={(e) => onMenu({ x: e.clientX, y: e.clientY }, info.name)}>
          <MoreHorizontal size="0.923rem" />
        </button>
      </span>
    </div>
  );
}

function Block({ title, children }: { title: ReactNode; children: ReactNode }) {
  return (
    <section className="structure-block" aria-label={typeof title === "string" ? title : undefined}>
      <h3>{title}</h3>
      {children}
    </section>
  );
}

function MetaBlock({ meta, coverage, ctx, onOpenTable }: { meta: MetaKeyRules; coverage: Record<string, CoverageState>; ctx: RuleControls; onOpenTable: (table: string) => void }) {
  const empty = meta.kinds.length + meta.styles.length + meta.references.length === 0;
  return (
    <Block
      title={
        <>
          meta keys · <span className="mono">{meta.table}</span>{" "}
          <span className="o3 mono">
            ({meta.keyColumn} → {meta.valueColumn})
          </span>
        </>
      }
    >
      {empty && <div className="o3">no rule names a meta key here</div>}
      {meta.references.map((r) => (
        <div key={referenceKey(r)} className="block-line">
          <span className="mono">{r.when}</span>
          <ReferenceItem row={{ ...r, when: "" }} coverage={coverage} ctx={ctx} shows="target" onOpenTable={onOpenTable} />
        </div>
      ))}
      {meta.kinds.map((k, i) => (
        <div key={`kind:${i}`} className="block-line">
          <span className="mono">{scopeText(k.rule)}</span>
          <KindItem row={k} ctx={ctx} />
        </div>
      ))}
      {meta.styles.map((s) => (
        <div key={`style:${s.id}`} className="block-line">
          <span className="mono">{scopeText(s.rule)}</span>
          <StyleItem row={s} ctx={ctx} />
        </div>
      ))}
    </Block>
  );
}

function Ddl({ ddl }: { ddl: string }) {
  return (
    <div>
      <h3 style={{ display: "flex", alignItems: "center", gap: 6 }}>
        DDL
        <button className="icon-btn" aria-label="Copy DDL" onClick={() => void copyText(ddl, "DDL")}>
          <Copy size="0.923rem" />
        </button>
      </h3>
      <pre className="ddl">{ddl}</pre>
    </div>
  );
}

function Profile({ tab, profile }: { tab: TableTabState; profile: TableProfile }) {
  const [form, setForm] = useState<ProjectForm | null>(null);
  const ctx = useRuleControls(tab, form, setForm);
  const coverage = useAutoCoverage(tab.sessionId, tab.database, listedReferences(profile));
  const menu = useContextMenu();
  const openTable = (table: string) => useStore.getState().openTableTab(tab.database, table, "structure");

  const columnMenu = (at: MenuAnchor, column: string) => {
    const target: ColumnTarget = { table: profile.table, ruleName: profile.ruleName, column };
    const disabled = !ctx;
    const items: MenuEntry[] = [
      { label: "Add reference…", disabled, action: () => setForm(newReferenceForm(target)) },
      { label: "Add value style…", disabled, action: () => setForm(newStyleForm(target)) },
      { label: "Add kind…", disabled, action: () => setForm(newKindForm(target)) },
      "separator",
      { label: "Open rows", action: () => useStore.getState().updateTableTab(tab.id, { view: "data" }) },
    ];
    menu.open(at, items);
  };

  return (
    <div className="structure" data-stage="table:structure-view" data-stage-label="Table structure">
      <div className="structure-icon">
        <IconControl table={profile.table} ruleName={profile.ruleName} icon={profile.icon} ctx={ctx} />
      </div>
      <div>
        <h3>Columns</h3>
        <div className="structure-grid" role="table" aria-label={`Columns of ${profile.table}`}>
          <div className="structure-row head" role="row">
            {["name", "type", "keys"].map((h) => (
              <span key={h} role="columnheader">
                {h}
              </span>
            ))}
            {Object.values(RULE_LABELS).map((h) => (
              <span key={h} className="rule-head" role="columnheader">
                {h}
              </span>
            ))}
            <span className="col-menu" role="columnheader" />
          </div>
          {profile.columns.map((c) => (
            <ColumnRow key={c.column.name} column={c} coverage={coverage} ctx={ctx} onMenu={columnMenu} onOpenTable={openTable} />
          ))}
        </div>
      </div>
      {profile.meta.map((m) => (
        <MetaBlock key={m.table} meta={m} coverage={coverage} ctx={ctx} onOpenTable={openTable} />
      ))}
      {profile.referencedBy.length > 0 && (
        <Block title="Referenced by">
          {profile.referencedBy.map((r) => (
            <ReferenceItem key={referenceKey(r)} row={r} coverage={coverage} ctx={ctx} shows="source" onOpenTable={openTable} />
          ))}
        </Block>
      )}
      {profile.entities.length > 0 && (
        <Block title="Entities">
          {profile.entities.map((e) => (
            <div key={`${e.preset}:${e.name}:${e.filter ?? ""}`} className="block-line" role="group" aria-label={`entity ${e.name}`}>
              <span className="mono">{e.name}</span>
              <span className="facts">
                {e.filter && <span className="mono">where {e.filter}</span>}
                {e.summary.length > 0 && <span className="o2">summary <span className="mono">{e.summary.join(", ")}</span></span>}
                {e.created && <span className="o2">created <span className="mono">{e.created}</span></span>}
                <span className="origin">preset {e.preset}</span>
              </span>
            </div>
          ))}
        </Block>
      )}
      {profile.actions.length > 0 && (
        <Block title="Actions">
          {profile.actions.map((a) => (
            <div key={`${a.preset}:${a.name}`} className="block-line" role="group" aria-label={`action ${a.name}`}>
              <span className="mono">{a.name}</span>
              <span className="facts">
                <span className="o2">{a.scope}</span>
                <span className="mono">{writesSummary(a)}</span>
                <span className="origin">preset {a.preset}</span>
              </span>
            </div>
          ))}
        </Block>
      )}
      {profile.ddl !== null && <Ddl ddl={profile.ddl} />}
      {menu.element}
      <FormSheet form={form} ctx={ctx} />
    </div>
  );
}

/** One row per column with every rule that applies to it, then the table's meta-key rules, incoming references, entities and actions. */
export function StructureView({ tab }: { tab: TableTabState }) {
  const result = useTableProfile(tab);
  if (result.error && !result.data) return <ErrorPanel message={result.error.message} code={result.error.code} />;
  if (!result.data) return <div className="empty-state">Loading…</div>;
  return <Profile tab={tab} profile={result.data} />;
}
