###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

def get_context_file_path [] {
    "~/dev/nu-modules/PurposeOps/context.json" | path expand
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Save the context
export def save_context [context: record] {
    let context_path = get_context_file_path
    $context | to json | save -f $context_path
}
# Load the context
export def load_context [] {
    let context_path = get_context_file_path
    if not ($context_path | path exists) {
        # Create default context file if it doesn't exist
        create_default_context
    }
    open $context_path
}
# Create the default context
export def create_default_context [] {
    let context_path = get_context_file_path
    let config = load_config
    let localhost_info = ($config.hosts | get localhost)

    let default_context = {
        host: {
            localhost: $localhost_info
        }
    }

    # Create directory if it doesn't exist
    mkdir ($context_path | path dirname)
    $default_context | to json | save -f $context_path
}