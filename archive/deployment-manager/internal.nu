use ../context/context-manager.nu *
use ../config/ *

# Helper: Récupère la liste brute des déploiements pour un client donné
def deployments_list_for_customer [customer: string] {
    open $customers_config_path | get $customer | get deployments
}

# Internal helper function to set deployment
# Internal helper function to set deployment
export def set_deployment_internal [deployment_id: string] {
    let ctx = load_context
    
    # Récupérer le nom du client actuel
    if ($ctx | get customer | columns | is-empty) {
        error make {msg: "Aucun client sélectionné dans le contexte."}
    }
    let customer_name = ($ctx | get customer | columns | first)
    
    # Récupérer les infos complètes du déploiement
    let deployments = (open $customers_config_path | get $customer_name | get deployments)
    let deployment_record = ($deployments | where ($it.deployment_id == $deployment_id) | first)
    
    if ($deployment_record | is-empty) {
        error make {msg: $"Déploiement '($deployment_id)' introuvable pour le client '($customer_name)'."}
    }
    
    # 2. Mettre à jour le contexte avec l'OBJET COMPLET (et non plus juste l'ID)
    let new_context = ($ctx | upsert deployment $deployment_record)
    
    save_context $new_context
    
    # Feedback
    let service_name = ($deployment_record.service_name)
    let host_id = ($deployment_record.hosts | get 0.host_id)
    print $"📍 Déploiement actif : ($service_name)"
    print $" ID : ($deployment_id)"
    print $" sur hôte "($host_id)""
}

# Internal helper function to persist a new deployment under a customer
export def create_deployment_internal [customer_name: string, deployment: record] {
    let customers = (open $customers_config_path)
    let existing_deployments = ($customers | get $customer_name | get -o deployments | default [])
    let updated_customer = ($customers | get $customer_name | upsert deployments ($existing_deployments | append $deployment))

    $customers | upsert $customer_name $updated_customer | save $customers_config_path -f
}