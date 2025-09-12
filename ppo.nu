export use machine-manager.nu *
export use config-manager.nu *
export use customer-manager.nu *
export use prompt-manager.nu *
export use ssh-manager.nu *
export use docker-functions.nu *
export use context-manager.nu *
export use config-loader.nu *
export use service-manager.nu *
export use deployment-manager.nu *
export use templater.nu *

export def "ppos" [query?: string] {
    let commands = [
        "ppo close - Close current SSH master connection"
        "ppo closeall - Close all SSH master connections"
        "ppo ls conn - List active SSH master connections"
        "ppo p - Get current prompt context"
        "ppo ph - Get prompt host information"
        "ppo t - Toggle prompt display"
        "ppo h - Get current host detailed info"
        "ppo hname - Get current host name only"
        "ppo lsh - List all configured hosts"
        "ppo sh - Set/switch to a different host"
        "ppo ch - Create a new host"
        "ppo dh - Delete an existing host"
        "ppo dstop - Stop Docker containers"
        "ppo dstart - Start Docker containers"
        "ppo drestart - Restart Docker containers"
        "ppo dn extract - Extract Docker networks info"
        "ppo dps - Show Docker containers status"
        "ppo dnls - List Docker networks"
        "ppo c - List customer selected in the context file"
        "ppo cname - List the customer name selected in the context file"
        "ppo sc - Set/Switch the customer currently selected in the context file"
        "ppo cc - Create a new customer"
        "ppo dc - Delete an existing customer"
        "ppo lsc - List all available customers"
        "ppo lss - Return the list of all available services"
        "ppo lsd - Return the list of available deployments for the current customer"
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