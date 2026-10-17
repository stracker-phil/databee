/** The Document view: a section band per group, then its key/value or table rows. */
import { Fragment, useEffect, useMemo, useState } from "react";
import type { Address, Document, Group, Provenance, Value as StoredValue } from "../../api";
import { EDIT_VALUE_EVENT } from "../../commands";
import { groupAnchors, isEmpty, provenanceKey } from "../../lib/document";
import { useStore } from "../../state/store";
import { useOpenValue } from "../value/useOpenValue";
import { DocumentContext, type DocumentActions } from "./fields";
import { GroupHeader } from "./GroupHeader";
import { AlsoLinked } from "./AlsoLinked";
import { EventsGroup, FieldsGroup, MetaGroup, MoreRows, RowsGroup, type Marks } from "./groups";

interface Props {
  document: Document;
  /** The row this document is of; its own primary key switches to Rows. */
  owner: Address;
  focus: Provenance | null;
  onFocus(source: Provenance | null): void;
  onMore(groupId: string): void;
  /** Whether `e` (`value.edit`) edits the focused value here: the tab is active and shows this view. */
  editable: boolean;
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

export function DocumentView({ document, owner, focus, onFocus, onMore, editable }: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const { openValue, targetMenu } = useOpenValue(owner);
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
    }),
    [openValue, onFocus, owner],
  );
  const marks: Marks = { focus: focus ? provenanceKey(focus) : null, editing };
  const anchors = groupAnchors(document.groups);
  const shown = document.groups.filter((g) => !isEmpty(g));

  return (
    <DocumentContext.Provider value={actions}>
      <div className="document">
        {shown.map((group) => (
          <Fragment key={group.id}>
            <GroupHeader group={group} database={owner.database} anchor={anchors.get(group.id)} />
            <GroupBody group={group} marks={marks} />
            <MoreRows group={group} onMore={onMore} />
          </Fragment>
        ))}
        <AlsoLinked groups={document.groups} database={owner.database} />
      </div>
      {targetMenu}
    </DocumentContext.Provider>
  );
}
