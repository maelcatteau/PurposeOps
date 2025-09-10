###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Function to get a formatted string for prompt display
export def get-prompt-context [] {
    try {
        let context = load_context
        let host_name = ($context.host | columns | first)
        let vps_name = ($context.host | get $host_name | get name)
        let host_info = ($context.host | get $host_name)

        if not ($context.prompt_show) {
            ""
            return
        }
        
        # Different formatting based on host type
        if ($host_info.hostname == "localhost") {
            "🏠 local"
        } else {
            $"🌐 ($vps_name)"
        }
    } catch {
        "❓ unknown"
    }
}

# Function to get just the host name for minimal prompt
export def get-prompt-host [] {
    try {
        get-current-host
    } catch {
        "unknown"
    }
}

# Function to toggle on an off thr ppo prompt
export def toggle-prompt [] {
    let context = load_context
    if not ($context.prompt_show) {
        let new_context = $context | upsert prompt_show { true }
        save_context $new_context
        print $"📍 Context set to prompt_show: ($new_context.prompt_show)"
    } else {
        let new_context = $context | upsert prompt_show { false }
        save_context $new_context
        print $"📍 Context set to prompt_show: ($new_context.prompt_show)"
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo prompt" = get-prompt-context
export alias "ppo phost" = get-prompt-host
export alias "ppo toggle" = toggle-prompt