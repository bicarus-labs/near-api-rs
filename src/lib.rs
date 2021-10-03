mod examples;
pub mod types;

use std::str;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;

use near_jsonrpc_primitives::errors::RpcError;
use near_jsonrpc_primitives::message::{from_slice, Message};
use near_jsonrpc_primitives::types::changes::{
    RpcStateChangesInBlockByTypeRequest, RpcStateChangesInBlockByTypeResponse,
};
use near_jsonrpc_primitives::types::validator::RpcValidatorsOrderedRequest;
use near_primitives::hash::CryptoHash;
use near_primitives::types::{AccountId, BlockId, BlockReference, MaybeBlockId, ShardId};
use near_primitives::views::validator_stake_view::ValidatorStakeView;
use near_primitives::views::{
    BlockView, ChunkView, EpochValidatorInfo, GasPriceView, StatusResponse,
};
use types::FinalExecutionOutcomeView;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChunkId {
    BlockShardId(BlockId, ShardId),
    Hash(CryptoHash),
}

/// Timeout for establishing connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

type RpcRequest<T> = Result<T, RpcError>;

/// Prepare a `RPCRequest` with a given client, server address, method and parameters.
async fn call_method<P, R>(
    client: &Client,
    server_addr: &str,
    method: &str,
    params: P,
) -> RpcRequest<R>
where
    P: Serialize,
    R: serde::de::DeserializeOwned + 'static,
{
    let request = Message::request(
        method.to_string(),
        Some(serde_json::to_value(&params).unwrap()),
    );

    // TODO: simplify this.
    let result = client
        .post(server_addr)
        .json(&request)
        .send()
        .await
        .map_err(|err| RpcError::new_internal_error(None, format!("{:?}", err)));

    let resp = match result {
        Err(why) => Err(why),
        Ok(resp) => Ok(resp.bytes().await.unwrap()),
    };

    resp.and_then(|response| {
        from_slice(response.to_vec().as_slice())
            .map_err(|err| RpcError::parse_error(format!("Error {:?} in {:?}", err, response)))
    })
    .and_then(|msg| match msg {
        Message::Response(msg) => msg.result.and_then(|v| {
            serde_json::from_value(v.clone())
                .map_err(|err| RpcError::parse_error(format!("Failed to parse: {:?}", err)))
        }),
        _ => Err(RpcError::parse_error(format!(
            "Failed to parse JSON RPC response"
        ))),
    })
}

/// Expands a variable list of parameters into its serializable form. Is needed to make the params
/// of a nullary method equal to `[]` instead of `()` and thus make sure it serializes to `[]`
/// instead of `null`.
#[doc(hidden)]
macro_rules! expand_params {
    () => ([] as [(); 0]);
    ($($arg_name:ident,)+) => (($($arg_name,)+))
}

/// Generates JSON-RPC 2.0 client structs with automatic serialization
/// and deserialization. Method calls get correct types automatically.
macro_rules! jsonrpc_client {
    (
        $(#[$struct_attr:meta])*
        pub struct $struct_name:ident {$(
            $(#[$attr:meta])*
            pub fn $method:ident(&$selff:ident $(, $arg_name:ident: $arg_ty:ty)*)
                -> RpcRequest<$return_ty:ty>;
        )*}
    ) => (
        $(#[$struct_attr])*
        pub struct $struct_name {
            pub server_addr: String,
            pub client: Client,
        }

        impl $struct_name {
            /// Creates a new RPC client backed by the given transport implementation.
            pub fn new(server_addr: &str, client: Client) -> Self {
                $struct_name { server_addr: server_addr.to_string(), client }
            }

            $(
                $(#[$attr])*
                pub async fn $method(&$selff $(, $arg_name: $arg_ty)*)
                    -> RpcRequest<$return_ty>
                {
                    let method = String::from(stringify!($method));
                    let params = expand_params!($($arg_name,)*);
                    call_method(&$selff.client, &$selff.server_addr, &method, params).await
                }
            )*
        }
    )
}

jsonrpc_client!(
    #[derive(Clone)]
    pub struct JsonRpcClient {
        pub fn broadcast_tx_async(&self, tx: String) -> RpcRequest<String>;
        pub fn broadcast_tx_commit(&self, tx: String) -> RpcRequest<FinalExecutionOutcomeView>;
        pub fn status(&self) -> RpcRequest<StatusResponse>;
        #[allow(non_snake_case)]
        pub fn EXPERIMENTAL_check_tx(&self, tx: String) -> RpcRequest<serde_json::Value>;
        #[allow(non_snake_case)]
        pub fn EXPERIMENTAL_genesis_config(&self) -> RpcRequest<serde_json::Value>;
        #[allow(non_snake_case)]
        pub fn EXPERIMENTAL_broadcast_tx_sync(&self, tx: String) -> RpcRequest<serde_json::Value>;
        #[allow(non_snake_case)]
        pub fn EXPERIMENTAL_tx_status(&self, tx: String) -> RpcRequest<serde_json::Value>;
        pub fn health(&self) -> RpcRequest<()>;
        pub fn tx(&self, hash: String, account_id: AccountId) -> RpcRequest<FinalExecutionOutcomeView>;
        pub fn chunk(&self, id: ChunkId) -> RpcRequest<ChunkView>;
        pub fn validators(&self, block_id: MaybeBlockId) -> RpcRequest<EpochValidatorInfo>;
        pub fn gas_price(&self, block_id: MaybeBlockId) -> RpcRequest<GasPriceView>;
    }
);

impl JsonRpcClient {
    /// This is a soft-deprecated method to do query RPC request with a path and data positional
    /// parameters.
    pub async fn query_by_path(
        &self,
        path: String,
        data: String,
    ) -> RpcRequest<near_jsonrpc_primitives::types::query::RpcQueryResponse> {
        call_method(&self.client, &self.server_addr, "query", [path, data]).await
    }

    pub async fn query(
        &self,
        request: near_jsonrpc_primitives::types::query::RpcQueryRequest,
    ) -> RpcRequest<near_jsonrpc_primitives::types::query::RpcQueryResponse> {
        call_method(&self.client, &self.server_addr, "query", request).await
    }

    pub async fn block_by_id(&self, block_id: BlockId) -> RpcRequest<BlockView> {
        call_method(&self.client, &self.server_addr, "block", [block_id]).await
    }

    pub async fn block(&self, request: BlockReference) -> RpcRequest<BlockView> {
        call_method(&self.client, &self.server_addr, "block", request).await
    }

    #[allow(non_snake_case)]
    pub async fn EXPERIMENTAL_changes(
        &self,
        request: RpcStateChangesInBlockByTypeRequest,
    ) -> RpcRequest<RpcStateChangesInBlockByTypeResponse> {
        call_method(
            &self.client,
            &self.server_addr,
            "EXPERIMENTAL_changes",
            request,
        )
        .await
    }

    #[allow(non_snake_case)]
    pub async fn EXPERIMENTAL_validators_ordered(
        &self,
        request: RpcValidatorsOrderedRequest,
    ) -> RpcRequest<Vec<ValidatorStakeView>> {
        call_method(
            &self.client,
            &self.server_addr,
            "EXPERIMENTAL_validators_ordered",
            request,
        )
        .await
    }

    #[allow(non_snake_case)]
    pub async fn EXPERIMENTAL_receipt(
        &self,
        request: near_jsonrpc_primitives::types::receipts::RpcReceiptRequest,
    ) -> RpcRequest<near_jsonrpc_primitives::types::receipts::RpcReceiptResponse> {
        call_method(
            &self.client,
            &self.server_addr,
            "EXPERIMENTAL_receipt",
            request,
        )
        .await
    }

    #[allow(non_snake_case)]
    pub async fn EXPERIMENTAL_protocol_config(
        &self,
        request: near_jsonrpc_primitives::types::config::RpcProtocolConfigRequest,
    ) -> RpcRequest<near_jsonrpc_primitives::types::config::RpcProtocolConfigResponse> {
        call_method(
            &self.client,
            &self.server_addr,
            "EXPERIMENTAL_protocol_config",
            request,
        )
        .await
    }
}

fn create_client() -> Client {
    Client::builder()
        .timeout(CONNECT_TIMEOUT)
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Create new JSON RPC client that connects to the given address.
pub fn new_client(server_addr: &str) -> JsonRpcClient {
    JsonRpcClient::new(server_addr, create_client())
}
