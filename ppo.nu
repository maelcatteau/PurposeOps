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

export def "ppos" [query?: string] {
    let commands = [
        "ppo close - Close current SSH master connection"
        "ppo closeall - Close all SSH master connections"
        "ppo ls conn - List active SSH master connections"
        "ppo p - Get current prompt context"
        "ppo p h - Get prompt host information"
        "ppo t - Toggle prompt display"
        "ppo h - Get current host detailed info"
        "ppo h name - Get current host name only"
        "ppo ls h - List all configured hosts"
        "ppo s h - Set/switch to a different host"
        "ppo c h - Create a new host"
        "ppo d h - Delete an existing host"
        "ppo d stop - Stop Docker containers"
        "ppo d start - Start Docker containers"
        "ppo d restart - Restart Docker containers"
        "ppo d n extract - Extract Docker networks info"
        "ppo d ps - Show Docker containers status"
        "ppo d nls - List Docker networks"
        "ppo c - List customer selected in the context file"
        "ppo c name - List the customer name selected in the context file"
        "ppo s c - Set/Switch the customer currently selected in the context file"
        "ppo c c - Create a new customer"
        "ppo d c - Delete an existing customer"
        "ppo ls c - List all available customers"
        "ppo ls s - Return the list of all available services"
        "ppo ls d - Return the list of available deployments for the current customer"
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