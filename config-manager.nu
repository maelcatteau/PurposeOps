###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use customer-manager.nu *
use machine-manager.nu *
use service-manager.nu *
use config-loader.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################
# Function to create a new host in the config
export def create_host [] {
    let config = load_config
    let new_host_name = (input "Enter the new host_name : ")
    let new_hostname = (input "Enter the new hostname (ip) : ")
    let new_user = (input "Enter the user for the new host : ")
    let new_port = (input "Enter the port for the new host : ")
    let new_id_file = (input "Enter the path for the ssh id file for the new host : ")
    let new_arch = (input "Enter the correct architecture ('x86_64', 'arm64') : ")
    let docker_context = "remote-" + $new_host_name
    let vps_name = "vps-" + $new_host_name

    let new_host_info = {
        name: $vps_name
        hostname: $new_hostname
        user: $new_user
        port: $new_port
        identity_file: $new_id_file
        arch: $new_arch
        docker_context: $docker_context
    }
    let new_host_info_json = $new_host_info | to json
    print $"Voulez vous valider ce nouvel hote ? ($new_host_info_json)"
    let validation = (input "[y|n] ? :")
    if $validation == "y" {
        let new_hosts = $config | get hosts | insert $new_host_name $new_host_info
        $config | upsert hosts { $new_hosts } | save ./config.json -f
    } else {
        print "Opération annulée"
    }
    
}

# Function to delete an existing host 
export def delete_host [] {
    let config = load_config
    let current_host = get-current-host
    let hosts_info = prepare_hosts_for_fzf $config $current_host
    # Check that we have customers
    if ($hosts_info | is-empty) {
        print "❌ No hosts available in configuration"
        return
    }

    # Selection with fzf
    let selected = try {
        $hosts_info | fzf --header="🖥️  Select a host"
    } catch {
        ""  # If fzf is cancelled
    }
    # Check selection
    if ($selected | str trim | is-empty) {
        print "Operation cancelled - no host selected"
        return
    }
    # Extract selected host name (first column)
    let selected_host = extract_customer_from_fzf $selected
    let new_hosts_list = ($config.hosts | reject $selected_host)
    let selected_host_json = ($config.hosts | get $selected_host | to json)
    print $"Do you want to confirm to remove this host : ($selected_host_json)"
    let validation = (input "[y|n] ? ")
    print $validation
    if $validation == "y" {
        let new_config = ($config | upsert hosts { $new_hosts_list } | save ./config.json -f) 
    } else {
        print "Operation cancelled, you haven't validated"
    }
}

# Function to create a new customer
export def create_customer [] {
    let config = load_config
    
    let customer_name = (input "Customer name: ")
    let abbreviation = (input "Abbreviation: ")
    let host_id = (input "Host ID: ")
    let path_on_host = (input "Path on host: ")
    
    # Quick validation
    if not ($host_id in ($config.hosts | columns)) {
        print $"❌ Host '($host_id)' not found!"
        return
    }
    
    let new_customer_info = {
        abbreviation: $abbreviation
        deployments: []
        hosts: [{ host_id: $host_id, path_on_host: $path_on_host }]
    }
    
    print ($new_customer_info | to json --indent 2)
    let validation = (input "Create? [y/n]: ")
    
    if $validation == "y" {
        let new_customers = $config | get customers | insert $customer_name $new_customer_info
        $config | upsert customers $new_customers | save ./config.json -f
        print "✅ Done!"
    }
}

# Function to delete an existing customer
export def delete_customer [] {
    let config = load_config
    let current_customer = get-current-customer
    let customers_info = prepare_customers_for_fzf $config $current_customer
    # Check that we have customers
    if ($customers_info | is-empty) {
        print "❌ No hosts available in configuration"
        return
    }

    # Selection with fzf
    let selected = try {
        $customers_info | fzf --header="🖥️  Select a customer"
    } catch {
        ""  # If fzf is cancelled
    }
    # Check selection
    if ($selected | str trim | is-empty) {
        print "Operation cancelled - no customer selected"
        return
    }
    # Extract selected host name (first column)
    let selected_customer = extract_customer_from_fzf $selected
    let new_customers_list = ($config.customers | reject $selected_customer)
    let selected_customer_json = ($config.customers | get $selected_customer | to json)
    print $"Do you want to confirm to delete this customer : ($selected_customer_json)"
    let validation = (input "[y|n] ? ")
    print $validation
    if $validation == "y" {
        let new_config = ($config | upsert customers { $new_customers_list } | save ./config.json -f) 
    } else {
        print "Operation cancelled, you haven't validated"
    }
}


# Function to create a new service
export def create_service [] {
    let config = load_config
    
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
        let new_service = $config | get services | insert $service_name $new_service_info
        $config | upsert services $new_service | save ./config.json -f
        print "✅ Done!"
    }
}

# Function to delete an existing service
export def delete_service [] {
    let config = load_config
    let services_list = prepare_services_for_fzf $config
    # Check that we have services
    if ($services_list | is-empty) {
        print "❌ No hosts available in configuration"
        return
    }

    # Selection with fzf
    let selected = try {
        $services_list | fzf --header="🖥️  Select a service"
    } catch {
        ""  # If fzf is cancelled
    }
    # Check selection
    if ($selected | str trim | is-empty) {
        print "Operation cancelled - no service selected"
        return
    }
    # Extract selected host name (first column)
    let selected_service = extract_service_from_fzf $selected
    let new_services_list = ($config.services | reject $selected_service)
    let selected_service_json = ($config.services | get $selected_service | to json)
    print $"Do you want to confirm you want to deleted this service : ($selected_service_json)"
    let validation = (input "[y|n] ? ")
    print $validation
    if $validation == "y" {
        let new_config = ($config | upsert services { $new_services_list } | save ./config.json -f) 
    } else {
        print "Operation cancelled, you haven't validated"
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################
export alias "ppo ch" = create_host
export alias "ppo dh" = delete_host
export alias "ppo cc" = create_customer
export alias "ppo dc" = delete_customer
export alias "ppo cs" = create_service
export alias "ppo ds" = delete_service