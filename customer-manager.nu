###########################################################################################################################################################
###########################################################################################################################################################
#####################                                          Internal helper functions                                            #######################
###########################################################################################################################################################
###########################################################################################################################################################

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


def set_customer_internal [customer: string, config: record] {
    let context = load_context
    let customer_info = ($config.customers | get $customer)

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
# Get current customer
export def get-current-customer [] {
    let context = load_context
    $context.customer | columns | first
}

# Get current host information
export def get-current-customer-info [] {
    let context = load_context
    let current_customer = ($context.customer | columns | first)
    $context.customer | get $current_customer
}

# Function to change customer (with fuzzy finder)
export def set-customer [customer?: string] {
    let config = load_config
    let current_customer = get-current-customer
    # If a customer is specified directly, use the old logic
    if $customer != null {
        if not ($customer in $config.customers) {
            print $"❌ Host '($customer)' not found in configuration"
            print $"Available hosts: ($config.hosts | columns | str join ', ')"
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

    # Extract selected host name (first column)
    let selected_customer = extract_customer_from_fzf $selected

    # Switch to selected host
    set_customer_internal $selected_customer $config
}

###########################################################################################################################################################
###########################################################################################################################################################
#####################                                                     Aliases                                                   #######################
###########################################################################################################################################################
###########################################################################################################################################################

export alias "ppo customer" = get-current-customer-info
export alias "ppo cname" = get-current-customer
export alias "ppo scustomer" = set-customer