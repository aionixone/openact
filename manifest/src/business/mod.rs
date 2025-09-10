// Business logic modules
pub mod validator;
pub mod resolver;
pub mod extractor;
pub mod trn_generator;

// Re-export commonly used types from validator
pub use validator::{OpenApiValidator, ValidationResult, ValidationError, ValidationWarning};

// Re-export commonly used types from resolver
pub use resolver::{ReferenceResolver, ResolvedDocument};

// Re-export commonly used types from extractor
pub use extractor::{
    EndpointExtractor, ApiEndpoint, EndpointParameter, 
    RequestBodyInfo, ResponseInfo, SchemaInfo,
    TrnConfig, GeneratedTrn
};

// Re-export Action TRN generator types
pub use trn_generator::{
    ActionTrnConfig, ActionTrnGenerator, ActionTrn, ActionTrnError, ActionTrnResult, ActionTrnMetadata,
    ParsedTrn, validate_action_trn, generate_action_trn
};
