import { useState } from "react";
import { FileCode2, Star, Table2 } from "lucide-react";
import type { Favorite } from "../api";
import { useStore } from "../state/store";
import { ContextMenu } from "./ContextMenu";

export function FavoritesBar() {
  const favorites = useStore((s) => s.favorites);
  const canAdd = useStore((s) => s.activeTabId !== null);
  const { openFavorite, removeFavorite, renameFavorite, moveFavorite, addCurrentTabToFavorites, openDialog } = useStore.getState();
  const [menu, setMenu] = useState<{ x: number; y: number; favorite: Favorite } | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);

  return (
    <nav className="favorites" aria-label="Favorites">
      <button className="icon-btn" aria-label="Add current tab to favorites" title="Add to favorites (⌘D)" disabled={!canAdd} onClick={addCurrentTabToFavorites}>
        <Star size={14} />
      </button>
      {favorites.length === 0 && <span className="empty">Pin tables and queries here with ⌘D</span>}
      {favorites.map((f) => (
        <button
          key={f.id}
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
            setOverId(f.id);
          }}
          onDragEnd={() => {
            setDragId(null);
            setOverId(null);
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragId && dragId !== f.id) void moveFavorite(dragId, f.id);
            setDragId(null);
            setOverId(null);
          }}
        >
          {f.target.kind === "table" ? <Table2 size={12} /> : <FileCode2 size={12} />}
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
              action: () => void useStore.getState().saveFavoriteScope(menu.favorite),
            },
            "separator",
            { label: "Remove", action: () => void removeFavorite(menu.favorite.id) },
          ]}
        />
      )}
    </nav>
  );
}
