export use docker-functions.nu *
export use machine-manager.nu *
export use ssh-manager.nu *
export use prompt-manager.nu *
export use config_manager.nu *

export def "ppos" [query?: string] {
    let commands = [
        "ppo close - Close current SSH master connection"
        "ppo closeall - Close all SSH master connections"
        "ppo lsconn - List active SSH master connections"
        "ppo prompt - Get current prompt context"
        "ppo phost - Get prompt host information"
        "ppo toggle - Toggle prompt display"
        "ppo host - Get current host detailed info"
        "ppo hostname - Get current host name only"
        "ppo lshost - List all configured hosts"
        "ppo shost - Set/switch to a different host"
        "ppo chost - Create a new host"
        "ppo dstop - Stop Docker containers"
        "ppo dstart - Start Docker containers"
        "ppo drestart - Restart Docker containers"
        "ppo dnetextract - Extract Docker networks info"
        "ppo dps - Show Docker containers status"
        "ppo dnls - List Docker networks"
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
