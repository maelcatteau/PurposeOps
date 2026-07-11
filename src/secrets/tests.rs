use super::*;

#[test]
fn roundtrip_un_destinataire() {
    let identity = Identity::generate();
    let recipient = identity.to_public();

    let encrypted = encrypt_secret("odoo_password_2025", &[recipient]).unwrap();
    assert!(is_encrypted(&encrypted));

    let decrypted = decrypt_secret(&encrypted, &[identity]).unwrap();
    assert_eq!(decrypted, "odoo_password_2025");
}

#[test]
fn roundtrip_plusieurs_destinataires_n_importe_laquelle_dechiffre() {
    let mael = Identity::generate();
    let sylvie = Identity::generate();
    let recipients = vec![mael.to_public(), sylvie.to_public()];

    let encrypted = encrypt_secret("clé ssh privée de mcm", &recipients).unwrap();

    // Chacune des deux identités, utilisée seule, doit pouvoir déchiffrer.
    assert_eq!(decrypt_secret(&encrypted, &[mael]).unwrap(), "clé ssh privée de mcm");
    assert_eq!(decrypt_secret(&encrypted, &[sylvie]).unwrap(), "clé ssh privée de mcm");
}

#[test]
fn identite_non_destinataire_ne_dechiffre_pas() {
    let mael = Identity::generate();
    let etranger = Identity::generate();

    let encrypted = encrypt_secret("secret", &[mael.to_public()]).unwrap();

    assert!(decrypt_secret(&encrypted, &[etranger]).is_err());
}

#[test]
fn aucune_identite_locale_donne_une_erreur_explicite() {
    let mael = Identity::generate();
    let encrypted = encrypt_secret("secret", &[mael.to_public()]).unwrap();

    let err = decrypt_secret(&encrypted, &[]).unwrap_err();
    assert!(err.to_string().contains("aucune clé locale"));
}

#[test]
fn valeur_non_chiffree_rejetee() {
    let identity = Identity::generate();
    let err = decrypt_secret("odoo_password_2025", &[identity]).unwrap_err();
    assert!(err.to_string().contains("non chiffrée"));
}

#[test]
fn is_encrypted_detecte_le_prefixe() {
    assert!(is_encrypted("enc:abcdef"));
    assert!(!is_encrypted("odoo_password_2025"));
    assert!(!is_encrypted(""));
}

#[test]
fn encrypt_sans_destinataire_echoue() {
    assert!(encrypt_secret("secret", &[]).is_err());
}
