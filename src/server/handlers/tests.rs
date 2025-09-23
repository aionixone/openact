#![cfg(test)]
#![cfg(feature = "server")]

use crate::utils::trn;

#[test]
fn test_trn_validation() {
    // Valid TRNs
    assert!(trn::validate_trn("trn:openact:test-tenant:connection/mock").is_ok());
    assert!(trn::validate_trn("trn:openact:test-tenant:task/ping@v1").is_ok());
    
    // Invalid TRNs
    assert!(trn::validate_trn("invalid").is_err());
    assert!(trn::validate_trn("trn:wrong:tenant:resource/id").is_err());
    assert!(trn::validate_trn("trn:openact::resource/id").is_err()); // empty tenant
    assert!(trn::validate_trn("trn:openact:tenant:resource").is_err()); // no slash
    assert!(trn::validate_trn("trn:openact:tenant:/id").is_err()); // empty type
    assert!(trn::validate_trn("trn:openact:tenant:type/").is_err()); // empty id
}

#[test]
fn test_parse_connection_trn() {
    let (tenant, id) = trn::parse_connection_trn("trn:openact:test-tenant:connection/mock@v1").unwrap();
    assert_eq!(tenant, "test-tenant");
    assert_eq!(id, "mock@v1");
    
    assert!(trn::parse_connection_trn("trn:openact:tenant:task/id").is_err());
}

#[test]
fn test_parse_task_trn() {
    let (tenant, id) = trn::parse_task_trn("trn:openact:test-tenant:task/ping@v1").unwrap();
    assert_eq!(tenant, "test-tenant");
    assert_eq!(id, "ping@v1");
    
    assert!(trn::parse_task_trn("trn:openact:tenant:connection/id").is_err());
}
