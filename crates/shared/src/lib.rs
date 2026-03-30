pub mod shared {
    tonic::include_proto!("shared");
}

pub mod client_api {
    tonic::include_proto!("client_api");
}

pub mod worker_api {
    tonic::include_proto!("worker_api");
}

pub mod executor {
    tonic::include_proto!("executor");
}

pub use shared::*;
pub use client_api::*;
pub use worker_api::*;
pub use executor::*;
