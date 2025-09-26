use ../context/context-manager.nu *
use ../machine-manager.nu *
use ../config/ *
use internal.nu *
use validations.nu *

# Get current customer
export def get-current-customer [] {
    let context = load_context
    if ($context.customer | is-empty) {
        "No customer currently selected"
    } else {
        let customer_name = ($context.customer | columns | first)
        { $customer_name: ($context.customer | get $customer_name)}
    }
}

# Function to change customer (with fuzzy finder)
export def set-customer [customer?: string] {
    let customers = open $customers_config_path | columns
    let current_customer = get-current-customer
    
    # If a customer is specified directly, use the old logic
    if $customer != null {
        if not ($customer in $customers) {
            print $"❌ Customer '($customer)' not found in configuration"
            return
        }
        set_customer_internal $customer
        return
    }

    let selected_customer = select_item $customers_config_path "customer"
    if $selected_customer == null { return }

    let current_host = get-current-host | columns | first
    let host_customer_consistency = check_host_customer_consistency $selected_customer
    
    if $host_customer_consistency {
        set_customer_internal $selected_customer
    } else {
        let correct_host = host_for_customer $selected_customer
        print $"L'hôte actuel '($current_host)' n'est pas l'hote du client ($selected_customer), voulez vous changer d'hôte aussi ?"
        let validation = (input "Valider ? [y|n] ")
        if $validation == "y" {
            set-host $correct_host
            set_customer_internal $selected_customer
        }
    }
}

# Function to list available customers
export def list-customers [] {
    let customers = open $customers_config_path
    let current_customer = get-current-customer | columns | first

    $customers 
    | transpose customer_name customer_data
    | each { |row|
        let cleaned_deployments = if ("deployments" in ($row.customer_data | columns)) {
            $row.customer_data.deployments | each { |deployment| 
                $deployment | reject hosts 
            }
        } else {
            []
        }

        {
            customer_name: $row.customer_name,
            abbreviation: $row.customer_data.abbreviation,
            deployments: $cleaned_deployments,
            current: ($row.customer_name == $current_customer)
        }
    }
}
