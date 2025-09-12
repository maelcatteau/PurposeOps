###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Return the template directory path
def get-template-dir-path [] {
    "~/dev/nu-modules/PurposeOps/templates" | path expand
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Generate a docker compose from the template
export def generate-compose [
    service_name: string
    docker_service_name: string
] {
    let template_dir = get-template-dir-path

    # Check that the file exists
    let compose_template = $"($template_dir)/($service_name)/docker-compose.yml"
    let variables_file = $"($template_dir)/($service_name)/template.yml"

    if not ($compose_template | path exists) {
        error make { msg: $"File not found: ($compose_template)" }
    }

    if not ($variables_file | path exists) {
        error make { msg: $"File not found: ($variables_file)" }
    }

    # Loading the files
    let template_content = open $compose_template --raw
    let variables_def = (open $variables_file | get variables)

    print $"📝 Generating compose for docker service: ($docker_service_name)"
    print "Please provide values for the following variables:"

    # Automatic variables
    mut user_variables = { 
        "service_name": $docker_service_name,
        "container_name": $docker_service_name
    }
    
    let sorted_vars = $variables_def | transpose key definition | sort-by definition.level

    for var in $sorted_vars {
        let var_name = $var.key
        let var_def = $var.definition

        # Skip auto-variables
        if $var_name in ["service_name", "container_name"] {
            continue
        }

        let prompt = $"  ($var_def.description)"
        let example_text = if "example" in $var_def { 
            " " + "(ex: " + ($var_def.example | into string) + ")" 
        } else { 
            "" 
        }

        let user_input = input $"($prompt)($example_text): "
        $user_variables = ($user_variables | upsert $var_name $user_input)
    }

    # Special treatment for networks
    if "networks" in $user_variables {
        let networks_list = ($user_variables.networks | split row "," | each { |net| $net | str trim })

        # Format for the network section (yaml list)
        let networks_section = ($networks_list | each { |net| $"- ($net)" } | str join "\n      ")
        $user_variables = ($user_variables | upsert "networks_section" $networks_section)

        # Format for network definition (yaml objects)
        let networks_definition = ($networks_list | each { |net| 
            $"($net):\n    external: true" 
        } | str join "\n  ")
        $user_variables = ($user_variables | upsert "networks_definition" $networks_definition)
    }

    # Applying variables to the template
    mut final_compose = $template_content

    for var in ($user_variables | transpose key value) {
        let placeholder = $"{{($var.key)}}"
        $final_compose = ($final_compose | str replace -a $placeholder $var.value)
    }

    print $"✅ Docker compose generated successfully!"
    
    $final_compose
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo g dc" = generate-compose
