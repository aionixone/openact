use tree_sitter::TreeCursor;
use std::borrow::Cow;
use std::sync::Arc;

/// Enumeration for traversal strategies
///
/// Defines different orders for tree traversal. Note: In-order traversal is only applicable to binary trees,
/// and there is no clear definition for general N-ary trees, so it is not included in this enumeration.
#[derive(Debug, Clone, Copy)]
pub enum TraversalOrder {
    /// Pre-order traversal: Parent node -> Child nodes
    PreOrder,
    /// Post-order traversal: Child nodes -> Parent node
    PostOrder,
    /// Breadth-first traversal: Level-order traversal
    BreadthFirst,
}

/// Lightweight tree traversal iterator
///
/// Uses direct matching instead of trait objects to reduce virtual call overhead.
/// Supports zero-copy traversal, with all node references pointing to the original tree structure.
pub struct TreeIterator<'a> {
    order: TraversalOrder,
    stack: Vec<&'a TreeCursorSyntaxNode>,
    queue: std::collections::VecDeque<&'a TreeCursorSyntaxNode>,
    post_order_stack: Vec<(&'a TreeCursorSyntaxNode, usize)>,
}

impl<'a> TreeIterator<'a> {
    /// Creates a pre-order traversal iterator
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::{CstParser, TreeIterator};
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let iter = TreeIterator::new_preorder(&cst);
    /// for node in iter {
    ///     println!("Node: {}", node.kind);
    /// }
    /// ```
    pub fn new_preorder(root: &'a TreeCursorSyntaxNode) -> Self {
        TreeIterator {
            order: TraversalOrder::PreOrder,
            stack: vec![root],
            queue: std::collections::VecDeque::new(),
            post_order_stack: Vec::new(),
        }
    }
    
    /// Creates a post-order traversal iterator
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::{CstParser, TreeIterator};
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let iter = TreeIterator::new_postorder(&cst);
    /// for node in iter {
    ///     println!("Node: {}", node.kind);
    /// }
    /// ```
    pub fn new_postorder(root: &'a TreeCursorSyntaxNode) -> Self {
        TreeIterator {
            order: TraversalOrder::PostOrder,
            stack: Vec::new(),
            queue: std::collections::VecDeque::new(),
            post_order_stack: vec![(root, 0)],
        }
    }
    
    /// Creates a breadth-first traversal iterator
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::{CstParser, TreeIterator};
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let iter = TreeIterator::new_breadth_first(&cst);
    /// for node in iter {
    ///     println!("Node: {}", node.kind);
    /// }
    /// ```
    pub fn new_breadth_first(root: &'a TreeCursorSyntaxNode) -> Self {
        TreeIterator {
            order: TraversalOrder::BreadthFirst,
            stack: Vec::new(),
            queue: std::collections::VecDeque::from([root]),
            post_order_stack: Vec::new(),
        }
    }
}

impl<'a> Iterator for TreeIterator<'a> {
    type Item = &'a TreeCursorSyntaxNode;
    
    fn next(&mut self) -> Option<Self::Item> {
        match self.order {
            TraversalOrder::PreOrder => {
                if let Some(node) = self.stack.pop() {
                    // Push child nodes onto the stack in reverse order (so they pop in order)
                    for child in node.children.iter().rev() {
                        self.stack.push(child);
                    }
                    Some(node)
                } else {
                    None
                }
            }
            TraversalOrder::PostOrder => {
                while let Some((node, child_idx)) = self.post_order_stack.pop() {
                    if child_idx >= node.children.len() {
                        return Some(node);
                    } else {
                        self.post_order_stack.push((node, child_idx + 1));
                        self.post_order_stack.push((&node.children[child_idx], 0));
                    }
                }
                None
            }
            TraversalOrder::BreadthFirst => {
                if let Some(node) = self.queue.pop_front() {
                    for child in &node.children {
                        self.queue.push_back(child);
                    }
                    Some(node)
                } else {
                    None
                }
            }
        }
    }
}

/// CST node wrapper
///
/// This is our wrapper for tree-sitter nodes, providing additional features:
/// - Zero-copy text extraction
/// - Shared source reference (Arc<str>)
/// - Multiple traversal methods
/// - Field name recording
/// - Error detection
/// - Memory optimization (using Arc<str> instead of Arc<Vec<u8>>)
#[derive(Debug, Clone)]
pub struct TreeCursorSyntaxNode {
    /// Node type (e.g., "object", "array", "string", etc.)
    pub kind: String,
    /// Start byte position of the node in the source code
    pub start_byte: usize,
    /// End byte position of the node in the source code
    pub end_byte: usize,
    /// Start position of the node in the source code (line and column)
    pub start_point: tree_sitter::Point,
    /// End position of the node in the source code (line and column)
    pub end_point: tree_sitter::Point,
    /// Whether the node is named (not a symbol node)
    pub named: bool,
    /// Whether the node contains syntax errors
    pub error: bool,
    /// Field name (if this node is the value of a field)
    pub field_name: Option<String>,
    
    /// Shared source using Arc<str>, more efficient than Arc<Vec<u8>>
    /// Allows direct string slicing, avoiding UTF-8 conversion
    source: Arc<str>,
    
    /// List of child nodes
    /// Consider implementing lazy loading in the future, constructing only on first access
    pub children: Vec<TreeCursorSyntaxNode>,
}

impl TreeCursorSyntaxNode {
    /// Constructs a node from a TreeCursor (creates a new Arc source)
    ///
    /// # Arguments
    /// * `cursor` - tree-sitter cursor
    /// * `src` - source code byte array
    ///
    /// # Returns
    /// Newly constructed CST node
    pub fn from_cursor(cursor: &TreeCursor, src: &[u8]) -> Self {
        let source_str = String::from_utf8_lossy(src);
        let shared_source = Arc::from(source_str.as_ref());
        Self::from_cursor_with_shared_source(cursor, shared_source)
    }
    
    /// Constructs a node from an existing Arc source (for child nodes, avoiding redundant cloning)
    ///
    /// # Arguments
    /// * `cursor` - tree-sitter cursor
    /// * `source` - shared source reference
    ///
    /// # Returns
    /// Newly constructed CST node, sharing source with parent node
    pub fn from_cursor_with_shared_source(cursor: &TreeCursor, source: Arc<str>) -> Self {
        Self::from_cursor_with_shared_source_and_field(cursor, source, None)
    }
    
    pub fn from_cursor_with_shared_source_and_field(cursor: &TreeCursor, source: Arc<str>, field_name: Option<String>) -> Self {
        let node = cursor.node();
        let kind = node.kind().to_string();
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_point = node.start_position();
        let end_point = node.end_position();
        let named = node.is_named();
        let error = node.has_error();

        TreeCursorSyntaxNode {
            kind,
            start_byte: start_byte as usize,
            end_byte: end_byte as usize,
            start_point,
            end_point,
            named,
            error,
            field_name,
            source,
            children: Vec::new(),
        }
    }
    
    /// Lazily retrieves the text content of the node
    ///
    /// Advantage of using Arc<str>: allows direct string slicing,
    /// no need for UTF-8 conversion, achieving true zero-copy.
    ///
    /// # Returns
    /// Text content corresponding to the node
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::CstParser;
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let text = cst.text();
    /// println!("Node text: {}", text);
    /// ```
    pub fn text(&self) -> Cow<str> {
        Cow::Borrowed(&self.source[self.start_byte..self.end_byte])
    }
    
    /// Checks if the node contains syntax errors
    ///
    /// # Returns
    /// Returns true if there are errors in the node or its subtree
    pub fn has_error(&self) -> bool {
        self.error
    }
    
    /// Retrieves the field name of the node
    ///
    /// In JSON, both keys and values in an object's key-value pairs have corresponding field names.
    ///
    /// # Returns
    /// String reference of the field name, or None if there is no field name
    pub fn field_name(&self) -> Option<&str> {
        self.field_name.as_deref()
    }
    
    /// Creates a pre-order traversal iterator (zero-copy)
    ///
    /// Pre-order traversal visits the parent node first, then all child nodes in order.
    ///
    /// # Returns
    /// Pre-order traversal iterator
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::CstParser;
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// for node in cst.iter_preorder() {
    ///     println!("Visiting: {}", node.kind);
    /// }
    /// ```
    pub fn iter_preorder(&self) -> TreeIterator {
        TreeIterator::new_preorder(self)
    }
    
    /// Creates a post-order traversal iterator (zero-copy)
    ///
    /// Post-order traversal visits all child nodes first, then the parent node.
    ///
    /// # Returns
    /// Post-order traversal iterator
    pub fn iter_postorder(&self) -> TreeIterator {
        TreeIterator::new_postorder(self)
    }
    
    /// Creates a breadth-first traversal iterator (zero-copy)
    ///
    /// Breadth-first traversal visits nodes in level-order.
    ///
    /// # Returns
    /// Breadth-first traversal iterator
    pub fn iter_breadth_first(&self) -> TreeIterator {
        TreeIterator::new_breadth_first(self)
    }
    
    /// Finds nodes of a specific type using depth-first search
    ///
    /// # Arguments
    /// * `kind` - The type of node to search for
    ///
    /// # Returns
    /// References to all nodes matching the type
    ///
    /// # Example
    /// ```
    /// # use apidom_cst::CstParser;
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let strings = cst.find_nodes_by_kind("string");
    /// for string_node in strings {
    ///     println!("Found string: {}", string_node.text());
    /// }
    /// ```
    pub fn find_nodes_by_kind(&self, kind: &str) -> Vec<&TreeCursorSyntaxNode> {
        let mut result = Vec::new();
        self.collect_nodes_by_kind(kind, &mut result);
        result
    }
    
    /// Recursively collects nodes of a specified type (internal helper method)
    fn collect_nodes_by_kind<'a>(&'a self, kind: &str, result: &mut Vec<&'a TreeCursorSyntaxNode>) {
        if self.kind == kind {
            result.push(self);
        }
        for child in &self.children {
            child.collect_nodes_by_kind(kind, result);
        }
    }
    
    /// Retrieves the shared source reference
    ///
    /// Used to check memory usage and reference count.
    /// Arc<str> is more efficient than Arc<Vec<u8>> because it avoids UTF-8 validation overhead.
    ///
    /// # Returns
    /// Arc-wrapped source reference
    pub fn shared_source(&self) -> &Arc<str> {
        &self.source
    }
}