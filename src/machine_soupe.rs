//! Import des exports CSV d'une machine à soupe Asserva (« Histo_fab »).
//!
//! Trois formats CSV existent sur ces machines (constaté sur de vrais
//! exports fournis par l'éleveur) : `Histo_dis` (routage — quelle formule
//! part vers quelles vannes, sans quantité), `Histo_Modif` (journal de
//! réglages par vanne, pas une consommation) et `Histo_fab` (fabrication —
//! quantité consigne/reçue d'eau et de chaque produit nommé par gâchée).
//! Seul `Histo_fab` porte de vraies quantités consommées ; c'est le seul
//! traité ici. Un quatrième fichier observé (`Sv_7`, sans extension) est un
//! instantané interne largement binaire, pas un CSV — délibérément non géré.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// Nombre de couples (Nom produit, Consigne, Reçue) après les 6 colonnes
/// fixes (No formule, Date, Heure début, Heure fin, Consigne Eau, Reçue
/// Eau) — observé constant à 32 sur les exports fournis.
const MAX_PRODUITS: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LigneFabrication {
    pub date: Option<String>,
    pub heure_debut: Option<String>,
    pub no_formule: Option<i64>,
    pub produit: String,
    pub quantite_consigne: f64,
    pub quantite_recue: f64,
}

/// Décode les octets d'un CSV en texte. Les exports Asserva observés sont en
/// Windows-1252/ISO-8859-1 (accents sur un seul octet, ex. 0xE9 pour « é »),
/// pas en UTF-8 — un `String::from_utf8` direct échoue ou tronque le
/// fichier au premier octet invalide. On tente l'UTF-8 d'abord (cas d'un
/// export déjà correctement encodé) et on retombe sur un décodage Latin-1
/// strict (chaque octet = son point de code Unicode, exact pour la plage
/// utile ici) sinon.
fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&byte| byte as char).collect(),
    }
}

pub(crate) fn parse_date_fr(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "00/00/00" {
        return None;
    }
    // Comme pour les imports PDF Cooperl (voir economic_import::iso_date),
    // chrono n'ajoute pas 2000 à une année à 2 chiffres : "31/12/25" est lu
    // comme l'an 25, pas 2025, sans ce correctif.
    let date = NaiveDate::parse_from_str(raw, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(raw, "%d/%m/%y"))
        .ok()?;
    let year = if date.year() < 100 {
        date.year() + 2000
    } else {
        date.year()
    };
    NaiveDate::from_ymd_opt(year, date.month(), date.day())
        .map(|date| date.format("%Y-%m-%d").to_string())
}

pub(crate) fn number_fr(raw: &str) -> Option<f64> {
    let value = raw.trim().replace(',', ".");
    if value.is_empty() {
        return None;
    }
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Un produit placeholder jamais configuré s'appelle littéralement
/// « Produit_N » (N=1..32) sur ces machines — jamais utilisé, à ignorer
/// même s'il a par erreur une quantité non nulle.
fn est_produit_reel(nom: &str) -> bool {
    let nom = nom.trim();
    if nom.is_empty() {
        return false;
    }
    let lower = nom.to_lowercase();
    match lower.strip_prefix("produit_") {
        Some(suffix) => suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()),
        None => true,
    }
}

pub fn parse_fabrication_csv(bytes: &[u8]) -> Result<Vec<LigneFabrication>, String> {
    let text = decode_text(bytes);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .trim(csv::Trim::All)
        .flexible(true)
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| format!("En-tête CSV illisible : {error}"))?
        .clone();
    let normalized: Vec<String> = headers
        .iter()
        .map(|value| value.trim().to_lowercase())
        .collect();
    let index_of = |label: &str| normalized.iter().position(|value| value == label);
    let idx_formule = index_of("no formule").ok_or_else(|| "Colonne « No formule » manquante : ce fichier n'est pas un export Histo_fab de machine à soupe".to_string())?;
    let idx_date = index_of("date").ok_or_else(|| "Colonne « Date » manquante".to_string())?;
    let idx_heure_debut = index_of("heure début").or_else(|| index_of("heure debut"));
    let idx_premier_nom = index_of("nom p_1").ok_or_else(|| "Colonne « Nom P_1 » manquante : ce fichier n'est pas un export Histo_fab de machine à soupe".to_string())?;

    let mut lignes = Vec::new();
    for (numero, record) in reader.records().enumerate() {
        let record = record.map_err(|error| format!("Ligne {} illisible : {error}", numero + 2))?;
        let get = |index: usize| record.get(index).unwrap_or("").trim();
        let no_formule = get(idx_formule).parse::<i64>().ok();
        let date = parse_date_fr(get(idx_date));
        let heure_debut = idx_heure_debut
            .map(|index| get(index).to_string())
            .filter(|value| !value.is_empty());
        for produit_index in 0..MAX_PRODUITS {
            let base = idx_premier_nom + produit_index * 3;
            let Some(nom) = record.get(base) else { break };
            let nom = nom.trim();
            if !est_produit_reel(nom) {
                continue;
            }
            let consigne = record.get(base + 1).and_then(number_fr).unwrap_or(0.0);
            let recue = record.get(base + 2).and_then(number_fr).unwrap_or(0.0);
            if consigne == 0.0 && recue == 0.0 {
                continue;
            }
            lignes.push(LigneFabrication {
                date: date.clone(),
                heure_debut: heure_debut.clone(),
                no_formule,
                produit: nom.to_string(),
                quantite_consigne: consigne,
                quantite_recue: recue,
            });
        }
    }
    Ok(lignes)
}

/// Liste triée des noms de produits réels rencontrés (pour l'écran de
/// correspondance manuelle produit → silo).
pub fn produits_distincts(lignes: &[LigneFabrication]) -> Vec<String> {
    let mut noms: Vec<String> = lignes.iter().map(|ligne| ligne.produit.clone()).collect();
    noms.sort();
    noms.dedup();
    noms
}

#[cfg(test)]
mod tests {
    use super::*;

    /// En-tête et deux lignes réelles d'un export Histo_fab (machine à
    /// soupe Asserva, éleveur ORY EMMANUEL) — les 32 colonnes produit sont
    /// tronquées à 4 ici, le parseur est indifférent au nombre réel tant que
    /// la structure par triplet est respectée.
    fn extrait_reel(nb_produits: usize) -> Vec<u8> {
        let mut header = "No formule;Date;Heure début;Heure fin;Consigne Eau;Reçue Eau".to_string();
        let mut ligne1 = "2;31/12/25;18:03;18:11;223;224".to_string();
        let mut ligne2 = "1;01/01/25;11:03;11:14;450;454".to_string();
        let produits = ["Produit_1", "Lacta Safe", "Select Cochette", "Produit_4"];
        for index in 0..nb_produits {
            header.push_str(&format!(";Nom P_{};Consigne;Reçue", index + 1));
            match index {
                0 => {
                    ligne1.push_str(";Produit_1;0;0");
                    ligne2.push_str(";Produit_1;0;0");
                }
                1 => {
                    ligne1.push_str(";Lacta Safe;74;92");
                    ligne2.push_str(";Lacta Safe;0;0");
                }
                2 => {
                    ligne1.push_str(";Select Cochette;0;0");
                    ligne2.push_str(";Select Cochette;155;155");
                }
                _ => {
                    ligne1.push_str(&format!(
                        ";{};0;0",
                        produits.get(index).copied().unwrap_or("Produit_X")
                    ));
                    ligne2.push_str(&format!(
                        ";{};0;0",
                        produits.get(index).copied().unwrap_or("Produit_X")
                    ));
                }
            }
        }
        format!("{header};\n{ligne1};\n{ligne2};\n").into_bytes()
    }

    #[test]
    fn lit_les_vraies_quantites_et_ignore_les_produits_a_zero() {
        let bytes = extrait_reel(4);
        let lignes = parse_fabrication_csv(&bytes).expect("l'export Histo_fab doit être lu");
        // Ligne 1 : seule « Lacta Safe » a une quantité non nulle.
        // Ligne 2 : seule « Select Cochette » a une quantité non nulle.
        // « Produit_1 »/« Produit_4 » (placeholders jamais configurés) sont
        // toujours ignorés même si un jour ils portaient une valeur.
        assert_eq!(lignes.len(), 2);
        assert_eq!(lignes[0].produit, "Lacta Safe");
        assert_eq!(lignes[0].date.as_deref(), Some("2025-12-31"));
        assert_eq!(lignes[0].heure_debut.as_deref(), Some("18:03"));
        assert_eq!(lignes[0].no_formule, Some(2));
        assert_eq!(lignes[0].quantite_consigne, 74.0);
        assert_eq!(lignes[0].quantite_recue, 92.0);
        assert_eq!(lignes[1].produit, "Select Cochette");
        assert_eq!(lignes[1].date.as_deref(), Some("2025-01-01"));
        assert_eq!(lignes[1].quantite_recue, 155.0);
    }

    #[test]
    fn produits_distincts_est_triee_et_dedupliquee() {
        let bytes = extrait_reel(4);
        let lignes = parse_fabrication_csv(&bytes).expect("l'export doit être lu");
        assert_eq!(
            produits_distincts(&lignes),
            vec!["Lacta Safe".to_string(), "Select Cochette".to_string()]
        );
    }

    #[test]
    fn decode_un_fichier_latin1_sans_planter() {
        // Reproduit un octet 0xE9 (« é » en Windows-1252/ISO-8859-1) tel
        // qu'observé dans les en-têtes réels (« Heure début », « Reçue ») —
        // invalide en UTF-8, un `String::from_utf8` direct échouerait.
        let mut bytes = b"No formule;Date;Heure d\xe9but;Heure fin;Consigne Eau;Re\xe7ue Eau;Nom P_1;Consigne;Re\xe7ue;\n".to_vec();
        bytes.extend_from_slice(b"2;31/12/25;18:03;18:11;223;224;Lacta Safe;74;92;\n");
        let lignes = parse_fabrication_csv(&bytes).expect("un export Latin-1 doit rester lisible");
        assert_eq!(lignes.len(), 1);
        assert_eq!(lignes[0].produit, "Lacta Safe");
    }

    #[test]
    fn refuse_un_fichier_qui_nest_pas_un_histo_fab() {
        let bytes = b"a;b;c\n1;2;3\n".to_vec();
        assert!(parse_fabrication_csv(&bytes).is_err());
    }
}
