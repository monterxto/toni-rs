//! RPC support for execution context
//!
//! Provides RPC types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with microservice
//! transports (gRPC, Kafka, NATS, Redis, MQTT, RabbitMQ, TCP).

mod rpc_context;
mod rpc_data;

pub use rpc_context::RpcContext;
pub use rpc_data::RpcData;
