---
name: gah
description: Use when staging partial file changes, splitting commits, or excluding hunks - non-interactive alternative to `git add -p` for AI agents that cannot use interactive prompts
---

# gah - Git Add Hunks (for AI Agents)

Stage specific hunks non-interactively. Use instead of `git add -p` which requires interactive input.

## Critical Guidelines

1. **MUST use anchors, not indices** — Indices shift after each staging operation. Anchors stay stable.
2. **MUST preview before staging** — Never guess hunk numbers. Always run `gah preview` first.
3. **MUST use `--dry-run` for destructive or uncertain operations** — Verify before committing.
4. **MUST use `--json` for programmatic decisions** — Parse structured output, don't regex text.

## When to Use

- Staging partial changes from a file
- Separating unrelated changes into different commits
- Excluding debug/log/console statements from a commit
- Staging only changes relevant to a specific task

## Workflow

```
1. gah preview <file>           → See hunks with anchors
2. Identify target hunks        → Note anchors (not indices)
3. gah add <file> -a <anchor>   → Stage by anchor
4. git commit                   → Commit staged changes
5. Repeat for remaining         → Anchors of unstaged hunks unchanged
```

## Commands

### Preview changes

```bash
gah preview <file>        # See all hunks with indices and anchors
gah preview --all         # All modified files
gah preview <file> --json # Machine-readable (use for programmatic decisions)
```

### Stage hunks

```bash
# By anchor (PREFERRED — stable across staging operations)
gah add <file> --anchor Apparent
gah add <file> -a App              # prefix match works

# By index (fragile — indices shift after staging)
gah add <file> --hunks 1,3,5
gah add <file> --hunks 1-3

# By content pattern
gah add <file> --grep "pattern"
gah add <file> --grep "debug|console" --invert  # exclude matches

# By line range (working tree lines)
gah add <file> --lines 100-150

# Combine filters
gah add <file> --grep "feature" --lines 50-200

# Verify first
gah add <file> -a Foo --dry-run
```

## JSON Output Schema

```json
{
  "file": "path/to/file",
  "hunks": [
    {
      "index": 1,
      "anchor": "Apparent",
      "header": "@@ -10,5 +10,7 @@",
      "old_start": 10,
      "old_count": 5,
      "new_start": 10,
      "new_count": 7,
      "content": " context\n+added\n-removed",
      "function_context": "fn example(",
      "additions": 2,
      "deletions": 1
    }
  ]
}
```

## Error Handling

| Error | Cause | Fix |
|-------|-------|-----|
| "not a git repository" | Not in git repo | cd to repo root |
| "No changes to stage" | File has no unstaged changes | Check `git status` |
| "hunk N does not exist" | Index out of range | Re-run `gah preview` |
| "No hunks match pattern" | Grep found nothing | Try different pattern |
| "Ambiguous anchor prefix" | Multiple anchors match | Use longer prefix |

## Example: Splitting Mixed Changes

Scenario: `src/auth.rs` has both a bugfix and debug logging.

```bash
$ gah preview src/auth.rs --json
{
  "file": "src/auth.rs",
  "hunks": [
    {"index": 1, "anchor": "ValidateToken", "additions": 3, "deletions": 1},
    {"index": 2, "anchor": "DebugPrint", "additions": 5, "deletions": 0}
  ]
}

# Stage only the bugfix
$ gah add src/auth.rs -a ValidateToken
Staged 1 hunk (3 additions, 1 deletion)

$ git commit -m "fix: validate token expiry correctly"

# Debug logging still unstaged for separate commit or removal
```
