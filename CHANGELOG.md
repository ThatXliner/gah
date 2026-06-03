# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `--split` on `preview` and `add`: re-diffs at zero context so git's merged
  hunks break into the smallest separable pieces, each with its own anchor.
- Partial-hunk staging: `--lines` now trims a matched hunk to only the changed
  lines within the range, including individual lines out of an adjacent
  replacement block that `git add -p`'s split mode cannot separate.

### Changed

- **Breaking:** `--lines` now stages only the changed lines within the range
  rather than every hunk that overlaps it. Pass a range covering the whole hunk
  to recover the previous behavior.

## [0.2.2] - 2026-05-31

### Fixed

- Suppress ANSI color codes when `gah` is run by an AI coding agent. The TTY
  check alone missed agents running inside a real terminal, where stdout is a
  TTY but the consumer is an LLM that chokes on escape sequences. Known agent
  environments (Claude, Cursor, Codex, Gemini, Copilot, Devin, and others) are
  now detected and color is disabled for them.

## [0.2.1] - 2026-05-27

### Changed

- TTY-aware output: color is emitted only when stdout is a terminal.

## [0.2.0] - 2026-05-27

### Added

- AST symbol filtering via tree-sitter (`--symbol`), behind the `tree-sitter`
  feature, for Python, JavaScript, TypeScript, Rust, and Go.

## [0.1.0] - 2026-05-27

### Added

- Initial release: non-interactive, hunk-based staging for git. Preview hunks
  and stage them by index, content-hash anchor, regex, or working-tree line
  range.

[Unreleased]: https://github.com/ThatXliner/gah/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/ThatXliner/gah/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/ThatXliner/gah/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/ThatXliner/gah/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ThatXliner/gah/releases/tag/v0.1.0
