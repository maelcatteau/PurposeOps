use ../context/context-manager.nu *
use ../config/ *

# Internal logic to change host (factorization)
export def set_host_internal [host: string] {
    let context = load_context
    let host_info = (open $hosts_config_path | get $host)

    # Create new context with selected host
    let new_context = $context | upsert host { $host: $host_info}

    # Save context
    save_context $new_context
    print $"📍 Context set to: ($host_info.name)"
}
