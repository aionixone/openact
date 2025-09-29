use openact_core::types::Trn;

#[test]
fn parse_action_with_version_and_subtype() {
    let s = "trn:openact:tenant1:action/http/get-ip@v2";
    let trn = Trn::new(s.to_string());
    let c = trn.parse_action().expect("should parse");
    assert_eq!(c.tenant, "tenant1");
    assert_eq!(c.connector, "http");
    assert_eq!(c.name, "get-ip");
    assert_eq!(c.version, 2);
}

#[test]
fn parse_action_without_version_defaults_zero() {
    let s = "trn:openact:tenant1:action/http/get-ip";
    let trn = Trn::new(s.to_string());
    let c = trn.parse_action().expect("should parse");
    assert_eq!(c.tenant, "tenant1");
    assert_eq!(c.connector, "http");
    assert_eq!(c.name, "get-ip");
    assert_eq!(c.version, 0);
}

#[test]
fn parse_connection_with_version_and_subtype() {
    let s = "trn:openact:demo:connection/postgres/prod-db@v1";
    let trn = Trn::new(s.to_string());
    let c = trn.parse_connection().expect("should parse");
    assert_eq!(c.tenant, "demo");
    assert_eq!(c.connector, "postgres");
    assert_eq!(c.name, "prod-db");
    assert_eq!(c.version, 1);
}

#[test]
fn invalid_prefix_rejected() {
    let s = "arn:openact:tenant1:action/http/get-ip@v1"; // wrong prefix
    let trn = Trn::new(s.to_string());
    assert!(trn.parse_action().is_none());
}

