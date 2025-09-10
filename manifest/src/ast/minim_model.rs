use std::collections::HashMap;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub enum Element {
    Null(NullElement),
    Boolean(BooleanElement),
    Number(NumberElement),
    String(StringElement),
    Array(ArrayElement),
    Object(ObjectElement),
    Member(Box<MemberElement>),
    Ref(RefElement),
    Link(LinkElement),
    Custom(String, Box<CustomElement>),
}

impl Element {
    pub fn as_string(&self) -> Option<&StringElement> {
        match self {
            Element::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&ObjectElement> {
        match self {
            Element::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&ArrayElement> {
        match self {
            Element::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<&NumberElement> {
        match self {
            Element::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<&BooleanElement> {
        match self {
            Element::Boolean(b) => Some(b),
            _ => None,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Element::Null(_) => Value::Null,
            Element::Boolean(e) => Value::Bool(e.content),
            Element::Number(e) => {
                if e.content.is_finite() {
                    serde_json::Number::from_f64(e.content)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            },
            Element::String(e) => Value::String(e.content.clone()),
            Element::Array(e) => Value::Array(e.content.iter().map(|el| el.to_value()).collect()),
            Element::Object(e) => e.to_value(),
            Element::Member(_) => Value::Null,
            Element::Ref(e) => Value::String(e.path.clone()),
            Element::Link(e) => Value::String(e.href.clone()),
            Element::Custom(_, e) => e.content.clone(),
        }
    }
}

pub struct ElementRegistry {
    registry: HashMap<String, fn(Value) -> Element>,
}

impl ElementRegistry {
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    pub fn register(&mut self, element_type: &str, constructor: fn(Value) -> Element) {
        self.registry.insert(element_type.to_string(), constructor);
    }

    pub fn create(&self, element_type: &str, value: Value) -> Option<Element> {
        self.registry.get(element_type).map(|ctor| ctor(value))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub classes: ArrayElement,
    pub children: Vec<Element>,
    pub parent: Option<Box<Element>>,
    pub content: Vec<MemberElement>,
}

impl ObjectElement {
    pub fn new() -> Self {
        Self {
            element: "object".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            classes: ArrayElement::new_empty(),
            children: vec![],
            parent: None,
            content: vec![],
        }
    }

    pub fn set_element_type(&mut self, element_type: &str) {
        self.element = element_type.to_string();
    }

    pub fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        for member in &self.content {
            if let Element::String(StringElement { content, .. }) = *member.key.clone() {
                map.insert(content, member.value.to_value());
            }
        }
        Value::Object(map)
    }

    pub fn get_member(&self, key: &str) -> Option<&MemberElement> {
        self.content.iter().find(|m| {
            matches!(
                *m.key,
                Element::String(StringElement { ref content, .. }) if content == key
            )
        })
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.get_member(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&Element> {
        self.get_member(key).map(|m| m.value.as_ref())
    }

    pub fn set(&mut self, key: &str, value: Element) {
        if let Some(member) = self.content.iter_mut().find(|m| {
            matches!(
                *m.key,
                Element::String(StringElement { ref content, .. }) if content == key
            )
        }) {
            member.value = Box::new(value);
        } else {
            self.content.push(MemberElement {
                key: Box::new(Element::String(StringElement::new(key))),
                value: Box::new(value),
            });
        }
    }

    pub fn add_class(&mut self, class_name: &str) {
        self.classes.content.push(Element::String(StringElement::new(class_name)));
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetaElement {
    pub properties: HashMap<String, Value>,
}

impl Default for MetaElement {
    fn default() -> Self {
        MetaElement {
            properties: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AttributesElement {
    pub properties: HashMap<String, Value>,
}

impl Default for AttributesElement {
    fn default() -> Self {
        AttributesElement {
            properties: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StringElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub content: String,
}

impl StringElement {
    pub fn new(s: &str) -> Self {
        Self {
            element: "string".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: s.to_string(),
        }
    }

    pub fn set_element_type(&mut self, element_type: &str) {
        self.element = element_type.to_string();
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn add_class(&mut self, class_name: &str) {
        self.meta.properties.insert(
            "class".to_string(),
            Value::String(class_name.to_string())
        );
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemberElement {
    pub key: Box<Element>,
    pub value: Box<Element>,
}

impl MemberElement {
    pub fn new(key: Element, value: Element) -> Self {
        Self {
            key: Box::new(key),
            value: Box::new(value),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BooleanElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub content: bool,
}

impl BooleanElement {
    pub fn new(value: bool) -> Self {
        Self {
            element: "boolean".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: value,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NumberElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub content: f64,
}

impl NumberElement {
    pub fn new(value: f64) -> Self {
        Self {
            element: "number".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: value,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NullElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
}

impl Default for NullElement {
    fn default() -> Self {
        Self {
            element: "null".into(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ArrayElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub content: Vec<Element>,
}

impl ArrayElement {
    pub fn new_empty() -> Self {
        Self {
            element: "array".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: vec![],
        }
    }

    pub fn from_strings(strings: &[&str]) -> Self {
        Self {
            element: "array".to_string(),
            meta: MetaElement::default(),
            attributes: AttributesElement::default(),
            content: strings.iter()
                .map(|s| Element::String(StringElement::new(s)))
                .collect(),
        }
    }

    pub fn set_element_type(&mut self, element_type: &str) {
        self.element = element_type.to_string();
    }

    pub fn add_class(&mut self, class_name: &str) {
        self.meta.properties.insert(
            "class".to_string(),
            Value::String(class_name.to_string())
        );
    }

    pub fn get(&self, index: usize) -> Option<&Element> {
        self.content.get(index)
    }

    pub fn set(&mut self, index: usize, element: Element) {
        if index < self.content.len() {
            self.content[index] = element;
        } else {
            self.content.resize(index + 1, Element::Null(NullElement::default()));
            self.content[index] = element;
        }
    }

    pub fn first(&self) -> Option<&Element> {
        self.content.first()
    }

    pub fn second(&self) -> Option<&Element> {
        self.content.get(1)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RefElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub relation: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomElement {
    pub element: String,
    pub meta: MetaElement,
    pub attributes: AttributesElement,
    pub content: Value,
}

#[derive(Debug, Clone)]
pub struct ArraySlice {
    pub items: Vec<Element>,
}

impl ArraySlice {
    pub fn map<F>(&self, f: F) -> ArraySlice
    where
        F: Fn(&Element) -> Element,
    {
        ArraySlice {
            items: self.items.iter().map(f).collect(),
        }
    }

    pub fn filter<F>(&self, f: F) -> ArraySlice
    where
        F: Fn(&Element) -> bool,
    {
        ArraySlice {
            items: self.items.iter().cloned().filter(f).collect(),
        }
    }

    pub fn length(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Trait for schema elements that wrap an ObjectElement
pub trait SchemaElement {
    /// Get a reference to the underlying ObjectElement
    fn object(&self) -> &ObjectElement;
    
    /// Get a mutable reference to the underlying ObjectElement
    fn object_mut(&mut self) -> &mut ObjectElement;
    
    /// Helper method to get a field value
    fn get_field(&self, key: &str) -> Option<&Element> {
        self.object().get(key)
    }
    
    /// Helper method to set a field value
    fn set_field(&mut self, key: &str, value: Element) {
        self.object_mut().set(key, value);
    }
    
    /// Helper method to get a string field
    fn get_string_field(&self, key: &str) -> Option<&StringElement> {
        self.get_field(key).and_then(Element::as_string)
    }
    
    /// Helper method to set a string field
    fn set_string_field(&mut self, key: &str, value: StringElement) {
        self.set_field(key, Element::String(value));
    }
    
    /// Helper method to get an array field
    fn get_array_field(&self, key: &str) -> Option<&ArrayElement> {
        self.get_field(key).and_then(Element::as_array)
    }
    
    /// Helper method to set an array field
    fn set_array_field(&mut self, key: &str, value: ArrayElement) {
        self.set_field(key, Element::Array(value));
    }
    
    /// Helper method to get an object field
    fn get_object_field(&self, key: &str) -> Option<&ObjectElement> {
        self.get_field(key).and_then(Element::as_object)
    }
    
    /// Helper method to set an object field
    fn set_object_field(&mut self, key: &str, value: ObjectElement) {
        self.set_field(key, Element::Object(value));
    }
}
