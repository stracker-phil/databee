interface ScopeDef {
  id: string;
  /** Null only for the root scope; any other parent must already be registered. */
  parent: string | null;
  /** Shown in Settings → Shortcuts. */
  label: string;
  isActive(): boolean;
}

const scopes = new Map<string, ScopeDef>();

/** Registers a scope, replacing one with the same id. Throws on an unknown parent. */
export function registerScope(def: ScopeDef): void {
  if (def.parent !== null && !scopes.has(def.parent)) {
    throw new Error(`Scope "${def.id}" has unknown parent "${def.parent}"`);
  }
  scopes.set(def.id, def);
}

export function scopeDef(id: string): ScopeDef | undefined {
  return scopes.get(id);
}

export function activeScopes(): string[] {
  return [...scopes.values()].filter((def) => def.isActive()).map((def) => def.id);
}

function ancestors(scope: string): string[] {
  const chain: string[] = [];
  for (let parent = scopes.get(scope)?.parent ?? null; parent; parent = scopes.get(parent)?.parent ?? null) {
    chain.push(parent);
  }
  return chain;
}

export function isAncestor(ancestor: string, scope: string): boolean {
  return ancestors(scope).includes(ancestor);
}

/** Depth in the parent chain: the root is 0, its children 1, and so on. */
export function specificity(scope: string): number {
  return ancestors(scope).length;
}

export function scopeLabel(id: string): string {
  return scopes.get(id)?.label ?? id;
}
