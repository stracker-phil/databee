# Commits

- Every session or step ends in a commit, so it can be reverted. A step may be split into several commits;
  commit each finished part as soon as it is green.
- Never push. Only commit locally.
- Unit tests and lint are green (through the `ci` agent) before every commit.
- Run the `clean-up-code` skill on the uncommitted changes before every commit.
- First line: concise, imperative, sentence case, max 60 chars, no prefix, no trailing period. Two related
  changes may share it, joined by a comma or a semicolon.
- No body by default; add one only when the "why" isn't obvious.
- Author: whoever runs the commit (the local git config); never set or override it.
- Stage files by path; check `git status` for stray files first.
