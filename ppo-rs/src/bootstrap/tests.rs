//! Tests de la partie pure de `bootstrap` (sélection des capacités manquantes) — le reste
//! du module (détection/installation via SSH) n'est vérifiable qu'en direct.

use super::*;

#[test]
fn capability_labels_are_unique() {
    let mut labels: Vec<&str> = CAPABILITIES.iter().map(|c| c.label).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), CAPABILITIES.len());
}

#[test]
fn capability_commands_are_non_empty() {
    for cap in CAPABILITIES {
        assert!(!cap.detect.trim().is_empty(), "{} : detect vide", cap.label);
        assert!(!cap.install.trim().is_empty(), "{} : install vide", cap.label);
    }
}

#[test]
fn missing_capabilities_returns_none_when_all_present() {
    assert!(missing_capabilities(|_| true).is_empty());
}

#[test]
fn missing_capabilities_returns_all_when_none_present() {
    assert_eq!(missing_capabilities(|_| false).len(), CAPABILITIES.len());
}

#[test]
fn missing_capabilities_filters_selectively() {
    let missing = missing_capabilities(|c| c.label == "Docker");
    assert!(!missing.iter().any(|c| c.label == "Docker"));
    assert!(missing.iter().any(|c| c.label == "Netdata"));
    assert_eq!(missing.len(), CAPABILITIES.len() - 1);
}
