#![forbid(unsafe_code)]

pub mod auth;
pub mod server;

pub use auth::{verify_totp, Device, PairRequest, PairResponse, RemoteAuth};
pub use server::{start, ApiConfig, ApiHandle};
