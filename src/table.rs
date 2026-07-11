//! Formateur de tableau texte aligné, sans bordures (à la `docker ps`/`kubectl get`) —
//! remplace le `println!("{a}\t{b}\t{c}")` brut des commandes `ls*`/`dps`/`dnls`, illisible
//! dès qu'une colonne varie beaucoup en longueur (ex : noms d'image Docker sur 40+ lignes).

/// Assemble l'en-tête + les lignes en un bloc de texte aligné. Chaque colonne est
/// large comme sa plus longue valeur (en-tête compris) ; la dernière colonne n'est pas
/// paddée (pas d'espaces de fin inutiles). `""` si `rows` est vide (rien à afficher).
pub fn render(headers: &[&str], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let widths = column_widths(headers, rows);
    let mut lines = vec![render_row(headers, &widths)];
    for row in rows {
        let cells: Vec<&str> = row.iter().map(String::as_str).collect();
        lines.push(render_row(&cells, &widths));
    }
    lines.join("\n")
}

/// Affiche directement sur stdout (no-op si `rows` est vide).
pub fn print(headers: &[&str], rows: &[Vec<String>]) {
    let out = render(headers, rows);
    if !out.is_empty() {
        println!("{out}");
    }
}

fn column_widths(headers: &[&str], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell.chars().count());
            }
        }
    }
    widths
}

fn render_row(cells: &[&str], widths: &[usize]) -> String {
    let last = cells.len().saturating_sub(1);
    cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == last {
                c.to_string()
            } else {
                let width = widths.get(i).copied().unwrap_or(0);
                format!("{c:<width$}")
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests;
