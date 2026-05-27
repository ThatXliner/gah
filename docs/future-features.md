# Future Features

## Symbol-based hunk selection

**Status:** Planned

Stage hunks by the symbol (function, class, method) they touch, rather than by index or anchor.

```bash
gah add src/main.rs --symbol "fn process_request"
gah add src/lib.rs --symbol "impl Parser"
```

### Implementation notes

- Use tree-sitter for AST parsing
- Map hunk line ranges to AST nodes
- Support multiple languages (Rust, TypeScript, Python, Go, etc.)
- Fall back gracefully when tree-sitter grammar unavailable

### Why this is useful

- More semantic than line ranges
- Survives refactors better than anchors (symbol names change less often than content)
- Natural for AI agents: "stage the changes to function X"

### Considerations

- Adds tree-sitter dependency (binary size increase)
- Need to support many languages or provide extension mechanism
- What happens when a hunk spans multiple symbols?
