use super::*;

#[test]
fn vide_donne_chaine_vide() {
    assert_eq!(render(&["A", "B"], &[]), "");
}

#[test]
fn largeur_de_colonne_prend_le_max_entete_et_valeurs() {
    let rows = vec![
        vec!["x".to_string(), "yy".to_string()],
        vec!["zzz".to_string(), "w".to_string()],
    ];
    assert_eq!(column_widths(&["A", "BB"], &rows), vec![3, 2]);
}

#[test]
fn derniere_colonne_non_paddee() {
    let rows = vec![vec!["short".to_string(), "s".to_string()]];
    let out = render(&["NAME", "STATUS"], &rows);
    let data_line = out.lines().nth(1).unwrap();
    assert!(data_line.ends_with('s'));
    assert!(!data_line.ends_with(' '));
}

#[test]
fn une_ligne_par_entree_plus_len_entete() {
    let rows = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string(), "d".to_string()],
        vec!["e".to_string(), "f".to_string()],
    ];
    let out = render(&["A", "B"], &rows);
    assert_eq!(out.lines().count(), 4); // en-tête + 3 lignes
    assert!(out.lines().next().unwrap().starts_with('A'));
}

#[test]
fn cellule_plus_courte_que_len_headers_ne_panique_pas() {
    let rows = vec![vec!["only-one".to_string()]];
    let out = render(&["A", "B", "C"], &rows);
    let mut lines = out.lines();
    assert!(lines.next().unwrap().starts_with('A'));
    assert_eq!(lines.next().unwrap(), "only-one");
}
