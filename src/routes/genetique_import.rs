//! Import en masse des factures génétiques au format CSV.
//!
//! Même pipeline que les imports existants (truies, inventaire) : modèle
//! téléchargeable, aperçu ligne à ligne avec anomalies, puis confirmation
//! transactionnelle. Un modèle CSV seul, sans ce pipeline, aurait laissé
//! croire à un import possible : c'est la raison pour laquelle il n'existait
//! pas jusqu'ici (voir §3 de l'état du projet).

use super::*;

/// Colonnes attendues, dans l'ordre du modèle téléchargeable.
const COLONNES: &[&str] = &[
    "date",
    "num_facture",
    "fournisseur",
    "designation",
    "nb_animaux",
    "poids_total",
    "prix_moyen",
    "montant_ht",
    "bande_code",
    "note",
];

/// Ligne d'exemple du modèle, dans le même ordre que `COLONNES`.
const EXEMPLE: &[&str] = &[
    "2026-01-15",
    "FA-2026-001",
    "DanBred",
    "Cochettes L1065",
    "12",
    "2280",
    "285,50",
    "3426,00",
    "B1.26",
    "Exemple à supprimer",
];

pub(super) async fn modele_csv() -> Response {
    // BOM UTF-8 + point-virgule : Excel francophone ouvre le fichier
    // directement en colonnes, comme les modèles truies et eau/électricité.
    // L'en-tête est construit à partir de `COLONNES`, la liste que l'import
    // lit ensuite : modèle et analyse ne peuvent pas diverger.
    let body = format!(
        "\u{feff}{}\r\n{}\r\n",
        COLONNES.join(";"),
        EXEMPLE.join(";")
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=modele_import_genetique.csv"),
    );
    (headers, body).into_response()
}

/// Aperçu : analyse le fichier, journalise chaque ligne et son sort, mais
/// n'écrit aucune facture. Rien n'est appliqué avant `confirmer`.
pub(super) async fn importer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    let (form, file, uploaded_name) = parity::multipart_fields(multipart, "fichier").await?;
    verify_csrf(&session, &form)?;
    let bytes = file.ok_or_else(|| AppError::Invalid("Fichier CSV manquant".into()))?;
    let filename = uploaded_name.unwrap_or_else(|| "import-genetique.csv".into());
    analyser(&state, &session, &bytes, &filename).await
}

/// Cœur de l'aperçu, séparé de la lecture du formulaire multipart pour être
/// testable directement à partir d'un contenu CSV.
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
    for obligatoire in ["date", "num_facture", "montant_ht"] {
        if !entetes.iter().any(|value| value == obligatoire) {
            return Err(AppError::Invalid(format!(
                "Colonne {obligatoire} manquante : téléchargez le modèle CSV depuis la page Économie."
            )));
        }
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut vues = std::collections::HashSet::new();
    let mut apercu = Vec::new();
    let mut ajouts = 0_i64;
    let mut erreurs = 0_i64;
    let mut tx = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut tx, &digest).await?;
    sqlx::query(
        "UPDATE importjournal SET statut='expire' WHERE statut='apercu' AND cree_le<datetime('now','-1 day')",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,'genetique',?,'apercu',?,?)")
        .bind(&token)
        .bind(&filename)
        .bind(session.uid)
        .bind(&digest)
        .execute(&mut *tx)
        .await?;

    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| AppError::Invalid(error.to_string()))?;
        let ligne_num = index as i64 + 2;
        let cellule = |cle: &str| {
            entetes
                .iter()
                .position(|entete| entete == cle)
                .and_then(|position| record.get(position))
                .map(str::trim)
                .filter(|valeur| !valeur.is_empty())
                .map(str::to_string)
        };

        let date_brute = cellule("date");
        let date = date_brute.as_deref().and_then(parse_iso_date);
        let facture = cellule("num_facture");
        let montant_brut = cellule("montant_ht");
        let montant = montant_brut.as_deref().and_then(parse_french_number);
        let bande_code = cellule("bande_code");

        let mut action = "ajouter";
        let mut anomalie = None;
        if date.is_none() {
            action = "erreur";
            anomalie = Some("Date manquante ou invalide : format attendu AAAA-MM-JJ".into());
        } else if facture.is_none() {
            action = "erreur";
            anomalie = Some("Numéro de facture manquant".into());
        } else if montant.is_none() {
            action = "erreur";
            anomalie = Some("Montant HT manquant ou illisible".into());
        } else if !vues.insert(facture.clone().unwrap_or_default().to_lowercase()) {
            action = "erreur";
            anomalie = Some("Facture en double dans le fichier".into());
        } else {
            // Même règle de doublon que l'import PDF : une facture génétique
            // est identifiée par son numéro.
            let existe: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM achatgenetique WHERE lower(trim(COALESCE(num_facture,'')))=lower(trim(?))",
            )
            .bind(facture.as_deref().unwrap_or_default())
            .fetch_one(&mut *tx)
            .await?;
            if existe > 0 {
                action = "erreur";
                anomalie = Some("Facture déjà enregistrée en base".into());
            }
        }
        // Une bande inconnue n'invalide pas la ligne : la facture est importée
        // non affectée, comme une facture PDF sans bande reconnue.
        let mut bande_connue = None;
        if let Some(code) = bande_code.as_deref() {
            let trouvee: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE code=?")
                .bind(code)
                .fetch_one(&mut *tx)
                .await?;
            bande_connue = Some(trouvee > 0);
            if trouvee == 0 && action == "ajouter" {
                anomalie = Some(format!(
                    "Bande {code} inconnue : facture importée non affectée"
                ));
            }
        }
        if action == "erreur" {
            erreurs += 1;
        } else {
            ajouts += 1;
        }

        let donnees = json!({
            "date": date,
            "num_facture": facture,
            "fournisseur": cellule("fournisseur"),
            "designation": cellule("designation"),
            "nb_animaux": cellule("nb_animaux").as_deref().and_then(|v| v.parse::<i64>().ok()),
            "poids_total": cellule("poids_total").as_deref().and_then(parse_french_number),
            "prix_moyen": cellule("prix_moyen").as_deref().and_then(parse_french_number),
            "montant_ht": montant,
            "bande_code": bande_connue.unwrap_or(false).then(|| bande_code.clone()).flatten(),
            "note": cellule("note"),
        });
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
            .bind(&token)
            .bind(ligne_num)
            .bind(action)
            .bind(&anomalie)
            .bind(donnees.to_string())
            .execute(&mut *tx)
            .await?;
        apercu.push(json!({
            "ligne": ligne_num,
            "action": action,
            "anomalie": anomalie,
            "date": date,
            "num_facture": facture,
            "fournisseur": donnees["fournisseur"],
            "designation": donnees["designation"],
            "nb_animaux": donnees["nb_animaux"],
            "montant_ht": montant,
            "bande_code": bande_code,
        }));
    }

    let resume = json!({"ajouter": ajouts, "ignorer": 0, "erreur": erreurs});
    sqlx::query("UPDATE importjournal SET resume=? WHERE token=?")
        .bind(resume.to_string())
        .bind(&token)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let mut ctx = context(session);
    ctx.insert("token".into(), json!(token));
    ctx.insert("nom_fichier".into(), json!(filename));
    ctx.insert("resume".into(), resume);
    ctx.insert("lignes".into(), Value::Array(apercu));
    Ok(render(state, "genetique_import_apercu.html", Value::Object(ctx))?.into_response())
}

/// Applique l'aperçu en une seule transaction : soit toutes les factures
/// entrent, soit aucune.
pub(super) async fn confirmer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    let mut tx = state.pool.begin().await?;
    let proprietaire: Option<i64> = sqlx::query_scalar(
        "SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import='genetique'",
    )
    .bind(&token)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if proprietaire.is_none() {
        return Err(AppError::Invalid(
            "Aperçu introuvable ou déjà appliqué".into(),
        ));
    }
    if proprietaire != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let lignes = sqlx::query_as::<_, (i64, String)>(
        "SELECT numero_ligne,donnees_json FROM importligne WHERE token=? AND action='ajouter' ORDER BY numero_ligne",
    )
    .bind(&token)
    .fetch_all(&mut *tx)
    .await?;

    let mut ajoutees = 0_i64;
    for (numero, brut) in lignes {
        let donnees: Value = serde_json::from_str(&brut)
            .map_err(|_| AppError::Invalid(format!("Données invalides à la ligne {numero}")))?;
        let texte = |cle: &str| {
            donnees
                .get(cle)
                .and_then(Value::as_str)
                .filter(|valeur| !valeur.is_empty())
        };
        let nombre = |cle: &str| donnees.get(cle).and_then(Value::as_f64);
        let facture = texte("num_facture")
            .ok_or_else(|| AppError::Invalid(format!("Facture absente à la ligne {numero}")))?;
        // Contrôle rejoué au moment d'écrire : une facture saisie à la main
        // entre l'aperçu et la confirmation annule tout l'import.
        let existe: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM achatgenetique WHERE lower(trim(COALESCE(num_facture,'')))=lower(trim(?))",
        )
        .bind(facture)
        .fetch_one(&mut *tx)
        .await?;
        if existe > 0 {
            return Err(AppError::Invalid(format!(
                "La facture {facture} a été enregistrée depuis l'aperçu ; import entièrement annulé"
            )));
        }
        sqlx::query("INSERT INTO achatgenetique(date,num_facture,fournisseur,designation,nb_animaux,poids_total,prix_moyen,montant_ht,montant_net,bande_code,note) VALUES(?,?,?,?,?,?,?,?,NULL,?,?)")
            .bind(texte("date"))
            .bind(facture)
            .bind(texte("fournisseur").unwrap_or("Cooperl"))
            .bind(texte("designation"))
            .bind(donnees.get("nb_animaux").and_then(Value::as_i64))
            .bind(nombre("poids_total"))
            .bind(nombre("prix_moyen"))
            .bind(nombre("montant_ht"))
            .bind(texte("bande_code"))
            .bind(texte("note"))
            .execute(&mut *tx)
            .await?;
        ajoutees += 1;
    }
    sqlx::query(
        "UPDATE importjournal SET statut='applique',applique_le=CURRENT_TIMESTAMP WHERE token=?",
    )
    .bind(&token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // Les affectations par bande suivent la même mécanique que les imports PDF.
    db::auto_assign_economic_invoices(&state.pool).await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "import",
        "genetique",
        &format!("{ajoutees} facture(s) génétique, import {token}"),
        "/economique/genetique/import/confirmer",
    )
    .await;
    Ok(Redirect::to(&format!(
        "/economique?secteur=genetique&import_ok={ajoutees}"
    ))
    .into_response())
}

pub(super) async fn annuler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    sqlx::query("UPDATE importjournal SET statut='annule' WHERE token=? AND statut='apercu' AND type_import='genetique'")
        .bind(&token)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique?secteur=genetique").into_response())
}

/// Séparateur du fichier : point-virgule (export Excel francophone) ou virgule.
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
    use sqlx::sqlite::SqlitePoolOptions;

    async fn etat() -> AppResult<AppState> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        sqlx::raw_sql(
            "INSERT INTO bande(id,code,active) VALUES(1,'B1.26',1); \
             INSERT INTO utilisateur(id,identifiant,hash_mdp,role,actif) VALUES(1,'test','x','eleveur',1);",
        )
        .execute(&pool)
        .await?;
        Ok(AppState::new(
            Config {
                bind: "127.0.0.1:8080".parse().unwrap(),
                db_path: "/tmp/genetique-import-test.db".into(),
                secure_cookies: false,
            },
            pool,
            crate::templates::build().map_err(AppError::Internal)?,
        ))
    }

    fn session() -> SessionData {
        SessionData {
            uid: 1,
            identifiant: "test".into(),
            nom: "Test".into(),
            role: "eleveur".into(),
            sections: vec![],
            csrf: "test".into(),
            doit_changer_mdp: false,
            type_elevage: "naisseur_engraisseur".into(),
            module_genetique: true,
            module_prestataires: true,
            module_charcutiers_rfid: false,
            module_vente_directe: true,
        }
    }

    fn confirmation(token: &str) -> HashMap<String, String> {
        [("csrf_token", "test"), ("token", token)]
            .into_iter()
            .map(|(cle, valeur)| (cle.to_string(), valeur.to_string()))
            .collect()
    }

    async fn dernier_token(state: &AppState) -> AppResult<String> {
        Ok(
            sqlx::query_scalar("SELECT token FROM importjournal ORDER BY rowid DESC LIMIT 1")
                .fetch_one(&state.pool)
                .await?,
        )
    }

    /// Chemin complet : aperçu puis confirmation. Vérifie qu'aucune facture
    /// n'est écrite avant la confirmation, que les nombres à virgule française
    /// sont lus, et que la bande connue est conservée.
    #[tokio::test]
    async fn apercu_puis_confirmation_enregistre_les_factures() -> AppResult<()> {
        let state = etat().await?;
        let csv = "date;num_facture;fournisseur;designation;nb_animaux;poids_total;prix_moyen;montant_ht;bande_code;note\n2026-01-15;FA-1;DanBred;Cochettes;12;2280;285,50;3426,00;B1.26;\n";
        analyser(&state, &session(), csv.as_bytes(), "factures.csv").await?;

        let avant: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM achatgenetique")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(avant, 0, "l'aperçu ne doit rien enregistrer");

        let token = dernier_token(&state).await?;
        confirmer(
            State(state.clone()),
            Extension(session()),
            Form(confirmation(&token)),
        )
        .await?;

        let (facture, montant, bande, animaux): (String, f64, Option<String>, i64) =
            sqlx::query_as(
                "SELECT num_facture,montant_ht,bande_code,nb_animaux FROM achatgenetique",
            )
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(facture, "FA-1");
        assert_eq!(montant, 3426.0);
        assert_eq!(bande.as_deref(), Some("B1.26"));
        assert_eq!(animaux, 12);
        Ok(())
    }

    /// Une ligne fautive doit être signalée sans bloquer l'analyse des autres,
    /// et un doublon interne au fichier ne doit pas produire deux factures.
    #[tokio::test]
    async fn les_anomalies_sont_detectees_ligne_par_ligne() -> AppResult<()> {
        let state = etat().await?;
        let csv = "date;num_facture;montant_ht;bande_code\n                   2026-01-15;FA-1;100,00;B1.26\n                   2026-01-16;FA-1;50,00;B1.26\n                   pas-une-date;FA-2;50,00;\n                   2026-01-17;;50,00;\n                   2026-01-18;FA-3;;\n                   2026-01-19;FA-4;20,00;B-INCONNUE\n";
        analyser(&state, &session(), csv.as_bytes(), "factures.csv").await?;
        let token = dernier_token(&state).await?;
        let anomalies: Vec<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT numero_ligne,action,anomalie FROM importligne WHERE token=? ORDER BY numero_ligne",
        )
        .bind(&token)
        .fetch_all(&state.pool)
        .await?;
        let actions: Vec<&str> = anomalies.iter().map(|(_, a, _)| a.as_str()).collect();
        assert_eq!(
            actions,
            ["ajouter", "erreur", "erreur", "erreur", "erreur", "ajouter"]
        );
        // La bande inconnue est signalée mais la ligne reste importable.
        let derniere = anomalies.last().expect("une dernière ligne");
        assert!(derniere
            .2
            .as_deref()
            .unwrap_or_default()
            .contains("inconnue"));
        Ok(())
    }

    /// Le même fichier ne doit pas pouvoir être importé deux fois, même
    /// renommé : c'est le garde-fou déjà appliqué aux autres imports.
    #[tokio::test]
    async fn le_meme_fichier_est_refuse_une_seconde_fois() -> AppResult<()> {
        let state = etat().await?;
        let csv = "date;num_facture;montant_ht\n2026-01-15;FA-1;100,00\n";
        analyser(&state, &session(), csv.as_bytes(), "a.csv").await?;
        let refus = analyser(&state, &session(), csv.as_bytes(), "renomme.csv").await;
        assert!(refus.is_err(), "un fichier déjà importé doit être refusé");
        Ok(())
    }

    /// Un fichier sans les colonnes obligatoires doit être refusé avec un
    /// message qui renvoie vers le modèle, pas avec une erreur technique.
    #[tokio::test]
    async fn un_fichier_sans_colonne_obligatoire_est_refuse() -> AppResult<()> {
        let state = etat().await?;
        let erreur = analyser(&state, &session(), b"foo;bar\n1;2\n", "mauvais.csv")
            .await
            .expect_err("colonnes manquantes");
        assert!(format!("{erreur:?}").contains("modèle"));
        Ok(())
    }

    #[test]
    fn le_modele_fournit_une_valeur_dexemple_par_colonne() {
        // Garde-fou : une colonne ajoutée sans exemple décalerait tout le
        // fichier téléchargé d'une cellule.
        assert_eq!(COLONNES.len(), EXEMPLE.len());
        for obligatoire in ["date", "num_facture", "montant_ht"] {
            assert!(
                COLONNES.contains(&obligatoire),
                "colonne obligatoire {obligatoire} absente du modèle"
            );
        }
    }

    #[test]
    fn delimiteur_reconnait_le_point_virgule_et_la_virgule() {
        assert_eq!(delimiteur(b"a;b;c\n1;2;3"), b';');
        assert_eq!(delimiteur(b"a,b,c\n1,2,3"), b',');
        // Fichier vide : pas de panique d'indexation, point-virgule par défaut.
        assert_eq!(delimiteur(b""), b';');
    }
}
