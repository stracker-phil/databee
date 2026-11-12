# Stable controls

UI changes follow [docs/ux.md → Stable controls](../../docs/ux.md#stable-controls) and
[Motion](../../docs/ux.md#motion):

- A click never changes what is under the cursor; clicking the same spot again does the same thing again.
- Lists and pickers keep their order while open; toggling an item changes only its box or switch.
- Toggles keep their size (state by color, not by an added icon); optional buttons keep their slot.
- What appears fades in; what moves glides (`useGlide`, `useListGlide` from Taskboard's `src/lib/motion.ts`,
  ported when needed); nothing jumps from one state to another. Durations are `src/styles/motion.css` tokens.
- Folding into "…" follows the window width only.

Before finishing a UI change, click every new or changed control twice in the debug app, run
`scripts/dev/e2e.sh`, and check that nothing beside it moved.
