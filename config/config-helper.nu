use config.nu *

# Function to select an item
export def select_item [
    config_path: string
    item_type: string
]: nothing -> string {
    let items = open $config_path

    if ($items | is-empty) {
        print $"❌ No ($item_type) available in configuration"
        return ""
    }
    
    $items | columns | input list --fuzzy $"Select ($item_type) to delete:"
}

# Function to confirm the deletion of an item
export def confirm_deletion [
    selected: string
    item_type: string
    config: record
]: nothing -> bool {
    let item_config = $config | get $selected
    print $"Do you want to delete this ($item_type):"
    print $"Name: ($selected)"
    print $"Configuration: ($item_config | to yaml)"
    
    (input "[y|n] ? ") == "y"
}

export def delete [
    type: string  # "service", "customer", or "host"
]: nothing -> nothing {
    let config_map = {
        service: $services_config_path,
        customer: $customers_config_path,
        host: $hosts_config_path
    }

    # Validation du type
    if not ($type in ($config_map | columns)) {
        print $"❌ Unknown type: ($type). Use: service, customer, or host"
        return
    }

    let config_path = $config_map | get $type

    # Sélection
    let selected = select_item $config_path $type
    if $selected == null { return }

    # Confirmation et suppression
    let config = open $config_path
    if (confirm_deletion $selected $type $config) {
        # Suppression inline au lieu d'appeler delete_item
        ($config | reject $selected) | save $config_path -f
        print $"✅ ($type | str capitalize) '($selected)' deleted successfully"
    } else {
        print "❌ Operation cancelled"
    }
}