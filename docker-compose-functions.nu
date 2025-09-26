###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use customer-manager/ *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Helper function to select the right information for the right type of operation
def get_config [operation: string] {
    match $operation {
        "up" => {
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
export def run_docker_compose_command [command: list] {
    let docker_compose_command = (["compose"] | append $command)
    run_docker_command $docker_compose_command
}

# Generic function for Docker operations
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
        print "❌ You must specify an operation: --start, --stop, --restart, or --networks"
        return
    }

    # Configuration for each operation
    let config = get_config $operation

    # Get containers
    let containers_list = if $config.need_all {
        get_containers_list "{{.Names}}\t{{.Image}}\t{{.Status}}" --all
    } else {
        get_containers_list "{{.Names}}\t{{.Image}}\t{{.Status}}"
    }

    # Check if containers are available
    if (is_empty_content $containers_list) {
        print $"No containers available for ($operation)"
        return
    }

    # Selection with fzf
    let selected = try {
        $containers_list | lines | fzf --header=$config.header
    } catch {
        ""  # If fzf is cancelled, return empty string
    }

    # Check selection
    if (is_empty_content $selected) {
        print "Operation cancelled - no container selected"
        return
    }

    # Extract container name
    let container_name = get_container_name_from_fzf $selected

    # Execute operation according to type
    if $operation == "networks_extract" {
        print $"($config.verb) container: ($container_name)"
        let networks = run_docker_command ["inspect" $container_name] | from yaml | get NetworkSettings.Networks

        if $env.LAST_EXIT_CODE == 0 {
            print $"✅ ($config.past_participle) container ($container_name)"
            return $networks
        } else {
            print $"❌ Failed to extract networks from container ($container_name)"
            return
        }
    } else {
        # Standard operations (start, stop, restart)
        print $"($config.verb) container: ($container_name)"
        run_docker_command [$operation $container_name]

        # Check result
        if $env.LAST_EXIT_CODE == 0 {
            print $"✅ Container ($container_name) ($config.past_participle) successfully"
        } else {
            print $"❌ Failed to ($operation) container ($container_name)"
        }
    }
}