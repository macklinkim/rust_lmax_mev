//! NodeError + alloy-error classification per P2-A v0.4 Risk Decision 3.
//!
//! 5-variant `#[non_exhaustive]` enum. **Only `Transport` triggers HTTP
//! fallback in `NodeProvider`.** `Rpc` (server JSON-RPC error response)
//! and `Decode` are authoritative.

use alloy::transports::{RpcError, TransportErrorKind};

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// WebSocket handshake / connect failure.
    #[error("WebSocket connect failed: {0}")]
    WsConnect(String),

    /// TRUE transport failure: connect refused, TCP timeout, socket
    /// reset, DNS failure. **The only variant that triggers HTTP
    /// fallback in `NodeProvider`.**
    #[error("transport error: {0}")]
    Transport(String),

    /// JSON-RPC error response from the node (e.g., `eth_call` revert,
    /// invalid params, method not found). Authoritative; never fails
    /// over.
    #[error("JSON-RPC error: {0}")]
    Rpc(String),

    /// Response body decode/deserialize failure (malformed JSON,
    /// unexpected schema). Authoritative; never fails over.
    #[error("decode error: {0}")]
    Decode(String),

    /// Channel/handle closed by caller-side drop.
    #[error("provider closed")]
    Closed,
}

/// Classify an alloy `TransportError` into our 5-variant `NodeError`.
/// Critical: an `RpcError::ErrorResp` is a server JSON-RPC error
/// response — it must NOT collapse into `Transport` (which would
/// trigger fallback for what is actually an authoritative answer).
pub fn classify(e: RpcError<TransportErrorKind>) -> NodeError {
    match e {
        RpcError::ErrorResp(resp) => NodeError::Rpc(resp.to_string()),
        RpcError::DeserError { err, .. } => NodeError::Decode(err.to_string()),
        RpcError::SerError(err) => NodeError::Decode(err.to_string()),
        RpcError::NullResp => {
            NodeError::Decode("server returned null when non-null expected".into())
        }
        RpcError::UnsupportedFeature(s) => NodeError::Rpc(format!("unsupported feature: {s}")),
        RpcError::LocalUsageError(err) => NodeError::Rpc(format!("local usage error: {err}")),
        RpcError::Transport(kind) => NodeError::Transport(kind.to_string()),
    }
}
