// OpenAPI 3.0 specification implementation
// Independent implementation for OpenAPI 3.0 with its own validation logic

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use serde_json::Value;

// Helper functions for default values
fn default_false() -> bool {
    false
}

fn default_empty_vec<T>() -> Vec<T> {
    Vec::new()
}

fn default_empty_map<K, V>() -> HashMap<K, V> {
    HashMap::new()
}

fn default_simple_style() -> String {
    "simple".to_string()
}

/// OpenAPI 3.0 Root Document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenApi30Spec {
    /// The OpenAPI version (must be 3.0.x)
    pub openapi: String,
    /// Metadata about the API
    pub info: Info,
    /// Additional external documentation
    #[serde(rename = "externalDocs")]
    #[serde(default)]
    pub external_docs: Option<ExternalDocumentation>,
    /// Array of Server Objects
    #[serde(default = "default_empty_vec")]
    pub servers: Vec<Server>,
    /// Security requirement objects
    #[serde(default = "default_empty_vec")]
    pub security: Vec<SecurityRequirement>,
    /// List of tags
    #[serde(default = "default_empty_vec")]
    pub tags: Vec<Tag>,
    /// Available paths and operations
    pub paths: Paths,
    /// Reusable components
    #[serde(default)]
    pub components: Option<Components>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Reference Object - OpenAPI 3.0 only has $ref
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    #[serde(rename = "$ref")]
    pub reference: String,
}

/// Helper enum to represent either a direct value or a reference to it
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OrReference<T> {
    Reference(Reference),
    Item(T),
}

/// Info Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    /// The title of the API
    pub title: String,
    /// A description of the API
    #[serde(default)]
    pub description: Option<String>,
    /// A URL to the Terms of Service for the API
    #[serde(rename = "termsOfService")]
    #[serde(default)]
    pub terms_of_service: Option<String>,
    /// Contact information
    #[serde(default)]
    pub contact: Option<Contact>,
    /// License information
    #[serde(default)]
    pub license: Option<License>,
    /// The version of the OpenAPI document
    pub version: String,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Contact Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    /// The identifying name of the contact person/organization
    #[serde(default)]
    pub name: Option<String>,
    /// The URL pointing to the contact information
    #[serde(default)]
    pub url: Option<String>,
    /// The email address of the contact person/organization
    #[serde(default)]
    pub email: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// License Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct License {
    /// The license name used for the API
    pub name: String,
    /// A URL to the license used for the API
    #[serde(default)]
    pub url: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Server Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Server {
    /// A URL to the target host
    pub url: String,
    /// An optional string describing the host designated by the URL
    #[serde(default)]
    pub description: Option<String>,
    /// A map between a variable name and its value
    #[serde(default = "default_empty_map")]
    pub variables: HashMap<String, ServerVariable>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Server Variable Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerVariable {
    /// An enumeration of string values to be used if the substitution options are from a limited set
    #[serde(rename = "enum")]
    #[serde(default)]
    pub allowed_values: Option<Vec<String>>,
    /// The default value to use for substitution
    pub default: String,
    /// An optional description for the server variable
    #[serde(default)]
    pub description: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Components Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Components {
    /// An object to hold reusable Schema Objects
    #[serde(default = "default_empty_map")]
    pub schemas: HashMap<String, SchemaOrReference>,
    /// An object to hold reusable Response Objects
    #[serde(default = "default_empty_map")]
    pub responses: HashMap<String, ResponseOrReference>,
    /// An object to hold reusable Parameter Objects
    #[serde(default = "default_empty_map")]
    pub parameters: HashMap<String, ParameterOrReference>,
    /// An object to hold reusable Example Objects
    #[serde(default = "default_empty_map")]
    pub examples: HashMap<String, ExampleOrReference>,
    /// An object to hold reusable Request Body Objects
    #[serde(rename = "requestBodies")]
    #[serde(default = "default_empty_map")]
    pub request_bodies: HashMap<String, RequestBodyOrReference>,
    /// An object to hold reusable Header Objects
    #[serde(default = "default_empty_map")]
    pub headers: HashMap<String, HeaderOrReference>,
    /// An object to hold reusable Security Scheme Objects
    #[serde(rename = "securitySchemes")]
    #[serde(default = "default_empty_map")]
    pub security_schemes: HashMap<String, SecuritySchemeOrReference>,
    /// An object to hold reusable Link Objects
    #[serde(default = "default_empty_map")]
    pub links: HashMap<String, LinkOrReference>,
    /// An object to hold reusable Callback Objects
    #[serde(default = "default_empty_map")]
    pub callbacks: HashMap<String, CallbackOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Schema Object - OpenAPI 3.0 specific implementation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(rename = "multipleOf")]
    #[serde(default)]
    pub multiple_of: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(rename = "exclusiveMaximum")]
    #[serde(default)]
    pub exclusive_maximum: Option<bool>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(rename = "exclusiveMinimum")]
    #[serde(default)]
    pub exclusive_minimum: Option<bool>,
    #[serde(rename = "maxLength")]
    #[serde(default)]
    pub max_length: Option<i64>,
    #[serde(rename = "minLength")]
    #[serde(default)]
    pub min_length: Option<i64>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(rename = "maxItems")]
    #[serde(default)]
    pub max_items: Option<i64>,
    #[serde(rename = "minItems")]
    #[serde(default)]
    pub min_items: Option<i64>,
    #[serde(rename = "uniqueItems")]
    #[serde(default)]
    pub unique_items: Option<bool>,
    #[serde(rename = "maxProperties")]
    #[serde(default)]
    pub max_properties: Option<i64>,
    #[serde(rename = "minProperties")]
    #[serde(default)]
    pub min_properties: Option<i64>,
    #[serde(default = "default_empty_vec")]
    pub required: Vec<String>,
    #[serde(default = "default_empty_vec")]
    pub r#enum: Vec<Value>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub not: Option<Box<SchemaOrReference>>,
    #[serde(rename = "allOf")]
    #[serde(default = "default_empty_vec")]
    pub all_of: Vec<SchemaOrReference>,
    #[serde(rename = "oneOf")]
    #[serde(default = "default_empty_vec")]
    pub one_of: Vec<SchemaOrReference>,
    #[serde(rename = "anyOf")]
    #[serde(default = "default_empty_vec")]
    pub any_of: Vec<SchemaOrReference>,
    #[serde(default)]
    pub items: Option<Box<SchemaOrReference>>,
    #[serde(default = "default_empty_map")]
    pub properties: HashMap<String, SchemaOrReference>,
    #[serde(rename = "additionalProperties")]
    #[serde(default)]
    pub additional_properties: Option<AdditionalProperties>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub default: Option<Value>,
    /// OpenAPI 3.0 specific: nullable property
    #[serde(default)]
    pub nullable: Option<bool>,
    #[serde(default)]
    pub discriminator: Option<Discriminator>,
    #[serde(rename = "readOnly")]
    #[serde(default)]
    pub read_only: Option<bool>,
    #[serde(rename = "writeOnly")]
    #[serde(default)]
    pub write_only: Option<bool>,
    #[serde(default)]
    pub example: Option<Value>,
    #[serde(rename = "externalDocs")]
    #[serde(default)]
    pub external_docs: Option<ExternalDocumentation>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    /// OpenAPI 3.0 specific: XML metadata
    #[serde(default)]
    pub xml: Option<Xml>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Additional Properties can be either a boolean or a Schema/Reference
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<SchemaOrReference>),
}

/// Discriminator Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discriminator {
    /// The name of the property in the payload that will hold the discriminator value
    #[serde(rename = "propertyName")]
    pub property_name: String,
    /// An object to hold mappings between payload values and schema names or references
    #[serde(default = "default_empty_map")]
    pub mapping: HashMap<String, String>,
}

/// XML Object - OpenAPI 3.0 specific
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Xml {
    /// Replaces the name of the element/attribute used for the described schema property
    #[serde(default)]
    pub name: Option<String>,
    /// The URI of the namespace definition
    #[serde(default)]
    pub namespace: Option<String>,
    /// The prefix to be used for the name
    #[serde(default)]
    pub prefix: Option<String>,
    /// Declares whether the property definition translates to an attribute instead of an element
    #[serde(default = "default_false")]
    pub attribute: bool,
    /// MAY be used only for an array definition
    #[serde(default = "default_false")]
    pub wrapped: bool,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Response Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// A description of the response
    pub description: String,
    /// Maps a header name to its definition
    #[serde(default = "default_empty_map")]
    pub headers: HashMap<String, HeaderOrReference>,
    /// A map containing descriptions of potential response payloads
    #[serde(default = "default_empty_map")]
    pub content: HashMap<String, MediaType>,
    /// A map of operations links that can be followed from the response
    #[serde(default = "default_empty_map")]
    pub links: HashMap<String, LinkOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Media Type Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaType {
    /// The schema defining the content of the request, response, or parameter
    #[serde(default)]
    pub schema: Option<SchemaOrReference>,
    /// Example of the media type
    #[serde(default)]
    pub example: Option<Value>,
    /// Examples of the media type
    #[serde(default = "default_empty_map")]
    pub examples: HashMap<String, ExampleOrReference>,
    /// A map between a property name and its encoding information
    #[serde(default = "default_empty_map")]
    pub encoding: HashMap<String, Encoding>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Example Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    /// Short description for the example
    #[serde(default)]
    pub summary: Option<String>,
    /// Long description for the example
    #[serde(default)]
    pub description: Option<String>,
    /// Embedded literal example
    #[serde(default)]
    pub value: Option<Value>,
    /// A URL that points to the literal example
    #[serde(rename = "externalValue")]
    #[serde(default)]
    pub external_value: Option<String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Header Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// A brief description of the header
    #[serde(default)]
    pub description: Option<String>,
    /// Determines whether this header is mandatory
    #[serde(default = "default_false")]
    pub required: bool,
    /// Specifies that the header is deprecated
    #[serde(default = "default_false")]
    pub deprecated: bool,
    /// Sets the ability to pass empty-valued headers
    #[serde(rename = "allowEmptyValue")]
    #[serde(default = "default_false")]
    pub allow_empty_value: bool,
    /// Describes how the header value will be serialized (only "simple" allowed)
    #[serde(default = "default_simple_style")]
    pub style: String,
    /// When this is true, header values of type array or object generate separate headers
    #[serde(default)]
    pub explode: Option<bool>,
    /// Determines whether the header value should allow reserved characters
    #[serde(rename = "allowReserved")]
    #[serde(default = "default_false")]
    pub allow_reserved: bool,
    /// The schema defining the type used for the header
    #[serde(default)]
    pub schema: Option<SchemaOrReference>,
    /// A map containing the representations for the header
    #[serde(default = "default_empty_map")]
    pub content: HashMap<String, MediaType>,
    /// Example of the header's potential value
    #[serde(default)]
    pub example: Option<Value>,
    /// Examples of the header's potential value
    #[serde(default = "default_empty_map")]
    pub examples: HashMap<String, ExampleOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Paths Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paths {
    /// Path items
    #[serde(flatten)]
    pub paths: HashMap<String, PathItem>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Path Item Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathItem {
    /// Allows for an external definition of this path item
    #[serde(rename = "$ref")]
    #[serde(default)]
    pub reference: Option<String>,
    /// An optional, string summary, intended to apply to all operations in this path
    #[serde(default)]
    pub summary: Option<String>,
    /// An optional, string description, intended to apply to all operations in this path
    #[serde(default)]
    pub description: Option<String>,
    /// A definition of a GET operation on this path
    #[serde(default)]
    pub get: Option<Operation>,
    /// A definition of a PUT operation on this path
    #[serde(default)]
    pub put: Option<Operation>,
    /// A definition of a POST operation on this path
    #[serde(default)]
    pub post: Option<Operation>,
    /// A definition of a DELETE operation on this path
    #[serde(default)]
    pub delete: Option<Operation>,
    /// A definition of a OPTIONS operation on this path
    #[serde(default)]
    pub options: Option<Operation>,
    /// A definition of a HEAD operation on this path
    #[serde(default)]
    pub head: Option<Operation>,
    /// A definition of a PATCH operation on this path
    #[serde(default)]
    pub patch: Option<Operation>,
    /// A definition of a TRACE operation on this path
    #[serde(default)]
    pub trace: Option<Operation>,
    /// An alternative server array to service all operations in this path
    #[serde(default = "default_empty_vec")]
    pub servers: Vec<Server>,
    /// A list of parameters that are applicable for all the operations under this path
    #[serde(default = "default_empty_vec")]
    pub parameters: Vec<ParameterOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Operation Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// A list of tags for API documentation control
    #[serde(default = "default_empty_vec")]
    pub tags: Vec<String>,
    /// A short summary of what the operation does
    #[serde(default)]
    pub summary: Option<String>,
    /// A verbose explanation of the operation behavior
    #[serde(default)]
    pub description: Option<String>,
    /// Additional external documentation for this operation
    #[serde(rename = "externalDocs")]
    #[serde(default)]
    pub external_docs: Option<ExternalDocumentation>,
    /// Unique string used to identify the operation
    #[serde(rename = "operationId")]
    #[serde(default)]
    pub operation_id: Option<String>,
    /// A list of parameters that are applicable for this operation
    #[serde(default = "default_empty_vec")]
    pub parameters: Vec<ParameterOrReference>,
    /// The request body applicable for this operation
    #[serde(rename = "requestBody")]
    #[serde(default)]
    pub request_body: Option<RequestBodyOrReference>,
    /// The list of possible responses as they are returned from executing this operation
    pub responses: Responses,
    /// A map of possible out-of band callbacks related to the parent operation
    #[serde(default = "default_empty_map")]
    pub callbacks: HashMap<String, CallbackOrReference>,
    /// Declares this operation to be deprecated
    #[serde(default = "default_false")]
    pub deprecated: bool,
    /// A declaration of which security mechanisms can be used for this operation
    #[serde(default = "default_empty_vec")]
    pub security: Vec<SecurityRequirement>,
    /// An alternative server array to service this operation
    #[serde(default = "default_empty_vec")]
    pub servers: Vec<Server>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Responses Object
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Responses {
    /// The documentation of responses other than the ones declared for specific HTTP response codes
    #[serde(default)]
    pub default: Option<ResponseOrReference>,
    /// HTTP status code responses
    #[serde(flatten)]
    pub responses: HashMap<String, ResponseOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

impl<'de> Deserialize<'de> for Responses {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        struct ResponsesVisitor;

        impl<'de> Visitor<'de> for ResponsesVisitor {
            type Value = Responses;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a responses object")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut default = None;
                let mut responses = HashMap::new();
                let mut extensions = HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "default" {
                        default = Some(map.next_value()?);
                    } else if key.starts_with("x-") {
                        // Handle extension fields
                        extensions.insert(key, map.next_value()?);
                    } else if is_http_status_code(&key) {
                        // Handle HTTP status code responses (including patterns like "2XX")
                        responses.insert(key, map.next_value()?);
                    } else {
                        // Unknown field - try to parse as extension but ignore errors
                        let _: serde_json::Value = map.next_value()?;
                        // Optionally log warning about unknown field
                    }
                }

                Ok(Responses {
                    default,
                    responses,
                    extensions,
                })
            }
        }

        deserializer.deserialize_map(ResponsesVisitor)
    }
}

/// Check if a string represents a valid HTTP status code or pattern
fn is_http_status_code(key: &str) -> bool {
    // Check for exact status codes (100-599)
    if let Ok(code) = key.parse::<u32>() {
        return (100..=599).contains(&code);
    }
    
    // Check for status code patterns like "1XX", "2XX", etc.
    if key.len() == 3 && key.ends_with("XX") {
        if let Some(first_char) = key.chars().next() {
            return ('1'..='5').contains(&first_char);
        }
    }
    
    false
}

/// Security Requirement Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityRequirement(pub HashMap<String, Vec<String>>);

/// Tag Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// The name of the tag
    pub name: String,
    /// A description for the tag
    #[serde(default)]
    pub description: Option<String>,
    /// Additional external documentation for this tag
    #[serde(rename = "externalDocs")]
    #[serde(default)]
    pub external_docs: Option<ExternalDocumentation>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// External Documentation Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalDocumentation {
    /// A description of the target documentation
    #[serde(default)]
    pub description: Option<String>,
    /// The URL for the target documentation
    pub url: String,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Parameter Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    /// The name of the parameter
    pub name: String,
    /// The location of the parameter
    #[serde(rename = "in")]
    pub location: String,
    /// A brief description of the parameter
    #[serde(default)]
    pub description: Option<String>,
    /// Determines whether this parameter is mandatory
    #[serde(default = "default_false")]
    pub required: bool,
    /// Specifies that a parameter is deprecated
    #[serde(default = "default_false")]
    pub deprecated: bool,
    /// Sets the ability to pass empty-valued parameters
    #[serde(rename = "allowEmptyValue")]
    #[serde(default = "default_false")]
    pub allow_empty_value: bool,
    /// Describes how the parameter value will be serialized
    #[serde(default)]
    pub style: Option<String>,
    /// When this is true, parameter values of type array or object generate separate parameters
    #[serde(default)]
    pub explode: Option<bool>,
    /// Determines whether the parameter value should allow reserved characters
    #[serde(rename = "allowReserved")]
    #[serde(default = "default_false")]
    pub allow_reserved: bool,
    /// The schema defining the type used for the parameter
    #[serde(default)]
    pub schema: Option<SchemaOrReference>,
    /// A map containing the representations for the parameter
    #[serde(default = "default_empty_map")]
    pub content: HashMap<String, MediaType>,
    /// Example of the parameter's potential value
    #[serde(default)]
    pub example: Option<Value>,
    /// Examples of the parameter's potential value
    #[serde(default = "default_empty_map")]
    pub examples: HashMap<String, ExampleOrReference>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Request Body Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestBody {
    /// A brief description of the request body
    #[serde(default)]
    pub description: Option<String>,
    /// The content of the request body
    pub content: HashMap<String, MediaType>,
    /// Determines if the request body is required in the request
    #[serde(default = "default_false")]
    pub required: bool,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Security Scheme Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SecurityScheme {
    #[serde(rename = "apiKey")]
    ApiKey {
        /// The name of the header, query or cookie parameter to be used
        name: String,
        /// The location of the API key
        #[serde(rename = "in")]
        location: String,
        /// A description for security scheme
        #[serde(default)]
        description: Option<String>,
        /// Extension fields
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "http")]
    Http {
        /// The name of the HTTP Authorization scheme to be used
        scheme: String,
        /// A hint to the client to identify how the bearer token is formatted
        #[serde(rename = "bearerFormat")]
        #[serde(default)]
        bearer_format: Option<String>,
        /// A description for security scheme
        #[serde(default)]
        description: Option<String>,
        /// Extension fields
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        /// An object containing configuration information for the flow types supported
        flows: OAuthFlows,
        /// A description for security scheme
        #[serde(default)]
        description: Option<String>,
        /// Extension fields
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
    #[serde(rename = "openIdConnect")]
    OpenIdConnect {
        /// OpenId Connect URL to discover OAuth2 configuration values
        #[serde(rename = "openIdConnectUrl")]
        open_id_connect_url: String,
        /// A description for security scheme
        #[serde(default)]
        description: Option<String>,
        /// Extension fields
        #[serde(flatten)]
        extensions: HashMap<String, Value>,
    },
}

/// OAuth Flows Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Implicit flow
    #[serde(default)]
    pub implicit: Option<ImplicitOAuthFlow>,
    /// Configuration for the OAuth Resource Owner Password flow
    #[serde(default)]
    pub password: Option<PasswordOAuthFlow>,
    /// Configuration for the OAuth Client Credentials flow
    #[serde(rename = "clientCredentials")]
    #[serde(default)]
    pub client_credentials: Option<ClientCredentialsFlow>,
    /// Configuration for the OAuth Authorization Code flow
    #[serde(rename = "authorizationCode")]
    #[serde(default)]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Implicit OAuth Flow Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImplicitOAuthFlow {
    /// The authorization URL to be used for this flow
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    /// The URL to be used for obtaining refresh tokens
    #[serde(rename = "refreshUrl")]
    #[serde(default)]
    pub refresh_url: Option<String>,
    /// The available scopes for the OAuth2 security scheme
    pub scopes: HashMap<String, String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Password OAuth Flow Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PasswordOAuthFlow {
    /// The token URL to be used for this flow
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The URL to be used for obtaining refresh tokens
    #[serde(rename = "refreshUrl")]
    #[serde(default)]
    pub refresh_url: Option<String>,
    /// The available scopes for the OAuth2 security scheme
    pub scopes: HashMap<String, String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Client Credentials Flow Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientCredentialsFlow {
    /// The token URL to be used for this flow
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The URL to be used for obtaining refresh tokens
    #[serde(rename = "refreshUrl")]
    #[serde(default)]
    pub refresh_url: Option<String>,
    /// The available scopes for the OAuth2 security scheme
    pub scopes: HashMap<String, String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Authorization Code OAuth Flow Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizationCodeOAuthFlow {
    /// The authorization URL to be used for this flow
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    /// The token URL to be used for this flow
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The URL to be used for obtaining refresh tokens
    #[serde(rename = "refreshUrl")]
    #[serde(default)]
    pub refresh_url: Option<String>,
    /// The available scopes for the OAuth2 security scheme
    pub scopes: HashMap<String, String>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Link Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// The name of an existing, resolvable OAS operation
    #[serde(rename = "operationId")]
    #[serde(default)]
    pub operation_id: Option<String>,
    /// A relative or absolute URI reference to an OAS operation
    #[serde(rename = "operationRef")]
    #[serde(default)]
    pub operation_ref: Option<String>,
    /// A map representing parameters to pass to an operation
    #[serde(default = "default_empty_map")]
    pub parameters: HashMap<String, Value>,
    /// A literal value or expression to use as a request body
    #[serde(rename = "requestBody")]
    #[serde(default)]
    pub request_body: Option<Value>,
    /// A description of the link
    #[serde(default)]
    pub description: Option<String>,
    /// A server object to be used by the target operation
    #[serde(default)]
    pub server: Option<Server>,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Callback Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Callback {
    /// Path items for callback
    #[serde(flatten)]
    pub paths: HashMap<String, PathItem>,
    /// Extension fields  
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

/// Encoding Object
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Encoding {
    /// The Content-Type for encoding a specific property
    #[serde(rename = "contentType")]
    #[serde(default)]
    pub content_type: Option<String>,
    /// A map allowing additional information to be provided as headers
    #[serde(default = "default_empty_map")]
    pub headers: HashMap<String, HeaderOrReference>,
    /// Describes how a specific property value will be serialized
    #[serde(default)]
    pub style: Option<String>,
    /// When this is true, property values of type array or object generate separate parameters
    #[serde(default)]
    pub explode: Option<bool>,
    /// Determines whether the parameter value should allow reserved characters
    #[serde(rename = "allowReserved")]
    #[serde(default = "default_false")]
    pub allow_reserved: bool,
    /// Extension fields
    #[serde(flatten)]
    pub extensions: HashMap<String, Value>,
}

// Union types using the OrReference pattern
pub type SchemaOrReference = OrReference<Schema>;
pub type ResponseOrReference = OrReference<Response>;
pub type ParameterOrReference = OrReference<Parameter>;
pub type ExampleOrReference = OrReference<Example>;
pub type RequestBodyOrReference = OrReference<RequestBody>;
pub type HeaderOrReference = OrReference<Header>;
pub type SecuritySchemeOrReference = OrReference<SecurityScheme>;
pub type LinkOrReference = OrReference<Link>;
pub type CallbackOrReference = OrReference<Callback>;

// Reference implementation
impl Reference {
    pub fn new(reference: String) -> Self {
        Self {
            reference,
        }
    }
}

// OpenAPI 3.0 specific validation functions
impl OpenApi30Spec {
    /// Validate the OpenAPI version format
    pub fn validate_version(&self) -> Result<(), String> {
        if !self.openapi.starts_with("3.0.") {
            return Err(format!("Invalid OpenAPI version: {}. Must be 3.0.x", self.openapi));
        }
        Ok(())
    }
}

impl Info {
    /// Validate that required fields are not empty
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Info title cannot be empty".to_string());
        }
        if self.version.trim().is_empty() {
            return Err("Info version cannot be empty".to_string());
        }
        Ok(())
    }
}

impl Parameter {
    /// Validate the parameter according to OpenAPI 3.0 rules
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Parameter name cannot be empty".to_string());
        }

        match self.location.as_str() {
            "path" | "query" | "header" | "cookie" => {},
            _ => return Err(format!("Invalid parameter location: {}", self.location)),
        }

        // Path parameters must be required
        if self.location == "path" && !self.required {
            return Err("Path parameters must be required".to_string());
        }

        // Validate style based on location
        if let Some(style) = &self.style {
            match self.location.as_str() {
                "path" => {
                    match style.as_str() {
                        "matrix" | "label" | "simple" => {},
                        _ => return Err(format!("Invalid style '{}' for path parameter", style)),
                    }
                },
                "query" => {
                    match style.as_str() {
                        "form" | "spaceDelimited" | "pipeDelimited" | "deepObject" => {},
                        _ => return Err(format!("Invalid style '{}' for query parameter", style)),
                    }
                },
                "header" => {
                    if style != "simple" {
                        return Err(format!("Invalid style '{}' for header parameter, only 'simple' allowed", style));
                    }
                },
                "cookie" => {
                    if style != "form" {
                        return Err(format!("Invalid style '{}' for cookie parameter, only 'form' allowed", style));
                    }
                },
                _ => {},
            }
        }

        // Either schema or content must be present, but not both
        match (&self.schema, self.content.is_empty()) {
            (Some(_), false) => return Err("Parameter cannot have both schema and content".to_string()),
            (None, true) => return Err("Parameter must have either schema or content".to_string()),
            _ => {},
        }

        Ok(())
    }

    /// Get the effective explode value based on style
    pub fn effective_explode(&self) -> bool {
        if let Some(explode) = self.explode {
            return explode;
        }

        // Default explode based on style
        match self.style.as_deref() {
            Some("form") => true,
            _ => false,
        }
    }
}

impl Header {
    /// Get the effective explode value based on style
    pub fn effective_explode(&self) -> bool {
        if let Some(explode) = self.explode {
            return explode;
        }

        // For headers, style is always "simple" and explode defaults to false
        false
    }
}

impl Tag {
    /// Validate tag requirements
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Tag name cannot be empty".to_string());
        }
        Ok(())
    }
}

impl ExternalDocumentation {
    /// Validate external documentation requirements
    pub fn validate(&self) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Err("External documentation URL cannot be empty".to_string());
        }
        Ok(())
    }
}

impl Responses {
    /// Validate that responses object has at least one response
    pub fn validate(&self) -> Result<(), String> {
        if self.default.is_none() && self.responses.is_empty() {
            return Err("Responses object must have at least one response".to_string());
        }
        Ok(())
    }

    pub fn new() -> Self {
        Self {
            default: None,
            responses: HashMap::new(),
            extensions: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_openapi_version_validation() {
        let spec = OpenApi30Spec {
            openapi: "3.0.0".to_string(),
            info: Info {
                title: "Test API".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: HashMap::new(),
            },
            external_docs: None,
            servers: vec![],
            security: vec![],
            tags: vec![],
            paths: Paths {
                paths: HashMap::new(),
                extensions: HashMap::new(),
            },
            components: None,
            extensions: HashMap::new(),
        };

        assert!(spec.validate_version().is_ok());

        let mut invalid_spec = spec.clone();
        invalid_spec.openapi = "3.1.0".to_string();
        assert!(invalid_spec.validate_version().is_err());

        let mut invalid_spec2 = spec.clone();
        invalid_spec2.openapi = "2.0.0".to_string();
        assert!(invalid_spec2.validate_version().is_err());
    }

    #[test]
    fn test_parameter_validation() {
        let parameter = Parameter {
            name: "test".to_string(),
            location: "query".to_string(),
            description: None,
            required: false,
            deprecated: false,
            allow_empty_value: false,
            style: None,
            explode: None,
            allow_reserved: false,
            schema: Some(SchemaOrReference::Item(Schema {
                title: None,
                multiple_of: None,
                maximum: None,
                exclusive_maximum: None,
                minimum: None,
                exclusive_minimum: None,
                max_length: None,
                min_length: None,
                pattern: None,
                max_items: None,
                min_items: None,
                unique_items: None,
                max_properties: None,
                min_properties: None,
                required: vec![],
                r#enum: vec![],
                r#type: Some("string".to_string()),
                not: None,
                all_of: vec![],
                one_of: vec![],
                any_of: vec![],
                items: None,
                properties: HashMap::new(),
                additional_properties: None,
                description: None,
                format: None,
                default: None,
                nullable: None,
                discriminator: None,
                read_only: None,
                write_only: None,
                example: None,
                external_docs: None,
                deprecated: None,
                xml: None,
                extensions: HashMap::new(),
            })),
            content: HashMap::new(),
            example: None,
            examples: HashMap::new(),
            extensions: HashMap::new(),
        };

        assert!(parameter.validate().is_ok());

        // Test effective_explode functionality
        assert_eq!(parameter.effective_explode(), false); // No style, defaults to false

        let mut form_param = parameter.clone();
        form_param.style = Some("form".to_string());
        assert_eq!(form_param.effective_explode(), true); // form style defaults to true

        let mut explicit_param = parameter.clone();
        explicit_param.explode = Some(true);
        assert_eq!(explicit_param.effective_explode(), true); // Explicit value takes precedence
    }

    #[test]
    fn test_security_scheme_serialization() {
        let api_key = SecurityScheme::ApiKey {
            name: "api_key".to_string(),
            location: "header".to_string(),
            description: None,
            extensions: HashMap::new(),
        };

        let json = serde_json::to_string(&api_key).unwrap();
        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        assert_eq!(api_key, deserialized);
    }

    #[test]
    fn test_reference_object() {
        let reference = Reference::new("#/components/schemas/Pet".to_string());
        let json = serde_json::to_string(&reference).unwrap();
        assert!(json.contains("$ref"));
    }

    #[test]
    fn test_header_explode_behavior() {
        let header = Header {
            description: None,
            required: false,
            deprecated: false,
            allow_empty_value: false,
            style: "simple".to_string(),
            explode: None,
            allow_reserved: false,
            schema: None,
            content: HashMap::new(),
            example: None,
            examples: HashMap::new(),
            extensions: HashMap::new(),
        };

        // Headers always default explode to false
        assert_eq!(header.effective_explode(), false);

        let mut explicit_header = header.clone();
        explicit_header.explode = Some(true);
        assert_eq!(explicit_header.effective_explode(), true);
    }
}

impl Default for Schema {
    fn default() -> Self {
        Self {
            title: None,
            multiple_of: None,
            maximum: None,
            exclusive_maximum: None,
            minimum: None,
            exclusive_minimum: None,
            max_length: None,
            min_length: None,
            pattern: None,
            max_items: None,
            min_items: None,
            unique_items: None,
            max_properties: None,
            min_properties: None,
            required: Vec::new(),
            r#enum: Vec::new(),
            r#type: None,
            not: None,
            all_of: Vec::new(),
            one_of: Vec::new(),
            any_of: Vec::new(),
            items: None,
            properties: HashMap::new(),
            additional_properties: None,
            description: None,
            format: None,
            default: None,
            nullable: None,
            discriminator: None,
            read_only: None,
            write_only: None,
            example: None,
            external_docs: None,
            deprecated: None,
            xml: None,
            extensions: HashMap::new(),
        }
    }
}