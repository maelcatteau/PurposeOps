###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                  Imports                                                      #######################
###########################################################################################################################################################
###########################################################################################################################################################

use context-manager.nu *
use config-loader.nu *
use machine-manager.nu *

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

def set_customer_internal [customer: string, config: record] {
    let context = load_context
    let customer_info = ($config.customers | get $customer | reject deployments hosts)

    # Create new context with selected host
    let new_context = $context | upsert customer { $customer: $customer_info}

    # Save context
    save_context $new_context
    print $"📍 Context set to: ($new_context.customer | columns | first)"
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                             Public functions                                                  #######################
###########################################################################################################################################################
###########################################################################################################################################################

# Check that the host is consistent with the customer
export def check_host_customer_consistency [customer: string] {
    let config = load_config
    let context = load_context
    let current_host = get-current-host
    let customer_hosts = hosts_for_customer $customer
    
    $current_host in $customer_hosts
}

# Check the host for a given customer
export def hosts_for_customer [customer: string] {
    let config = load_config
    $config.customers | get $customer | get hosts | get host_id
}

# Prepare the customers list for using it in fzf 
export def prepare_customers_for_fzf [config: record, current_customer: string] {
    $config.customers
    | transpose customer_name customer_info
    | each {|row|
        let status = if ($row.customer_name == $current_customer) { " 👉 CURRENT" } else { "" }
        
        # Compter le nombre d'hosts
        let host_count = $row.customer_info.hosts | length
        let hosts_text = if ($host_count == 1) { "1 host" } else { $"($host_count) hosts" }
        
        # Icône customer
        let type_icon = "👥"
        
        # Format: ICON │ customer_NAME │ ABBREVIATION │ HOST_COUNT │ STATUS
        $"($type_icon) │ ($row.customer_name) │ ($row.customer_info.abbreviation) │ ($hosts_text)($status)"
    }
}
export def extract_customer_from_fzf [selected_line: string] {
    try {
        # Split by │ separator and clean whitespace
        let parts = ($selected_line 
            | split row "│" 
            | each { |part| $part | str trim }
            | where $it != "")
        
        # Verify we have enough parts and get customer name (should be 2nd element after cleaning)
        if ($parts | length) >= 3 {
            $parts | get 2  # Customer name is the 2nd element (index 1) after icon
        } else {
            error make {
                msg: "Invalid fzf selection format"
                help: $"Expected format: ICON │ CUSTOMER_NAME │ ..., got: ($selected_line)"
            }
        }
    } catch {
        ""
    }
}

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
    let config = load_config
    let current_customer = get-current-customer
    # If a customer is specified directly, use the old logic
    if $customer != null {
        if not ($customer in $config.customers) {
            print $"❌ Host '($customer)' not found in configuration"
            return
        }

        set_customer_internal $customer $config
        return
    }

    # Otherwise, use fzf for interactive selection
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
        print "Operation cancelled - no host selected"
        return
    }

    # Extract selected customer name (first column)
    let selected_customer = extract_customer_from_fzf $selected
    let current_host = get-current-host
    let host_customer_consistency = check_host_customer_consistency $selected_customer
    if $host_customer_consistency {
        # Switch to selected customer
        set_customer_internal $selected_customer $config
    } else {
        let correct_host = host_for_customer $selected_customer
        print $"L'hôte actuel '($current_host)' n'est pas l'hote du client ($selected_customer), voulez vous changer d'hôte aussi ?"
        let validation = (input "Valider ? [y|n] ")
        if $validation == "y" {
            set-host $correct_host
            set_customer_internal $selected_customer $config
        }
    }
    
}

# Function to list available customers
export def list-customers [] {
    let config = load_config
    let current_customer = get-current-customer

    $config.customers 
    | transpose customer_name customer_data
    | each { |row|
        # Gérer les déploiements (peut être absent pour certains clients)
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

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo c" = get-current-customer
export alias "ppo sc" = set-customer
export alias "ppo lsc" = list-customers