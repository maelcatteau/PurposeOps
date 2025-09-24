
export use hosts-config-manager.nu *
export use services-config-manager.nu *
export use customers-config-manager.nu *
export use config-helper.nu *
export use config.nu *


export alias "ppo ch" = create_host
export alias "ppo dh" = delete host
export alias "ppo cc" = create_customer
export alias "ppo dc" = delete customer
export alias "ppo cs" = create_service
export alias "ppo ds" = delete service