#![forbid(unsafe_code)]

pub mod host;
pub mod manifest;

pub use host::{PluginError, PluginHost, PluginInfo, PluginRun};
pub use manifest::{
    IndexEntry, ManifestError, PluginIndex, PluginManifest, PluginPermissions, PLUGIN_API_VERSION,
};
