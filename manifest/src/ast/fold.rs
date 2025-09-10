use serde_json::Value;

use crate::ast::minim_model::*;

pub trait Fold {
    /// Fold any element - the main entry point
    fn fold_element(&mut self, element: Element) -> Element {
        match element {
            Element::Null(e) => self.fold_null_element(e),
            Element::Boolean(e) => self.fold_boolean_element(e),
            Element::Number(e) => self.fold_number_element(e),
            Element::String(e) => self.fold_string_element(e),
            Element::Array(e) => self.fold_array_element(e),
            Element::Object(e) => self.fold_object_element(e),
            Element::Member(e) => self.fold_member_element(*e),
            Element::Ref(e) => self.fold_ref_element(e),
            Element::Link(e) => self.fold_link_element(e),
            Element::Custom(name, e) => self.fold_custom_element(name, *e),
        }
    }

    /// Fold null elements
    fn fold_null_element(&mut self, element: NullElement) -> Element {
        Element::Null(NullElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
        })
    }

    /// Fold boolean elements
    fn fold_boolean_element(&mut self, element: BooleanElement) -> Element {
        Element::Boolean(BooleanElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            content: element.content,
        })
    }

    /// Fold number elements
    fn fold_number_element(&mut self, element: NumberElement) -> Element {
        Element::Number(NumberElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            content: element.content,
        })
    }

    /// Fold string elements
    fn fold_string_element(&mut self, element: StringElement) -> Element {
        Element::String(StringElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            content: element.content,
        })
    }

    /// Fold array elements - recursively folds all children
    fn fold_array_element(&mut self, element: ArrayElement) -> Element {
        Element::Array(ArrayElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            content: element.content.into_iter()
                .map(|child| self.fold_element(child))
                .collect(),
        })
    }

    /// Fold object elements - recursively folds all members
    fn fold_object_element(&mut self, element: ObjectElement) -> Element {
        Element::Object(ObjectElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            classes: self.fold_array_element_direct(element.classes),
            children: element.children.into_iter()
                .map(|child| self.fold_element(child))
                .collect(),
            parent: element.parent.map(|p| Box::new(self.fold_element(*p))),
            content: element.content.into_iter()
                .map(|member| self.fold_member_element_direct(member))
                .collect(),
        })
    }

    /// Fold member elements - recursively folds key and value
    fn fold_member_element(&mut self, element: MemberElement) -> Element {
        Element::Member(Box::new(MemberElement {
            key: Box::new(self.fold_element(*element.key)),
            value: Box::new(self.fold_element(*element.value)),
        }))
    }

    /// Fold ref elements
    fn fold_ref_element(&mut self, element: RefElement) -> Element {
        Element::Ref(RefElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            path: element.path,
        })
    }

    /// Fold link elements
    fn fold_link_element(&mut self, element: LinkElement) -> Element {
        Element::Link(LinkElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            relation: element.relation,
            href: element.href,
        })
    }

    /// Fold custom elements
    fn fold_custom_element(&mut self, name: String, element: CustomElement) -> Element {
        Element::Custom(name, Box::new(CustomElement {
            element: element.element,
            meta: self.fold_meta_element(element.meta),
            attributes: self.fold_attributes_element(element.attributes),
            content: self.fold_json_value(element.content),
        }))
    }

    /// Fold meta elements
    fn fold_meta_element(&mut self, meta: MetaElement) -> MetaElement {
        MetaElement {
            properties: meta.properties.into_iter()
                .map(|(k, v)| (k, self.fold_json_value(v)))
                .collect(),
        }
    }

    /// Fold attributes elements
    fn fold_attributes_element(&mut self, attributes: AttributesElement) -> AttributesElement {
        AttributesElement {
            properties: attributes.properties.into_iter()
                .map(|(k, v)| (k, self.fold_json_value(v)))
                .collect(),
        }
    }

    /// Create metadata for an element
    fn create_meta_from_node(&mut self, _node: &TreeCursorSyntaxNode) -> MetaElement {
        MetaElement::default()
    }

    /// Fold JSON values (for meta and attributes)
    fn fold_json_value(&mut self, value: Value) -> Value {
        match value {
            Value::Array(arr) => Value::Array(
                arr.into_iter()
                    .map(|v| self.fold_json_value(v))
                    .collect()
            ),
            Value::Object(obj) => Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, self.fold_json_value(v)))
                    .collect()
            ),
            other => other,
        }
    }

    // Helper methods for internal use
    fn fold_array_element_direct(&mut self, element: ArrayElement) -> ArrayElement {
        match self.fold_array_element(element) {
            Element::Array(arr) => arr,
            _ => unreachable!("fold_array_element should always return Array"),
        }
    }

    fn fold_member_element_direct(&mut self, element: MemberElement) -> MemberElement {
        match self.fold_member_element(element) {
            Element::Member(member) => *member,
            _ => unreachable!("fold_member_element should always return Member"),
        }
    }
}

/// Default folder that preserves all elements unchanged.
/// 
/// This is useful as a base for implementing specific transformations
/// by overriding only the methods you need to change.
/// 

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultFolder;

impl Fold for DefaultFolder {
    fn fold_json_value(&mut self, value: Value) -> Value {
        match value {
            Value::Array(arr) => Value::Array(
                arr.into_iter()
                    .map(|v| self.fold_json_value(v))
                    .collect()
            ),
            Value::Object(obj) => Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, self.fold_json_value(v)))
                    .collect()
            ),
            other => other,
        }
    }
}

/// A folder that applies multiple folders in sequence.
/// 
/// This allows you to compose multiple transformations together.
/// 

/// let folder = CompositeFolder::new(vec![
///     Box::new(FirstFolder),
///     Box::new(SecondFolder),
/// ]);
/// ```
pub struct CompositeFolder {
    folders: Vec<Box<dyn Fold>>,
}

impl CompositeFolder {
    /// Create a new composite folder with the given folders
    pub fn new(folders: Vec<Box<dyn Fold>>) -> Self {
        Self { folders }
    }
    
    /// Add a folder to the composition
    pub fn add_folder(&mut self, folder: Box<dyn Fold>) {
        self.folders.push(folder);
    }
}

impl Fold for CompositeFolder {
    fn fold_element(&mut self, mut element: Element) -> Element {
        for folder in &mut self.folders {
            element = folder.fold_element(element);
        }
        element
    }

    fn fold_json_value(&mut self, mut value: Value) -> Value {
        for folder in &mut self.folders {
            value = folder.fold_json_value(value);
        }
        value
    }
}

/// Utility functions for common folding operations
pub mod utils {
    use super::*;

    /// Fold an element tree and collect all elements of a specific type
    pub fn collect_elements<F>(element: Element, predicate: F) -> Vec<Element>
    where
        F: FnMut(&Element) -> bool,
    {
        let mut collector = ElementCollector::new(predicate);
        collector.fold_element(element);
        collector.collected
    }

    /// Fold an element tree and count elements matching a predicate
    pub fn count_elements<F>(element: Element, predicate: F) -> usize
    where
        F: FnMut(&Element) -> bool,
    {
        collect_elements(element, predicate).len()
    }

    /// Find the first element matching a predicate
    pub fn find_element<F>(element: Element, predicate: F) -> Option<Element>
    where
        F: FnMut(&Element) -> bool,
    {
        let mut finder = ElementFinder::new(predicate);
        finder.fold_element(element);
        finder.found
    }

    /// Check if any element in the tree matches a predicate
    pub fn any_element<F>(element: Element, predicate: F) -> bool
    where
        F: FnMut(&Element) -> bool,
    {
        find_element(element, predicate).is_some()
    }

    /// Transform all string elements in the tree
    pub fn map_strings<F>(element: Element, f: F) -> Element
    where
        F: Fn(String) -> String + Clone + 'static,
    {
        let mut mapper = StringMapper::new(f);
        mapper.fold_element(element)
    }

    /// Transform all number elements in the tree
    pub fn map_numbers<F>(element: Element, f: F) -> Element
    where
        F: Fn(f64) -> f64 + Clone + 'static,
    {
        let mut mapper = NumberMapper::new(f);
        mapper.fold_element(element)
    }

    // Helper structs for utility functions
    struct ElementCollector<F> {
        predicate: F,
        collected: Vec<Element>,
    }

    impl<F> ElementCollector<F>
    where
        F: FnMut(&Element) -> bool,
    {
        fn new(predicate: F) -> Self {
            Self {
                predicate,
                collected: Vec::new(),
            }
        }
    }

    impl<F> Fold for ElementCollector<F>
    where
        F: FnMut(&Element) -> bool,
    {
        fn fold_element(&mut self, element: Element) -> Element {
            if (self.predicate)(&element) {
                self.collected.push(element.clone());
            }
            
            // Continue folding to traverse the tree - use self instead of DefaultFolder
            match element {
                Element::Array(arr) => self.fold_array_element(arr),
                Element::Object(obj) => self.fold_object_element(obj),
                Element::Member(member) => self.fold_member_element(*member),
                Element::Custom(name, custom) => self.fold_custom_element(name, *custom),
                other => other, // Leaf elements don't need further traversal
            }
        }

        fn fold_json_value(&mut self, value: Value) -> Value {
            match value {
                Value::Array(arr) => Value::Array(
                    arr.into_iter()
                        .map(|v| self.fold_json_value(v))
                        .collect()
                ),
                Value::Object(obj) => Value::Object(
                    obj.into_iter()
                        .map(|(k, v)| (k, self.fold_json_value(v)))
                        .collect()
                ),
                other => other,
            }
        }
    }

    struct ElementFinder<F> {
        predicate: F,
        found: Option<Element>,
    }

    impl<F> ElementFinder<F>
    where
        F: FnMut(&Element) -> bool,
    {
        fn new(predicate: F) -> Self {
            Self {
                predicate,
                found: None,
            }
        }
    }

    impl<F> Fold for ElementFinder<F>
    where
        F: FnMut(&Element) -> bool,
    {
        fn fold_element(&mut self, element: Element) -> Element {
            if self.found.is_none() && (self.predicate)(&element) {
                self.found = Some(element.clone());
            }
            
            // Continue folding unless we found what we're looking for
            if self.found.is_none() {
                DefaultFolder.fold_element(element)
            } else {
                element
            }
        }

        fn fold_json_value(&mut self, value: Value) -> Value {
            if self.found.is_none() {
                match value {
                    Value::Array(arr) => Value::Array(
                        arr.into_iter()
                            .map(|v| self.fold_json_value(v))
                            .collect()
                    ),
                    Value::Object(obj) => Value::Object(
                        obj.into_iter()
                            .map(|(k, v)| (k, self.fold_json_value(v)))
                            .collect()
                    ),
                    other => other,
                }
            } else {
                value
            }
        }
    }

    struct StringMapper<F> {
        mapper: F,
    }

    impl<F> StringMapper<F>
    where
        F: Fn(String) -> String,
    {
        fn new(mapper: F) -> Self {
            Self { mapper }
        }
    }

    impl<F> Fold for StringMapper<F>
    where
        F: Fn(String) -> String,
    {
        fn fold_string_element(&mut self, mut element: StringElement) -> Element {
            element.content = (self.mapper)(element.content);
            Element::String(element)
        }
    }

    struct NumberMapper<F> {
        mapper: F,
    }

    impl<F> NumberMapper<F>
    where
        F: Fn(f64) -> f64,
    {
        fn new(mapper: F) -> Self {
            Self { mapper }
        }
    }

    impl<F> Fold for NumberMapper<F>
    where
        F: Fn(f64) -> f64,
    {
        fn fold_number_element(&mut self, mut element: NumberElement) -> Element {
            element.content = (self.mapper)(element.content);
            Element::Number(element)
        }
    }
}

/// Specialized folders for common use cases
pub mod folders {
    use super::*;

    /// A folder that normalizes string content (trims whitespace, converts to lowercase)
    #[derive(Debug, Default, Clone, Copy)]
    pub struct StringNormalizer;

    impl Fold for StringNormalizer {
        fn fold_string_element(&mut self, mut element: StringElement) -> Element {
            element.content = element.content.trim().to_lowercase();
            Element::String(element)
        }
    }

    /// A folder that removes empty arrays and objects
    #[derive(Debug, Default, Clone, Copy)]
    pub struct EmptyRemover;

    impl Fold for EmptyRemover {
        fn fold_array_element(&mut self, element: ArrayElement) -> Element {
            let folded = DefaultFolder.fold_array_element(element);
            if let Element::Array(arr) = folded {
                if arr.content.is_empty() {
                    Element::Null(NullElement::default())
                } else {
                    Element::Array(arr)
                }
            } else {
                unreachable!()
            }
        }

        fn fold_object_element(&mut self, element: ObjectElement) -> Element {
            let folded = DefaultFolder.fold_object_element(element);
            if let Element::Object(obj) = folded {
                if obj.content.is_empty() {
                    Element::Null(NullElement::default())
                } else {
                    Element::Object(obj)
                }
            } else {
                unreachable!()
            }
        }
    }

    /// A folder that validates and repairs element structures
    #[derive(Debug, Default, Clone, Copy)]
    pub struct StructureValidator;

    impl Fold for StructureValidator {
        fn fold_object_element(&mut self, mut element: ObjectElement) -> Element {
            // Ensure all members have string keys
            element.content.retain(|member| {
                matches!(*member.key, Element::String(_))
            });
            
            Element::Object(element)
        }
        
        fn fold_ref_element(&mut self, mut element: RefElement) -> Element {
            // Validate ref path format
            if !element.path.starts_with('#') && !element.path.starts_with("http") {
                element.path = format!("#{}", element.path);
            }
            
            Element::Ref(element)
        }
    }

    /// A folder that converts elements to a specific type when possible
    pub struct TypeConverter {
        target_type: String,
    }

    impl TypeConverter {
        pub fn new(target_type: String) -> Self {
            Self { target_type }
        }
    }

    impl Fold for TypeConverter {
        fn fold_element(&mut self, element: Element) -> Element {
            match (&self.target_type[..], &element) {
                ("string", Element::Number(num)) => {
                    Element::String(StringElement::new(&num.content.to_string()))
                }
                ("string", Element::Boolean(bool)) => {
                    Element::String(StringElement::new(&bool.content.to_string()))
                }
                ("number", Element::String(str)) => {
                    if let Ok(num) = str.content.parse::<f64>() {
                        Element::Number(NumberElement {
                            element: "number".to_string(),
                            meta: str.meta.clone(),
                            attributes: str.attributes.clone(),
                            content: num,
                        })
                    } else {
                        element
                    }
                }
                ("boolean", Element::String(str)) => {
                    match str.content.to_lowercase().as_str() {
                        "true" | "1" | "yes" => Element::Boolean(BooleanElement {
                            element: "boolean".to_string(),
                            meta: str.meta.clone(),
                            attributes: str.attributes.clone(),
                            content: true,
                        }),
                        "false" | "0" | "no" => Element::Boolean(BooleanElement {
                            element: "boolean".to_string(),
                            meta: str.meta.clone(),
                            attributes: str.attributes.clone(),
                            content: false,
                        }),
                        _ => element,
                    }
                }
                _ => DefaultFolder.fold_element(element),
            }
        }
    }
}

/// CST to AST conversion trait
/// 
/// This trait extends the Fold mechanism to support converting Concrete Syntax Trees (CST)
/// from tree-sitter into Abstract Syntax Trees (AST) using the Minim model.
/// 
/// The trait provides a bridge between the concrete representation of source code
/// and the abstract element model used by API DOM.
pub trait FoldFromCst: Fold {
    /// Convert a CST node to an AST element
    /// 
    /// This is the main entry point for CST to AST conversion.
    /// 
    /// # Arguments
    /// * `node` - The CST node to convert
    /// 
    /// # Returns
    /// An Element representing the AST equivalent of the CST node
    /// 
    /// # Example
    /// ```ignore
    /// use apidom_ast::fold::{FoldFromCst, JsonFolder};
    /// use apidom_cst::CstParser;
    /// 
    /// let cst = CstParser::parse(r#"{"key": "value"}"#);
    /// let mut folder = JsonFolder::new();
    /// let ast = folder.fold_from_cst(&cst);
    /// ```
    fn fold_from_cst(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a CST node to an Element based on its kind
    /// 
    /// This method dispatches to specific conversion methods based on the node type.
    /// Implementors can override this for custom node type handling.
    fn fold_cst_node(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        match node.kind.as_str() {
            "document" => {
                // Document node contains the actual JSON value as its first named child
                // Find the first named child which should be the actual JSON value
                for child in &node.children {
                    if child.named {
                        return self.fold_cst_node(child);
                    }
                }
                // Empty document - return null
                Element::Null(NullElement {
                    element: "null".to_string(),
                    meta: MetaElement::default(),
                    attributes: AttributesElement::default(),
                })
            }
            "object" => self.fold_cst_object(node),
            "array" => self.fold_cst_array(node),
            "string" => self.fold_cst_string(node),
            "number" => self.fold_cst_number(node),
            "true" | "false" => self.fold_cst_boolean(node),
            "null" => self.fold_cst_null(node),
            "pair" => self.fold_cst_pair(node),
            _ => self.fold_cst_unknown(node),
        }
    }
    
    /// Convert an object CST node to an ObjectElement
    fn fold_cst_object(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert an array CST node to an ArrayElement
    fn fold_cst_array(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a string CST node to a StringElement
    fn fold_cst_string(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a number CST node to a NumberElement
    fn fold_cst_number(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a boolean CST node to a BooleanElement
    fn fold_cst_boolean(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a null CST node to a NullElement
    fn fold_cst_null(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Convert a pair CST node to a MemberElement
    fn fold_cst_pair(&mut self, node: &TreeCursorSyntaxNode) -> Element;
    
    /// Handle unknown or unsupported CST node types
    fn fold_cst_unknown(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        // Default: create a custom element for unknown types
        Element::Custom(
            node.kind.clone(),
            Box::new(CustomElement {
                element: node.kind.clone(),
                meta: self.create_meta_from_node(node),
                attributes: AttributesElement::default(),
                content: Value::String(node.text().to_string()),
            })
        )
    }
}

/// JSON-specific folder for converting JSON CST to AST
/// 
/// This folder implements the conversion logic for JSON syntax trees
/// produced by tree-sitter's JSON parser.
/// 
/// # Features
/// - Handles all JSON value types (object, array, string, number, boolean, null)
/// - Preserves source location information in metadata
/// - Supports error recovery for malformed JSON
/// - Efficient conversion with minimal allocations
/// 
/// # Example
/// ```ignore
/// use apidom_ast::fold::{FoldFromCst, JsonFolder};
/// use apidom_cst::CstParser;
/// 
/// let source = r#"{"name": "John", "age": 30, "active": true}"#;
/// let cst = CstParser::parse(source);
/// let mut folder = JsonFolder::new();
/// let ast = folder.fold_from_cst(&cst);
/// 
/// // The AST can now be processed using the fold mechanism
/// ```
#[derive(Debug, Default)]
pub struct JsonFolder {
    /// Whether to include source location information in metadata
    include_source_info: bool,

    #[allow(dead_code)]
    /// Whether to preserve formatting information (whitespace, etc.)
    preserve_formatting: bool,
}

impl JsonFolder {
    /// Create a new JSON folder with default settings
    pub fn new() -> Self {
        Self {
            include_source_info: true,
            preserve_formatting: false,
        }
    }
    
    /// Create a JSON folder with custom settings
    pub fn with_options(include_source_info: bool, preserve_formatting: bool) -> Self {
        Self {
            include_source_info,
            preserve_formatting,
        }
    }
    
    /// Create metadata from CST node location information
    #[allow(dead_code)]
    fn create_meta_from_node(&self, node: &TreeCursorSyntaxNode) -> MetaElement {
        let mut meta = MetaElement::default();
        
        if self.include_source_info {
            meta.properties.insert(
                "sourceLocation".to_string(),
                Value::Object({
                    let mut map = serde_json::Map::new();
                    map.insert("start".to_string(), Value::Object({
                        let mut start = serde_json::Map::new();
                        start.insert("line".to_string(), Value::Number(((node.start_point.row + 1) as i64).into()));
                        start.insert("column".to_string(), Value::Number(((node.start_point.column + 1) as i64).into()));
                        start.insert("byte".to_string(), Value::Number(serde_json::Number::from_f64(node.start_byte as f64).unwrap_or_else(|| 0.into())));
                        start
                    }));
                    map.insert("end".to_string(), Value::Object({
                        let mut end = serde_json::Map::new();
                        end.insert("line".to_string(), Value::Number(((node.end_point.row + 1) as i64).into()));
                        end.insert("column".to_string(), Value::Number(((node.end_point.column + 1) as i64).into()));
                        end.insert("byte".to_string(), Value::Number(serde_json::Number::from_f64(node.end_byte as f64).unwrap_or_else(|| 0.into())));
                        end
                    }));
                    map
                })
            );
        }
        
        if node.has_error() {
            meta.properties.insert(
                "hasError".to_string(),
                Value::Bool(true)
            );
        }
        
        if let Some(field_name) = node.field_name() {
            meta.properties.insert(
                "fieldName".to_string(),
                Value::String(field_name.to_string())
            );
        }
        
        meta
    }
    
    /// Extract string content from a string CST node, handling escape sequences
    fn extract_string_content(&self, node: &TreeCursorSyntaxNode) -> String {
        let text = node.text();
        
        // Remove surrounding quotes and handle escape sequences
        if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
            let inner = &text[1..text.len()-1];
            self.unescape_json_string(inner)
        } else {
            text.to_string()
        }
    }
    
    /// Handle JSON string escape sequences
    fn unescape_json_string(&self, s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('"') => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some('/') => result.push('/'),
                    Some('b') => result.push('\u{0008}'),
                    Some('f') => result.push('\u{000C}'),
                    Some('n') => result.push('\n'),
                    Some('r') => result.push('\r'),
                    Some('t') => result.push('\t'),
                    Some('u') => {
                        // Unicode escape sequence \uXXXX
                        let hex: String = chars.by_ref().take(4).collect();
                        if hex.len() == 4 {
                            if let Ok(code_point) = u32::from_str_radix(&hex, 16) {
                                if let Some(unicode_char) = char::from_u32(code_point) {
                                    result.push(unicode_char);
                                } else {
                                    // Invalid unicode, keep as-is
                                    result.push_str(&format!("\\u{}", hex));
                                }
                            } else {
                                // Invalid hex, keep as-is
                                result.push_str(&format!("\\u{}", hex));
                            }
                        } else {
                            // Incomplete escape, keep as-is
                            result.push_str(&format!("\\u{}", hex));
                        }
                    }
                    Some(other) => {
                        // Unknown escape, keep as-is
                        result.push('\\');
                        result.push(other);
                    }
                    None => {
                        // Trailing backslash
                        result.push('\\');
                    }
                }
            } else {
                result.push(ch);
            }
        }
        
        result
    }
    
    /// Find child nodes by kind
    #[allow(dead_code)]
    fn find_children_by_kind<'a>(&self, node: &'a TreeCursorSyntaxNode, kind: &str) -> Vec<&'a TreeCursorSyntaxNode> {
        node.children.iter()
            .filter(|child| child.kind == kind)
            .collect()
    }
    
    /// Find named children (excluding punctuation and whitespace)
    fn find_named_children<'a>(&self, node: &'a TreeCursorSyntaxNode) -> Vec<&'a TreeCursorSyntaxNode> {
        node.children.iter()
            .filter(|child| child.named)
            .collect()
    }
}

// Import the CST node type
use crate::cst::TreeCursorSyntaxNode;

impl Fold for JsonFolder {
    fn create_meta_from_node(&mut self, node: &TreeCursorSyntaxNode) -> MetaElement {
        let mut meta = MetaElement::default();
        
        if self.include_source_info {
            meta.properties.insert(
                "sourceLocation".to_string(),
                Value::Object({
                    let mut map = serde_json::Map::new();
                    map.insert("start".to_string(), Value::Object({
                        let mut start = serde_json::Map::new();
                        start.insert("line".to_string(), Value::Number(serde_json::Number::from_f64((node.start_point.row + 1) as f64).unwrap()));
                        start.insert("column".to_string(), Value::Number(serde_json::Number::from_f64((node.start_point.column + 1) as f64).unwrap()));
                        start.insert("byte".to_string(), Value::Number(serde_json::Number::from_f64(node.start_byte as f64).unwrap()));
                        start
                    }));
                    map.insert("end".to_string(), Value::Object({
                        let mut end = serde_json::Map::new();
                        end.insert("line".to_string(), Value::Number(serde_json::Number::from_f64((node.end_point.row + 1) as f64).unwrap()));
                        end.insert("column".to_string(), Value::Number(serde_json::Number::from_f64((node.end_point.column + 1) as f64).unwrap()));
                        end.insert("byte".to_string(), Value::Number(serde_json::Number::from_f64(node.end_byte as f64).unwrap()));
                        end
                    }));
                    map
                })
            );
        }
        
        if node.has_error() {
            meta.properties.insert(
                "hasError".to_string(),
                Value::Bool(true)
            );
        }
        
        if let Some(field_name) = node.field_name() {
            meta.properties.insert(
                "fieldName".to_string(),
                Value::String(field_name.to_string())
            );
        }
        
        meta
    }
}

impl FoldFromCst for JsonFolder {
    fn fold_from_cst(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        self.fold_cst_node(node)
    }
    
    fn fold_cst_object(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        let mut members = Vec::new();
        
        // Find all pair children
        for child in &node.children {
            if child.kind == "pair" {
                if let Element::Member(member) = self.fold_cst_pair(child) {
                    members.push(*member);
                }
            }
        }
        
        Element::Object(ObjectElement {
            element: "object".to_string(),
            meta,
            attributes: AttributesElement::default(),
            classes: ArrayElement {
                element: "array".to_string(),
                meta: MetaElement::default(),
                attributes: AttributesElement::default(),
                content: vec![],
            },
            children: vec![],
            parent: None,
            content: members,
        })
    }
    
    fn fold_cst_array(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        let mut elements = Vec::new();
        
        // Find all named children (values)
        let named_children = self.find_named_children(node);
        for child in named_children {
            elements.push(self.fold_cst_node(child));
        }
        
        Element::Array(ArrayElement {
            element: "array".to_string(),
            meta,
            attributes: AttributesElement::default(),
            content: elements,
        })
    }
    
    fn fold_cst_string(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        let content = self.extract_string_content(node);
        
        Element::String(StringElement {
            element: "string".to_string(),
            meta,
            attributes: AttributesElement::default(),
            content,
        })
    }
    
    fn fold_cst_number(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        let text = node.text();
        
        // Parse the number
        let content = text.parse::<f64>().unwrap_or(0.0);
        
        Element::Number(NumberElement {
            element: "number".to_string(),
            meta,
            attributes: AttributesElement::default(),
            content,
        })
    }
    
    fn fold_cst_boolean(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        let content = node.text() == "true";
        
        Element::Boolean(BooleanElement {
            element: "boolean".to_string(),
            meta,
            attributes: AttributesElement::default(),
            content,
        })
    }
    
    fn fold_cst_null(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        let meta = self.create_meta_from_node(node);
        
        Element::Null(NullElement {
            element: "null".to_string(),
            meta,
            attributes: AttributesElement::default(),
        })
    }
    
    fn fold_cst_pair(&mut self, node: &TreeCursorSyntaxNode) -> Element {
        // A pair should have exactly 2 named children: key and value
        let named_children = self.find_named_children(node);
        
        if named_children.len() >= 2 {
            let key = self.fold_cst_node(named_children[0]);
            let value = self.fold_cst_node(named_children[1]);
            
            Element::Member(Box::new(MemberElement {
                key: Box::new(key),
                value: Box::new(value),
            }))
        } else {
            // Malformed pair, create a custom element
            Element::Custom(
                "malformed_pair".to_string(),
                Box::new(CustomElement {
                    element: "malformed_pair".to_string(),
                    meta: self.create_meta_from_node(node),
                    attributes: AttributesElement::default(),
                    content: Value::String(node.text().to_string()),
                })
            )
        }
    }
}

/// Convenience function to convert JSON CST to AST
/// 
/// This function provides a simple interface for converting a JSON CST
/// to an AST using the default JsonFolder configuration.
/// 
/// # Arguments
/// * `cst_root` - The root CST node to convert
/// 
/// # Returns
/// An Element representing the AST
/// 
/// # Example
/// ```ignore
/// use apidom_ast::fold::json_cst_to_ast;
/// use apidom_cst::CstParser;
/// 
/// let cst = CstParser::parse(r#"{"hello": "world"}"#);
/// let ast = json_cst_to_ast(&cst);
/// ```
pub fn json_cst_to_ast(cst_root: &TreeCursorSyntaxNode) -> Element {
    let mut folder = JsonFolder::new();
    folder.fold_from_cst(cst_root)
}

/// Convenience function to convert JSON source directly to AST
/// 
/// This function combines parsing and conversion in a single step.
/// 
/// # Arguments
/// * `source` - The JSON source code to parse and convert
/// 
/// # Returns
/// An Element representing the AST
/// 
/// # Example
/// ```ignore
/// use apidom_ast::fold::json_source_to_ast;
/// 
/// let ast = json_source_to_ast(r#"{"name": "Alice", "age": 25}"#);
/// ```
pub fn json_source_to_ast(source: &str) -> Element {
    let cst = crate::cst::CstParser::parse(source);
    json_cst_to_ast(&cst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::utils::*;
    use super::folders::*;

    #[test]
    fn test_default_folder_preserves_structure() {
        let element = Element::String(StringElement::new("test"));
        let mut folder = DefaultFolder;
        let result = folder.fold_element(element.clone());
        
        if let (Element::String(original), Element::String(folded)) = (element, result) {
            assert_eq!(original.content, folded.content);
        } else {
            panic!("Type mismatch");
        }
    }
    
    #[test]
    fn test_string_normalizer() {
        let element = Element::String(StringElement::new("  HELLO WORLD  "));
        let mut folder = StringNormalizer;
        let result = folder.fold_element(element);
        
        if let Element::String(str_elem) = result {
            assert_eq!(str_elem.content, "hello world");
        } else {
            panic!("Expected string element");
        }
    }
    
    #[test]
    fn test_collect_elements() {
        let array = Element::Array(ArrayElement {
            element: "array".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: vec![
                Element::String(StringElement::new("test1")),
                Element::Number(NumberElement {
                    element: "number".to_string(),
                    meta: MetaElement::default(),
                    attributes: AttributesElement::default(),
                    content: 42.0,
                }),
                Element::String(StringElement::new("test2")),
            ],
        });

        let strings = collect_elements(array, |e| matches!(e, Element::String(_)));
        assert_eq!(strings.len(), 2);
    }

    #[test]
    fn test_map_strings() {
        let array = Element::Array(ArrayElement {
            element: "array".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: vec![
                Element::String(StringElement::new("hello")),
                Element::String(StringElement::new("world")),
            ],
        });

        let result = map_strings(array, |s| s.to_uppercase());
        
        if let Element::Array(arr) = result {
            if let Element::String(str1) = &arr.content[0] {
                assert_eq!(str1.content, "HELLO");
            }
            if let Element::String(str2) = &arr.content[1] {
                assert_eq!(str2.content, "WORLD");
            }
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_type_converter() {
        let element = Element::String(StringElement::new("42"));
        let mut converter = TypeConverter::new("number".to_string());
        let result = converter.fold_element(element);
        
        if let Element::Number(num) = result {
            assert_eq!(num.content, 42.0);
        } else {
            panic!("Expected number element");
        }
    }
    
    #[test]
    fn test_empty_remover() {
        let empty_array = Element::Array(ArrayElement {
            element: "array".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: vec![],
        });

        let mut remover = EmptyRemover;
        let result = remover.fold_element(empty_array);
        
        assert!(matches!(result, Element::Null(_)));
    }
    
    #[test]
    fn test_composite_folder() {
        let element = Element::String(StringElement::new("  42  "));
        
        let mut composite = CompositeFolder::new(vec![
            Box::new(StringNormalizer),
            Box::new(TypeConverter::new("number".to_string())),
        ]);
        
        let result = composite.fold_element(element);
        
        if let Element::Number(num) = result {
            assert_eq!(num.content, 42.0);
        } else {
            panic!("Expected number element");
        }
    }

    #[test]
    fn test_json_object_conversion() {
        let json = r#"{"name": "John", "age": 30}"#;
        let cst = crate::cst::CstParser::parse(json);
        let mut folder = JsonFolder::new();
        let ast = folder.fold_from_cst(&cst);
        
        // 添加调试输出
        println!("转换结果类型: {:?}", std::mem::discriminant(&ast));
        match &ast {
            Element::Object(_) => println!("是 Object"),
            Element::Array(_) => println!("是 Array"),
            Element::String(_) => println!("是 String"),
            Element::Number(_) => println!("是 Number"),
            Element::Boolean(_) => println!("是 Boolean"),
            Element::Null(_) => println!("是 Null"),
            _ => println!("是其他类型"),
        }
        
        if let Element::Object(obj) = ast {
            assert_eq!(obj.content.len(), 2);
            
            // 检查第一个成员
            let first_member = &obj.content[0];
            if let Element::String(key) = first_member.key.as_ref() {
                assert_eq!(key.content, "name");
            }
            if let Element::String(value) = first_member.value.as_ref() {
                assert_eq!(value.content, "John");
            }
        } else {
            panic!("Expected Object element");
        }
    }

    #[test]
    fn test_fold_json_value() {
        let mut folder = DefaultFolder;
        let value = serde_json::Value::Array(vec![
            serde_json::Value::Number(serde_json::Number::from(1)),  
            serde_json::Value::String("test".to_string()),
            serde_json::Value::Object({
                let mut map = serde_json::Map::new();
                map.insert("key".to_string(), serde_json::Value::Bool(true));
                map
            })
        ]);

        let result = folder.fold_json_value(value);
        assert!(matches!(result, Value::Array(_)));
    }

    #[test]
    fn test_composite_folder_json_value() {
        let mut composite = CompositeFolder::new(vec![
            Box::new(DefaultFolder),
            Box::new(DefaultFolder),
        ]);

        let value = Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("test".to_string(), Value::Number(serde_json::Number::from(42)));
            map
        });

        let result = composite.fold_json_value(value);
        assert!(matches!(result, Value::Object(_)));
    }
}

#[cfg(test)]
mod cst_tests {
    use super::*;
    use crate::cst::CstParser;

    #[test]
    fn test_json_object_conversion() {
        let source = r#"{"name": "John", "age": 30}"#;
        let cst = CstParser::parse(source);
        let mut folder = JsonFolder::new();
        let ast = folder.fold_from_cst(&cst);
        
        match ast {
            Element::Object(obj) => {
                assert_eq!(obj.content.len(), 2);
                // Check that we have name and age members
                assert!(obj.has_key("name"));
                assert!(obj.has_key("age"));
            }
            _ => panic!("Expected Object element"),
        }
    }
    
    #[test]
    fn test_json_array_conversion() {
        let source = r#"[1, 2, "three", true, null]"#;
        let cst = CstParser::parse(source);
        let mut folder = JsonFolder::new();
        let ast = folder.fold_from_cst(&cst);
        
        match ast {
            Element::Array(arr) => {
                assert_eq!(arr.content.len(), 5);
                // Check types
                assert!(matches!(arr.content[0], Element::Number(_)));
                assert!(matches!(arr.content[1], Element::Number(_)));
                assert!(matches!(arr.content[2], Element::String(_)));
                assert!(matches!(arr.content[3], Element::Boolean(_)));
                assert!(matches!(arr.content[4], Element::Null(_)));
            }
            _ => panic!("Expected Array element"),
        }
    }
    
    #[test]
    fn test_json_string_escaping() {
        let source = r#""Hello\nWorld\t\"Quote\"""#;
        let cst = CstParser::parse(source);
        let mut folder = JsonFolder::new();
        let ast = folder.fold_from_cst(&cst);
        
        match ast {
            Element::String(s) => {
                assert_eq!(s.content, "Hello\nWorld\t\"Quote\"");
            }
            _ => panic!("Expected String element"),
        }
    }
    
    #[test]
    fn test_json_number_conversion() {
        let test_cases = vec![
            ("42", 42.0),
            ("-17", -17.0),
            ("3.14159", 3.14159),
            ("1.23e-4", 1.23e-4),
            ("0", 0.0),
        ];
        
        for (source, expected) in test_cases {
            let cst = CstParser::parse(source);
            let mut folder = JsonFolder::new();
            let ast = folder.fold_from_cst(&cst);
            
            match ast {
                Element::Number(n) => {
                    assert!((n.content - expected).abs() < f64::EPSILON, 
                           "Expected {}, got {}", expected, n.content);
                }
                _ => panic!("Expected Number element for {}", source),
            }
        }
    }
    
    #[test]
    fn test_json_boolean_conversion() {
        let cst_true = CstParser::parse("true");
        let mut folder = JsonFolder::new();
        let ast_true = folder.fold_from_cst(&cst_true);
        
        match ast_true {
            Element::Boolean(b) => assert!(b.content),
            _ => panic!("Expected Boolean element"),
        }
        
        let cst_false = CstParser::parse("false");
        let ast_false = folder.fold_from_cst(&cst_false);
        
        match ast_false {
            Element::Boolean(b) => assert!(!b.content),
            _ => panic!("Expected Boolean element"),
        }
    }
    
    #[test]
    fn test_json_null_conversion() {
        let cst = CstParser::parse("null");
        let mut folder = JsonFolder::new();
        let ast = folder.fold_from_cst(&cst);
        
        assert!(matches!(ast, Element::Null(_)));
    }
    
    #[test]
    fn test_source_location_metadata() {
        let source = r#"{"key": "value"}"#;
        let cst = CstParser::parse(source);
        let mut folder = JsonFolder::with_options(true, false);
        let ast = folder.fold_from_cst(&cst);
        
        if let Element::Object(obj) = ast {
            if let Some(location) = obj.meta.properties.get("sourceLocation") {
                assert!(matches!(location, Value::Object(_)));
            } else {
                panic!("Expected sourceLocation in metadata");
            }
        } else {
            panic!("Expected Object element");
        }
    }
    
    #[test]
    fn test_convenience_functions() {
        let source = r#"{"test": true}"#;
        
        // Test json_source_to_ast
        let ast = json_source_to_ast(source);
        match ast {
            Element::Object(obj) => {
                assert!(obj.has_key("test"));
            }
            _ => panic!("Expected Object element"),
        }
        
        // Test json_cst_to_ast
        let cst = CstParser::parse(source);
        let ast2 = json_cst_to_ast(&cst);
        match ast2 {
            Element::Object(obj) => {
                assert!(obj.has_key("test"));
            }
            _ => panic!("Expected Object element"),
        }
    }
}

