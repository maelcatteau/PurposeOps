use ../context/context-manager.nu *
use ../config/ *

# Internal helper function to set customer
export def set_customer_internal [customer: string] {
    let context = load_context
    let customer_info = open $customers_config_path | get $customer | reject deployments hosts

    # Create new context with selected host
    let new_context = $context | upsert customer { $customer: $customer_info}

    # Save context
    save_context $new_context
    print $"📍 Context set to: ($new_context.customer | columns | first)"
}
