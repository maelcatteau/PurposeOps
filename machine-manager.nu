###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use context/context-manager.nu *
use config/ *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################
def extract_host_from_fzf [selected_line: string] {

    # Split by │ and clean each part
    let parts = ($selected_line 
        | split row "│"
        | each { |part| $part | str trim }
        | where $it != "")

    # Expected structure: [index, icon, host_name, description]
    # Host name is at index 2 (3rd element)
    if ($parts | length) >= 3 {
        let host_name = ($parts | get 2)
        print $"✅ Extracted host: '($host_name)'"
        return $host_name
    }

    print $"❌ Unexpected format - not enough parts ($parts | length)"
    return ""
}

# Internal logic to change host (factorization)
def set_host_internal [host: string] {
    let context = load_context
    let host_info = (open $hosts_config_path | get $host)

    # Create new context with selected host
    let new_context = $context | upsert host { $host: $host_info}

    # Save context
    save_context $new_context
    print $"📍 Context set to: ($host_info.name)"
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################
export def prepare_hosts_for_fzf [current_host: string] {
    let hosts = open $hosts_config_path
    $hosts 
    | transpose host info 
    | each {|row|
        let status = if ($row.host == $current_host) { " 👉 CURRENT" } else { "" }
        let type_icon = if ($row.info.hostname == "localhost") { "🏠" } else { "🌐" }

        # Format similar to your containers: ICON │ HOST_NAME │ DESCRIPTION │ STATUS
        $"($type_icon) │ ($row.host) │ ($row.info.name)($status)"
    }
}

# Function to change host (with fuzzy finder)
export def set-host [host?: string] {  # <- Optional parameter now
    let current_host = get-current-host | columns | first
    let hosts_list = open $hosts_config_path | columns

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

    # Otherwise, use fzf for interactive selection
    let hosts_info = prepare_hosts_for_fzf $current_host

    # Check that we have hosts
    if ($hosts_info | is-empty) {
        print "❌ No hosts available in configuration"
        return
    }

    # Selection with fzf
    let selected = try {
        $hosts_info | fzf --header="🖥️  Select a host" --height=40%
    } catch {
        ""  # If fzf is cancelled
    }

    # Check selection
    if ($selected | str trim | is-empty) {
        print "Operation cancelled - no host selected"
        return
    }

    # Extract selected host name (first column)
    let selected_host = extract_host_from_fzf $selected

    # Switch to selected host
    set_host_internal $selected_host
}

# Get current host
export def get-current-host [] {
    let context = load_context
    let host_name = $context.host | columns | first
    { $host_name: ($context.host | get $host_name) }
}

# Function to list available hosts
export def list-hosts [] {
    let hosts = open $hosts_config_path
    let current_host = get-current-host

    $hosts | transpose host info | each {|row|
        {
            host: $row.host
            name: $row.info.name
            type: (if ($row.info.hostname == "localhost") { "local" } else { "remote" })
            current: ($row.host == $current_host)
        }
    }
}


###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo h" = get-current-host
export alias "ppo lsh" = list-hosts
export alias "ppo sh" = set-host