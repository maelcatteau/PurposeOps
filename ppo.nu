export use machine-manager/ *
export use customer-manager/ *
export use ssh-manager.nu *
export use docker-functions.nu *
export use service-manager.nu *
export use deployment-manager.nu *
export use templater.nu *
export use customer-manager/ *
export use context/ *
export use config/ *

export def "ppos" [query?: string] {
    let commands = [
        "close - Close current SSH master connection"
        "closeall - Close all SSH master connections"
        "ls conn - List active SSH master connections"
        "p - Get current prompt context"
        "ph - Get prompt host information"
        "t - Toggle prompt display"
        "h - Get current host detailed info"
        "hname - Get current host name only"
        "lsh - List all configured hosts"
        "sh - Set/switch to a different host"
        "ch - Create a new host"
        "dh - Delete an existing host"
        "dstop - Stop Docker containers"
        "dstart - Start Docker containers"
        "drestart - Restart Docker containers"
        "dn extract - Extract Docker networks info"
        "dps - Show Docker containers status"
        "dnls - List Docker networks"
        "c - List customer selected in the context file"
        "cname - List the customer name selected in the context file"
        "sc - Set/Switch the customer currently selected in the context file"
        "cc - Create a new customer"
        "dc - Delete an existing customer"
        "lsc - List all available customers"
        "lss - Return the list of all available services"
        "lsd - Return the list of available deployments for the current customer"
    ]

    let selected = if (which fzf | is-not-empty) {
        if ($query | is-empty) {
            $commands | to text | fzf --height=12 --border=rounded --prompt="PPO> "
        } else {
            $commands | to text | fzf --height=12 --border=rounded --prompt="PPO> " $"--query=($query)"
        }
    } else {
        # Fallback: filtrer manuellement si query fournie
        let filtered_commands = if ($query | is-empty) {
            $commands
        } else {
            $commands | where ($it | str contains $query)
        }
        $filtered_commands | input list "Select command: "
    }

    if not ($selected | is-empty) {
        let command = ($selected | split row " - " | get 0)
        print $"🚀 Executing: ($command)"
        
        # Exécuter directement la commande
        let ppo_file = "~/dev/nu-modules/PurposeOps/ppo.nu"
        nu -c $"overlay use ($ppo_file); ($command)"
    }
}

