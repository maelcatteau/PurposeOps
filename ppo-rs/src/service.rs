//! Commandes service (port de `service-manager.nu`).

use anyhow::Result;

use crate::config;

/// `lss` — noms des services disponibles.
pub fn cmd_lss() -> Result<()> {
    for name in config::load_services()?.keys() {
        println!("{name}");
    }
    Ok(())
}
