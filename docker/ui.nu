use config.nu *

# Pure function for user interaction
export def select_container [containers: list<string>, header: string] {
    if ($containers | is-empty) {
        return null
    }
    $containers | input list --fuzzy $header
}

# Pure function to format success message
export def format_success_message [container: string, past_participle: string] {
    $"✅ Container ($container) ($past_participle) successfully"
}

# Pure function to format error message  
export def format_error_message [container: string, operation: string] {
    $"❌ Failed to ($operation) container ($container)"
}

# Pure function to format operation message
export def format_operation_message [container: string, verb: string] {
    $"($verb) container: ($container)"
}
