/** The Document view: a section band per group, then its key/value or table rows. */
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import type { Address, Document, Group, Provenance, Value as StoredValue } from "../../api";
import { EDIT_VALUE_EVENT, RULE_MENU } from "../../commands";
import { withRemovedRows, type DocumentMarks } from "../../lib/diff";
import { groupAnchors, isEmpty, provenanceKey } from "../../lib/document";
import { useStore } from "../../state/store";
import { ContextMenu } from "../ContextMenu";
import { useOpenValue } from "../value/useOpenValue";
import { DocumentContext, type DocumentActions } from "./fields";
import { GroupHeader } from "./GroupHeader";
import { AlsoLinked } from "./AlsoLinked";
import { EventsGroup, FieldsGroup, MetaGroup, MoreRows, RowsGroup, type Marks } from "./groups";
import { useCollapsedGroups } from "./useCollapsedGroups";

interface Props {
  document: Document;
  /** The row this document is of; its own primary key switches to Rows. */
  owner: Address;
  focus: Provenance | null;
  onFocus(source: Provenance | null): void;
  onMore(groupId: string): void;
  /** Whether `e` (`value.edit`) edits the focused value here: the tab is active and shows this view. */
  editable: boolean;
  /** Change marks of live mode, a snapshot or compare; removed rows show in their groups. */
  diff?: DocumentMarks | null;
}

function GroupBody({ group, marks }: { group: Group; marks: Marks }) {
  if (group.error) return <div className="row group-error" role="alert">{group.error}</div>;
  if (group.via.kind === "events") return <EventsGroup group={group} marks={marks} />;
  if (group.shape === "fields") return <FieldsGroup group={group} marks={marks} />;
  if (group.shape === "meta") return <MetaGroup group={group} marks={marks} />;
  return <RowsGroup group={group} marks={marks} />;
}

/** Queues a `setCell` on the value's provenance row, which a single-column key names (meta rows included). */
function saveValue(owner: Address, value: StoredValue, text: string | null) {
  const source = value.source;
  if (source?.row.length !== 1) return;
  const [key] = source.row;
  void useStore.getState().queueChange({ type: "setCell", database: owner.database, table: source.table, keyColumn: key.column, key: key.value, column: source.column, value: text });
}

/** `value.edit` opens the editor on the focused value while this view is the one shown. */
function useEditCommand(editable: boolean, focus: Provenance | null, setEditing: (key: string | null) => void) {
  useEffect(() => {
    if (!editable || !focus) return;
    const edit = () => setEditing(provenanceKey(focus));
    window.addEventListener(EDIT_VALUE_EVENT, edit);
    return () => window.removeEventListener(EDIT_VALUE_EVENT, edit);
  }, [editable, focus, setEditing]);
}

/** Keeps the focused value in view as `j`/`k` move it. */
function useRevealFocus(focus: Provenance | null) {
  const ref = useRef<HTMLDivElement>(null);
  const key = focus ? provenanceKey(focus) : null;
  useEffect(() => {
    if (key) ref.current?.querySelector(".row .focus")?.scrollIntoView?.({ block: "nearest" });
  }, [key]);
  return ref;
}

export function DocumentView({ document, owner, focus, onFocus, onMore, editable, diff = null }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const ref = useRevealFocus(focus);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const { openValue, targetMenu } = useOpenValue(owner);
  const collapse = useCollapsedGroups(owner);
  useEditCommand(editable, focus, setEditing);

  const actions = useMemo<DocumentActions>(
    () => ({
      onOpen: openValue,
      focus: onFocus,
      save: (value, text) => {
        setEditing(null);
        saveValue(owner, value, text);
      },
      cancelEdit: () => setEditing(null),
      menu: (e) => setMenu({ x: e.clientX, y: e.clientY }),
    }),
    [openValue, onFocus, owner],
  );
  const marks: Marks = { focus: focus ? provenanceKey(focus) : null, editing, diff };
  const groups = useMemo(() => withRemovedRows(document.groups, diff?.removedRows ?? []), [document.groups, diff]);
  const anchors = groupAnchors(groups);
  const shown = groups.filter((g) => !isEmpty(g));

  return (
    <DocumentContext.Provider value={actions}>
      <div className="document" ref={ref}>
        {shown.map((group) => {
          const collapsed = collapse.isCollapsed(group.id);
          return (
            <Fragment key={group.id}>
              <GroupHeader group={group} database={owner.database} anchor={anchors.get(group.id)} collapsed={collapsed} onToggle={collapse.toggle} />
              {!collapsed && <GroupBody group={group} marks={marks} />}
              {!collapsed && <MoreRows group={group} onMore={onMore} />}
            </Fragment>
          );
        })}
        <AlsoLinked groups={groups} database={owner.database} />
      </div>
      {targetMenu}
      {menu && <ContextMenu {...menu} items={RULE_MENU} onClose={() => setMenu(null)} />}
    </DocumentContext.Provider>
  );
}
