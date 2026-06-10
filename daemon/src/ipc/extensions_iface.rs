//! D-Bus interface: org.opengg.Daemon.Extensions

use std::sync::Arc;
use zbus::interface;

use crate::extensions::ExtensionManager;

pub struct ExtensionsInterface {
    mgr: Arc<ExtensionManager>,
}

impl ExtensionsInterface {
    pub fn new(mgr: Arc<ExtensionManager>) -> Self {
        Self { mgr }
    }
}

#[interface(name = "org.opengg.Daemon.Extensions")]
impl ExtensionsInterface {
    /// JSON array of known daemon extensions: `[{ id, name, running }]`.
    async fn list(&self) -> String {
        self.mgr.list_json().await
    }

    /// Persist enable/disable for an extension and start/stop its daemon part live.
    async fn set_enabled(&self, id: &str, enabled: bool) -> zbus::fdo::Result<()> {
        self.mgr.set_enabled(id, enabled).await;
        Ok(())
    }
}
