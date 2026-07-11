###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use docker/core.nu *
use docker/ui.nu *
use docker/operations.nu with_host_info

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Helper function to select the right information for the right type of operation
def get_config [operation: string] {
    match $operation {
        "start" => {
            need_all: true
            header: "Select a stack to up :"
            verb: "Up'ing"
            past_participle: "up'ed"
        }
        "stop" => {
            need_all: false
            header: "Select a stack to down :"
            verb: "Down'ing"
            past_participle: "down'ed"
        }
        "restart" => {
            need_all: false
            header: "Select a stack to restart :"
            verb: "Restarting"
            past_participle: "restarted"
        }
        _ => {
            error make {msg: $"Configuration not found for operation: ($operation)"}
        }
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################
export def run_docker_compose_command [command: list, host_info: record] {
    let docker_compose_command = (["compose"] | append $command)
    run_docker_command $docker_compose_command $host_info
}

# Generic function for Docker Compose stack operations
export def docker_compose_stack_operation [
    --up(-u),      # Flag to up a stack
    --down(-d),       # Flag to down
    --restart(-r),    # Flag to restart
] {
    # Determine operation based on flags
    let operation = if $up {
        "start"
    } else if $down {
        "stop"
    } else if $restart {
        "restart"
    } else {
        print "❌ You must specify an operation: --up, --down, or --restart"
        return
    }

    # Configuration for each operation
    let config = get_config $operation

    with_host_info {|host_info|
        # Get containers
        let containers_list = get_containers $config.need_all $host_info

        # Check if containers are available
        if ($containers_list | is-empty) {
            print $"No containers available for ($operation)"
            return
        }

        # Selection
        let container_name = select_container $containers_list $config.header
        if ($container_name | is-empty) {
            print "Operation cancelled - no container selected"
            return
        }

        # Execute operation
        print (format_operation_message $container_name $config.verb)
        run_docker_compose_command [$operation $container_name] $host_info

        # Check result
        if $env.LAST_EXIT_CODE == 0 {
            print (format_success_message $container_name $config.past_participle)
        } else {
            print (format_error_message $container_name $operation)
        }
    }
}
