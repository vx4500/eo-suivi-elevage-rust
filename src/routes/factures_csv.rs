//! Import CSV des factures, tous secteurs.
//!
//! Porte de sortie quand le PDF du fournisseur n'est pas reconnu : les
//! analyseurs PDF sont calés sur Cooperl, Uniporc et trois marques de semence,
//! et un éleveur qui change de coopérative ou reçoit un scan n'avait alors que
//! la saisie une par une. Le fichier est converti en lignes d'import
//! ordinaires : l'aperçu, le choix des bandes, la détection de doublons et la
//! confirmation sont exactement ceux de l'import PDF.

use super::*;

/// Colonnes attendues, dans l'ordre du modèle téléchargeable.
const COLONNES: &[&str] = &[
    "secteur",
    "date",
    "num_facture",
    "fournisseur",
    "designation",
    "quantite",
    "pu_ht",
    "montant_ht",
    "montant_ttc",
    "poids_total",
    "silo",
    "bande_code",
];

const EXEMPLES: &[&[&str]] = &[
    &[
        "aliment",
        "2026-01-15",
        "FA-2026-001",
        "Moulin du Bocage",
        "Croissance granulé",
        "12,5",
        "312,40",
        "3905,00",
        "",
        "",
        "S2",
        "",
    ],
    &[
        "veto",
        "2026-01-18",
        "VE-2026-014",
        "Clinique des Trois Vallées",
        "Vaccin circovirus",
        "200",
        "1,15",
        "230,00",
        "",
        "",
        "",
        "B1.26",
    ],
    &[
        "semence",
        "2026-01-20",
        "SE-2026-007",
        "Coop IA",
        "Doses Piétrain",
        "24",
        "",
        "384,00",
        "460,80",
        "",
        "",
        "",
    ],
    &[
        "genetique",
        "2026-01-22",
        "GE-2026-003",
        "DanBred",
        "Cochettes L1065",
        "12",
        "",
        "3426,00",
        "",
        "2280",
        "",
        "B1.26",
    ],
];

/// Secteurs acceptés dans la colonne `secteur`, avec leurs synonymes.
fn secteur_normalise(valeur: &str) -> Option<&'static str> {
    let valeur = valeur
        .to_lowercase()
        .replace(['é', 'è', 'ê'], "e")
        .trim()
        .to_string();
    match valeur.as_str() {
        "aliment" | "alimentation" | "nutrition" => Some("aliment"),
        "veto" | "veterinaire" | "sanitaire" | "pharmacie" => Some("veto"),
        "semence" | "ia" | "doses" => Some("semence"),
        "genetique" | "cochettes" | "verrats" | "reproducteurs" => Some("genetique"),
        _ => None,
    }
}

pub(super) async fn modele_csv() -> Response {
    // BOM UTF-8 + point-virgule : Excel francophone ouvre directement en
    // colonnes, comme les autres modèles du logiciel.
    let mut body = format!("\u{feff}{}\r\n", COLONNES.join(";"));
    for exemple in EXEMPLES {
        body.push_str(&format!("{}\r\n", exemple.join(";")));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=modele_import_factures.csv"),
    );
    (headers, body).into_response()
}

pub(super) async fn importer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    let (form, file, uploaded_name) = parity::multipart_fields(multipart, "fichier").await?;
    verify_csrf(&session, &form)?;
    let bytes = file.ok_or_else(|| AppError::Invalid("Fichier CSV manquant".into()))?;
    let filename = uploaded_name.unwrap_or_else(|| "import-factures.csv".into());
    analyser(&state, &session, &bytes, &filename).await
}

/// Analyse le CSV et prépare l'aperçu. N'écrit aucune facture.
async fn analyser(
    state: &AppState,
    session: &SessionData,
    bytes: &[u8],
    nom_depose: &str,
) -> AppResult<Response> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(AppError::Invalid("Fichier trop volumineux".into()));
    }
    let filename: String = nom_depose
        .chars()
        .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
        .take(180)
        .collect();
    let digest = contenu_sha256(bytes);
    let delimiter = delimiteur(bytes);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let entetes: Vec<String> = reader
        .headers()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .iter()
        .map(|value| value.trim().trim_start_matches('\u{feff}').to_lowercase())
        .collect();
    for obligatoire in ["secteur", "date", "num_facture", "montant_ht"] {
        if !entetes.iter().any(|value| value == obligatoire) {
            return Err(AppError::Invalid(format!(
                "Colonne {obligatoire} manquante : téléchargez le modèle CSV des factures."
            )));
        }
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut transaction = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut transaction, &digest).await?;
    sqlx::query(
        "UPDATE importjournal SET statut='expire' WHERE statut='apercu' AND cree_le<datetime('now','-1 day')",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,'economique:csv',?,'apercu',?,?)")
        .bind(&token).bind(&filename).bind(session.uid).bind(&digest)
        .execute(&mut *transaction).await?;

    let mut vues = std::collections::HashSet::new();
    let mut compteurs = HashMap::<String, i64>::new();
    let mut avertissements = Vec::<String>::new();
    let mut numero = 0_i64;
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| AppError::Invalid(error.to_string()))?;
        let ligne_fichier = index as i64 + 2;
        numero += 1;
        let cellule = |cle: &str| {
            entetes
                .iter()
                .position(|entete| entete == cle)
                .and_then(|position| record.get(position))
                .map(str::trim)
                .filter(|valeur| !valeur.is_empty())
                .map(str::to_string)
        };
        let secteur = cellule("secteur")
            .as_deref()
            .and_then(secteur_normalise)
            .unwrap_or("");
        let date = cellule("date").as_deref().and_then(parse_iso_date);
        let facture = cellule("num_facture");
        let montant = cellule("montant_ht")
            .as_deref()
            .and_then(parse_french_number);
        let quantite = cellule("quantite").as_deref().and_then(parse_french_number);
        let pu = cellule("pu_ht").as_deref().and_then(parse_french_number);
        let designation = cellule("designation");
        let bande_code = cellule("bande_code");

        let line = ImportLine {
            kind: secteur.to_string(),
            date: date.clone(),
            reference: facture.clone(),
            label: designation
                .clone()
                .unwrap_or_else(|| format!("Facture {}", facture.clone().unwrap_or_default())),
            quantity: quantite,
            unit_price: pu,
            amount: montant,
            details: json!({
                "source_ligne": ligne_fichier,
                "fournisseur": cellule("fournisseur"),
                "produit": designation.clone(),
                "designation": designation,
                "silo": cellule("silo"),
                "tonnage": quantite,
                "quantite": quantite,
                "nb_doses": quantite.map(|value| value.round() as i64),
                "nb_animaux": quantite.map(|value| value.round() as i64),
                "poids_total": cellule("poids_total").as_deref().and_then(parse_french_number),
                "prix_moyen": pu,
                "pu_ht": pu,
                "montant_ht": montant,
                "montant_ttc": cellule("montant_ttc").as_deref().and_then(parse_french_number),
                "montant_net": montant,
                "bande_code": bande_code,
            }),
        };

        let (action, anomalie) = if secteur.is_empty() {
            (
                "erreur".to_string(),
                Some("Secteur inconnu : attendu aliment, veto, semence ou genetique".to_string()),
            )
        } else if date.is_none() {
            (
                "erreur".to_string(),
                Some("Date manquante ou invalide : format attendu AAAA-MM-JJ".to_string()),
            )
        } else {
            economic_preview_action(&mut transaction, &line, &mut vues).await?
        };
        *compteurs.entry(action.clone()).or_default() += 1;
        if action == "erreur" {
            avertissements.push(format!(
                "Ligne {ligne_fichier} : {}",
                anomalie.clone().unwrap_or_default()
            ));
        }
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
            .bind(&token).bind(numero).bind(&action).bind(&anomalie)
            .bind(serde_json::to_string(&line).map_err(|error| AppError::Internal(error.into()))?)
            .execute(&mut *transaction).await?;
    }
    if numero == 0 {
        return Err(AppError::Invalid(
            "Le fichier ne contient aucune ligne de facture".into(),
        ));
    }
    avertissements.truncate(10);
    let resume = json!({
        "ajouter": compteurs.get("ajouter").copied().unwrap_or_default(),
        "remplacer": compteurs.get("remplacer").copied().unwrap_or_default(),
        "ignorer": compteurs.get("ignorer").copied().unwrap_or_default(),
        "erreur": compteurs.get("erreur").copied().unwrap_or_default(),
        "avertissements": avertissements,
    });
    sqlx::query("UPDATE importjournal SET resume=? WHERE token=?")
        .bind(resume.to_string())
        .bind(&token)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Redirect::to(&format!("/economique/import/{token}")).into_response())
}

/// Même détection que les autres imports CSV du logiciel.
fn delimiteur(bytes: &[u8]) -> u8 {
    let entete: &[u8] = &bytes[..bytes.len().min(1024)];
    let points_virgules = entete.iter().filter(|&&byte| byte == b';').count();
    let virgules = entete.iter().filter(|&&byte| byte == b',').count();
    if points_virgules >= virgules {
        b';'
    } else {
        b','
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_secteurs_acceptent_les_variantes_courantes() {
        assert_eq!(secteur_normalise("Aliment"), Some("aliment"));
        assert_eq!(secteur_normalise("vétérinaire"), Some("veto"));
        assert_eq!(secteur_normalise("IA"), Some("semence"));
        assert_eq!(secteur_normalise("Cochettes"), Some("genetique"));
        assert_eq!(secteur_normalise("carburant"), None);
    }

    #[test]
    fn le_modele_couvre_les_quatre_secteurs() {
        assert_eq!(EXEMPLES.len(), 4);
        for exemple in EXEMPLES {
            assert_eq!(exemple.len(), COLONNES.len());
            assert!(secteur_normalise(exemple[0]).is_some());
        }
    }
}
