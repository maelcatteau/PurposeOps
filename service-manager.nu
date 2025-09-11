###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use config-loader.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# List all the available services
export def list_all_services [] {
    let config = load_config
    $config.services
}

# Prepare the services list for using it in fzf 
export def prepare_services_for_fzf [config: record] {
    $config.services
    | transpose service_name service_info
    | each {|row|

        # Compter le nombre de variables
        let var_count = $row.service_info.variables | length
        let vars_text = if ($var_count == 0) { 
            "no vars" 
        } else if ($var_count == 1) { 
            "1 var" 
        } else { 
            $"($var_count) vars" 
        }

        # Icône service
        let type_icon = "⚙️"

        # Extraire le nom du template depuis le chemin
        let template_name = ($row.service_info.template_dir_path | str replace '.*/' '' | str replace '~/' '')

        # Format: ICON │ SERVICE_NAME │ TEMPLATE │ VARIABLES │ STATUS
        $"($type_icon) │ ($row.service_name) │ ($template_name) │ ($vars_text)"
    }
}

export def extract_service_from_fzf [selected_line: string] {
    try {
        # Split by │ separator and clean whitespace
        let parts = ($selected_line 
            | split row "│" 
            | each { |part| $part | str trim }
            | where $it != "")
        
        # Verify we have enough parts and get customer name (should be 2nd element after cleaning)
        if ($parts | length) >= 3 {
            $parts | get 2  # Customer name is the 2nd element (index 1) after icon
        } else {
            error make {
                msg: "Invalid fzf selection format"
                help: $"Expected format: ICON │ CUSTOMER_NAME │ ..., got: ($selected_line)"
            }
        }
    } catch {
        ""
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo ls s" = list_all_services