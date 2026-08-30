use std::collections::HashMap;

use repo_service::worker::Worker;
use serde_json::Value;

/// Routes one JSON-RPC method call to its handler. Every later task in this plan adds a `match`
/// arm here; until then, every method reports as unknown rather than hanging or panicking, so
/// this task's transport-only test can prove the stdio loop safely.
pub fn dispatch(
    method: &str,
    _params: Value,
    _repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    Err(format!("unknown method: {method}"))
}
