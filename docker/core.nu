use config.nu *
use ../ssh-manager.nu run_with_master

# Quote un argument pour un shell POSIX distant (chaque argument devient un mot unique,
# même s'il contient des espaces, ex: un script passé à 'sh -c').
def shell-quote [arg: string] {
    $"'($arg | str replace --all "'" "'\\''")'"
}

# Pure function to execute docker command
export def run_docker_command [command: list, host_info: record] {
    if ($host_info.hostname == "localhost") {
        run-external "docker" ...$command
    } else {
        let docker_cmd_string = (["docker"] | append $command | each { |arg| shell-quote $arg } | str join " ")
        run_with_master $host_info $docker_cmd_string
    }
}

# Pure function to get containers list
export def get_containers [need_all: bool, host_info: record] {
    let ps_cmd = if $need_all { ["ps" "-a"] } else { ["ps"] }
    run_docker_command $ps_cmd $host_info | from ssv -a | get NAMES
}

# Pure function to extract networks
export def extract_networks [container_name: string, host_info: record] {
    run_docker_command ["inspect" $container_name] $host_info 
    | from yaml 
    | get NetworkSettings.Networks
}

# Pure function to execute container operation
export def execute_container_operation [operation: string, container: string, host_info: record] {
    let config = get_config $operation
    run_docker_command [$config.docker_command $container] $host_info
}
