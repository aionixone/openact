use crate::{error::{CoreError, CoreResult}, store::ActionRepository, ConnectorKind, Trn};

/// Resolve an action TRN by (tenant, connector, name) and an optional version.
/// When `version` is None, selects the highest available version for the tuple.
pub async fn resolve_action_trn_by_name<A>(
    store: &A,
    tenant: &str,
    connector: &ConnectorKind,
    name: &str,
    version: Option<i64>,
) -> CoreResult<Trn>
where
    A: ActionRepository + ?Sized,
{
    // List all actions for the connector (already canonicalized by caller)
    let records = store
        .list_by_connector(connector)
        .await
        .map_err(|e| CoreError::Db(e.to_string()))?;

    // Filter by name and tenant (parsed from TRN)
    let mut candidates: Vec<_> = records
        .into_iter()
        .filter(|r| r.name == name)
        .filter(|r| r.trn.parse_action().map(|c| c.tenant == tenant).unwrap_or(false))
        .collect();

    if candidates.is_empty() {
        return Err(CoreError::NotFound(format!(
            "Action not found: {}.{} (tenant: {})",
            connector, name, tenant
        )));
    }

    // Sort by parsed version and pick
    candidates.sort_by_key(|r| r.trn.parse_action().map(|c| c.version).unwrap_or(0));

    if let Some(v) = version {
        candidates
            .into_iter()
            .rev()
            .find(|r| r.trn.parse_action().map(|c| c.version == v).unwrap_or(false))
            .map(|r| r.trn)
            .ok_or_else(|| {
                CoreError::NotFound(format!(
                    "Action not found: {}.{}@v{} (tenant: {})",
                    connector, name, v, tenant
                ))
            })
    } else {
        Ok(candidates.pop().unwrap().trn)
    }
}

