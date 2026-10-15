import type { EntityLink } from "../../api";
import { useStore } from "../../state/store";

export function LinkChip({ link, label }: { link: EntityLink; label?: string }) {
  const openEntity = useStore((s) => s.openEntity);
  return (
    <button
      className="chip"
      aria-label={`Open ${link.entity} ${link.id}`}
      title={`${link.entity} #${link.id}`}
      onClick={() => openEntity(link.entity, link.id)}
    >
      {link.image && <img src={link.image} alt="" loading="lazy" />}
      <span className="chip-label">{label ?? link.label ?? `${link.entity} #${link.id}`}</span>
      <span className="chip-id">#{link.id}</span>
    </button>
  );
}
