###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use context-manager.nu *
use customer-manager.nu *
use machine-manager.nu *
use service-manager.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Function to get a formatted string for prompt display
export def get-prompt-context [] {
    try {
        let context = load_context
        let host_name = get-current-host
        let host_info = get-current-host-info
        let customer_info = get-current-customer-info

        if not ($context.prompt_show) {
            ""
            return
        }
        
        # Different formatting based on host type
        if ($host_info.hostname == "localhost") {
            $"🏠 local - ($customer_info.abbreviation)"
        } else {
            $"🌐 ($host_info.name) - ($customer_info.abbreviation)"
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

export alias "ppo p" = get-prompt-context
export alias "ppo ph" = get-prompt-host
export alias "ppo t" = toggle-prompt