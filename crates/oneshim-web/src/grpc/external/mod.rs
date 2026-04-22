//! D13 V2c external gRPC binding.
//! Feature-gated: compile iff `grpc-dashboard-external` enabled.

pub mod cert_resolver;
pub mod metrics;
pub mod tls_config;
