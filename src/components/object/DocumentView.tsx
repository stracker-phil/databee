import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import type { Address, Document, Group, Provenance, Value as StoredValue } from "../../api";
import { EDIT_VALUE_EVENT, runCommand } from "../../commands";
import { copyText } from "../../lib/clipboard";
import { cellText } from "../../lib/format";
import { withRemovedRows, type DocumentMarks } from "../../lib/diff";
import { groupAnchors, isEmpty, provenanceKey } from "../../lib/document";
import { useStore } from "../../state/store";
import { useContextMenu } from "../ContextMenu";
import { cellMenu } from "../grid/cellMenu";
import { useOpenValue } from "../value/useOpenValue";
import { DocumentContext, type DocumentActions } from "./fields";
import { GroupHeader } from "./GroupHeader";
import { EventsGroup, FieldsGroup, MetaGroup, MoreRows, RowsGroup, type Marks } from "./groups";
import { useCollapsedGroups } from "./useCollapsedGroups";

interface Props {
  document: Document;
  /** The row this document is of; its own primary key switches to Rows. */
  owner: Address;
  focus: Provenance | null;
  onFocus: (source: Provenance | null) => void;
  onMore: (groupId: string) => void;
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

/** Where a value sits in its group, as the grid's rows and columns, for the menu's row copy items. */
function placeOf(groups: Group[], value: StoredValue) {
  for (const group of groups) {
    const row = group.rows.findIndex((r) => r.fields.some(([, v]) => v === value));
    if (row < 0) continue;
    const fields = group.rows[row].fields;
    return {
      row,
      col: fields.findIndex(([, v]) => v === value),
      names: fields.map(([name]) => name),
      rows: group.rows.map((r) => r.fields.map(([, v]) => v.raw)),
    };
  }
  return null;
}

function useEditCommand(editable: boolean, focus: Provenance | null, setEditing: (key: string | null) => void) {
  useEffect(() => {
    if (!editable || !focus) return;
    const edit = () => setEditing(provenanceKey(focus));
    window.addEventListener(EDIT_VALUE_EVENT, edit);
    return () => window.removeEventListener(EDIT_VALUE_EVENT, edit);
  }, [editable, focus, setEditing]);
}

/**
 * Scrolls the focused value into view, down the pane and sideways in its group; again a frame later, when a long
 * group has rendered the row it scrolled to.
 */
function useRevealFocus(focus: Provenance | null) {
  const ref = useRef<HTMLDivElement>(null);
  const key = focus ? provenanceKey(focus) : null;
  useEffect(() => {
    if (!key) return;
    const reveal = () => ref.current?.querySelector(".row .focus")?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    reveal();
    const frame = requestAnimationFrame(reveal);
    return () => cancelAnimationFrame(frame);
  }, [key]);
  return ref;
}

export function DocumentView({ document, owner, focus, onFocus, onMore, editable, diff = null }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const ref = useRevealFocus(focus);
  const { open: openMenu, element: menu } = useContextMenu();
  const { openValue, targetMenu } = useOpenValue(owner);
  const collapse = useCollapsedGroups(owner);
  useEditCommand(editable, focus, setEditing);

  const groups = useMemo(() => withRemovedRows(document.groups, diff?.removedRows ?? []), [document.groups, diff]);
  const actions = useMemo<DocumentActions>(
    () => ({
      onOpen: openValue,
      focus: onFocus,
      save: (value, text) => {
        setEditing(null);
        saveValue(owner, value, text);
      },
      cancelEdit: () => setEditing(null),
      menu: (e, value) => {
        const place = placeOf(groups, value);
        if (!place) return;
        const source = value.source;
        const edit =
          editable && source?.row.length === 1
            ? { start: () => setEditing(provenanceKey(source)), setNull: () => saveValue(owner, value, null) }
            : undefined;
        openMenu(
          e,
          cellMenu({
            ...place,
            value,
            commands: "object",
            copy: (text, what) => void copyText(text, what),
            copyValue: () => (source ? runCommand("object.copyRaw") : void copyText(cellText(value.raw), "value")),
            edit,
          }),
        );
      },
    }),
    [openValue, onFocus, owner, openMenu, groups, editable],
  );
  const focusKey = focus ? provenanceKey(focus) : null;
  const marks = useMemo<Marks>(() => ({ focus: focusKey, editing, diff }), [focusKey, editing, diff]);
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
      </div>
      {targetMenu}
      {menu}
    </DocumentContext.Provider>
  );
}
