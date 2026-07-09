use ../machine-manager/ *
use ../ssh-manager.nu *
use config.nu *
use core.nu *
use ui.nu *

# Higher-order function for container operations
export def with_host_info [operation: closure] {
    let host_name = get-current-host | columns | first
    let host_info = get-current-host | get $host_name
    let current_host = get-current-host
    
    let resolved_host_info = if ($current_host == "localhost" or $host_info.hostname == "localhost") {
        {hostname: "localhost"}
    } else {
        $host_info
    }
    
    do $operation $resolved_host_info
}

# Functional approach to container operations
export def docker_container_operation [operation: string] {
    with_host_info {|host_info|
        let config = get_config $operation
        let containers_list = get_containers $config.need_all $host_info

        if ($containers_list | is-empty) { 
            print $"No containers available for ($operation)"
            return null
        } else { 
            let container_name = select_container $containers_list $config.header
            if ($container_name | is-empty) {
                print "Operation cancelled - no container selected"
                return null
            } else {
                if $operation == "networks_extract" {
                    print (format_operation_message $container_name $config.verb)
                    let networks = extract_networks $container_name $host_info
                    if $env.LAST_EXIT_CODE == 0 {
                        print (format_success_message $container_name $config.past_participle)
                        return $networks
                    } else {
                        print (format_error_message $container_name $operation)
                        return null
                    }
                } else {
                    print (format_operation_message $container_name $config.verb)
                    execute_container_operation $operation $container_name $host_info
                    if $env.LAST_EXIT_CODE == 0 {
                        print (format_success_message $container_name $config.past_participle)
                    } else {
                        print (format_error_message $container_name $operation)
                    }
                }
            }
        }
    }
}
