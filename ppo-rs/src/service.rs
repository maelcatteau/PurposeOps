//! Commandes service (port de `service-manager.nu` et `services-config-manager.nu`).

use anyhow::Result;

use crate::config::{self, Service};
use crate::ui;

/// `lss` — noms des services disponibles.
pub fn cmd_lss() -> Result<()> {
    for name in config::load_services()?.keys() {
        println!("{name}");
    }
    Ok(())
}

/// `cs` (create_service) — wizard interactif, aperçu YAML, confirmation, insertion
/// dans services.yaml. Port de `services-config-manager.nu`'s `create_service`.
pub fn cmd_cs() -> Result<()> {
    let Some(service_name) = ui::text("Service name : ") else {
        return Ok(());
    };

    let mut services = config::load_services()?;
    if services.contains_key(&service_name) {
        println!("❌ Service '{service_name}' already exists");
        return Ok(());
    }

    let Some(template_dir_path) = ui::text("Template directory path : ") else {
        return Ok(());
    };
    let Some(template_compose_path) = ui::text("Template docker compose path : ") else {
        return Ok(());
    };

    let new_service = Service {
        template_dir_path,
        template_compose_path,
        variables: vec![],
    };

    println!("{}", serde_yaml_ng::to_string(&new_service)?);
    if !ui::confirm("Create?") {
        return Ok(());
    }

    services.insert(service_name, new_service);
    config::save_yaml_map(&config::services_config_path(), &services)
}

/// `ds` (delete service) — sélection fuzzy + confirmation + suppression.
pub fn cmd_ds() -> Result<()> {
    config::delete_from_map::<Service>(config::services_config_path(), "service")
}
