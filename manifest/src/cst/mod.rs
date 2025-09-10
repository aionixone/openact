mod node;
pub use node::{TreeCursorSyntaxNode, TreeIterator, TraversalOrder};

use tree_sitter::{Parser, TreeCursor, Language};
use std::cell::RefCell;
use std::sync::Arc;

/// Supported source types
///
/// Defines the different format types that the CST parser can handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
}

impl SourceType {
    /// Infers the source type from a file extension
    ///
    /// # Arguments
    /// * `extension` - The file extension (e.g., "json", "yaml", "yml")
    ///
    /// # Returns
    /// The corresponding source type, or None if unrecognized
    ///
    /// # Example
    /// ```
    /// use apidom_cst::SourceType;
    ///
    /// assert_eq!(SourceType::from_extension("json"), Some(SourceType::Json));
    /// assert_eq!(SourceType::from_extension("yaml"), Some(SourceType::Yaml));
    /// assert_eq!(SourceType::from_extension("yml"), Some(SourceType::Yaml));
    /// assert_eq!(SourceType::from_extension("txt"), None);
    /// ```
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_lowercase().as_str() {
            "json" => Some(SourceType::Json),
            "yaml" | "yml" => Some(SourceType::Yaml),
            _ => None,
        }
    }
    
    /// Gets the display name of the source type
    ///
    /// # Returns
    /// A string representation of the source type
    pub fn display_name(&self) -> &'static str {
        match self {
            SourceType::Json => "JSON",
            SourceType::Yaml => "YAML",
        }
    }

    /// Detects the source type using heuristics
    ///
    /// Infers the most likely format type based on content characteristics.
    ///
    /// # Arguments
    /// * `source` - The source string to detect
    ///
    /// # Returns
    /// The recommended source type
    pub fn detect_from_content(source: &str) -> Self {
        let source = source.trim();
        
        // Obvious JSON characteristics
        if source.starts_with('{') || source.starts_with('[') {
            return SourceType::Json;
        }
        
        // YAML document separator
        if source.starts_with("---") {
            return SourceType::Yaml;
        }
        
        // Check for YAML-style key-value pairs (key: value not in quotes)
        let lines: Vec<&str> = source.lines().collect();
        let mut yaml_indicators = 0;
        let mut json_indicators = 0;
        
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue; // Skip empty lines and comments
            }
            
            // YAML-style key-value pair
            if trimmed.contains(':') && !trimmed.starts_with('"') && !trimmed.starts_with('{') {
                yaml_indicators += 1;
            }
            
            // YAML list item
            if trimmed.starts_with("- ") {
                yaml_indicators += 1;
            }
            
            // JSON-style quoted key
            if trimmed.contains(r#"":"#) {
                json_indicators += 1;
            }
        }
        
        if yaml_indicators > json_indicators {
            SourceType::Yaml
        } else {
            SourceType::Json
        }
    }
}

// Thread-local Parser to avoid multi-threading contention
//
// Each thread has its own Parser instance, avoiding the overhead of a global lock.
// Maintains separate parser instances for JSON and YAML.
thread_local! {
    static THREAD_LOCAL_JSON_PARSER: RefCell<Option<Parser>> = RefCell::new(None);
    static THREAD_LOCAL_YAML_PARSER: RefCell<Option<Parser>> = RefCell::new(None);
}

/// General helper function for Parser operations
///
/// Automatically retrieves the thread-local Parser for the corresponding type, performs the operation, and then returns it.
/// This avoids repetitive get/return boilerplate code.
///
/// # Arguments
/// * `source_type` - The source type
/// * `f` - The operation to perform, receiving a mutable Parser reference
///
/// # Returns
/// The return value of the operation
fn with_parser<F, R>(source_type: SourceType, f: F) -> R 
where 
    F: FnOnce(&mut Parser) -> R 
{
    match source_type {
        SourceType::Json => {
            THREAD_LOCAL_JSON_PARSER.with(|parser_cell| {
                let mut parser_opt = parser_cell.borrow_mut();
                if parser_opt.is_none() {
                    let mut parser = Parser::new();
                    let language = tree_sitter_json::LANGUAGE;
                    parser.set_language(&Language::new(language)).unwrap();
                    *parser_opt = Some(parser);
                }
                
                // Take out the parser, perform the operation, then put it back
                let mut parser = parser_opt.take().unwrap();
                let result = f(&mut parser);
                *parser_opt = Some(parser);
                result
            })
        }
        SourceType::Yaml => {
            THREAD_LOCAL_YAML_PARSER.with(|parser_cell| {
                let mut parser_opt = parser_cell.borrow_mut();
                if parser_opt.is_none() {
                    let mut parser = Parser::new();
                    let language = tree_sitter_yaml::LANGUAGE;
                    parser.set_language(&Language::new(language)).unwrap();
                    *parser_opt = Some(parser);
                }
                
                // Take out the parser, perform the operation, then put it back
                let mut parser = parser_opt.take().unwrap();
                let result = f(&mut parser);
                *parser_opt = Some(parser);
                result
            })
        }
    }
}

/// General function to recursively traverse and construct CST child nodes
///
/// This function is extracted to avoid repeating the same logic in multiple places.
///
/// # Arguments
/// * `cursor` - tree-sitter cursor
/// * `shared_source` - Shared source reference
/// * `parent` - Parent node, whose children will be populated
fn descend_and_build_children(
    cursor: &mut TreeCursor,
    shared_source: &Arc<str>,
    parent: &mut TreeCursorSyntaxNode
) {
    if cursor.goto_first_child() {
        // We need to get the parent node before moving to the first child
        cursor.goto_parent();
        let parent_node = cursor.node(); // Get the parent node for field name lookup
        cursor.goto_first_child(); // Move back to first child
        
        let mut child_index = 0u32;
        
        loop {
            // Get field name for this child from parent
            let field_name = parent_node.field_name_for_child(child_index);
            
            let mut child = TreeCursorSyntaxNode::from_cursor_with_shared_source_and_field(
                cursor, 
                shared_source.clone(),
                field_name.map(|s| s.to_string())
            );
            descend_and_build_children(cursor, shared_source, &mut child);
            parent.children.push(child);
            
            child_index += 1;
            if !cursor.goto_next_sibling() { 
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// CST parser builder
///
/// Provides a fluent API to parse JSON and YAML and perform various operations.
/// Supports automatic type detection and manual source type specification.
///
/// # Example
/// ```
/// use apidom_cst::{CstParser, SourceType};
///
/// // Automatic detection (default JSON)
/// let cst = CstParser::parse(r#"{"key": "value"}"#);
///
/// // Explicit type specification
/// let json_cst = CstParser::parse_as(r#"{"key": "value"}"#, SourceType::Json);
/// let yaml_cst = CstParser::parse_as("key: value", SourceType::Yaml);
///
/// // Chained calls
/// let strings: Vec<_> = CstParser::parse_as("key: value", SourceType::Yaml)
///     .find_nodes_by_kind("flow_entry")
///     .into_iter()
///     .map(|node| node.text())
///     .collect();
/// ```
pub struct CstParser;

impl CstParser {
    /// Parses source code into a CST (default JSON format)
    ///
    /// This is a backward-compatible method that defaults to treating input as JSON.
    /// Use the `parse_as` method to parse other formats.
    ///
    /// # Arguments
    /// * `source` - The source string to parse
    ///
    /// # Returns
    /// The parsed CST root node
    ///
    /// # Example
    /// ```
    /// use apidom_cst::CstParser;
    /// let cst = CstParser::parse(r#"{"name": "example"}"#);
    /// println!("Root kind: {}", cst.kind);
    /// ```
    pub fn parse(source: &str) -> TreeCursorSyntaxNode {
        Self::parse_as(source, SourceType::Json)
    }
    
    /// Parses source code of a specified type into a CST
    ///
    /// This is the main entry point for parsing source code into a concrete syntax tree.
    /// Uses a thread-local Parser for optimal performance.
    ///
    /// # Arguments
    /// * `source` - The source string to parse
    /// * `source_type` - The source type
    ///
    /// # Returns
    /// The parsed CST root node
    ///
    /// # Example
    /// ```
    /// use apidom_cst::{CstParser, SourceType};
    ///
    /// // Parse JSON
    /// let json_cst = CstParser::parse_as(r#"{"name": "example"}"#, SourceType::Json);
    ///
    /// // Parse YAML
    /// let yaml_cst = CstParser::parse_as("name: example", SourceType::Yaml);
    /// ```
    pub fn parse_as(source: &str, source_type: SourceType) -> TreeCursorSyntaxNode {
        with_parser(source_type, |parser| {
            // Parse the source to get a Tree
            let tree = parser.parse(source, None)
                .unwrap_or_else(|| panic!("Failed to parse {} source", source_type.display_name()));

            // Use cursor to construct our wrapper from the root node
            let mut cursor = tree.walk();
            let shared_source: Arc<str> = Arc::from(source);
            let mut root = TreeCursorSyntaxNode::from_cursor_with_shared_source(&cursor, shared_source.clone());

            // Recursively traverse all child nodes, populating children
            descend_and_build_children(&mut cursor, &shared_source, &mut root);

            root
        })
    }
    
    /// Smart parsing: attempts to automatically detect the source type
    ///
    /// Uses heuristics to detect the most likely format, then attempts to parse.
    /// If detection is incorrect, it will try the other format.
    ///
    /// # Arguments
    /// * `source` - The source string to parse
    ///
    /// # Returns
    /// The parsed CST root node and the detected source type
    ///
    /// # Example
    /// ```
    /// use apidom_cst::CstParser;
    ///
    /// let (cst, detected_type) = CstParser::parse_smart(r#"{"key": "value"}"#);
    /// println!("Detected type: {}", detected_type.display_name());
    /// ```
    pub fn parse_smart(source: &str) -> (TreeCursorSyntaxNode, SourceType) {
        // Use heuristics to detect format
        let detected_type = SourceType::detect_from_content(source);
        
        // First try the detected format
        if let Ok(tree) = Self::try_parse_as(source, detected_type) {
            return (tree, detected_type);
        }
        
        // If it fails, try the other format
        let fallback_type = match detected_type {
            SourceType::Json => SourceType::Yaml,
            SourceType::Yaml => SourceType::Json,
        };
        
        let tree = Self::parse_as(source, fallback_type);
        (tree, fallback_type)
    }
    
    /// Attempts to parse source code of a specified type without throwing exceptions
    ///
    /// # Arguments
    /// * `source` - The source string to parse
    /// * `source_type` - The source type
    ///
    /// # Returns
    /// Ok(CST) if parsing is successful, Err otherwise
    fn try_parse_as(source: &str, source_type: SourceType) -> Result<TreeCursorSyntaxNode, String> {
        with_parser(source_type, |parser| {
            let tree = match parser.parse(source, None) {
                Some(tree) => tree,
                None => {
                    return Err(format!("Failed to parse {} source", source_type.display_name()));
                }
            };
            
            // Check for syntax errors
            let root_node = tree.root_node();
            if root_node.has_error() {
                return Err(format!("{} source has syntax errors", source_type.display_name()));
            }
            
            // Construct CST
            let mut cursor = tree.walk();
            let shared_source: Arc<str> = Arc::from(source);
            let mut root = TreeCursorSyntaxNode::from_cursor_with_shared_source(&cursor, shared_source.clone());

            descend_and_build_children(&mut cursor, &shared_source, &mut root);

            Ok(root)
        })
    }
}

/// Convenience function: converts a whole JSON source into our `TreeCursorSyntaxNode` tree
///
/// This is an alias for `CstParser::parse`, retained for backward compatibility.
/// It is recommended to use `CstParser::parse` or `CstParser::parse_as`.
///
/// # Arguments
/// * `source` - JSON source string
///
/// # Returns
/// The parsed CST root node
pub fn parse_json_to_cst(source: &str) -> TreeCursorSyntaxNode {
    CstParser::parse(source)
}

/// Extends TreeCursorSyntaxNode to support builder pattern
impl TreeCursorSyntaxNode {
    /// Builder method to create a preorder traversal iterator
    ///
    /// # Returns
    /// A preorder traversal iterator
    ///
    /// # Example
    /// ```
    /// use apidom_cst::CstParser;
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let nodes: Vec<_> = cst.preorder().collect();
    /// ```
    pub fn preorder(&self) -> TreeIterator {
        self.iter_preorder()
    }
    
    /// Builder method to create a postorder traversal iterator
    ///
    /// # Returns
    /// A postorder traversal iterator
    pub fn postorder(&self) -> TreeIterator {
        self.iter_postorder()
    }
    
    /// Builder method to create a breadth-first traversal iterator
    ///
    /// # Returns
    /// A breadth-first traversal iterator
    pub fn breadth_first(&self) -> TreeIterator {
        self.iter_breadth_first()
    }
}

/// Example: demonstrates how to use the new CST features
///
/// This function showcases various features of the CST parser, including:
/// - Multi-format parsing (JSON and YAML)
/// - Smart format detection
/// - Basic parsing and error detection
/// - Lazy text extraction
/// - Field name recording
/// - Multiple traversal methods
/// - Node finding
/// - Memory optimization
///
/// # Arguments
/// * `source` - The source string to demonstrate
/// * `source_type` - Optional source type, uses smart detection if not specified
pub fn demonstrate_cst_features_multi_format(source: &str, source_type: Option<SourceType>) {
    println!("=== Multi-format CST Feature Demonstration ===");
    println!("Input source: {}", source);
    
    // 1. Parse source into CST
    let (cst, detected_type) = match source_type {
        Some(st) => {
            println!("Specified format: {}", st.display_name());
            (CstParser::parse_as(source, st), st)
        }
        None => {
            println!("Using smart detection...");
            let (cst, detected) = CstParser::parse_smart(source);
            println!("Detected format: {}", detected.display_name());
            (cst, detected)
        }
    };
    
    println!("\n1. Basic Information:");
    println!("   Format type: {}", detected_type.display_name());
    println!("   Root node type: {}", cst.kind);
    println!("   Has errors: {}", cst.has_error());
    println!("   Number of children: {}", cst.children.len());
    
    // 2. Demonstrate lazy text extraction
    println!("\n2. Lazy Text Extraction:");
    println!("   Root node text length: {} bytes", cst.text().len());
    
    // 3. Demonstrate format-specific node types
    println!("\n3. Format-specific Node Types:");
    match detected_type {
        SourceType::Json => {
            let objects = cst.find_nodes_by_kind("object");
            let arrays = cst.find_nodes_by_kind("array");
            let strings = cst.find_nodes_by_kind("string");
            let numbers = cst.find_nodes_by_kind("number");
            
            println!("   JSON Objects: {}", objects.len());
            println!("   JSON Arrays: {}", arrays.len());
            println!("   Strings: {}", strings.len());
            println!("   Numbers: {}", numbers.len());
            
            // Display string content
            if !strings.is_empty() {
                println!("   String Content:");
                for (i, string) in strings.iter().take(3).enumerate() {
                    println!("     {}: {}", i + 1, string.text());
                }
            }
        }
        SourceType::Yaml => {
            let documents = cst.find_nodes_by_kind("document");
            let block_mappings = cst.find_nodes_by_kind("block_mapping");
            let block_sequences = cst.find_nodes_by_kind("block_sequence");
            let plain_scalars = cst.find_nodes_by_kind("plain_scalar");
            let quoted_scalars = cst.find_nodes_by_kind("double_quote_scalar");
            
            println!("   YAML Documents: {}", documents.len());
            println!("   Block Mappings: {}", block_mappings.len());
            println!("   Block Sequences: {}", block_sequences.len());
            println!("   Plain Scalars: {}", plain_scalars.len());
            println!("   Quoted Scalars: {}", quoted_scalars.len());
            
            // Display scalar content
            if !plain_scalars.is_empty() {
                println!("   Scalar Content:");
                for (i, scalar) in plain_scalars.iter().take(3).enumerate() {
                    println!("     {}: {}", i + 1, scalar.text());
                }
            }
        }
    }
    
    // 4. Demonstrate builder-style iterator traversal
    println!("\n4. Preorder Traversal of First 10 Nodes:");
    for (i, node) in cst.preorder().take(10).enumerate() {
        let error_mark = if node.has_error() { " [ERROR]" } else { "" };
        println!("   {}: {} ({}..{}){}", 
                 i + 1, node.kind, node.start_byte, node.end_byte, error_mark);
    }
    
    // 5. Demonstrate field name recording (primarily for JSON)
    if detected_type == SourceType::Json {
        println!("\n5. JSON Field Name Recording:");
        let pairs = cst.find_nodes_by_kind("pair");
        for (i, pair) in pairs.iter().take(3).enumerate() {
            println!("   Pair {}: {}", i + 1, pair.text().chars().take(50).collect::<String>());
            for child in &pair.children {
                if let Some(field_name) = child.field_name() {
                    let text_preview = child.text().chars().take(30).collect::<String>();
                    println!("     - {}: {}", field_name, text_preview);
                }
            }
        }
    }
    
    // 6. Error detection
    println!("\n6. Error Detection:");
    fn find_errors(node: &TreeCursorSyntaxNode, path: &str, count: &mut usize) {
        if *count >= 5 { return; } // Limit output quantity
        if node.has_error() {
            println!("   Error node: {} at {}", node.kind, path);
            *count += 1;
        }
        for (i, child) in node.children.iter().enumerate() {
            find_errors(child, &format!("{}.{}", path, i), count);
        }
    }
    let mut error_count = 0;
    find_errors(&cst, "root", &mut error_count);
    if error_count == 0 {
        println!("   ✓ No syntax errors found");
    }
    
    // 7. Demonstrate shared source optimization
    println!("\n7. Memory Optimization:");
    println!("   Source sharing: All nodes share the same source, reducing memory usage");
    println!("   Arc reference count: {}", std::sync::Arc::strong_count(cst.shared_source()));
    
    // 8. Performance statistics
    println!("\n8. Performance Statistics:");
    let total_nodes = cst.preorder().count();
    println!("   Total number of nodes: {}", total_nodes);
    println!("   Average node size: {:.1} bytes", source.len() as f64 / total_nodes as f64);
}

/// Backward-compatible demonstration function (default JSON)
///
/// # Arguments
/// * `json_source` - The JSON string to demonstrate
pub fn demonstrate_cst_features(json_source: &str) {
    demonstrate_cst_features_multi_format(json_source, Some(SourceType::Json));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    /// Test the new builder API
    #[test]
    fn test_builder_api() {
        let src = r#"{ "foo": [1, 2, 3] }"#;
        
        // Test basic parsing
        let cst = CstParser::parse(src);
        assert!(!cst.children.is_empty());
        
        // Test builder-style traversal
        let preorder_count = cst.preorder().count();
        let postorder_count = cst.postorder().count();
        let breadth_first_count = cst.breadth_first().count();
        
        // All traversal methods should visit the same number of nodes
        assert_eq!(preorder_count, postorder_count);
        assert_eq!(postorder_count, breadth_first_count);
        
        // Test chained operations
        let strings: Vec<_> = cst.find_nodes_by_kind("string")
            .into_iter()
            .map(|node| node.text())
            .collect();
        assert!(!strings.is_empty());
    }

    /// A very simple JSON, test that the top level can always be parsed into a node with children,
    /// and the first child is of type `object`.
    #[test]
    fn test_parse_object() {
        let src = r#"{ "foo": 42 }"#;
        let cst = CstParser::parse(src);

        // The top level must have children
        assert!(!cst.children.is_empty(), "root should have children");

        // The first child is a JSON object
        let obj = &cst.children[0];
        assert_eq!(obj.kind, "object");
        // And its text should exactly match the original text with outer whitespace removed
        assert_eq!(obj.text().trim(), r#"{ "foo": 42 }"#);

        // The object should have at least one pair
        assert!(obj.children.iter().any(|n| n.kind == "pair"), "object should contain a pair");
    }

    /// Test arrays
    #[test]
    fn test_parse_array() {
        let src = r#"[1, true, null]"#;
        let cst = CstParser::parse(src);
        let arr = &cst.children[0];
        assert_eq!(arr.kind, "array");

        // The array has 3 elements
        // In tree-sitter-json CST, literal nodes are usually direct children
        let literal_kinds: Vec<_> = arr
            .children
            .iter()
            // Filter out numbers, true, null
            .filter(|n| ["number", "true", "null"].contains(&n.kind.as_str()))
            .map(|n| &n.kind)
            .collect();
        assert_eq!(literal_kinds, &["number", "true", "null"]);
    }

    /// Nested test
    #[test]
    fn test_nested() {
        let src = r#"{ "a": [ { "b": "c" } ] }"#;
        let cst = CstParser::parse(src);

        // Path cst.children[0] → "object"
        let obj = &cst.children[0];
        // It should have a pair
        let pair = obj
            .children
            .iter()
            .find(|n| n.kind == "pair")
            .expect("object must contain a pair");
        // The value of the pair should be an array
        // Since `from_cursor` only records its text and kind, the actual value node is in children
        let array_node = pair
            .children
            .iter()
            .find(|n| n.kind == "array")
            .expect("pair should contain an array");
        assert!(!array_node.children.is_empty());

        // There should be an inner object at the deepest level
        let inner_obj = array_node
            .children
            .iter()
            .find(|n| n.kind == "object")
            .expect("array should contain an object");
        assert!(inner_obj
            .children
            .iter()
            .any(|n| n.kind == "pair" && n.text().contains("\"b\"")));
    }

    /// Test field name recording
    #[test]
    fn test_field_names() {
        let src = r#"{ "key": "value" }"#;
        let cst = CstParser::parse(src);
        let obj = &cst.children[0];
        
        // Find the pair node
        let pair = obj.children.iter().find(|n| n.kind == "pair").unwrap();
        
        // The pair's children should have key and value fields
        let key_node = pair.children.iter().find(|n| n.field_name() == Some("key"));
        let value_node = pair.children.iter().find(|n| n.field_name() == Some("value"));
        
        assert!(key_node.is_some(), "Should have key field");
        assert!(value_node.is_some(), "Should have value field");
        
        if let Some(key) = key_node {
            assert_eq!(key.text().as_ref(), r#""key""#);
        }
        if let Some(value) = value_node {
            assert_eq!(value.text().as_ref(), r#""value""#);
        }
    }

    /// Test error node handling
    #[test]
    fn test_error_handling() {
        let src = r#"{ "incomplete": }"#; // Intentional syntax error
        let cst = CstParser::parse(src);
        
        // Should detect errors
        fn has_error_in_tree(node: &TreeCursorSyntaxNode) -> bool {
            if node.has_error() {
                return true;
            }
            node.children.iter().any(has_error_in_tree)
        }
        
        assert!(has_error_in_tree(&cst), "Should detect syntax error in malformed JSON");
    }

    /// Test iterator traversal
    #[test]
    fn test_iterators() {
        let src = r#"{ "a": [1, 2] }"#;
        let cst = CstParser::parse(src);
        
        // Test preorder traversal
        let preorder_kinds: Vec<String> = cst
            .preorder()
            .map(|node| node.kind.clone())
            .collect();
        
        // Preorder traversal should visit parent nodes before child nodes
        assert!(preorder_kinds.contains(&"document".to_string()));
        assert!(preorder_kinds.contains(&"object".to_string()));
        assert!(preorder_kinds.contains(&"array".to_string()));
        
        // Test postorder traversal
        let postorder_kinds: Vec<String> = cst
            .postorder()
            .map(|node| node.kind.clone())
            .collect();
        
        // Postorder traversal should visit child nodes before parent nodes
        assert!(postorder_kinds.contains(&"document".to_string()));
        assert!(postorder_kinds.contains(&"object".to_string()));
        
        // Test breadth-first traversal
        let breadth_first_kinds: Vec<String> = cst
            .breadth_first()
            .map(|node| node.kind.clone())
            .collect();
        
        assert!(breadth_first_kinds.contains(&"document".to_string()));
        assert!(breadth_first_kinds.contains(&"object".to_string()));
        
        // Test finding specific nodes
        let numbers = cst.find_nodes_by_kind("number");
        assert_eq!(numbers.len(), 2); // Should find 1 and 2
    }

    /// Test whitespace and comment handling
    #[test]
    fn test_whitespace_and_comments() {
        // JSON standard does not support comments, but we test whitespace handling
        let src = r#"
        {
            "key"  :   "value"  
        }
        "#;
        let cst = CstParser::parse(src);
        let obj = &cst.children[0];
        assert_eq!(obj.kind, "object");
        
        // Should parse correctly even with extra whitespace
        let pair = obj.children.iter().find(|n| n.kind == "pair").unwrap();
        assert!(pair.children.iter().any(|n| n.text().trim() == r#""key""#));
    }

    /// Test deep nesting performance
    #[test]
    fn test_deep_nesting() {
        // Create deeply nested JSON
        let mut src = String::new();
        let depth = 100;
        
        // Construct deeply nested array
        for _ in 0..depth {
            src.push('[');
        }
        src.push_str("42");
        for _ in 0..depth {
            src.push(']');
        }
        
        let start = std::time::Instant::now();
        let cst = CstParser::parse(&src);
        let duration = start.elapsed();
        
        // Verify parsing success and reasonable performance (should complete in a few milliseconds)
        assert!(!cst.children.is_empty());
        assert!(duration.as_millis() < 100, "Deep nesting should parse quickly");
        
        // Verify nesting depth
        fn count_depth(node: &TreeCursorSyntaxNode) -> usize {
            if node.children.is_empty() {
                1
            } else {
                1 + node.children.iter().map(count_depth).max().unwrap_or(0)
            }
        }
        
        let actual_depth = count_depth(&cst);
        assert!(actual_depth > 50, "Should handle deep nesting");
    }

    /// Test error handling for incomplete JSON
    #[test]
    fn test_incomplete_json_cases() {
        let test_cases = vec![
            r#"{ "key": }"#,           // Missing value
            r#"{ "key" "value" }"#,    // Missing colon
            r#"{ "key": "value" "#,    // Missing closing brace
            r#"[1, 2, ]"#,            // trailing comma
            r#""unterminated string"#, // Unterminated string
        ];
        
        for (i, src) in test_cases.iter().enumerate() {
            let cst = CstParser::parse(src);
            
            // Check for error detection
            fn has_error_recursive(node: &TreeCursorSyntaxNode) -> bool {
                node.has_error() || node.children.iter().any(has_error_recursive)
            }
            
            assert!(
                has_error_recursive(&cst),
                "Test case {} should detect error in: {}",
                i, src
            );
        }
    }

    /// Test special characters and Unicode
    #[test]
    fn test_special_characters() {
        let src = r#"{ "emoji": "🚀", "chinese": "你好", "escape": "line1\nline2" }"#;
        let cst = CstParser::parse(src);
        let obj = &cst.children[0];
        
        // Should handle Unicode characters correctly
        let pairs = obj.find_nodes_by_kind("pair");
        assert_eq!(pairs.len(), 3);
        
        // Verify correct extraction of text containing special characters
        let emoji_pair = pairs.iter()
            .find(|p| p.text().contains("emoji"))
            .expect("Should find emoji pair");
        assert!(emoji_pair.text().contains("🚀"));
    }

    /// Test edge cases
    #[test]
    fn test_edge_cases() {
        let test_cases = vec![
            ("", "Empty string"),
            ("{}", "Empty object"),
            ("[]", "Empty array"),
            ("null", "Null value"),
            ("true", "Boolean true"),
            ("false", "Boolean false"),
            ("0", "Number 0"),
            (r#""""#, "Empty string"),
            (r#"{"":""}"#, "Empty key and value"),
        ];
        
        for (src, description) in test_cases {
            let cst = CstParser::parse(src);
            
            // All of these should be valid JSON, with no errors
            fn has_error_recursive(node: &TreeCursorSyntaxNode) -> bool {
                node.has_error() || node.children.iter().any(has_error_recursive)
            }
            
            if !src.is_empty() {
                assert!(
                    !has_error_recursive(&cst),
                    "{} should parse without errors",
                    description
                );
            }
        }
    }

    /// Test demonstration functionality
    #[test]
    fn test_demonstration() {
        let json = r#"{ "name": "CST Demo", "version": 1.0, "features": ["parsing", "iteration"] }"#;
        
        // This test mainly ensures the demonstration function does not panic
        // In actual use, this function will print to stdout
        demonstrate_cst_features(json);
        
        // Verify basic functionality still works
        let cst = CstParser::parse(json);
        assert!(!cst.has_error());
        assert!(!cst.children.is_empty());
    }

    /// Test extremely long strings and extreme Unicode
    #[test]
    fn test_extreme_cases() {
        // Test extremely long strings
        let long_string = "a".repeat(10000);
        let long_json = format!(r#"{{"key": "{}"}}"#, long_string);
        let cst = CstParser::parse(&long_json);
        assert!(!cst.has_error());
        
        // Verify correct handling of long strings
        let strings = cst.find_nodes_by_kind("string_content");
        let long_content = strings.iter().find(|s| s.text().len() > 5000);
        assert!(long_content.is_some(), "Should handle very long strings");
        
        // Test extreme Unicode escapes
        let unicode_json = r#"{"emoji": "\ud83d\ude80", "chinese": "\u4f60\u597d", "complex": "\ud83c\udf08\ud83e\udd84"}"#;
        let cst = CstParser::parse(unicode_json);
        assert!(!cst.has_error());
        
        // Test various escape characters
        let escape_json = r#"{"escapes": "\"\\\/\b\f\n\r\t"}"#;
        let cst = CstParser::parse(escape_json);
        assert!(!cst.has_error());
    }

    /// Test extreme nesting (stress test)
    #[test]
    fn test_extreme_nesting() {
        // Test extremely deep array nesting
        let depth = 1000;
        let mut json = String::new();
        for _ in 0..depth {
            json.push('[');
        }
        json.push_str("null");
        for _ in 0..depth {
            json.push(']');
        }
        
        let start = std::time::Instant::now();
        let cst = CstParser::parse(&json);
        let duration = start.elapsed();
        
        assert!(!cst.has_error());
        assert!(duration.as_millis() < 1000, "Should handle extreme nesting efficiently");
        
        // Test extremely deep object nesting
        let mut obj_json = String::new();
        for i in 0..500 {
            obj_json.push_str(&format!(r#"{{"level{}":"#, i));
        }
        obj_json.push_str("\"value\"");
        for _ in 0..500 {
            obj_json.push('}');
        }
        
        let cst = CstParser::parse(&obj_json);
        assert!(!cst.has_error());
    }

    /// Test concurrency safety
    #[test]
    fn test_concurrent_parsing() {
        use std::thread;
        use std::sync::Arc;
        
        let test_cases = vec![
            r#"{"thread": 1, "data": [1, 2, 3]}"#,
            r#"{"thread": 2, "data": {"nested": true}}"#,
            r#"{"thread": 3, "data": "string value"}"#,
            r#"{"thread": 4, "data": null}"#,
        ];
        
        let test_cases = Arc::new(test_cases);
        let mut handles = vec![];
        
        // Launch multiple threads to parse simultaneously
        for i in 0..4 {
            let cases = test_cases.clone();
            let handle = thread::spawn(move || {
                let json = &cases[i];
                let cst = CstParser::parse(json);
                assert!(!cst.has_error());
                
                // Verify each thread has its own parser
                let objects = cst.find_nodes_by_kind("object");
                assert!(!objects.is_empty());
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Test memory efficiency (Arc sharing)
    #[test]
    fn test_memory_efficiency() {
        let json = r#"{"large": "data", "with": ["many", "nested", {"objects": true}]}"#;
        let cst = CstParser::parse(json);
        
        // Verify all nodes share the same source
        let initial_count = std::sync::Arc::strong_count(cst.shared_source());
        
        // Traverse all nodes, they should all share the same Arc
        let mut node_count = 0;
        for _node in cst.preorder() {
            node_count += 1;
        }
        
        // Arc reference count should equal the number of nodes + 1 (root node)
        assert!(node_count > 10, "Should have multiple nodes");
        assert!(initial_count > 1, "Source should be shared among nodes");
        
        println!("Number of nodes: {}, Arc reference count: {}", node_count, initial_count);
    }

    /// Test error recovery capability
    #[test]
    fn test_error_recovery() {
        let malformed_cases = vec![
            (r#"{"key": value}"#, "Unquoted value"),
            (r#"{"key": "value",}"#, "Trailing comma"),
            (r#"{key: "value"}"#, "Unquoted key"),
            (r#"{"key": "value" "another": "value"}"#, "Missing comma"),
            (r#"[1, 2, 3,]"#, "Array trailing comma"),
            (r#"{"nested": {"incomplete": }"#, "Incomplete nesting"),
        ];
        
        for (json, description) in malformed_cases {
            let cst = CstParser::parse(json);
            
            // Should detect errors but not crash
            fn has_any_error(node: &TreeCursorSyntaxNode) -> bool {
                if node.has_error() {
                    return true;
                }
                node.children.iter().any(has_any_error)
            }
            
            assert!(has_any_error(&cst), "Should detect error: {}", description);
            
            // Even with errors, should be able to traverse the tree structure
            let node_count = cst.preorder().count();
            assert!(node_count > 0, "Should be able to construct partial tree structure even with errors");
        }
    }

    /// Test SourceType enum functionality
    #[test]
    fn test_source_type() {
        // Test inferring type from extension
        assert_eq!(SourceType::from_extension("json"), Some(SourceType::Json));
        assert_eq!(SourceType::from_extension("JSON"), Some(SourceType::Json));
        assert_eq!(SourceType::from_extension("yaml"), Some(SourceType::Yaml));
        assert_eq!(SourceType::from_extension("yml"), Some(SourceType::Yaml));
        assert_eq!(SourceType::from_extension("YAML"), Some(SourceType::Yaml));
        assert_eq!(SourceType::from_extension("txt"), None);
        assert_eq!(SourceType::from_extension(""), None);
        
        // Test display name
        assert_eq!(SourceType::Json.display_name(), "JSON");
        assert_eq!(SourceType::Yaml.display_name(), "YAML");
        
        // Test equality
        assert_eq!(SourceType::Json, SourceType::Json);
        assert_eq!(SourceType::Yaml, SourceType::Yaml);
        assert_ne!(SourceType::Json, SourceType::Yaml);
    }

    /// Test JSON parsing functionality
    #[test]
    fn test_json_parsing() {
        let json_src = r#"{"name": "test", "values": [1, 2, 3], "nested": {"key": "value"}}"#;
        
        // Use default method (should be JSON)
        let cst1 = CstParser::parse(json_src);
        assert!(!cst1.has_error());
        
        // Explicitly specify JSON
        let cst2 = CstParser::parse_as(json_src, SourceType::Json);
        assert!(!cst2.has_error());
        
        // Verify structure
        assert!(!cst2.children.is_empty());
        let obj = &cst2.children[0];
        assert_eq!(obj.kind, "object");
        
        // Find specific nodes
        let strings = cst2.find_nodes_by_kind("string");
        assert!(!strings.is_empty());
        
        let numbers = cst2.find_nodes_by_kind("number");
        assert_eq!(numbers.len(), 3); // 1, 2, 3
    }

    /// Test YAML parsing functionality
    #[test]
    fn test_yaml_parsing() {
        let yaml_src = r#"
name: test
values:
  - 1
  - 2
  - 3
nested:
  key: value
"#;
        
        // Explicitly specify YAML
        let cst = CstParser::parse_as(yaml_src, SourceType::Yaml);
        assert!(!cst.has_error());
        
        // Verify root node type
        assert_eq!(cst.kind, "stream");
        
        // YAML should have document child nodes
        let documents = cst.find_nodes_by_kind("document");
        assert!(!documents.is_empty());
        
        // Find YAML-specific node types
        let block_mappings = cst.find_nodes_by_kind("block_mapping");
        assert!(!block_mappings.is_empty());
        
        let block_sequences = cst.find_nodes_by_kind("block_sequence");
        assert!(!block_sequences.is_empty());
    }

    /// Test smart parsing functionality
    #[test]
    fn test_smart_parsing() {
        // Test JSON detection
        let json_src = r#"{"key": "value"}"#;
        let (cst, detected_type) = CstParser::parse_smart(json_src);
        assert_eq!(detected_type, SourceType::Json);
        assert!(!cst.has_error());
        
        // Test YAML detection (when JSON parsing fails)
        let yaml_src = "key: value\nlist:\n  - item1\n  - item2";
        let (cst, detected_type) = CstParser::parse_smart(yaml_src);
        assert_eq!(detected_type, SourceType::Yaml);
        assert!(!cst.has_error());
        
        // Test obvious JSON (with braces)
        let json_array = r#"[1, 2, 3]"#;
        let (cst, detected_type) = CstParser::parse_smart(json_array);
        assert_eq!(detected_type, SourceType::Json);
        assert!(!cst.has_error());
    }

    /// Test concurrent multi-format parsing
    #[test]
    fn test_concurrent_multi_format_parsing() {
        use std::thread;
        use std::sync::Arc;
        
        let test_cases = vec![
            (r#"{"json": "data"}"#, SourceType::Json),
            ("yaml: data", SourceType::Yaml),
            (r#"[1, 2, 3]"#, SourceType::Json),
            ("list:\n  - item1\n  - item2", SourceType::Yaml),
        ];
        
        let test_cases = Arc::new(test_cases);
        let mut handles = vec![];
        
        // Launch multiple threads to parse different formats simultaneously
        for i in 0..4 {
            let cases = test_cases.clone();
            let handle = thread::spawn(move || {
                let (source, source_type) = &cases[i];
                let cst = CstParser::parse_as(source, *source_type);
                assert!(!cst.has_error());
                
                // Verify parsing results
                assert!(!cst.children.is_empty());
            });
            handles.push(handle);
        }
        
        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Test YAML-specific features
    #[test]
    fn test_yaml_specific_features() {
        // Test multi-document YAML
        let multi_doc_yaml = r#"
---
doc1: value1
---
doc2: value2
"#;
        
        let cst = CstParser::parse_as(multi_doc_yaml, SourceType::Yaml);
        assert!(!cst.has_error());
        
        // Should have multiple document nodes
        let documents = cst.find_nodes_by_kind("document");
        assert!(documents.len() >= 2);
        
        // Test YAML list syntax
        let yaml_list = r#"
items:
  - name: item1
    value: 100
  - name: item2
    value: 200
"#;
        
        let cst = CstParser::parse_as(yaml_list, SourceType::Yaml);
        assert!(!cst.has_error());
        
        let block_sequences = cst.find_nodes_by_kind("block_sequence");
        assert!(!block_sequences.is_empty());
        
        let block_mappings = cst.find_nodes_by_kind("block_mapping");
        assert!(!block_mappings.is_empty());
    }

    /// Test format error handling
    #[test]
    fn test_format_error_handling() {
        // Test invalid JSON
        let invalid_json = r#"{"key": value}"#; // Unquoted value
        let cst = CstParser::parse_as(invalid_json, SourceType::Json);
        
        // Should detect errors
        fn has_error_recursive(node: &TreeCursorSyntaxNode) -> bool {
            node.has_error() || node.children.iter().any(has_error_recursive)
        }
        assert!(has_error_recursive(&cst));
        
        // Test invalid YAML
        let invalid_yaml = "key: value\n  invalid indentation";
        let cst = CstParser::parse_as(invalid_yaml, SourceType::Yaml);
        
        // YAML parsers are usually more forgiving, but should still handle
        // Even with errors, should be able to construct partial tree
        assert!(!cst.children.is_empty());
    }

    /// Test builder API with multi-format support
    #[test]
    fn test_builder_api_multi_format() {
        // JSON builder style
        let json_src = r#"{"items": ["a", "b", "c"]}"#;
        let json_cst = CstParser::parse_as(json_src, SourceType::Json);
        
        let json_strings: Vec<_> = json_cst
            .preorder()
            .filter(|node| node.kind == "string")
            .map(|node| node.text())
            .collect();
        assert!(!json_strings.is_empty());
        
        // YAML builder style
        let yaml_src = r#"
items:
  - a
  - b
  - c
"#;
        let yaml_cst = CstParser::parse_as(yaml_src, SourceType::Yaml);
        
        let yaml_scalars: Vec<_> = yaml_cst
            .breadth_first()
            .filter(|node| node.kind == "plain_scalar")
            .map(|node| node.text())
            .collect();
        assert!(!yaml_scalars.is_empty());
    }

    /// Test advanced YAML features and edge cases
    #[test]
    fn test_yaml_advanced_features() {
        // Test multi-document YAML with comments
        let multi_doc_with_comments = r#"
# First document
---
name: "Document 1"
items:
  - item1  # Inline comment
  - item2
  # This is a comment line
metadata:
  version: 1.0

# Second document
---
name: "Document 2"
items:
  - item3
  - item4
metadata:
  version: 2.0
"#;
        
        let cst = CstParser::parse_as(multi_doc_with_comments, SourceType::Yaml);
        assert!(!cst.has_error());
        
        // Should have multiple document nodes
        let documents = cst.find_nodes_by_kind("document");
        assert!(documents.len() >= 2, "Should have at least 2 documents");
        
        // Test comment nodes
        let comments = cst.find_nodes_by_kind("comment");
        assert!(!comments.is_empty(), "Should find comment nodes");
        
        // Test complex indentation and nesting
        let complex_yaml = r#"
root:
  level1:
    level2:
      - array_item1:
          nested_key: value1
      - array_item2:
          nested_key: value2
    another_level2:
      key: value
  another_level1:
    - simple_item
    - complex_item:
        sub_key: sub_value
"#;
        
        let cst = CstParser::parse_as(complex_yaml, SourceType::Yaml);
        assert!(!cst.has_error());
        
        let block_mappings = cst.find_nodes_by_kind("block_mapping");
        assert!(!block_mappings.is_empty(), "Should have block mappings");
        
        let block_sequences = cst.find_nodes_by_kind("block_sequence");
        assert!(!block_sequences.is_empty(), "Should have block sequences");
    }
    
    /// Test various scalar types in YAML
    #[test]
    fn test_yaml_scalar_types() {
        let yaml_scalars = r#"
string_plain: plain string
string_quoted: "quoted string"
string_single: 'single quoted'
number_int: 42
number_float: 3.14
boolean_true: true
boolean_false: false
null_value: null
empty_value: 
multiline: |
  This is a multiline
  string using literal
  block scalar style
folded: >
  This is a folded
  string that will be
  joined on a single line
"#;
        
        let cst = CstParser::parse_as(yaml_scalars, SourceType::Yaml);
        assert!(!cst.has_error());
        
        // Test different types of scalars (based on actual node types)
        let plain_scalars = cst.find_nodes_by_kind("plain_scalar");
        assert!(!plain_scalars.is_empty(), "Should have plain scalars");
        
        let double_quoted_scalars = cst.find_nodes_by_kind("double_quote_scalar");
        assert!(!double_quoted_scalars.is_empty(), "Should have double-quoted scalars");
        
        let single_quoted_scalars = cst.find_nodes_by_kind("single_quote_scalar");
        assert!(!single_quoted_scalars.is_empty(), "Should have single-quoted scalars");
        
        let block_scalars = cst.find_nodes_by_kind("block_scalar");
        assert!(!block_scalars.is_empty(), "Should have block scalars");
        
        let integer_scalars = cst.find_nodes_by_kind("integer_scalar");
        assert!(!integer_scalars.is_empty(), "Should have integer scalars");
        
        let float_scalars = cst.find_nodes_by_kind("float_scalar");
        assert!(!float_scalars.is_empty(), "Should have float scalars");
        
        let boolean_scalars = cst.find_nodes_by_kind("boolean_scalar");
        assert!(!boolean_scalars.is_empty(), "Should have boolean scalars");
        
        let null_scalars = cst.find_nodes_by_kind("null_scalar");
        assert!(!null_scalars.is_empty(), "Should have null scalars");
    }
    
    /// Test YAML error recovery and edge cases
    #[test]
    fn test_yaml_error_cases() {
        let problematic_cases = vec![
            // Inconsistent indentation
            ("inconsistent_indent", r#"
items:
  - item1
    - item2  # Incorrect indentation
"#),
            // Mixed tabs and spaces
            ("mixed_tabs_spaces", "items:\n\t- item1\n  - item2"),
            // Unterminated quote
            ("unterminated_quote", r#"key: "unterminated string"#),
            // Invalid key-value pair
            ("invalid_mapping", "key1: value1\nkey2 value2"),  // Missing colon
        ];
        
        for (description, yaml) in problematic_cases {
            let cst = CstParser::parse_as(yaml, SourceType::Yaml);
            
            // YAML parsers are usually more forgiving, but should still handle
            // Even with errors, should be able to construct partial tree
            assert!(!cst.children.is_empty(), "Should construct partial tree even with errors: {}", description);
            
            // Can check for error nodes
            fn has_any_error(node: &TreeCursorSyntaxNode) -> bool {
                if node.has_error() {
                    return true;
                }
                node.children.iter().any(has_any_error)
            }
            
            // Some cases may detect errors
            let has_error = has_any_error(&cst);
            println!("YAML case '{}' error detection: {}", description, has_error);
        }
    }
    
    /// Test accuracy of smart detection
    #[test]
    fn test_smart_detection_accuracy() {
        let test_cases = vec![
            // Obvious JSON cases
            (r#"{"json": true}"#, SourceType::Json, "JSON object"),
            (r#"[1, 2, 3]"#, SourceType::Json, "JSON array"),
            (r#"{"nested": {"deep": "value"}}"#, SourceType::Json, "Nested JSON"),
            
            // Obvious YAML cases  
            ("key: value", SourceType::Yaml, "Simple YAML mapping"),
            ("---\nkey: value", SourceType::Yaml, "YAML document separator"),
            ("- item1\n- item2", SourceType::Yaml, "YAML list"),
            ("key:\n  nested: value", SourceType::Yaml, "Nested YAML"),
            
            // Edge cases
            ("key:value", SourceType::Yaml, "YAML without space"),
            ("# Pure comment\nkey: value", SourceType::Yaml, "YAML with comment"),
            (r#"{"key":123}"#, SourceType::Json, "Compact JSON"),
        ];
        
        for (source, expected_type, description) in test_cases {
            let detected_type = SourceType::detect_from_content(source);
            assert_eq!(
                detected_type, expected_type,
                "Detection failed: {} - Expected {}, got {}",
                description, expected_type.display_name(), detected_type.display_name()
            );
            
            // Verify smart parsing also handles correctly
            let (cst, smart_detected) = CstParser::parse_smart(source);
            assert!(!cst.has_error(), "Smart parsing failed: {}", description);
            
            // Smart parsing result should match heuristic detection or be a fallback
            if smart_detected != expected_type {
                println!("Smart parsing used fallback: {} -> {}", 
                        expected_type.display_name(), smart_detected.display_name());
            }
        }
    }
    
    /// Test memory optimization effects
    #[test]
    fn test_memory_optimization_arc_str() {
        let large_json = format!(r#"{{
  "description": "Test Arc<str> memory optimization",
  "data": [{}],
  "metadata": {{
    "count": {},
    "memory_optimized": true
  }}
}}"#, 
            (0..50).map(|i| format!(r#"{{"id": {}, "name": "item{}", "value": "{}"}}"#, i, i, "x".repeat(10)))
                    .collect::<Vec<_>>().join(", "),
            50
        );
        
        let cst = CstParser::parse(&large_json);
        
        // Verify all nodes share the same Arc<str>
        let source_arc = cst.shared_source();
        let initial_count = Arc::strong_count(source_arc);
        
        // Traverse all nodes, verify they all share the same source
        let mut node_count = 0;
        let mut shared_count = 0;
        
        for node in cst.iter_preorder() {
            node_count += 1;
            if Arc::ptr_eq(node.shared_source(), source_arc) {
                shared_count += 1;
            }
        }
        
        assert_eq!(node_count, shared_count, "All nodes should share the same source");
        assert!(initial_count > 10, "Arc reference count should reflect node count");
        
        // Verify zero-copy text extraction with Arc<str>
        let strings = cst.find_nodes_by_kind("string");
        for string_node in strings.iter().take(5) {
            let text = string_node.text();
            // text() should return Cow::Borrowed, indicating zero-copy
            match text {
                Cow::Borrowed(_) => {
                    // This is what we want: zero-copy
                }
                Cow::Owned(_) => {
                    panic!("Unexpected copy occurred: {}", text);
                }
            }
        }
        
        println!("Memory optimization verification: {} nodes share the same source", node_count);
        println!("Arc<str> reference count: {}", initial_count);
    }
}