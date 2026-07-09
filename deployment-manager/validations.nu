use ../context/context-manager.nu *
use ../config/ *

# Get current deployment (Returns the full Record directly from context)
export def get-current-deployment-info [] {
    let ctx = load_context
    let dep = ($ctx | get -o deployment)
    
    if ($dep | is-empty) {
        error make {msg: "Aucun déploiement sélectionné dans le contexte."}
    }
    
    # Vérification de type : si c'est une chaîne, c'est un ancien format
    if ($dep | describe) == "string" {
        error make {msg: "Format de contexte obsolète (ID simple). Veuillez re-sélectionner le déploiement avec 'ppo sd' pour mettre à jour."}
    }
    
    # On retourne l'objet tel quel, sans chercher dans customers.yaml
    return $dep
}

# Get current deployment ID (Helper qui extrait l'ID du record)
export def get-current-deployment [] {
    let dep = (get-current-deployment-info)
    return ($dep | get deployment_id)
}

# Check that the current deployment is consistent with the current customer
export def check_deployment_customer_consistency [] {
    let ctx = load_context
    
    if ($ctx | get customer | columns | is-empty) {
        return false
    }
    
    let dep = ($ctx | get -o deployment)
    if ($dep | is-empty) {
        return false
    }
    
    # Si c'est un record complet, on considère que c'est valide 
    # (car la validation a été faite lors du set-deployment)
    return (($dep | describe) == "record")
}

# Check the host for a given deployment ID (Garde la logique de recherche fichier car prend un ID en argument)
export def host_for_deployment [deployment_id: string] {
    let customers = (open $customers_config_path | columns)
    
    for $cust in $customers {
        let data = (open $customers_config_path | get $cust)
        let deps = ($data | get -o deployments)
        
        if ($deps | is-empty) { continue }

        let match = ($deps | where ($it.deployment_id == $deployment_id))
        
        if ($match | is-not-empty) {
            return ($match | get 0.hosts | get 0.host_id)
        }
    }
    
    return null
}

# List all the deployments available for the current customer (Pour la sélection interactive)
export def list_deployments_for_current_customer [] {
    let current_customer = (get-current-customer | columns | first)
    let data = (open $customers_config_path | get $current_customer)
    let deps = ($data | get -o deployments)
    
    if ($deps | is-empty) {
        return []
    }
    
    return ($deps | select deployment_id service_name hosts)
}