//! Import de l'export « Historique truie » (une ligne par cycle de
//! reproduction/bande et par truie, ~67 colonnes) produit par les anciens
//! logiciels d'élevage. C'est un format bien plus riche que le petit CSV
//! d'identité géré par `truies_import` (num_travail/rfid/race/bande) : il
//! porte aussi tout l'historique reproduction (IA, écho, retour,
//! avortement, mise-bas, sevrage), les six mesures ELD prises à des étapes
//! différentes, et la réforme.
//!
//! Comme pour `truies_import`, on procède en deux temps (aperçu puis
//! confirmation) via `importjournal`/`importligne`, avec `type_import =
//! 'historique_truie'`. Chaque ligne source devient une ligne
//! `importligne` dont `donnees_json` porte tous les champs déjà normalisés
//! (dates ISO, nombres). Les truies déjà présentes sont mises à jour
//! (identité complétée), jamais dupliquées ; les événements et mesures sont
//! insérés uniquement s'ils n'existent pas déjà (même truie/type/date ou
//! même truie/date/période), pour que réimporter le même fichier soit sans
//! effet.
use super::*;
use crate::machine_soupe::{number_fr, parse_date_fr};

/// Colonnes attendues, dans l'ordre exact de l'export. Sert à la fois à
/// générer le modèle téléchargeable et à retrouver chaque valeur par nom
/// (l'ordre réel des colonnes dans un fichier fourni peut varier légèrement
/// selon la version de l'export, d'où la recherche par en-tête plutôt que
/// par position fixe).
const COLUMNS: &[&str] = &[
    "N° travail",
    "N° national",
    "Race",
    "N° national père",
    "N° national mère",
    "RFID",
    "Bande",
    "Rang",
    "Mère à cochettes",
    "Date 1ère IA",
    "Nb. doses",
    "Verrats",
    "Commentaires IA",
    "ISSF",
    "Durée gestation",
    "Echographie",
    "Résultat dernière écho",
    "Date retour",
    "Type de retour",
    "Commentaire retour",
    "Date avortement",
    "Commentaire avortement",
    "Mise-bas théorique",
    "Date 1ère mise-bas",
    "Race Portée",
    "Structure",
    "NV",
    "MN",
    "NT",
    "Mo",
    "AD",
    "RE",
    "Tx perte sous la mère",
    "Commentaire mise-bas",
    "Date pesée",
    "Poids total porcelet (kg)",
    "Pertes",
    "Tx pertes sevrés par",
    "Tx traitement sevrés par",
    "Date dernier sevrage",
    "Type sevrage",
    "Nb. sevrés",
    "Poids de portée au sevrage",
    "Commentaire sevrage",
    "Date entrée quarantaine",
    "Poids entrée quarantaine",
    "ELD entrée quarantaine",
    "EMD entrée quarantaine",
    "Date sortie quarantaine",
    "Poids sortie quarantaine",
    "ELD sortie quarantaine",
    "EMD sortie quarantaine",
    "ELD IA/Régumate",
    "Date ELD IA/Régumate",
    "ELD gestante",
    "Date ELD gestante",
    "ELD entrée mater.",
    "Date ELD entrée mater.",
    "ELD sortie mater.",
    "Date ELD sortie mater.",
    "ELD autre",
    "Date ELD autre",
    "Date réforme",
    "Type réforme",
    "Cause réforme",
    "A réformer",
    "Observation",
];

pub(super) async fn modele_csv() -> Response {
    let mut body = String::from("\u{feff}");
    body.push_str(&COLUMNS.join(";"));
    body.push_str("\r\n");
    // Une ligne d'exemple correspondant à un seul cycle ; un même n° de
    // travail peut apparaître sur plusieurs lignes (une par bande/cycle).
    let example = [
        "T001", "FR000000001", "Large White", "-", "-", "250000000001", "B1.26", "1", "Non",
        "01/01/2026", "2", "-", "-", "-", "114", "23/01/2026", "P", "-", "-", "-", "-", "-",
        "25/04/2026", "24/04/2026", "-", "MATERNITE / M1", "14", "0", "14", "0", "0", "0",
        "0,00", "-", "-", "-", "-", "0,00", "0,00", "20/05/2026", "NORMAL", "12", "-", "-", "-",
        "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-", "-",
        "-", "-", "-", "-", "Non", "Exemple à supprimer",
    ];
    body.push_str(&example.join(";"));
    body.push_str("\r\n");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=modele_import_historique_truies.csv"),
    );
    (headers, body).into_response()
}

fn cell<'a>(index: &HashMap<String, usize>, record: &'a csv::StringRecord, key: &str) -> Option<&'a str> {
    let position = *index.get(&normalize_header(key))?;
    record
        .get(position)
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "-")
}

fn normalize_header(raw: &str) -> String {
    raw.trim().trim_start_matches('\u{feff}').to_lowercase()
}

fn cell_date(index: &HashMap<String, usize>, record: &csv::StringRecord, key: &str) -> Option<String> {
    cell(index, record, key).and_then(parse_date_fr)
}

fn cell_num(index: &HashMap<String, usize>, record: &csv::StringRecord, key: &str) -> Option<f64> {
    cell(index, record, key).and_then(number_fr)
}

fn cell_int(index: &HashMap<String, usize>, record: &csv::StringRecord, key: &str) -> Option<i64> {
    cell(index, record, key).and_then(|value| value.parse::<i64>().ok())
}

/// Reconstruit une valeur JSON par ligne source, tous les champs déjà
/// normalisés (dates en AAAA-MM-JJ, nombres décodés). Cette valeur est ce
/// qui est stocké dans `importligne.donnees_json` et relu tel quel à la
/// confirmation.
fn row_to_json(index: &HashMap<String, usize>, record: &csv::StringRecord) -> Value {
    json!({
        "num_travail": cell(index, record, "N° travail"),
        "num_national": cell(index, record, "N° national"),
        "race": cell(index, record, "Race"),
        "pere_national": cell(index, record, "N° national père"),
        "mere_national": cell(index, record, "N° national mère"),
        "rfid": cell(index, record, "RFID"),
        "bande_code": cell(index, record, "Bande"),
        "rang": cell_int(index, record, "Rang"),
        "mere_cochette": cell(index, record, "Mère à cochettes").is_some_and(|v| v.eq_ignore_ascii_case("oui")),
        "date_ia": cell_date(index, record, "Date 1ère IA"),
        "nb_doses": cell_int(index, record, "Nb. doses"),
        "verrats": cell(index, record, "Verrats"),
        "commentaire_ia": cell(index, record, "Commentaires IA"),
        "date_echo": cell_date(index, record, "Echographie"),
        "resultat_echo": cell(index, record, "Résultat dernière écho"),
        "date_retour": cell_date(index, record, "Date retour"),
        "type_retour": cell(index, record, "Type de retour"),
        "commentaire_retour": cell(index, record, "Commentaire retour"),
        "date_avortement": cell_date(index, record, "Date avortement"),
        "commentaire_avortement": cell(index, record, "Commentaire avortement"),
        "date_mise_bas": cell_date(index, record, "Date 1ère mise-bas"),
        "structure": cell(index, record, "Structure"),
        "nv": cell_int(index, record, "NV"),
        "mn": cell_int(index, record, "MN"),
        "nt": cell_int(index, record, "NT"),
        "mo": cell_int(index, record, "Mo"),
        "ad": cell_int(index, record, "AD"),
        "re": cell_int(index, record, "RE"),
        "commentaire_mise_bas": cell(index, record, "Commentaire mise-bas"),
        "pertes": cell(index, record, "Pertes"),
        "date_dernier_sevrage": cell_date(index, record, "Date dernier sevrage"),
        "type_sevrage": cell(index, record, "Type sevrage"),
        "nb_sevres": cell_int(index, record, "Nb. sevrés"),
        "poids_portee_sevrage": cell_num(index, record, "Poids de portée au sevrage"),
        "commentaire_sevrage": cell(index, record, "Commentaire sevrage"),
        "date_entree_quarantaine": cell_date(index, record, "Date entrée quarantaine"),
        "poids_entree_quarantaine": cell_num(index, record, "Poids entrée quarantaine"),
        "eld_entree_quarantaine": cell_num(index, record, "ELD entrée quarantaine"),
        "date_sortie_quarantaine": cell_date(index, record, "Date sortie quarantaine"),
        "poids_sortie_quarantaine": cell_num(index, record, "Poids sortie quarantaine"),
        "eld_sortie_quarantaine": cell_num(index, record, "ELD sortie quarantaine"),
        "eld_ia_regumate": cell_num(index, record, "ELD IA/Régumate"),
        "date_eld_ia_regumate": cell_date(index, record, "Date ELD IA/Régumate"),
        "eld_gestante": cell_num(index, record, "ELD gestante"),
        "date_eld_gestante": cell_date(index, record, "Date ELD gestante"),
        "eld_entree_mater": cell_num(index, record, "ELD entrée mater."),
        "date_eld_entree_mater": cell_date(index, record, "Date ELD entrée mater."),
        "eld_sortie_mater": cell_num(index, record, "ELD sortie mater."),
        "date_eld_sortie_mater": cell_date(index, record, "Date ELD sortie mater."),
        "eld_autre": cell_num(index, record, "ELD autre"),
        "date_eld_autre": cell_date(index, record, "Date ELD autre"),
        "date_reforme": cell_date(index, record, "Date réforme"),
        "type_reforme": cell(index, record, "Type réforme"),
        "cause_reforme": cell(index, record, "Cause réforme"),
        "a_reformer": cell(index, record, "A réformer").is_some_and(|v| v.eq_ignore_ascii_case("oui")),
        "observation": cell(index, record, "Observation"),
    })
}

/// Cœur du parsing, sans dépendance à axum ni à la base — donc directement
/// testable. Détecte la ligne d'en-tête (la première ligne de l'export est
/// parfois un intitulé de site, ex. « 35DA9 », pas les colonnes), construit
/// l'index nom de colonne → position, puis convertit chaque ligne de
/// données non vide en JSON déjà normalisé.
fn parse_historique_csv(text: &str) -> AppResult<Vec<Value>> {
    let mut lines = text.lines();
    let header_line = loop {
        match lines.next() {
            Some(line) => {
                if normalize_header(line).contains("travail") {
                    break line.to_string();
                }
            }
            None => {
                return Err(AppError::Invalid(
                    "Colonne « N° travail » introuvable dans l'en-tête".into(),
                ))
            }
        }
    };
    let remaining: String = lines.collect::<Vec<_>>().join("\n");
    let content = format!("{header_line}\n{remaining}");
    // `flexible(true)` : l'export a un point-virgule final sur la ligne
    // d'en-tête (dernière colonne « Observation » suivie de « ; »), ce qui
    // crée une 68ᵉ colonne fantôme sans nom ; les lignes de données, elles,
    // n'ont pas ce point-virgule final et ne comptent donc que 67 champs.
    // Sans ce réglage, le lecteur CSV rejette chaque ligne comme
    // « nombre de champs incohérent ».
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .trim(csv::Trim::All)
        .has_headers(false)
        .flexible(true)
        .from_reader(content.as_bytes());
    let mut records = reader.records();
    let header = records
        .next()
        .ok_or_else(|| AppError::Invalid("Fichier vide".into()))?
        .map_err(|error| AppError::Invalid(error.to_string()))?;
    let mut index = HashMap::new();
    for (position, value) in header.iter().enumerate() {
        index.insert(normalize_header(value), position);
    }
    if !index.contains_key(&normalize_header("N° travail")) {
        return Err(AppError::Invalid("Colonne « N° travail » manquante".into()));
    }
    let mut rows = Vec::new();
    for record in records {
        let record = record.map_err(|error| AppError::Invalid(error.to_string()))?;
        if record.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        rows.push(row_to_json(&index, &record));
    }
    Ok(rows)
}

pub(super) async fn importer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let mut data = None;
    let mut filename = "import-historique-truies.csv".to_string();
    let mut csrf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        match field.name().map(str::to_string).as_deref() {
            Some("csrf_token") => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            Some("fichier") => {
                filename = field
                    .file_name()
                    .unwrap_or("import-historique-truies.csv")
                    .chars()
                    .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
                    .take(180)
                    .collect();
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            _ => {}
        }
    }
    if csrf.as_deref() != Some(session.csrf.as_str()) {
        return Err(AppError::Forbidden);
    }
    let bytes = data.ok_or_else(|| AppError::Invalid("Fichier CSV manquant".into()))?;
    if bytes.len() > 15 * 1024 * 1024 {
        return Err(AppError::Invalid("Fichier trop volumineux".into()));
    }
    let digest = contenu_sha256(&bytes);
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|&byte| byte as char).collect());
    let parsed = parse_historique_csv(&text)?;

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut errors = 0_i64;
    let mut truies = HashSet::new();
    let mut lignes = 0_i64;
    let mut tx = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut tx, &digest).await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,'historique_truie',?,'apercu',?,?)")
        .bind(&token)
        .bind(&filename)
        .bind(session.uid)
        .bind(&digest)
        .execute(&mut *tx)
        .await?;
    for (line_number, payload) in parsed.into_iter().enumerate() {
        let number = payload
            .get("num_travail")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let action = if number.is_empty() { "erreur" } else { "ajouter" };
        let anomaly = number
            .is_empty()
            .then(|| "Numéro de travail manquant".to_string());
        if action == "erreur" {
            errors += 1;
        } else {
            truies.insert(number);
        }
        lignes += 1;
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
            .bind(&token)
            .bind(line_number as i64 + 2)
            .bind(action)
            .bind(&anomaly)
            .bind(payload.to_string())
            .execute(&mut *tx)
            .await?;
    }
    let summary = json!({"lignes": lignes, "truies": truies.len(), "erreur": errors});
    sqlx::query("UPDATE importjournal SET resume=? WHERE token=?")
        .bind(summary.to_string())
        .bind(&token)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let mut ctx = context(&session);
    ctx.insert("token".into(), json!(token));
    ctx.insert("nom_fichier".into(), json!(filename));
    ctx.insert("resume".into(), summary);
    Ok(render(&state, "import_historique_apercu.html", Value::Object(ctx))?.into_response())
}

/// Motifs de sortie normalisés (`SOW_EXIT_REASONS`) les plus proches d'un
/// motif libre de l'ancien logiciel. La cause détaillée d'origine est de
/// toute façon conservée telle quelle dans la note de la truie.
fn motif_sortie(type_reforme: Option<&str>, cause: Option<&str>) -> &'static str {
    // La colonne « Type réforme » de l'ancien logiciel ne porte que deux
    // valeurs génériques (« Vente » ou « Mort ») — la vraie raison est dans
    // « Cause réforme » (ex. « Renversement utérus/rectum »). Il faut donc
    // toujours regarder d'abord la cause détaillée, sinon « Vente » masque
    // systématiquement une cause médicale précise avant même d'être
    // examinée.
    let cause = cause.unwrap_or_default().to_lowercase();
    let type_reforme = type_reforme.unwrap_or_default().to_lowercase();
    if cause.contains("utérus") || cause.contains("uterus") || cause.contains("rectum") || cause.contains("prolapsus") {
        "Prolapsus"
    } else if cause.contains("locomot") || cause.contains("boit") {
        "Boiterie / appareil locomoteur"
    } else if cause.contains("génétique") || cause.contains("genetique") {
        "Performances insuffisantes"
    } else if type_reforme.contains("mort") || cause.contains("mort") || type_reforme.contains("euthanas") {
        "Mortalité de cause indéterminée"
    } else if type_reforme.contains("vente") || type_reforme.contains("transfert") {
        "Vente / transfert"
    } else {
        "Autre motif validé"
    }
}

pub(super) async fn confirmer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    let mut tx = state.pool.begin().await?;
    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import='historique_truie'",
    )
    .bind(&token)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT numero_ligne,donnees_json FROM importligne WHERE token=? AND action='ajouter' ORDER BY numero_ligne",
    )
    .bind(&token)
    .fetch_all(&mut *tx)
    .await?;

    let mut truies_traitees = HashSet::new();
    let mut evenements = 0_i64;
    let mut mesures = 0_i64;
    let mut reformes = 0_i64;

    for (line, raw) in &rows {
        let data: Value = serde_json::from_str(raw)
            .map_err(|_| AppError::Invalid(format!("Données invalides à la ligne {line}")))?;
        let outcome = apply_row(&mut tx, &token, *line, &data).await?;
        truies_traitees.insert(outcome.num_travail);
        evenements += outcome.evenements;
        mesures += outcome.mesures;
        reformes += outcome.reforme as i64;
    }

    sqlx::query(
        "UPDATE importjournal SET statut='applique',applique_le=CURRENT_TIMESTAMP WHERE token=?",
    )
    .bind(&token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "import",
        "historique_truie",
        &format!(
            "{} truie(s), {} événement(s), {} mesure(s), {} réforme(s), import {token}",
            truies_traitees.len(),
            evenements,
            mesures,
            reformes
        ),
        "/truies/import-historique/confirmer",
    )
    .await;
    Ok(Redirect::to(&format!(
        "/truies?import_historique_ok={}",
        truies_traitees.len()
    ))
    .into_response())
}

struct RowOutcome {
    num_travail: String,
    evenements: i64,
    mesures: i64,
    reforme: bool,
}

/// Applique une ligne source déjà normalisée (voir `row_to_json`) : upsert
/// de l'identité de la truie, événements de reproduction, mesures ELD/poids
/// et réforme. Idempotent — rejouer la même ligne n'ajoute rien de plus.
async fn apply_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    token: &str,
    line: i64,
    data: &Value,
) -> AppResult<RowOutcome> {
    let mut evenements = 0_i64;
    let mut mesures = 0_i64;
    let mut reforme = false;
    let str_field = |key: &str| data.get(key).and_then(Value::as_str);
        let num_field = |key: &str| data.get(key).and_then(Value::as_f64);
        let int_field = |key: &str| data.get(key).and_then(Value::as_i64);
        let bool_field = |key: &str| data.get(key).and_then(Value::as_bool).unwrap_or(false);
        let number = str_field("num_travail")
            .ok_or_else(|| AppError::Invalid(format!("Numéro absent à la ligne {line}")))?;

        // 1) Truie : créée si absente, sinon identité complétée uniquement
        //    là où elle est encore vide, pour ne jamais écraser une valeur
        //    déjà correcte saisie depuis dans l'application.
        let sow_id: i64 = if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM truie WHERE lower(trim(num_travail))=lower(trim(?))",
        )
        .bind(number)
        .fetch_optional(&mut **tx)
        .await?
        {
            sqlx::query(
                "UPDATE truie SET \
                 num_national=COALESCE(NULLIF(num_national,''),?), \
                 race=COALESCE(NULLIF(race,''),?), \
                 rfid=COALESCE(NULLIF(rfid,''),?), \
                 pere_national=COALESCE(NULLIF(pere_national,''),?), \
                 mere_national=COALESCE(NULLIF(mere_national,''),?), \
                 bande_code=COALESCE(?,bande_code), \
                 rang=CASE WHEN ?>rang THEN ? ELSE rang END, \
                 mere_cochette=CASE WHEN ?=1 THEN 1 ELSE mere_cochette END, \
                 updated_at=CURRENT_TIMESTAMP \
                 WHERE id=?",
            )
            .bind(str_field("num_national"))
            .bind(str_field("race"))
            .bind(str_field("rfid"))
            .bind(str_field("pere_national"))
            .bind(str_field("mere_national"))
            .bind(str_field("bande_code"))
            .bind(int_field("rang").unwrap_or(0))
            .bind(int_field("rang").unwrap_or(0))
            .bind(bool_field("mere_cochette") as i64)
            .bind(id)
            .execute(&mut **tx)
            .await?;
            id
        } else {
            sqlx::query(
                "INSERT INTO truie(num_travail,num_national,race,rfid,pere_national,mere_national,bande_code,rang,mere_cochette,statut,reformee,source_import_id) \
                 VALUES(?,?,?,?,?,?,?,?,?,'active',0,?)",
            )
            .bind(number)
            .bind(str_field("num_national"))
            .bind(str_field("race"))
            .bind(str_field("rfid"))
            .bind(str_field("pere_national"))
            .bind(str_field("mere_national"))
            .bind(str_field("bande_code"))
            .bind(int_field("rang").unwrap_or(0))
            .bind(bool_field("mere_cochette") as i64)
            .bind(token)
            .execute(&mut **tx)
            .await?
            .last_insert_rowid()
        };

        let band_id: Option<i64> = match str_field("bande_code") {
            Some(code) => sqlx::query_scalar("SELECT id FROM bande WHERE code=?")
                .bind(code)
                .fetch_optional(&mut **tx)
                .await?,
            None => None,
        };

        // 2) Événements de reproduction — un INSERT par type de fait daté
        //    présent sur la ligne, ignoré s'il existe déjà (même truie,
        //    même type, même date) pour rester rejouable sans dupliquer.
        macro_rules! insert_event {
            ($type_evt:expr, $date:expr, $sql:expr, $($binds:expr),* $(,)?) => {
                if let Some(date) = $date {
                    let existing: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM evenement WHERE truie_id=? AND type=? AND date=?",
                    )
                    .bind(sow_id)
                    .bind($type_evt)
                    .bind(&date)
                    .fetch_one(&mut **tx)
                    .await?;
                    if existing == 0 {
                        sqlx::query($sql)
                            .bind($type_evt)
                            .bind(&date)
                            .bind(sow_id)
                            .bind(band_id)
                            $(.bind($binds))*
                            .execute(&mut **tx)
                            .await?;
                        evenements += 1;
                    }
                }
            };
        }

        insert_event!(
            "ia",
            str_field("date_ia").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,produit,nb_doses,note) VALUES(?,?,?,?,?,?,?)",
            str_field("verrats"),
            int_field("nb_doses"),
            str_field("commentaire_ia"),
        );
        let echo_resultat = str_field("resultat_echo").map(|value| match value {
            "P" => "pleine",
            "N" => "vide",
            _ => "douteuse",
        });
        insert_event!(
            "echo",
            str_field("date_echo").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,resultat) VALUES(?,?,?,?,?)",
            echo_resultat,
        );
        insert_event!(
            "retour",
            str_field("date_retour").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,resultat,note) VALUES(?,?,?,?,?,?)",
            str_field("type_retour"),
            str_field("commentaire_retour"),
        );
        insert_event!(
            "avortement",
            str_field("date_avortement").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,note) VALUES(?,?,?,?,?)",
            str_field("commentaire_avortement"),
        );
        let mise_bas_note = [str_field("structure"), str_field("commentaire_mise_bas"), str_field("pertes")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let mise_bas_note = (!mise_bas_note.is_empty()).then_some(mise_bas_note);
        insert_event!(
            "mise_bas",
            str_field("date_mise_bas").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,adoptes,retires,note) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
            int_field("nt"),
            int_field("nv"),
            int_field("mn"),
            int_field("mo"),
            int_field("ad"),
            int_field("re"),
            mise_bas_note.as_deref(),
        );
        let poids_moyen = match (num_field("poids_portee_sevrage"), int_field("nb_sevres")) {
            (Some(total), Some(count)) if count > 0 => Some(total / count as f64),
            _ => None,
        };
        let sevrage_note = [str_field("type_sevrage"), str_field("commentaire_sevrage")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let sevrage_note = (!sevrage_note.is_empty()).then_some(sevrage_note);
        insert_event!(
            "sevrage",
            str_field("date_dernier_sevrage").map(str::to_string),
            "INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,poids_moyen,note) VALUES(?,?,?,?,?,?,?)",
            int_field("nb_sevres"),
            poids_moyen,
            sevrage_note.as_deref(),
        );

        // 3) Mesures ELD/poids — une ligne mesuretruie par étape présente,
        //    ignorée si une mesure identique (même truie/date/période)
        //    existe déjà.
        macro_rules! insert_mesure {
            ($periode:expr, $date:expr, $eld:expr, $poids:expr) => {
                if let Some(date) = $date {
                    if $eld.is_some() || $poids.is_some() {
                        let existing: i64 = sqlx::query_scalar(
                            "SELECT COUNT(*) FROM mesuretruie WHERE truie_id=? AND date=? AND periode=?",
                        )
                        .bind(sow_id)
                        .bind(&date)
                        .bind($periode)
                        .fetch_one(&mut **tx)
                        .await?;
                        if existing == 0 {
                            sqlx::query(
                                "INSERT INTO mesuretruie(truie_id,date,periode,eld,poids) VALUES(?,?,?,?,?)",
                            )
                            .bind(sow_id)
                            .bind(&date)
                            .bind($periode)
                            .bind($eld)
                            .bind($poids)
                            .execute(&mut **tx)
                            .await?;
                            mesures += 1;
                        }
                    }
                }
            };
        }
        insert_mesure!(
            "Entrée quarantaine",
            str_field("date_entree_quarantaine").map(str::to_string),
            num_field("eld_entree_quarantaine"),
            num_field("poids_entree_quarantaine")
        );
        insert_mesure!(
            "Sortie quarantaine",
            str_field("date_sortie_quarantaine").map(str::to_string),
            num_field("eld_sortie_quarantaine"),
            num_field("poids_sortie_quarantaine")
        );
        insert_mesure!(
            "IA/Régumate",
            str_field("date_eld_ia_regumate").map(str::to_string),
            num_field("eld_ia_regumate"),
            None::<f64>
        );
        insert_mesure!(
            "Gestante",
            str_field("date_eld_gestante").map(str::to_string),
            num_field("eld_gestante"),
            None::<f64>
        );
        insert_mesure!(
            "Entrée maternité",
            str_field("date_eld_entree_mater").map(str::to_string),
            num_field("eld_entree_mater"),
            None::<f64>
        );
        insert_mesure!(
            "Sortie maternité",
            str_field("date_eld_sortie_mater").map(str::to_string),
            num_field("eld_sortie_mater"),
            None::<f64>
        );
        insert_mesure!(
            "Autre",
            str_field("date_eld_autre").map(str::to_string),
            num_field("eld_autre"),
            None::<f64>
        );

        // 4) Réforme — met à jour la truie une fois pour toutes si une date
        //    de réforme est présente sur cette ligne, sans jamais revenir
        //    en arrière sur une réforme déjà enregistrée par ailleurs.
        if let Some(date_reforme) = str_field("date_reforme") {
            let deja_reformee: i64 =
                sqlx::query_scalar("SELECT reformee FROM truie WHERE id=?")
                    .bind(sow_id)
                    .fetch_one(&mut **tx)
                    .await?;
            if deja_reformee == 0 {
                let motif = motif_sortie(str_field("type_reforme"), str_field("cause_reforme"));
                let note_cause = str_field("cause_reforme").map(|cause| {
                    format!(
                        "Import historique — cause d'origine : {} ({})",
                        cause,
                        str_field("type_reforme").unwrap_or("-")
                    )
                });
                sqlx::query(
                    "UPDATE truie SET reformee=1,statut='sortie',date_reforme=?,motif_sortie=?,note=NULLIF(TRIM(COALESCE(NULLIF(note,'')||' / ','')||COALESCE(?,'')),''),updated_at=CURRENT_TIMESTAMP WHERE id=?",
                )
                .bind(date_reforme)
                .bind(motif)
                .bind(note_cause)
                .bind(sow_id)
                .execute(&mut **tx)
                .await?;
                reforme = true;
            }
        }

    Ok(RowOutcome {
        num_travail: number.to_string(),
        evenements,
        mesures,
        reforme,
    })
}

pub(super) async fn annuler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT cree_par FROM importjournal WHERE token=? AND type_import='historique_truie'",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await?
    .flatten();
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM importjournal WHERE token=? AND statut='apercu'")
        .bind(&token)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/truies").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    // En-tête et lignes ci-dessous sont de vrais extraits d'un export
    // « historique truie » fourni par un éleveur (fichier
    // `20260826_historique_truie_35DA9_268869.csv`), pas des données
    // inventées — pour que le test protège vraiment contre une régression
    // sur ce format.
    const HEADER: &str = "N° travail;N° national;Race;N° national père;N° national mère;RFID;Bande;Rang;Mère à cochettes;Date 1ère IA;Nb. doses;Verrats;Commentaires IA;ISSF;Durée gestation;Echographie;Résultat dernière écho;Date retour;Type de retour;Commentaire retour;Date avortement;Commentaire avortement;Mise-bas théorique;Date 1ère mise-bas;Race Portée;Structure;NV;MN;NT;Mo;AD;RE;Tx perte sous la mère;Commentaire mise-bas;Date pesée;Poids total porcelet (kg);Pertes;Tx pertes sevrés par;Tx traitement sevrés par;Date dernier sevrage;Type sevrage;Nb. sevrés;Poids de portée au sevrage;Commentaire sevrage;Date entrée quarantaine;Poids entrée quarantaine;ELD entrée quarantaine;EMD entrée quarantaine;Date sortie quarantaine;Poids sortie quarantaine;ELD sortie quarantaine;EMD sortie quarantaine;ELD IA/Régumate;Date ELD IA/Régumate;ELD gestante;Date ELD gestante;ELD entrée mater.;Date ELD entrée mater.;ELD sortie mater.;Date ELD sortie mater.;ELD autre;Date ELD autre;Date réforme;Type réforme;Cause réforme;A réformer;Observation;";

    const ROW_409298: &str = "\"409298\";\"FR35GW2202400005\";\"NU131\";\"-\";\"-\";\"20009990002300001346\";\"B1.05\";\"2\";\"Non\";\"26/01/2026\";\"2\";\"-\";\"-\";\"4\";\"112\";\"23/02/2026\";\"P\";\"-\";\"-\";\"-\";\"-\";\"-\";\"21/05/2026\";\"18/05/2026\";\"-\";\"MATERNITE / M1\";\"22\";\"0\";\"22\";\"0\";\"0\";\"0\";\"40,91\";\"-\";\"-\";\"-\";\"Faible à la naissance: 3 / Ecrasés / Truie bloquée: 6\";\"13,33\";\"6,67\";\"17/06/2026\";\"NORMAL\";\"13\";\"-\";\"-\";\"01/07/2025\";\"-\";\"19,00\";\"-\";\"-\";\"-\";\"-\";\"-\";\"11,00\";\"20/01/2026\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"Non\";\"-\"";

    const ROW_409344_REFORME: &str = "\"409344\";\"FR35GW2202400006\";\"NU131\";\"-\";\"-\";\"20009990002300001315\";\"B1.05\";\"2\";\"Non\";\"26/01/2026\";\"2\";\"-\";\"-\";\"4\";\"112\";\"23/02/2026\";\"P\";\"-\";\"-\";\"-\";\"-\";\"-\";\"21/05/2026\";\"18/05/2026\";\"-\";\"MATERNITE / M3\";\"13\";\"0\";\"13\";\"0\";\"0\";\"0\";\"23,08\";\"-\";\"-\";\"-\";\"Ecrasés / Truie bloquée: 3\";\"23,08\";\"0,00\";\"17/06/2026\";\"NORMAL\";\"10\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"6,00\";\"20/01/2026\";\"-\";\"-\";\"6,00\";\"18/05/2026\";\"-\";\"-\";\"-\";\"-\";\"17/06/2026\";\"Vente\";\"Renversement utérus/rectum\";\"Non\";\"-\"";

    const ROW_409826_QUARANTAINE: &str = "\"409826\";\"FR35GW2202400024\";\"NU131\";\"-\";\"-\";\"20009990002300001310\";\"B2.07\";\"1\";\"Non\";\"20/10/2025\";\"3\";\"Justin\";\"-\";\"-\";\"-\";\"17/11/2025\";\"N\";\"-\";\"-\";\"-\";\"-\";\"-\";\"12/02/2026\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"01/07/2025\";\"-\";\"18,00\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"-\";\"01/02/2026\";\"Vente\";\"Autre\";\"Oui\";\"-\"";

    #[test]
    fn detecte_en_tete_apres_ligne_de_site_et_normalise_les_valeurs() {
        // La première ligne d'un vrai export est un intitulé de site
        // (« 35DA9 »), pas les colonnes : doit être sautée sans erreur.
        let text = format!("35DA9\n{HEADER}\n{ROW_409298}\n");
        let rows = parse_historique_csv(&text).expect("parsing ok");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["num_travail"], "409298");
        assert_eq!(row["num_national"], "FR35GW2202400005");
        assert_eq!(row["race"], "NU131");
        assert_eq!(row["rfid"], "20009990002300001346");
        assert_eq!(row["bande_code"], "B1.05");
        assert_eq!(row["rang"], 2);
        assert_eq!(row["mere_cochette"], false);
        assert_eq!(row["date_ia"], "2026-01-26");
        assert_eq!(row["nb_doses"], 2);
        assert_eq!(row["date_echo"], "2026-02-23");
        assert_eq!(row["resultat_echo"], "P");
        assert_eq!(row["date_mise_bas"], "2026-05-18");
        assert_eq!(row["nt"], 22);
        assert_eq!(row["nv"], 22);
        assert_eq!(row["date_dernier_sevrage"], "2026-06-17");
        assert_eq!(row["nb_sevres"], 13);
        assert_eq!(row["eld_entree_quarantaine"], 19.0);
        assert_eq!(row["date_entree_quarantaine"], "2025-07-01");
        assert_eq!(row["eld_ia_regumate"], 11.0);
        assert_eq!(row["date_eld_ia_regumate"], "2026-01-20");
        assert!(row["date_reforme"].is_null());
    }

    async fn schema_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn cree_la_truie_ses_evenements_et_ses_mesures_eld() {
        let pool = schema_pool().await;
        let text = format!("{HEADER}\n{ROW_409298}\n");
        let rows = parse_historique_csv(&text).unwrap();
        let mut tx = pool.begin().await.unwrap();
        let outcome = apply_row(&mut tx, "test-token", 2, &rows[0]).await.unwrap();
        tx.commit().await.unwrap();

        assert_eq!(outcome.num_travail, "409298");
        assert!(!outcome.reforme);
        assert_eq!(outcome.evenements, 4); // ia, echo, mise_bas, sevrage
        assert_eq!(outcome.mesures, 2); // entrée + sortie maternité

        let sow: (String, i64) = sqlx::query_as("SELECT num_travail,reformee FROM truie")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(sow.0, "409298");
        assert_eq!(sow.1, 0);

        let events: Vec<String> = sqlx::query_scalar("SELECT type FROM evenement ORDER BY type")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(events, vec!["echo", "ia", "mise_bas", "sevrage"]);

        let mesures: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mesuretruie")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mesures, 2);
    }

    #[tokio::test]
    async fn applique_la_reforme_avec_un_motif_normalise_et_conserve_la_cause_dorigine() {
        let pool = schema_pool().await;
        let text = format!("{HEADER}\n{ROW_409344_REFORME}\n");
        let rows = parse_historique_csv(&text).unwrap();
        let mut tx = pool.begin().await.unwrap();
        let outcome = apply_row(&mut tx, "test-token", 2, &rows[0]).await.unwrap();
        tx.commit().await.unwrap();
        assert!(outcome.reforme);

        let sow: (i64, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT reformee,motif_sortie,date_reforme,note FROM truie WHERE num_travail='409344'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sow.0, 1);
        assert_eq!(sow.1, "Prolapsus"); // « Renversement utérus/rectum » → motif normalisé le plus proche
        assert_eq!(sow.2.as_deref(), Some("2026-06-17"));
        assert!(sow.3.unwrap().contains("Renversement utérus/rectum"));
    }

    #[tokio::test]
    async fn reimporter_la_meme_ligne_ne_duplique_rien() {
        let pool = schema_pool().await;
        let text = format!("{HEADER}\n{ROW_409826_QUARANTAINE}\n");
        let rows = parse_historique_csv(&text).unwrap();

        let mut tx = pool.begin().await.unwrap();
        let first = apply_row(&mut tx, "t1", 2, &rows[0]).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first.evenements, 2); // ia + echo
        assert_eq!(first.mesures, 1); // entrée quarantaine
        assert!(first.reforme);

        let mut tx2 = pool.begin().await.unwrap();
        let second = apply_row(&mut tx2, "t2", 2, &rows[0]).await.unwrap();
        tx2.commit().await.unwrap();
        assert_eq!(second.evenements, 0);
        assert_eq!(second.mesures, 0);
        assert!(!second.reforme, "la réforme déjà enregistrée ne doit pas être rejouée");

        let truies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(truies, 1);
        let mesures: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mesuretruie")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mesures, 1);
    }
}
