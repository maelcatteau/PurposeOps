export use operations.nu *
export use status.nu *

# Simplified public interface using partial application
export def start [] { docker_container_operation "start" }
export def stop [] { docker_container_operation "stop" }  
export def restart [] { docker_container_operation "restart" }
export def networks_extract [] { docker_container_operation "networks_extract" }

# Aliases
export alias "dstop" = stop
export alias "dstart" = start  
export alias "drestart" = restart
export alias "dn extract" = networks_extract
export alias "dps" = status
export alias "dnls" = network_list

