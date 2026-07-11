# Configuration for Docker operations
export def get_operation_config [] {
    {
        start: {
            need_all: true
            header: "Select a container to start:"
            verb: "Starting"
            past_participle: "started"
            docker_command: "start"
        }
        stop: {
            need_all: false
            header: "Select a container to stop:"
            verb: "Stopping" 
            past_participle: "stopped"
            docker_command: "stop"
        }
        restart: {
            need_all: false
            header: "Select a container to restart:"
            verb: "Restarting"
            past_participle: "restarted"
            docker_command: "restart"
        }
        networks_extract: {
            need_all: true
            header: "Select a container to extract networks from:"
            verb: "Extracting networks from"
            past_participle: "networks extracted from"
            docker_command: "inspect"
        }
    }
}

export def get_config [operation: string] {
    let configs = get_operation_config
    $configs | get $operation | default {
        error make {msg: $"Configuration not found for operation: ($operation)"}
    }
}
