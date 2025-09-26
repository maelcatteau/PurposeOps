use ../machine-manager/ *
use ../config/ *

# Check that the host is consistent with the customer
export def check_host_customer_consistency [customer: string] {
    let current_host = get-current-host
    let customer_hosts = hosts_for_customer $customer
    $current_host in $customer_hosts
}

# Check the host for a given customer
export def hosts_for_customer [customer: string] {
    open $customers_config_path | get $customer | get hosts | get host_id
}
