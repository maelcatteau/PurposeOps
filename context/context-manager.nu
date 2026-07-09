###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use ../config/ *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Save the context
export def save_context [context: record] {
    $context | to yaml | save -f $context_path
}
# Load the context
export def load_context [] {
    if not ($context_path | path exists) {
        create_default_context
    }
    
    let ctx = open $context_path
    
    # S'assurer que la clé deployment existe toujours
    if ($ctx | get -o deployment | is-empty) {
        let updated_ctx = ($ctx | upsert deployment null)
        save_context $updated_ctx
        return $updated_ctx
    }
    
    return $ctx
}
# Create the default context
export def create_default_context [] {
    let context_path = get_context_file_path
    let hosts = open $hosts_config_path
    let localhost_info = ($hosts | get localhost)

    let default_context = {
        host: {
            localhost: $localhost_info
        }
        prompt_show: true
        customer: {} # Ou null, selon votre préférence de structure
        deployment: null # Ajout de la clé deployment
    }

    # Create directory if it doesn't exist
    mkdir ($context_path | path dirname)
    $default_context | to yaml | save -f $context_path
}
