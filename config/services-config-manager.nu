use config.nu *

# Function to create a new service
export def create_service []: nothing -> nothing {
    
    let service_name = (input "Service name : ")
    let template_dir_path = (input "Template directory path : ")
    let template_compose_path = (input "Template docker compose path : ")
    
    let new_service_info = {
        template_dir_path: $template_dir_path
        template_compose_path: $template_compose_path
        variables: []
    }
    
    print ($new_service_info | to json --indent 2)
    let validation = (input "Create? [y/n]: ")
    
    if $validation == "y" {
        open $services_config_path | insert $service_name $new_service_info | save $services_config_path -f
    }
}