# 0009. A script runs as one multi-statement call

## Status

Accepted (retroactive, introduced in 6dfacd4, 2026-10-15)

## Context

Running a script statement by statement changes its semantics, costs a round trip per statement and
complicates cancelling. Sending it whole returns result sets without saying which statement produced each.

## Decision

The whole script goes to the server as one multi-statement text-protocol call. Results are matched to
statements using the lexer's statement split; only when the counts agree are column names of empty results
recovered.

## Consequences

- Native server semantics and one round trip per run.
- The split is a heuristic with known limits: it does not handle `DELIMITER` and ignores
  `NO_BACKSLASH_ESCAPES` and `ANSI_QUOTES`. Where they apply, the count differs and recovery is skipped.
- An error in the middle of a script loses the attribution of what ran before it.
