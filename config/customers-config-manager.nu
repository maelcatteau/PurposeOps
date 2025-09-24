use config.nu *

# Function to create a new customer
export def create_customer []: nothing -> nothing {
    let hosts = open $hosts_config_path
    
    let customer_name = (input "Customer name: ")
    let abbreviation = (input "Abbreviation: ")
    let host_id = (input "Host ID: ")
    let path_on_host = (input "Path on host: ")
    
    # Quick validation
    if not ($host_id in ($hosts | columns)) {
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
        open $customers_config_path | insert $customer_name $new_customer_info | save $customers_config_path -f
    }
}
