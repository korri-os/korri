//! Enablement policy for device-local plugins.
//!
//! Every plugin reaches korrid the same way: installed as a package and read
//! from the installation record. korrid compiles no plugin source into itself,
//! so no publisher holds a registration route a stranger cannot use.

use crate::plugin::{PluginError, PluginRegistry};

#[derive(Clone)]
pub enum RegistrySource {
    Installed,
    Selected(std::sync::Arc<PluginRegistry>),
}

impl RegistrySource {
    pub fn registry(&self) -> Result<PluginRegistry, PluginError> {
        match self {
            Self::Installed => installed_registry(),
            Self::Selected(registry) => Ok((**registry).clone()),
        }
    }
}

pub fn installed_registry() -> Result<PluginRegistry, PluginError> {
    PluginRegistry::from_installed(
        crate::plugin_installation::read().map_err(PluginError::Evaluation)?,
    )
}
