use ../context/context-manager.nu *
use ../config/ *
use ./validations.nu *
use ./internal.nu *
# Note: Assurez-vous que machine-manager est accessible si vous appelez set-host
use ../machine-manager/ * 

# Function to change deployment (with fuzzy finder)
export def set-deployment [deployment_id?: string] {
    let ctx = load_context
    
    # Vérification préalable : un client doit être sélectionné
    if ($ctx | get customer | columns | is-empty) {
        print "❌ Aucun client sélectionné. Veuillez d'abord utiliser 'ppo sc <client>'."
        return
    }
    
    let current_customer = ($ctx | get customer | columns | first)
    let available_deployments = (list_deployments_for_current_customer | get deployment_id)
    
    # Si un ID est fourni directement
    if $deployment_id != null {
        if not ($deployment_id in $available_deployments) {
            print $"❌ Déploiement '($deployment_id)' introuvable pour le client '($current_customer)'"
            return
        }
        set_deployment_internal $deployment_id
        return
    }

    # Mode interactif : Sélection via fzf/input
    # On utilise une logique similaire à select_item mais adaptée si besoin, 
    # ou on liste les IDs disponibles.
    let selected_dep = if (which fzf | is-not-empty) {
        $available_deployments | to text | fzf --prompt="Déploiement> "
    } else {
        $available_deployments | input list "Sélectionnez un déploiement : "
    }
    
    if ($selected_dep | is-empty) {
        return
    }
    
    # Vérification de cohérence Hôte
    # Le déploiement cible impose-t-il un hôte différent de l'actuel ?
    let current_host = (get-current-host | columns | first)
    let target_host = (host_for_deployment $selected_dep)
    
    if ($current_host == $target_host) {
        # Même hôte, on applique directement
        set_deployment_internal $selected_dep
    } else {
        # Hôte différent, on propose de changer
        print $"⚠️ Le déploiement '($selected_dep)' est sur l'hôte '($target_host)'."
        print $"   L'hôte actuel est '($current_host)'."
        print $"   Voulez-vous changer d'hôte pour '($target_host)' ?"
        
        let validation = (input "Valider le changement d'hôte ? [y|n] ")
        
        if ($validation == "y" or $validation == "Y") {
            # On appelle set-host du module machine-manager
            # Assurez-vous que set-host accepte juste l'ID de l'hôte
            set-host $target_host
            set_deployment_internal $selected_dep
        } else {
            print "⚠️ Changement de déploiement annulé. L'hôte reste inchangé."
        }
    }
}

# Function to create a new deployment for the currently selected customer
export def create_deployment []: nothing -> nothing {
    let ctx = load_context

    if ($ctx | get customer | columns | is-empty) {
        print "❌ Aucun client sélectionné. Utilisez 'sc <client>' d'abord."
        return
    }
    let customer_name = ($ctx | get customer | columns | first)

    let hosts = (open $hosts_config_path)

    print $"📍 Création d'un déploiement pour : ($customer_name)"

    let service_name = (input "Service name (ex: Odoo CE, Vaultwarden): ")
    let host_id = (input "Host ID: ")

    if not ($host_id in ($hosts | columns)) {
        print $"❌ Host '($host_id)' introuvable ! Hôtes disponibles : ($hosts | columns)"
        return
    }

    let path_for_service = (input "Path for service on host: ")
    let path_for_docker_compose = (input "Path for docker-compose file: ")
    let deployment_id = (input "Deployment id (unique): ")

    if (deployment_id_exists $deployment_id) {
        print $"❌ Le deployment_id '($deployment_id)' est déjà utilisé par un autre déploiement."
        return
    }

    mut new_deployment = {
        service_name: $service_name
        hosts: [{
            host_id: $host_id
            path_for_service: $path_for_service
            path_for_docker_compose: $path_for_docker_compose
        }]
        deployment_id: $deployment_id
    }

    # Champs optionnels pour les déploiements avec une base de données à sauvegarder (ex: Odoo)
    let has_db = (input "Ce déploiement a-t-il une base de données à sauvegarder ? [y/n]: ")
    if $has_db == "y" {
        let container_name = (input "Container name: ")
        let db_container_name = (input "DB container name: ")
        let database_name = (input "Database name: ")
        let db_host = (input "DB credentials - host: ")
        let db_port = (input "DB credentials - port: ")
        let db_user = (input "DB credentials - user: ")
        let db_password = (input "DB credentials - password: ")

        $new_deployment = (
            $new_deployment
            | insert container_name $container_name
            | insert db_container_name $db_container_name
            | insert database_name $database_name
            | insert db_credentials {
                host: $db_host
                port: $db_port
                user: $db_user
                password: $db_password
            }
        )
    }

    print ($new_deployment | to yaml)
    let validation = (input "Créer ce déploiement ? [y/n]: ")

    if $validation == "y" {
        create_deployment_internal $customer_name $new_deployment
        print $"✅ Déploiement '($deployment_id)' créé pour '($customer_name)'"
    } else {
        print "❌ Opération annulée"
    }
}