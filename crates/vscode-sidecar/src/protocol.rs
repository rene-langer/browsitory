//! Line-delimited JSON-RPC 2.0 over stdio — the wire protocol between the VSCode extension
//! host (eventually relaying from the webview via `postMessage`) and this sidecar process. One
//! JSON object per line on stdin (a request), one JSON object per line on stdout (a response).

use repo_service::wire::ErrorDto;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC error. `data` carries the shared error envelope (`repo_service::wire::ErrorDto`,
/// #496): the same `message` and the `kind` the frontend switches on.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: ErrorDto,
}

impl JsonRpcResponse {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                data: ErrorDto::from(message.clone()),
                message,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_from_json_rpc_shape() {
        let json =
            r#"{"jsonrpc":"2.0","id":7,"method":"get_status","params":{"repoPath":"/tmp/x"}}"#;

        let request: JsonRpcRequest = serde_json::from_str(json).expect("parse request");

        assert_eq!(request.id, 7);
        assert_eq!(request.method, "get_status");
        assert_eq!(request.params["repoPath"], "/tmp/x");
    }

    #[test]
    fn ok_response_serializes_without_an_error_field() {
        let response = JsonRpcResponse::ok(3, serde_json::json!({"a": 1}));

        let serialized = serde_json::to_string(&response).expect("serialize response");

        assert_eq!(serialized, r#"{"jsonrpc":"2.0","id":3,"result":{"a":1}}"#);
    }

    #[test]
    fn err_response_serializes_without_a_result_field() {
        let response = JsonRpcResponse::err(4, "boom".to_string());

        let serialized = serde_json::to_string(&response).expect("serialize response");

        assert_eq!(
            serialized,
            r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"boom","data":{"kind":"Other","message":"boom"}}}"#
        );
    }

    /// #496: the error's `data` names the kind the frontend switches on.
    #[test]
    fn err_response_carries_the_error_kind() {
        let response = JsonRpcResponse::err(5, repo_service::wire::WORKER_STOPPED.to_string());

        let serialized = serde_json::to_string(&response).expect("serialize response");

        assert_eq!(
            serialized,
            r#"{"jsonrpc":"2.0","id":5,"error":{"code":-32000,"message":"worker thread stopped","data":{"kind":"WorkerStopped","message":"worker thread stopped"}}}"#
        );
    }
}
