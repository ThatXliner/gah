# Future Features

## Symbol-based hunk selection

**Status:** Shipped in 0.2.0 (`--symbol`, behind the `tree-sitter` feature).

Stage hunks by the symbol (function, class, method) they touch. Supports Python,
JavaScript, TypeScript, Rust, and Go.

## Hunk splitting

**Status:** Shipped (`--split` and partial-hunk `--lines`).

- `--split` re-diffs at zero context so git's merged hunks break into the
  smallest separable pieces.
- `--lines` trims a hunk to only the changed lines in a range, including single
  lines out of an adjacent replacement block (the case `git add -p` split mode
  can't handle).

### Possible follow-ups

- A `--lines` mode that targets *old-file* line numbers as well as new-file.
- Splitting a contiguous replacement block automatically (currently requires
  naming lines via `--lines`).
