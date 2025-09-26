use ../context/context-manager.nu *
use ../config/ *
use internal.nu *

# Function to change host (with fuzzy finder)
export def set-host [host?: string] {
    let hosts_list = open $hosts_config_path | columns
    let current_host = get-current-host

    # If a host is specified directly, use the old logic
    if $host != null {
        if not ($host in $hosts_list) {
            print $"❌ Host '($host)' not found in configuration"
            print $"Available hosts: ($hosts_list)"
            return
        }
        set_host_internal $host
        return
    }

    # Use select_item for interactive selection
    let selected_host = select_item $hosts_config_path "host"
    if $selected_host == null { return }

    # Switch to selected host
    set_host_internal $selected_host
}

# Get current host
export def get-current-host [] {
    let context = load_context
    if ($context.host | is-empty) {
        "No host currently selected"
    } else {
        let host_name = $context.host | columns | first
        { $host_name: ($context.host | get $host_name) }
    }
}

# Function to list available hosts
export def list-hosts [] {
    let hosts = open $hosts_config_path
    let current_host = get-current-host | columns | first

    $hosts | transpose host info | each {|row|
        {
            host: $row.host
            name: $row.info.name
            type: (if ($row.info.hostname == "localhost") { "local" } else { "remote" })
            current: ($row.host == $current_host)
        }
    }
}
