###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use ../context-manager.nu *
use ../../customer-manager/core.nu get-current-customer
use ../../machine-manager/ *
use ../../service-manager.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Function to get a formatted string for prompt display
export def get-prompt-context [] {
    let context = load_context
    if not ($context.prompt_show) {
        ""
        return
    } else {
        try {
            let host_name = get-current-host | columns | first
            let host_info = get-current-host | get $host_name
            let customer_info = get-current-customer
            if ($customer_info != "No customer currently selected") {
                let customer_name = $customer_info | columns | first
                let customer_abbr = $customer_info | get $customer_name | get abbreviation
                # Different formatting based on host type
                if ($host_info.hostname == "localhost") {
                    $"🏠 local - ($customer_abbr)"
                } else {
                    $"🌐 ($host_info.name) - ($customer_abbr)"
                }
            } else {
                if ($host_info.hostname == "localhost") {
                    $"🏠 local"
                } else {
                    $"🌐 ($host_info.name)"
                }
            }
            
        } catch {
            "❓ unknown"
        }
    }
    
}

# Function to toggle on and off the prompt
# Ne permet pas de sélectionner les élements du prompt pour l'instant
#
export def toggle-prompt []: nothing -> nothing {
    let context = load_context
    let new_context = $context | upsert prompt_show (not $context.prompt_show)
    save_context $new_context
    print $"📍 Context set to prompt_show: ($new_context.prompt_show)"
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "p" = get-prompt-context
export alias "t" = toggle-prompt