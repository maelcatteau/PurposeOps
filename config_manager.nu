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
export def delete_host [hostname: string] {
    let config = load_config
    let new_hosts_list = ($config.hosts | reject $hostname)
    let new_hosts_list_json = ($new_hosts_list | to json)
    print $"Do you want to confirm this list of hosts : ($new_hosts_list_json)"
    let validation = (input "[y|n] ? ")
    print $validation
    if $validation == "y" {
        let new_config = ($config | upsert hosts { $new_hosts_list } | save ./config.json -f) 
    } else {
        print "Operation cancelled, you haven't validated"
    }
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################
export alias "ppo chost" = create_host
export alias "ppo dhost" = delete_host