import { useState } from "react";
import { FileCode2, Star, Table2 } from "lucide-react";
import type { Favorite } from "../api";
import { useBindingLabel } from "../commands";
import { useStore } from "../state/store";
import { ContextMenu } from "./ContextMenu";

/** `overId` while dragging over the bar outside any favorite: the drop moves to the end. */
const END = "\u0000end";

export function FavoritesBar() {
  const favorites = useStore((s) => s.favorites);
  const canAdd = useStore((s) => s.activeTabId !== null);
  const { openFavorite, removeFavorite, renameFavorite, moveFavorite, addCurrentTabToFavorites, openDialog } = useStore.getState();
  const [menu, setMenu] = useState<{ x: number; y: number; favorite: Favorite } | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const addKey = useBindingLabel("favorites.add");

  const endDrag = () => {
    setDragId(null);
    setOverId(null);
  };
  /** Drops the dragged favorite before `beforeId`, or at the end for null. */
  const dropBefore = (e: React.DragEvent, beforeId: string | null) => {
    e.preventDefault();
    e.stopPropagation();
    if (dragId && dragId !== beforeId) void moveFavorite(dragId, beforeId);
    endDrag();
  };

  return (
    <nav
      className={`favorites${overId === END ? " drop-end" : ""}`}
      aria-label="Favorites"
      data-stage="favorites"
      onDragOver={(e) => {
        if (!dragId) return;
        e.preventDefault();
        setOverId(END);
      }}
      onDrop={(e) => dropBefore(e, null)}
    >
      <button data-stage="favorites:add" className="icon-btn" aria-label="Add current tab to favorites" title={addKey ? `Add to favorites (${addKey})` : "Add to favorites"} disabled={!canAdd} onClick={addCurrentTabToFavorites}>
        <Star size="1.077rem" />
      </button>
      {favorites.length === 0 && <span className="empty">{addKey || "The star"} pins the current table or query here</span>}
      {favorites.map((f) => (
        <button
          key={f.id}
          data-stage={`favorite:${f.id}`}
          data-stage-list="favorites"
          className={`fav${dragId === f.id ? " dragging" : ""}${overId === f.id && dragId !== f.id ? " drop-before" : ""}`}
          title={f.target.kind === "table" ? `${f.target.database}.${f.target.table}` : f.target.sql}
          draggable
          onClick={() => openFavorite(f)}
          onContextMenu={(e) => {
            e.preventDefault();
            setMenu({ x: e.clientX, y: e.clientY, favorite: f });
          }}
          onDragStart={() => setDragId(f.id)}
          onDragOver={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setOverId(f.id);
          }}
          onDragEnd={endDrag}
          onDrop={(e) => dropBefore(e, f.id)}
        >
          {f.target.kind === "table" ? <Table2 size="0.923rem" /> : <FileCode2 size="0.923rem" />}
          {f.name}
        </button>
      ))}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            { label: "Open", action: () => openFavorite(menu.favorite) },
            {
              label: "Rename…",
              action: () =>
                openDialog({
                  type: "prompt",
                  title: "Rename favorite",
                  value: menu.favorite.name,
                  onSubmit: (name) => void renameFavorite(menu.favorite, name),
                }),
            },
            {
              label: menu.favorite.scope ? "Show for all connections" : "Show only for this server",
              action: () => void useStore.getState().toggleFavoriteScope(menu.favorite),
            },
            "separator",
            { label: "Remove", action: () => void removeFavorite(menu.favorite.id) },
          ]}
        />
      )}
    </nav>
  );
}
