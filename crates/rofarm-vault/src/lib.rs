#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_debug_implementations)]

pub mod audit;
pub mod backup;
pub mod binding;
pub mod crypto;
pub mod error;
pub mod import;
pub mod kdf;
pub mod vault;

pub use binding::{BindingKind, DpapiBinder, Fido2Binder, KeyBinder, MockBinder, TpmBinder};
pub use error::{VaultError, VaultErrorPulse, VaultResult};
pub use kdf::{generate_recovery_code, KdfParams};
pub use vault::{Vault, VaultHeader, VaultRecord, VAULT_VERSION};
