use openact_registry::ConnectorRegistrar;

#[cfg(feature = "http")]
fn http_registrar() -> ConnectorRegistrar {
    openact_connectors::http_registrar()
}

#[cfg(feature = "postgresql")]
fn postgres_registrar() -> ConnectorRegistrar {
    openact_connectors::postgresql_registrar()
}

/// Return all enabled registrars based on crate features
pub fn registrars() -> Vec<ConnectorRegistrar> {
    let mut list = Vec::new();

    #[cfg(feature = "http")]
    list.push(http_registrar());

    #[cfg(feature = "postgresql")]
    list.push(postgres_registrar());

    list
}
