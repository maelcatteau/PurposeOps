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