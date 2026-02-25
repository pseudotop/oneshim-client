//!

#[cfg(feature = "grpc")]
pub mod common {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.common.rs");
}

#[cfg(feature = "grpc")]
pub mod auth {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.auth.rs");
}

#[cfg(feature = "grpc")]
pub mod user_context {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.user_context.rs");
}

#[cfg(feature = "grpc")]
pub mod monitoring {
    #![allow(clippy::all)]
    #![allow(warnings)]
    include!("generated/oneshim.v1.monitoring.rs");
}
