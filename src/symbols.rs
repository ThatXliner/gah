//! AST-based symbol extraction using tree-sitter.
//!
//! This module is only available with the `tree-sitter` feature flag.

use std::ops::Range;
use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_range: Range<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Method,
    Struct,
    Enum,
    Interface,
    Constant,
    #[allow(dead_code)]
    Variable,
}

pub fn extract_symbols(source: &str, path: &Path) -> Option<Vec<Symbol>> {
    let lang = detect_language(path)?;
    let (ts_lang, query_str) = get_language_config(lang)?;

    let mut parser = Parser::new();
    parser.set_language(&ts_lang).ok()?;

    let tree = parser.parse(source, None)?;
    let query = Query::new(&ts_lang, query_str).ok()?;

    let mut cursor = QueryCursor::new();

    let mut symbols = Vec::new();

    // tree-sitter 0.24 uses while-let pattern instead of iterator
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let node: Node = capture.node;
            let capture_name = &query.capture_names()[capture.index as usize];

            let kind = match capture_name.as_ref() {
                "function" | "function.name" => SymbolKind::Function,
                "class" | "class.name" => SymbolKind::Class,
                "method" | "method.name" => SymbolKind::Method,
                "struct" | "struct.name" => SymbolKind::Struct,
                "enum" | "enum.name" => SymbolKind::Enum,
                "interface" | "interface.name" => SymbolKind::Interface,
                "constant" | "constant.name" => SymbolKind::Constant,
                _ => continue,
            };

            // For .name captures, get the parent node's range
            let (name, range_node): (String, Node) = if capture_name.ends_with(".name") {
                let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
                let parent = match node.parent() {
                    Some(p) => p,
                    None => continue,
                };
                (name, parent)
            } else {
                // For full node captures, find the name child
                let name = match find_name_in_node(&node, source) {
                    Some(n) => n,
                    None => continue,
                };
                (name, node)
            };

            let start_line = range_node.start_position().row as u32 + 1;
            let end_line = range_node.end_position().row as u32 + 1;

            symbols.push(Symbol {
                name,
                kind,
                line_range: start_line..end_line,
            });
        }
    }

    Some(symbols)
}

fn find_name_in_node(node: &tree_sitter::Node, source: &str) -> Option<String> {
    // Look for identifier/name child nodes
    let name_kinds = ["identifier", "name", "function_name", "type_identifier"];

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if name_kinds.contains(&child.kind()) {
                return child.utf8_text(source.as_bytes()).ok().map(String::from);
            }
            // Check field names
            if let Some(field) = node.field_name_for_child(i as u32) {
                if field == "name" || field == "identifier" {
                    return child.utf8_text(source.as_bytes()).ok().map(String::from);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
enum SupportedLanguage {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
}

fn detect_language(path: &Path) -> Option<SupportedLanguage> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "py" => Some(SupportedLanguage::Python),
        "js" | "jsx" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
        "ts" | "tsx" => Some(SupportedLanguage::TypeScript),
        "rs" => Some(SupportedLanguage::Rust),
        "go" => Some(SupportedLanguage::Go),
        _ => None,
    }
}

fn get_language_config(lang: SupportedLanguage) -> Option<(Language, &'static str)> {
    match lang {
        SupportedLanguage::Python => Some((
            tree_sitter_python::LANGUAGE.into(),
            r#"
            (function_definition name: (identifier) @function.name) @function
            (class_definition name: (identifier) @class.name) @class
            "#,
        )),
        SupportedLanguage::JavaScript => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            r#"
            (function_declaration name: (identifier) @function.name) @function
            (class_declaration name: (identifier) @class.name) @class
            (method_definition name: (property_identifier) @method.name) @method
            (arrow_function) @function
            "#,
        )),
        SupportedLanguage::TypeScript => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            r#"
            (function_declaration name: (identifier) @function.name) @function
            (class_declaration name: (type_identifier) @class.name) @class
            (method_definition name: (property_identifier) @method.name) @method
            (interface_declaration name: (type_identifier) @interface.name) @interface
            "#,
        )),
        SupportedLanguage::Rust => Some((
            tree_sitter_rust::LANGUAGE.into(),
            r#"
            (function_item name: (identifier) @function.name) @function
            (struct_item name: (type_identifier) @struct.name) @struct
            (enum_item name: (type_identifier) @enum.name) @enum
            (impl_item) @class
            "#,
        )),
        SupportedLanguage::Go => Some((
            tree_sitter_go::LANGUAGE.into(),
            r#"
            (function_declaration name: (identifier) @function.name) @function
            (method_declaration name: (field_identifier) @method.name) @method
            (type_declaration (type_spec name: (type_identifier) @struct.name)) @struct
            "#,
        )),
    }
}

/// Find symbols that overlap with a given line range.
pub fn symbols_in_range(symbols: &[Symbol], start_line: u32, end_line: u32) -> Vec<&Symbol> {
    symbols
        .iter()
        .filter(|s| s.line_range.start <= end_line && s.line_range.end >= start_line)
        .collect()
}

/// Check if a hunk touches a symbol with the given name (case-insensitive substring match).
pub fn hunk_matches_symbol(
    symbols: &[Symbol],
    hunk_start: u32,
    hunk_end: u32,
    pattern: &str,
) -> bool {
    let pattern_lower = pattern.to_lowercase();
    symbols_in_range(symbols, hunk_start, hunk_end)
        .iter()
        .any(|s| s.name.to_lowercase().contains(&pattern_lower))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_python_symbols() {
        let source = r#"
def greet(name):
    return f"Hello, {name}!"

class Calculator:
    def add(self, a, b):
        return a + b

def subtract(a, b):
    return a - b
"#;
        let path = PathBuf::from("test.py");
        let symbols = extract_symbols(source, &path).unwrap();

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"Calculator"));
        assert!(names.contains(&"subtract"));
    }

    #[test]
    fn test_rust_symbols() {
        let source = r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
    y: i32,
}

enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let path = PathBuf::from("test.rs");
        let symbols = extract_symbols(source, &path).unwrap();

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"Point"));
        assert!(names.contains(&"Color"));
    }

    #[test]
    fn test_symbols_in_range() {
        let symbols = vec![
            Symbol {
                name: "foo".to_string(),
                kind: SymbolKind::Function,
                line_range: 1..5,
            },
            Symbol {
                name: "bar".to_string(),
                kind: SymbolKind::Function,
                line_range: 10..15,
            },
            Symbol {
                name: "baz".to_string(),
                kind: SymbolKind::Function,
                line_range: 20..25,
            },
        ];

        let found = symbols_in_range(&symbols, 12, 18);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "bar");

        let found = symbols_in_range(&symbols, 4, 11);
        assert_eq!(found.len(), 2); // foo and bar
    }
}
