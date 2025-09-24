use config.nu *

# Function to create a new host in the config
export def create_host []: nothing -> nothing {
    let new_host_name = (input "Enter the new host_name : ")
    let new_hostname = (input "Enter the new hostname (ip) : ")
    let new_user = (input "Enter the user for the new host : ")
    let new_port = (input "Enter the port for the new host : ")
    if not ($new_port | into int | is-not-empty) {
        print "❌ Port must be a valid number"
        return
    }
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
        open $hosts_config_path | insert $new_host_name $new_host_info | save $hosts_config_path -f
    } else {
        print "Opération annulée"
    }
    
}