pub mod api_spec;

// Re-export commonly used types with explicit imports
pub use api_spec::{
    OpenApi30Spec, Info, Contact, License, Server, ServerVariable, ExternalDocumentation,
    Tag, Paths, PathItem, Operation, Parameter, RequestBody, Response, Responses, 
    Callback, Example, Link, Header, SecurityScheme, SecurityRequirement, Components,
    Schema, Discriminator, Xml, Reference, OrReference, MediaType, Encoding,
    // Type aliases
    SchemaOrReference, ResponseOrReference, ParameterOrReference, ExampleOrReference,
    RequestBodyOrReference, HeaderOrReference, SecuritySchemeOrReference, LinkOrReference,
    CallbackOrReference
};