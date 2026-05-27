# gah - Git Add Hunks (for AI Agents)

Stage specific hunks non-interactively. Use instead of `git add -p` which requires interactive input.

## When to use

- Staging partial changes from a file
- Separating unrelated changes into different commits
- Excluding debug/log statements from a commit
- Staging only the changes relevant to a specific task

## Commands

### Preview changes

```bash
# See all hunks with indices
gah preview <file>
gah preview --all

# Machine-readable
gah preview <file> --json
```

### Stage hunks

```bash
# By index (from preview output)
gah add <file> --hunks 1,3,5
gah add <file> --hunks 1-3

# By content pattern
gah add <file> --grep "pattern"
gah add <file> --grep "debug|console" --invert  # exclude

# By line range (working tree lines)
gah add <file> --lines 100-150

# Combine filters
gah add <file> --grep "feature" --lines 50-200

# Verify first
gah add <file> --hunks 1 --dry-run
```

## Workflow

1. `gah preview <file>` - see hunks numbered [1], [2], etc.
2. Identify which hunks to stage
3. `gah add <file> --hunks <indices>` - stage selected hunks
4. `git commit` - commit staged changes
5. Repeat for remaining changes

## JSON output schema

```json
{
  "file": "path/to/file",
  "hunks": [
    {
      "index": 1,
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

## Error handling

| Error | Meaning |
|-------|---------|
| "not a git repository" | Run in a git repo |
| "No changes to stage" | File has no unstaged changes |
| "hunk N does not exist" | Index out of range, re-run preview |
| "No hunks match pattern" | Grep found nothing, try different pattern |

## Tips

- Always preview before staging to get current hunk indices
- Use `--dry-run` when unsure
- Use `--json` for programmatic parsing
- Hunk indices change after staging - re-preview if staging multiple batches
