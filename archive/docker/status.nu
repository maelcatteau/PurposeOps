use core.nu *
use operations.nu *

# Pure function for status formatting
def format_status_data [data: table, show_ports: bool] {
    if $show_ports {
        $data | select NAMES IMAGE STATUS PORTS
    } else {
        $data | select NAMES IMAGE STATUS
    }
}

# Pure function for network data formatting  
def format_network_data [data: table] {
    $data | select NAME DRIVER SCOPE
}

# Show container status
export def main [
    filter?: string
    --ports(-p)
] {
    with_host_info {|host_info|
        run_docker_command ["ps"] $host_info
        | from ssv -a
        | if ($filter != null) { where NAMES =~ $filter } else { $in }
        | format_status_data $in $ports
    }
}

# List networks
export def network_list [filter?: string] {
    with_host_info {|host_info|
        let data = run_docker_command ["network", "ls"] $host_info
        | from ssv -a  
        | if ($filter != null) { where NAME =~ $filter } else { $in }
        format_network_data $data
    }
}
