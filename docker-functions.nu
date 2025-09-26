###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use machine-manager/ *
use ssh-manager.nu *

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
            header: "Select a container to start :"
            verb: "Starting"
            past_participle: "started"
        }
        "stop" => {
            need_all: false
            header: "Select a container to stop :"
            verb: "Stopping"
            past_participle: "stopped"
        }
        "restart" => {
            need_all: false
            header: "Select a container to restart :"
            verb: "Restarting"
            past_participle: "restarted"
        }
        "networks_extract" => {
            need_all: true
            header: "Select a container to extract networks from :"
            verb: "Extracting networks from"
            past_participle: "networks extracted from"
        }
        _ => {
            error make {msg: $"Configuration not found for operation: ($operation)"}
        }
    }
}

# Helper function to get container from fuzzy finder output
def get_container_name_from_fzf [fzf_output: string] {
    let container_name = ($fzf_output | str replace -a "│" "" 
        | str trim 
        | split row " " 
        | where $it != "" 
        | skip 1 
        | first)
    return $container_name
}

# New function that avoids escaping issues
def get_containers_list [
    format_string: string,
    --all(-a)  # Flag to include all containers (including stopped ones)
] {
    # Get data in JSON format (no escaping issues)
    let raw_data = if $all {
        run_docker_command ["ps" "-a" "--format" "json"]
    } else {
        run_docker_command ["ps" "--format" "json"]
    }

    # Parse and format on Nushell side
    $raw_data 
    | lines 
    | where $it != ""
    | each { |line| $line | from json }
    | each { |container|
        # Extract the fields we want (equivalent to your format_string)
        $"($container.Names)\t($container.Image)\t($container.Status)"
    }
    | str join "\n"
}

# Function to check if content is empty (just returns a boolean)
def is_empty_content [content: string] {
    ($content | str trim | is-empty)
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################
export def run_docker_command [command: list] {
    let host_name = get-current-host | columns | first
    let host_info = get-current-host | get $host_name
    let current_host = get-current-host

    if $current_host == "localhost" or $host_info.hostname == "localhost" {
        run-external "docker" ...$command
    } else {
        let docker_cmd_string = (["docker"] | append $command | str join " ")

        run_with_master $host_info $docker_cmd_string
    }
}

# Generic function for Docker operations
export def docker_container_operation [
    --start(-s),      # Flag to start
    --stop(-p),       # Flag to stop  
    --restart(-r),    # Flag to restart
    --networks(-n)    # Flag to extract networks
] {
    # Determine operation based on flags
    let operation = if $start {
        "start"
    } else if $stop {
        "stop"
    } else if $restart {
        "restart"
    } else if $networks {
        "networks_extract"
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
        let networks = run_docker_command ["inspect" $container_name] | from json | get NetworkSettings.Networks

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

# Show status of all containers
export def status [
    filter?: string     # Optional filter on container names
    --ports(-p)        # Flag to display ports
] {
    let base_data = if ($filter == null) {
        run_docker_command ["ps"] | from ssv -a
    } else {
        run_docker_command ["ps"] | from ssv -a | where NAMES =~ $filter
    }

    if $ports {
        $base_data | select NAMES IMAGE STATUS PORTS
    } else {
        $base_data | select NAMES IMAGE STATUS
    }
}

# Function to list existing networks
export def network_list [
    filter?: string    # Optional filter on network names
] {
    let base_data = if ($filter == null) {
        run_docker_command ["network", "ls"] | from ssv -a
    } else {
        run_docker_command ["network", "ls"] | from ssv -a | where NAME =~ $filter
    }
    $base_data | select NAME DRIVER SCOPE
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo dstop" = docker_container_operation --stop
export alias "ppo dstart" = docker_container_operation --start
export alias "ppo drestart" = docker_container_operation --restart
export alias "ppo dn extract" = docker_container_operation --networks
export alias "ppo dps" = status
export alias "ppo dnls" = network_list
