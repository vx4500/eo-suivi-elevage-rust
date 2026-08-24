use super::*;
use serde_json::Value as JsonValue;
use std::path::{Path as FsPath, PathBuf};

fn require_admin(session: &SessionData) -> AppResult<()> {
    if session.est_admin() {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn sow_band(pool: &SqlitePool, sow_id: i64) -> AppResult<Option<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=? ORDER BY b.active DESC,b.id DESC LIMIT 1",
    )
    .bind(sow_id)
    .fetch_optional(pool)
    .await?)
}

fn required_date(form: &HashMap<String, String>) -> AppResult<String> {
    form_date(form, "date")?
        .ok_or_else(|| AppError::Invalid("Date obligatoire (AAAA-MM-JJ)".into()))
}

async fn add_sow_event(
    state: &AppState,
    session: &SessionData,
    sow_id: i64,
    kind: &str,
    form: &HashMap<String, String>,
) -> AppResult<Response> {
    require_writer(session)?;
    verify_csrf(session, form)?;
    let date = required_date(form)?;
    let band_id = sow_band(&state.pool, sow_id).await?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE id=?")
        .bind(sow_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,verrat_id,produit,motif,delai_attente,resultat,nb_doses,note,suivi_actif) VALUES(?,?,?,?,?,?,?,?,?,?,?,0)")
        .bind(kind).bind(date).bind(sow_id).bind(band_id)
        .bind(form_i64(form,"verrat_id")).bind(form_text(form,"produit"))
        .bind(form_text(form,"motif")).bind(form_i64(form,"delai_attente"))
        .bind(form_text(form,"resultat")).bind(form_i64(form,"nb_doses"))
        .bind(form_text(form,"note")).execute(&state.pool).await?;
    db::journal(
        &state.pool,
        &session.nom,
        "ajouter",
        kind,
        &format!("truie={sow_id}"),
        &format!("/truie/{sow_id}/{kind}"),
    )
    .await;
    Ok(Redirect::to(&format!("/truie/{sow_id}")).into_response())
}

pub(super) async fn truie_chaleur(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(mut form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    let mut observations = Vec::new();
    for (key, label) in [
        ("chaleur_vulve", "aspect de la vulve"),
        ("chaleur_comportement", "comportement"),
        ("chaleur_immobilite", "réflexe d’immobilité"),
    ] {
        if form.contains_key(key) {
            observations.push(label);
        }
    }
    if !observations.is_empty() {
        let suffix = form_text(&form, "note")
            .map(|v| format!(" — {v}"))
            .unwrap_or_default();
        form.insert(
            "note".into(),
            format!("{}{suffix}", observations.join(", ")),
        );
    }
    add_sow_event(&state, &session, id, "chaleur", &form).await
}
pub(super) async fn truie_ia(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    add_sow_event(&state, &session, id, "ia", &form).await
}
pub(super) async fn truie_echo(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    add_sow_event(&state, &session, id, "echo", &form).await
}
pub(super) async fn truie_traitement(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if form_text(&form, "produit").is_none() {
        return Err(AppError::Invalid("Produit obligatoire".into()));
    }
    add_sow_event(&state, &session, id, "traitement", &form).await
}

pub(super) async fn truie_mise_bas(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = required_date(&form)?;
    let band_id = form_i64(&form, "bande_id").or(sow_band(&state.pool, id).await?);
    let live = form_i64(&form, "nes_vifs").unwrap_or(0).max(0);
    let still = form_i64(&form, "mort_nes").unwrap_or(0).max(0);
    let mummies = form_i64(&form, "momifies").unwrap_or(0).max(0);
    let total = live + still + mummies;
    let existing:Option<i64>=sqlx::query_scalar("SELECT id FROM evenement WHERE truie_id=? AND type='mise_bas' AND bande_id IS ? ORDER BY id DESC LIMIT 1")
        .bind(id).bind(band_id).fetch_optional(&state.pool).await?;
    let mut tx = state.pool.begin().await?;
    if let Some(event_id) = existing {
        sqlx::query("UPDATE evenement SET date=?,nes_totaux=?,nes_vifs=?,mort_nes=?,momifies=?,chetifs=?,ecrases=?,tues_truie=?,heure_debut=?,heure_fin=?,suivi_actif=?,delivrance_ok=?,note=? WHERE id=?")
            .bind(&date).bind(total).bind(live).bind(still).bind(mummies).bind(form_i64(&form,"chetifs"))
            .bind(form_i64(&form,"ecrases")).bind(form_i64(&form,"tues_truie")).bind(form_text(&form,"heure_debut"))
            .bind(form_text(&form,"heure_fin")).bind(form.contains_key("suivi_actif") as i64).bind(form_i64(&form,"delivrance_ok")).bind(form_text(&form,"note")).bind(event_id).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,chetifs,ecrases,tues_truie,heure_debut,heure_fin,suivi_actif,delivrance_ok,note) VALUES('mise_bas',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&date).bind(id).bind(band_id).bind(total).bind(live).bind(still).bind(mummies).bind(form_i64(&form,"chetifs"))
            .bind(form_i64(&form,"ecrases")).bind(form_i64(&form,"tues_truie")).bind(form_text(&form,"heure_debut"))
            .bind(form_text(&form,"heure_fin")).bind(form.contains_key("suivi_actif") as i64).bind(form_i64(&form,"delivrance_ok")).bind(form_text(&form,"note")).execute(&mut *tx).await?;
        if parse_stored_date(&date).is_some_and(|d| d <= Local::now().date_naive()) {
            sqlx::query("UPDATE truie SET rang=rang+1,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    if let Some(band_id) = band_id {
        sqlx::query("UPDATE truie SET bande_code=(SELECT code FROM bande WHERE id=?),perf_nt=?,perf_nv=?,perf_mn=?,perf_mo=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(band_id).bind(total as f64).bind(live as f64).bind(still as f64).bind(mummies as f64).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

pub(super) async fn truie_sevrage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = required_date(&form)?;
    let band_id = form_i64(&form, "bande_id").or(sow_band(&state.pool, id).await?);
    let weaned = form_i64(&form, "nb_sevres").unwrap_or(0).max(0);
    let adopted = form_i64(&form, "adoptes").unwrap_or(0).max(0);
    let removed = form_i64(&form, "retires").unwrap_or(0).max(0);
    let existing:Option<i64>=sqlx::query_scalar("SELECT id FROM evenement WHERE truie_id=? AND type='sevrage' AND bande_id IS ? ORDER BY id DESC LIMIT 1").bind(id).bind(band_id).fetch_optional(&state.pool).await?;
    let mut tx = state.pool.begin().await?;
    if let Some(event_id) = existing {
        sqlx::query("UPDATE evenement SET date=?,nb_sevres=?,poids_moyen=?,adoptes=?,retires=?,eld_entree=?,eld_sortie=?,note=? WHERE id=?")
            .bind(&date).bind(weaned).bind(form_f64(&form,"poids_moyen")).bind(adopted).bind(removed).bind(form_f64(&form,"eld_entree")).bind(form_f64(&form,"eld_sortie")).bind(form_text(&form,"note")).bind(event_id).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,poids_moyen,adoptes,retires,eld_entree,eld_sortie,note) VALUES('sevrage',?,?,?,?,?,?,?,?,?,?)")
            .bind(&date).bind(id).bind(band_id).bind(weaned).bind(form_f64(&form,"poids_moyen")).bind(adopted).bind(removed).bind(form_f64(&form,"eld_entree")).bind(form_f64(&form,"eld_sortie")).bind(form_text(&form,"note")).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE truie SET perf_sevres=?,perf_adoptes=?,perf_retires=?,bande_code=CASE WHEN date(?)<=date('now') AND bande_code=(SELECT code FROM bande WHERE id=?) THEN NULL ELSE bande_code END,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(weaned as f64).bind(adopted as f64).bind(removed as f64).bind(&date).bind(band_id).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

pub(super) async fn portee_bande_truie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((band_id, sow_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let band: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(band_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let sow: String = sqlx::query_scalar("SELECT num_travail FROM truie WHERE id=?")
        .bind(sow_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let live = form_f64(&form, "nes_vifs").or_else(|| form_f64(&form, "nv"));
    let adopted = form_f64(&form, "adoptes")
        .or_else(|| form_f64(&form, "ad"))
        .unwrap_or(0.0);
    let removed = form_f64(&form, "retires")
        .or_else(|| form_f64(&form, "re"))
        .unwrap_or(0.0);
    let weaned = form_f64(&form, "nb_sevres").or_else(|| form_f64(&form, "sev"));
    let loss_rate = match (live, weaned) {
        (Some(nv), Some(sev)) if nv + adopted > 0.0 => {
            Some(((nv + adopted - removed - sev).max(0.0) / (nv + adopted)) * 100.0)
        }
        _ => None,
    };
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE truie SET perf_adoptes=?,perf_retires=?,perf_sevres=?,perf_tx_perte=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(adopted).bind(removed).bind(weaned).bind(loss_rate).bind(sow_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE porteerang SET ad=?,re=?,sev=?,tx_pertes=? WHERE id=(SELECT id FROM porteerang WHERE num_travail=? AND bande=? ORDER BY rang DESC,id DESC LIMIT 1)")
        .bind(adopted).bind(removed).bind(weaned).bind(loss_rate).bind(sow).bind(&band).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/bande/{band_id}")).into_response())
}

pub(super) async fn truie_sortie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = required_date(&form)?;
    sqlx::query("UPDATE truie SET reformee=1,statut='sortie',date_reforme=?,motif_sortie=?,prix_sortie=?,num_apport_sortie=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(date).bind(form_text(&form,"motif_sortie").or_else(||form_text(&form,"motif"))).bind(form_f64(&form,"prix_sortie")).bind(form_text(&form,"num_apport_sortie")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

pub(super) async fn truie_reclasser_verrat(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let row: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT num_travail,race,note FROM truie WHERE id=? AND reformee=0")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((code, race, note)) = row else {
        return Err(AppError::NotFound);
    };
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO verrat(code,race,note,actif) SELECT ?,?,?,1 WHERE NOT EXISTS(SELECT 1 FROM verrat WHERE lower(code)=lower(?))").bind(&code).bind(race).bind(note).bind(&code).execute(&mut *tx).await?;
    sqlx::query("UPDATE truie SET reformee=1,statut='reclasse_verrat',date_reforme=?,motif_sortie='Reclassé verrat',updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(form_date(&form,"date")?).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to("/truies").into_response())
}

#[allow(dead_code)]
pub(super) async fn attente(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let capacite: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT valeur FROM reglage WHERE cle='capacite_verraterie'),31)",
    )
    .fetch_one(&state.pool)
    .await?;
    let occupation:i64=sqlx::query_scalar("SELECT COUNT(*) FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id LEFT JOIN salle s ON s.id=COALESCE(t.salle_id,c.salle_id) WHERE t.reformee=0 AND (lower(COALESCE(s.type,'')) LIKE '%verrater%' OR lower(COALESCE(s.nom,'')) LIKE '%verrater%')").fetch_one(&state.pool).await?;
    let rows=generic_rows(&state.pool,r#"
        SELECT t.id,t.num_travail,t.num_national,t.rfid,t.race,t.date_entree,t.rang,
          CASE
            WHEN t.rang=0 THEN 'Cochette'
            WHEN (SELECT e.type FROM evenement e WHERE e.truie_id=t.id ORDER BY e.date DESC,e.id DESC LIMIT 1)='sevrage' THEN 'Sortie maternité'
            WHEN lower(COALESCE((SELECT e.resultat FROM evenement e WHERE e.truie_id=t.id AND e.type='echo' ORDER BY e.date DESC,e.id DESC LIMIT 1),'')) IN ('vide','négative','negative','negatif','négatif') THEN 'Retour IA'
            ELSE 'Sans bande'
          END AS categorie,
          (SELECT e.type FROM evenement e WHERE e.truie_id=t.id ORDER BY e.date DESC,e.id DESC LIMIT 1) AS dernier_evenement,
          (SELECT e.date FROM evenement e WHERE e.truie_id=t.id ORDER BY e.date DESC,e.id DESC LIMIT 1) AS derniere_date,
          (SELECT e.date FROM evenement e WHERE e.truie_id=t.id AND e.type='sevrage' ORDER BY e.date DESC,e.id DESC LIMIT 1) AS dernier_sevrage,
          date((SELECT e.date FROM evenement e WHERE e.truie_id=t.id AND e.type='sevrage' ORDER BY e.date DESC,e.id DESC LIMIT 1),printf('+%d day',COALESCE((SELECT valeur FROM reglage WHERE cle='chaleur_post_sevrage_j'),5))) AS chaleur_prevue,
          COALESCE(si.nom,si.code) AS batiment,s.nom AS salle,c.nom AS case_nom
        FROM truie t
        LEFT JOIN casesalle c ON c.id=t.case_id
        LEFT JOIN salle s ON s.id=COALESCE(t.salle_id,c.salle_id)
        LEFT JOIN site si ON si.id=s.site_id
        WHERE t.reformee=0
          AND NOT EXISTS(SELECT 1 FROM bande b WHERE b.active=1 AND b.code=t.bande_code)
        ORDER BY CASE WHEN chaleur_prevue IS NULL THEN 1 ELSE 0 END,chaleur_prevue,t.num_travail
    "#).await?;
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let places_disponibles = places_disponibles(capacite, occupation);
    let attente = rows.len() as i64;
    let mut ctx = context(&session);
    ctx.insert("truies".into(), Value::Array(rows));
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(bands).unwrap_or_default(),
    );
    ctx.insert("capacite".into(), json!(capacite));
    ctx.insert("occupation".into(), json!(occupation));
    ctx.insert("places_disponibles".into(), json!(places_disponibles));
    ctx.insert("attente".into(), json!(attente));
    ctx.insert("saturee".into(), json!(occupation >= capacite));
    ctx.insert(
        "depasse_capacite".into(),
        json!(attente > places_disponibles),
    );
    render(&state, "attente.html", Value::Object(ctx))
}

pub(super) async fn cause_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let label = form_text(&form, "libelle")
        .ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    sqlx::query("INSERT INTO causeperte(libelle) SELECT ? WHERE NOT EXISTS(SELECT 1 FROM causeperte WHERE lower(libelle)=lower(?))").bind(&label).bind(&label).execute(&state.pool).await?;
    Ok(Redirect::to("/parametres#causes").into_response())
}
pub(super) async fn cause_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM causeperte WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/parametres#causes").into_response())
}

pub(super) async fn bande_engraisseur(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE bande SET engraisseur_id=?,poids_cible=?,instructions=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(form_i64(&form,"engraisseur_id")).bind(form_f64(&form,"poids_cible")).bind(form_text(&form,"instructions")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/bande/{id}")).into_response())
}

pub(super) async fn bande_inventaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number = form_i64(&form, "nombre")
        .filter(|v| *v >= 0)
        .ok_or_else(|| AppError::Invalid("Effectif invalide".into()))?;
    let date =
        form_date(&form, "date")?.ok_or_else(|| AppError::Invalid("Date obligatoire".into()))?;
    let code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,poids,libelle,type_saisie,est_stock) VALUES(?,?,?,?,?,'inventaire',1)").bind(date).bind(code).bind(number).bind(form_f64(&form,"poids")).bind(form_text(&form,"note").unwrap_or_else(||"Inventaire physique".into())).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/bande/{id}")).into_response())
}

pub(super) async fn mortalite_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((id, declaration_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query(
        "DELETE FROM declarationmort WHERE id=? AND bande_code=(SELECT code FROM bande WHERE id=?)",
    )
    .bind(declaration_id)
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to(&format!("/bande/{id}#mortalite")).into_response())
}

pub(super) async fn bande_transfert_porcs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(mut form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    form.insert("source".into(), format!("bande:{id}"));
    super::transferts_porcs(State(state), Extension(session), Form(form)).await
}

fn selected_bands(form: &HashMap<String, String>) -> Option<String> {
    let mut ids = form_selected_ids(form, "bande_");
    if let Some(raw) = form_text(form, "bandes") {
        ids.extend(raw.split(',').filter_map(|v| v.trim().parse::<i64>().ok()));
    }
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        None
    } else {
        Some(
            ids.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

macro_rules! economy_simple_update {
    ($name:ident,$table:literal,$column:literal,$parser:expr,$redirect:literal) => {
        pub(super) async fn $name(
            State(state): State<AppState>,
            Extension(session): Extension<SessionData>,
            Path(id): Path<i64>,
            Form(form): Form<HashMap<String, String>>,
        ) -> AppResult<Response> {
            require_writer(&session)?;
            verify_csrf(&session, &form)?;
            let value = $parser(&form);
            let sql = format!("UPDATE {} SET {}=? WHERE id=?", $table, $column);
            sqlx::query(&sql)
                .bind(value)
                .bind(id)
                .execute(&state.pool)
                .await?;
            Ok(Redirect::to($redirect).into_response())
        }
    };
}
economy_simple_update!(
    economique_aliment_site,
    "livraisonaliment",
    "site",
    |f: &HashMap<String, String>| form_text(f, "site"),
    "/economique"
);
economy_simple_update!(
    economique_veto_site,
    "achatveto",
    "site",
    |f: &HashMap<String, String>| form_text(f, "site"),
    "/economique"
);
economy_simple_update!(
    economique_genetique_bande,
    "achatgenetique",
    "bande_code",
    |f: &HashMap<String, String>| form_text(f, "bande_code"),
    "/economique"
);
economy_simple_update!(
    economique_semence_bande,
    "achatsemence",
    "bande_id",
    |f: &HashMap<String, String>| form_i64(f, "bande_id"),
    "/economique"
);
economy_simple_update!(
    economique_vente_bande,
    "venteapport",
    "bande_id",
    |f: &HashMap<String, String>| form_i64(f, "bande_id"),
    "/economique"
);
economy_simple_update!(
    economique_veto_bande,
    "achatveto",
    "bande_id",
    |f: &HashMap<String, String>| form_i64(f, "bande_id"),
    "/economique"
);

pub(super) async fn economique_aliment_bandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE livraisonaliment SET bandes=?,bande_id=NULL WHERE id=?")
        .bind(selected_bands(&form))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique").into_response())
}
pub(super) async fn economique_veto_bandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE achatveto SET bandes=?,bande_id=NULL WHERE id=?")
        .bind(selected_bands(&form))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique").into_response())
}
pub(super) async fn economique_semence_montant(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let amount = form_f64(&form, "montant_ht")
        .filter(|v| *v >= 0.0)
        .ok_or_else(|| AppError::Invalid("Montant invalide".into()))?;
    sqlx::query("UPDATE achatsemence SET montant_ht=?,montant_ttc=? WHERE id=?")
        .bind(amount)
        .bind(form_f64(&form, "montant_ttc"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique").into_response())
}

pub(super) async fn economique_vente_lot_bande(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((id, index)): Path<(i64, usize)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let raw: Option<String> = sqlx::query_scalar("SELECT lots_json FROM venteapport WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .flatten();
    let mut lots: JsonValue = raw
        .as_deref()
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_else(|| json!([]));
    let array = lots
        .as_array_mut()
        .ok_or_else(|| AppError::Invalid("Lots invalides".into()))?;
    let lot = array.get_mut(index).ok_or(AppError::NotFound)?;
    lot["bande_id"] = form_i64(&form, "bande_id")
        .map(JsonValue::from)
        .unwrap_or(JsonValue::Null);
    sqlx::query("UPDATE venteapport SET lots_json=? WHERE id=?")
        .bind(serde_json::to_string(&lots).map_err(|e| AppError::Internal(e.into()))?)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique").into_response())
}

async fn auto_link_economy(pool: &SqlitePool, include_sales: bool) -> AppResult<u64> {
    let mut changed = 0;
    for (table, min_day, max_day, center) in [
        ("livraisonaliment", -175_i64, 210_i64, 70_i64),
        ("achatveto", -115, 210, 55),
        ("achatsemence", -175, 30, -60),
    ] {
        let sql=format!("UPDATE {table} AS x SET bande_id=(SELECT b.id FROM bande b WHERE b.date_mb IS NOT NULL AND x.date IS NOT NULL AND julianday(x.date)-julianday(b.date_mb) BETWEEN ? AND ? AND (x.site IS NULL OR trim(x.site)='' OR b.site=x.site) ORDER BY ABS((julianday(x.date)-julianday(b.date_mb))-?) LIMIT 1) WHERE x.bande_id IS NULL AND x.date IS NOT NULL");
        changed += sqlx::query(&sql)
            .bind(min_day)
            .bind(max_day)
            .bind(center)
            .execute(pool)
            .await?
            .rows_affected();
    }
    if include_sales {
        changed+=sqlx::query("UPDATE venteapport AS v SET bande_id=(SELECT b.id FROM bande b WHERE b.date_mb IS NOT NULL AND v.date IS NOT NULL AND julianday(v.date)-julianday(b.date_mb) BETWEEN 150 AND 225 ORDER BY ABS((julianday(v.date)-julianday(b.date_mb))-185) LIMIT 1) WHERE v.bande_id IS NULL AND v.date IS NOT NULL").execute(pool).await?.rows_affected();
    }
    Ok(changed)
}
pub(super) async fn economique_auto_lier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let changed = auto_link_economy(&state.pool, true).await?;
    Ok(Redirect::to(&format!("/economique?liaisons={changed}")).into_response())
}
pub(super) async fn economique_rattacher_auto(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let changed = auto_link_economy(&state.pool, false).await?;
    Ok(Redirect::to(&format!("/economique?liaisons={changed}")).into_response())
}

fn ascii_pdf(lines: &[String]) -> Vec<u8> {
    fn clean(value: &str) -> String {
        value
            .chars()
            .map(|c| match c {
                '(' | '\\' | ')' => ' ',
                c if c.is_ascii() => c,
                _ => ' ',
            })
            .collect()
    }
    let mut content = String::from("BT /F1 11 Tf 50 790 Td 14 TL ");
    for line in lines.iter().take(52) {
        content.push_str(&format!("({}) Tj T* ", clean(line)));
    }
    content.push_str("ET");
    let objects=[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream",content.len(),content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, obj).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn pdf_response(filename: &str, lines: &[String]) -> AppResult<Response> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|e| AppError::Internal(e.into()))?,
    );
    Ok((headers, ascii_pdf(lines)).into_response())
}

pub(super) async fn export_mise_bas_pdf(
    State(state): State<AppState>,
    Extension(_session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let band: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT code,date_mb,site FROM bande WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((code, date, site)) = band else {
        return Err(AppError::NotFound);
    };
    let rows=generic_rows(&state.pool,&format!("SELECT t.num_travail,e.date,e.nes_vifs,e.mort_nes,e.momifies,e.adoptes,e.retires,e.nb_sevres FROM evenement e JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={} AND e.type IN ('mise_bas','sevrage') ORDER BY t.num_travail,e.date",id)).await?;
    let mut lines = vec![
        "EO-Suivi - Registre des mises-bas".into(),
        format!(
            "Bande: {code}   Date: {}   Site: {}",
            date.unwrap_or_else(|| "Date inconnue".into()),
            site.unwrap_or_default()
        ),
        "Truie | Date | NV | MN | Momifies | Adoptes | Retires | Sevres".into(),
    ];
    for row in rows {
        lines.push(format!(
            "{} | {} | {} | {} | {} | {} | {} | {}",
            row["num_travail"],
            row["date"],
            row["nes_vifs"],
            row["mort_nes"],
            row["momifies"],
            row["adoptes"],
            row["retires"],
            row["nb_sevres"]
        ));
    }
    pdf_response(&format!("mises-bas-{code}.pdf"), &lines)
}

pub(super) async fn export_registre_pdf(
    State(state): State<AppState>,
    Extension(_session): Extension<SessionData>,
) -> AppResult<Response> {
    let rows=generic_rows(&state.pool,"SELECT date,type,t.num_travail,b.code AS bande,produit,motif,resultat,note FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id LEFT JOIN bande b ON b.id=e.bande_id ORDER BY date DESC,e.id DESC LIMIT 500").await?;
    let mut lines = vec![
        "EO-Suivi - Registre d'elevage".into(),
        format!("Edition du {}", today_iso()),
        "Date | Type | Animal | Bande | Produit | Motif | Resultat".into(),
    ];
    for row in rows {
        lines.push(format!(
            "{} | {} | {} | {} | {} | {} | {}",
            row["date"],
            row["type"],
            row["num_travail"],
            row["bande"],
            row["produit"],
            row["motif"],
            row["resultat"]
        ));
    }
    pdf_response(&format!("registre-elevage-{}.pdf", today_iso()), &lines)
}

pub(super) async fn journal_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM journal WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/journal").into_response())
}

fn data_parent(state: &AppState) -> PathBuf {
    state
        .config
        .db_path
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .to_path_buf()
}

pub(super) async fn logo(State(state): State<AppState>) -> Response {
    for name in ["logo.png", "logo.jpg", "logo.svg"] {
        let path = data_parent(&state).join(name);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            let content = if name.ends_with("png") {
                "image/png"
            } else if name.ends_with("jpg") {
                "image/jpeg"
            } else {
                "image/svg+xml"
            };
            return ([(header::CONTENT_TYPE, content)], bytes).into_response();
        }
    }
    (StatusCode::NOT_FOUND, "Logo absent").into_response()
}

pub(super) async fn maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_admin(&session)?;
    let marker = data_parent(&state).join("mise-a-jour-en-attente.zip");
    let pending = tokio::fs::metadata(marker).await.ok().map(|m| m.len());
    let mut ctx = context(&session);
    ctx.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
    ctx.insert("archive_octets".into(), json!(pending));
    render(&state, "maj.html", Value::Object(ctx))
}

pub(super) async fn maj_lancer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    db::journal(
        &state.pool,
        &session.nom,
        "demander",
        "mise_a_jour",
        "Exécution à confirmer sur le serveur",
        "/maj/lancer",
    )
    .await;
    Ok(Redirect::to("/maj?demande=1").into_response())
}

async fn multipart_fields(
    mut multipart: Multipart,
    file_name: &str,
) -> AppResult<(HashMap<String, String>, Option<Vec<u8>>, Option<String>)> {
    let mut form = HashMap::new();
    let mut file = None;
    let mut uploaded_name = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Invalid(format!("Formulaire invalide: {e}")))?
    {
        let name = field.name().unwrap_or_default().to_string();
        let original = field.file_name().map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::Invalid(format!("Téléversement interrompu: {e}")))?;
        if name == file_name {
            uploaded_name = original;
            file = Some(bytes.to_vec());
        } else if let Ok(value) = String::from_utf8(bytes.to_vec()) {
            form.insert(name, value);
        }
    }
    Ok((form, file, uploaded_name))
}

pub(super) async fn maj_zip(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_admin(&session)?;
    let (form, file, _) = multipart_fields(multipart, "fichier").await?;
    verify_csrf(&session, &form)?;
    let bytes = file.ok_or_else(|| AppError::Invalid("Archive ZIP obligatoire".into()))?;
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(AppError::Invalid(
            "Le fichier n'est pas une archive ZIP valide".into(),
        ));
    }
    let path = data_parent(&state).join("mise-a-jour-en-attente.zip");
    tokio::fs::write(&path, bytes)
        .await
        .map_err(anyhow::Error::from)?;
    db::journal(
        &state.pool,
        &session.nom,
        "deposer",
        "mise_a_jour",
        &path.display().to_string(),
        "/maj/zip",
    )
    .await;
    Ok(Redirect::to("/maj?archive=1").into_response())
}

pub(super) async fn aliment_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    let category = form_text(&form, "categorie")
        .ok_or_else(|| AppError::Invalid("Catégorie obligatoire".into()))?;
    sqlx::query("INSERT INTO planaliment(categorie,jour_debut,jour_fin,aliment,quantite,unite,note,ordre) VALUES(?,?,?,?,?,?,?,?)").bind(category).bind(form_i64(&form,"jour_debut").unwrap_or(0)).bind(form_i64(&form,"jour_fin").unwrap_or(0)).bind(form_text(&form,"aliment")).bind(form_f64(&form,"quantite")).bind(form_text(&form,"unite").unwrap_or_else(||"kg/j".into())).bind(form_text(&form,"note")).bind(form_i64(&form,"ordre").unwrap_or(0)).execute(&state.pool).await?;
    Ok(Redirect::to("/parametres#alimentation").into_response())
}
pub(super) async fn aliment_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE planaliment SET categorie=?,jour_debut=?,jour_fin=?,aliment=?,quantite=?,unite=?,note=?,ordre=? WHERE id=?").bind(form_text(&form,"categorie")).bind(form_i64(&form,"jour_debut").unwrap_or(0)).bind(form_i64(&form,"jour_fin").unwrap_or(0)).bind(form_text(&form,"aliment")).bind(form_f64(&form,"quantite")).bind(form_text(&form,"unite")).bind(form_text(&form,"note")).bind(form_i64(&form,"ordre").unwrap_or(0)).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/parametres#alimentation").into_response())
}
pub(super) async fn aliment_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM planaliment WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/parametres#alimentation").into_response())
}

pub(super) async fn reglages_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    let mut tx = state.pool.begin().await?;
    for (key, value) in &form {
        if key == "csrf" {
            continue;
        }
        if let Ok(value) = value.trim().parse::<i64>() {
            if value >= 0 {
                sqlx::query("UPDATE reglage SET valeur=? WHERE cle=?")
                    .bind(value)
                    .bind(key)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(Redirect::to("/parametres#reglages").into_response())
}

pub(super) async fn parametres_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    let allowed = [
        "nom_elevage",
        "adresse_elevage",
        "telephone_elevage",
        "email_elevage",
        "public_url",
        "echo_j",
        "seuil_stock",
        "nas_path",
        "cloud_path",
        "mail_sauvegarde",
    ];
    let mut tx = state.pool.begin().await?;
    for key in allowed {
        if let Some(value) = form.get(key) {
            sqlx::query("INSERT INTO parametre(cle,valeur) VALUES(?,?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(key).bind(value.trim()).execute(&mut *tx).await?;
        }
    }
    // Type d'élevage : seule valeur reconnue parmi les 5 profils (voir 0bis de la
    // spécification) est acceptée ; une soumission corrompue est ignorée en silence.
    let new_type_elevage = form
        .get("type_elevage")
        .map(|v| v.trim().to_string())
        .filter(|v| auth::TYPES_ELEVAGE.iter().any(|(code, _)| code == v));
    if let Some(value) = &new_type_elevage {
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('type_elevage',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(value).execute(&mut *tx).await?;
    }
    // Modules optionnels (§0/§2/§4) : ces cases partagent le même formulaire
    // que le type d'élevage (marqueur : le champ type_elevage, un <select>,
    // est toujours présent quand CE formulaire est soumis). Sans ce garde-fou,
    // soumettre un des *autres* formulaires de cette page (qui postent aussi
    // vers /parametres/maj mais n'ont pas de cases à cocher module_*)
    // désactiverait silencieusement les modules à chaque enregistrement.
    let formulaire_type_elevage = form.contains_key("type_elevage");
    if formulaire_type_elevage {
        let module_genetique = form.contains_key("module_genetique");
        let module_prestataires = form.contains_key("module_prestataires");
        let module_charcutiers_rfid = form.contains_key("module_charcutiers_rfid");
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('module_genetique',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(if module_genetique{"1"}else{"0"}).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('module_prestataires',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(if module_prestataires{"1"}else{"0"}).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('module_charcutiers_rfid',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(if module_charcutiers_rfid{"1"}else{"0"}).execute(&mut *tx).await?;
        tx.commit().await?;
        // Les sessions déjà ouvertes voient les nouveaux réglages sans avoir à
        // se reconnecter, pour éviter un affichage incohérent le temps que
        // chacun se reconnecte.
        for mut entry in state.sessions.iter_mut() {
            if let Some(value) = &new_type_elevage {
                entry.value_mut().type_elevage = value.clone();
            }
            entry.value_mut().module_genetique = module_genetique;
            entry.value_mut().module_prestataires = module_prestataires;
            entry.value_mut().module_charcutiers_rfid = module_charcutiers_rfid;
        }
    } else {
        tx.commit().await?;
    }
    Ok(Redirect::to("/parametres").into_response())
}

pub(super) async fn demo_actif(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Response> {
    require_admin(&session)?;
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet")
        .fetch_one(&state.pool)
        .await?;
    Ok(axum::Json(json!({"actif":active>0})).into_response())
}

pub(super) async fn demo_basculer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet")
        .fetch_one(&state.pool)
        .await?;
    let mut tx = state.pool.begin().await?;
    if active > 0 {
        // Ordre enfants → parents pour respecter les clés étrangères
        // (foreign_keys=ON) : evenement référence truie et bande,
        // truie/bande référencent site/utilisateur (via engraisseur_id).
        for table in [
            "evenement",
            "porccharcutier",
            "truie",
            "bande",
            "utilisateur",
            "site",
        ] {
            let sql = format!(
                "DELETE FROM {table} WHERE id IN(SELECT row_id FROM demoobjet WHERE table_name=?)"
            );
            sqlx::query(&sql).bind(table).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM demoobjet")
            .execute(&mut *tx)
            .await?;
    } else {
        crate::demo::activer(&mut tx)
            .await
            .map_err(AppError::Internal)?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/parametres#demo").into_response())
}

pub(super) async fn qr_truie(
    State(state): State<AppState>,
    Extension(_session): Extension<SessionData>,
    Path(file): Path<String>,
) -> AppResult<Response> {
    let id = file
        .strip_suffix(".png")
        .unwrap_or(&file)
        .parse::<i64>()
        .map_err(|_| AppError::NotFound)?;
    let number: String = sqlx::query_scalar("SELECT num_travail FROM truie WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let code = qrcode::QrCode::new(format!("/truie/{id}?numero={number}").as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let image = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .build();
    Ok((
        [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
        image,
    )
        .into_response())
}

pub(super) async fn salle_lavage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "date")?;
    sqlx::query("UPDATE salle SET dernier_lavage=? WHERE id=?")
        .bind(date)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

pub(super) async fn sanitaire_generer_protocole(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let result=sqlx::query("INSERT INTO acteprotocole(libelle,cible,reference,jour,produit,note,actif,categorie) SELECT 'Traitement - '||trim(e.produit),'porcs','traitement',0,trim(e.produit),'Généré depuis les traitements enregistrés',1,'traitement' FROM evenement e WHERE e.type='traitement' AND trim(COALESCE(e.produit,''))<>'' AND lower(COALESCE(e.motif,'')) NOT LIKE '%vaccin%' AND lower(COALESCE(e.produit,'')) NOT LIKE '%vaccin%' AND NOT EXISTS(SELECT 1 FROM acteprotocole a WHERE a.actif=1 AND lower(trim(a.produit))=lower(trim(e.produit))) GROUP BY lower(trim(e.produit))").execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/sanitaire?generes={}", result.rows_affected())).into_response())
}

pub(super) async fn stock_doses(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let product = form_text(&form, "produit")
        .ok_or_else(|| AppError::Invalid("Produit obligatoire".into()))?;
    let doses = form_i64(&form, "doses_unite")
        .filter(|v| *v > 0)
        .ok_or_else(|| AppError::Invalid("Nombre de doses invalide".into()))?;
    let token = product
        .split_whitespace()
        .next()
        .unwrap_or(&product)
        .to_lowercase();
    sqlx::query("UPDATE achatveto SET doses_unite=? WHERE lower(produit) LIKE ?")
        .bind(doses)
        .bind(format!("{token}%"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/stock").into_response())
}

pub(super) async fn saisie_rapide(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let ajax_mode = form.get("ajax").map(|s| s == "1").unwrap_or(false);
    let kind = form_text(&form, "type")
        .ok_or_else(|| AppError::Invalid("Type de saisie obligatoire".into()))?;
    let date = form_date_or_today(&form, "date")?;
    // track updated resources for ajax response
    let mut updated_band_id: Option<i64> = None;
    let mut updated_cases: Vec<i64> = Vec::new();
    match kind.as_str() {
        "perte" => {
            let band_id = form_i64(&form, "bande_id")
                .ok_or_else(|| AppError::Invalid("Bande obligatoire".into()))?;
            let band: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
                .bind(band_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or(AppError::NotFound)?;
            let number = form_i64(&form, "nombre").unwrap_or(1).max(1);
            let case_id = form_i64(&form, "case_id");
            let mut stade = form_text(&form, "stade");
            if let Some(case_id) = case_id {
                let present = case_pig_count(&state.pool, case_id).await?;
                if number > present {
                    return Err(AppError::Invalid(format!(
                        "Effectif insuffisant dans la case : {present} porc(s) présent(s)"
                    )));
                }
                if let Some(deduced) = stade_from_case(&state.pool, case_id).await? {
                    stade = Some(deduced);
                }
            }
            sqlx::query("INSERT INTO declarationmort(bande_code,date,stade,case_id,cause,poids,nombre,declare_par,note) VALUES(?,?,?,?,?,?,?,?,?)").bind(band).bind(&date).bind(stade).bind(case_id).bind(form_text(&form,"cause")).bind(form_f64(&form,"poids")).bind(number).bind(&session.nom).bind(form_text(&form,"note")).execute(&state.pool).await?;
            if let Some(sow_id) = form_i64(&form, "truie_id") {
                sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,age_j,nb,cause,date) VALUES(?,?,?,?,?,?)").bind(sow_id).bind(band_id).bind(form_i64(&form,"age_j")).bind(number).bind(form_text(&form,"cause")).bind(&date).execute(&state.pool).await?;
            }
        }
        "sortie" => {
            let sow_id = form_i64(&form, "truie_id")
                .ok_or_else(|| AppError::Invalid("Truie obligatoire".into()))?;
            sqlx::query("UPDATE truie SET reformee=1,statut='sortie',date_reforme=?,motif_sortie=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(date).bind(form_text(&form,"motif")).bind(sow_id).execute(&state.pool).await?;
        }
        "eld" => {
            let sow_id = form_i64(&form, "truie_id")
                .ok_or_else(|| AppError::Invalid("Truie obligatoire".into()))?;
            let eld = form_f64(&form, "eld").filter(|v| *v >= 0.0);
            let poids = form_f64(&form, "poids").filter(|v| *v >= 0.0);
            let nec = form_f64(&form, "nec").filter(|v| (1.0..=5.0).contains(v));
            if eld.is_none() && poids.is_none() && nec.is_none() {
                return Err(AppError::Invalid(
                    "Saisissez au moins l’ELD, le poids ou la NEC".into(),
                ));
            }
            sqlx::query("INSERT INTO mesuretruie(truie_id,date,periode,eld,poids,nec,note) VALUES(?,?,?,?,?,?,?)").bind(sow_id).bind(date).bind(form_text(&form,"periode")).bind(eld).bind(poids).bind(nec).bind(form_text(&form,"note")).execute(&state.pool).await?;
        }
        "mise_bas" => {
            let sow_id = form_i64(&form, "truie_id")
                .ok_or_else(|| AppError::Invalid("Truie obligatoire".into()))?;
            let band_id = sow_band(&state.pool, sow_id).await?;
            let live = form_i64(&form, "nes_vifs").unwrap_or(0).max(0);
            let still = form_i64(&form, "mort_nes").unwrap_or(0).max(0);
            let mummies = form_i64(&form, "momifies").unwrap_or(0).max(0);
            let weak = form_i64(&form, "chetifs").unwrap_or(0).max(0);
            let crushed = form_i64(&form, "ecrases").unwrap_or(0).max(0);
            let killed = form_i64(&form, "tues_truie").unwrap_or(0).max(0);
            let total = live + still + mummies;
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM evenement WHERE truie_id=? AND type='mise_bas' AND bande_id IS ? ORDER BY id DESC LIMIT 1",
            )
            .bind(sow_id)
            .bind(band_id)
            .fetch_optional(&state.pool)
            .await?;
            let mut tx = state.pool.begin().await?;
            if let Some(event_id) = existing {
                sqlx::query("UPDATE evenement SET date=?,nes_totaux=?,nes_vifs=?,mort_nes=?,momifies=?,chetifs=?,ecrases=?,tues_truie=?,heure_debut=?,heure_fin=?,suivi_actif=?,delivrance_ok=?,note=? WHERE id=?")
                    .bind(&date).bind(total).bind(live).bind(still).bind(mummies).bind(weak).bind(crushed).bind(killed)
                    .bind(form_text(&form,"heure_debut")).bind(form_text(&form,"heure_fin")).bind(form.contains_key("suivi_actif") as i64).bind(form_i64(&form,"delivrance_ok")).bind(form_text(&form,"note"))
                    .bind(event_id).execute(&mut *tx).await?;
            } else {
                sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,chetifs,ecrases,tues_truie,heure_debut,heure_fin,suivi_actif,delivrance_ok,note) VALUES('mise_bas',?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(&date).bind(sow_id).bind(band_id).bind(total).bind(live).bind(still).bind(mummies).bind(weak).bind(crushed).bind(killed)
                    .bind(form_text(&form,"heure_debut")).bind(form_text(&form,"heure_fin")).bind(form.contains_key("suivi_actif") as i64).bind(form_i64(&form,"delivrance_ok")).bind(form_text(&form,"note"))
                    .execute(&mut *tx).await?;
                if parse_stored_date(&date).is_some_and(|day| day <= Local::now().date_naive()) {
                    sqlx::query(
                        "UPDATE truie SET rang=rang+1,updated_at=CURRENT_TIMESTAMP WHERE id=?",
                    )
                    .bind(sow_id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            if let Some(band_id) = band_id {
                sqlx::query("UPDATE truie SET bande_code=(SELECT code FROM bande WHERE id=?),perf_nt=?,perf_nv=?,perf_mn=?,perf_mo=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                    .bind(band_id).bind(total as f64).bind(live as f64).bind(still as f64).bind(mummies as f64).bind(sow_id)
                    .execute(&mut *tx).await?;
                updated_band_id = Some(band_id);
            }
            tx.commit().await?;
        }
        "ia" | "echo" | "chaleur" => {
            let sow_id = form_i64(&form, "truie_id")
                .ok_or_else(|| AppError::Invalid("Truie obligatoire".into()))?;
            let band_id = sow_band(&state.pool, sow_id).await?;
            sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,resultat,produit,nb_doses,note) VALUES(?,?,?,?,?,?,?,?)").bind(&kind).bind(date).bind(sow_id).bind(band_id).bind(form_text(&form,"resultat")).bind(form_text(&form,"produit")).bind(form_i64(&form,"nb_doses")).bind(form_text(&form,"note")).execute(&state.pool).await?;
        }
        "sevrage" => {
            // traitement batch de sevrage depuis la saisie rapide
            let band_id = form_i64(&form, "bande_id")
                .ok_or_else(|| AppError::Invalid("Bande obligatoire".into()))?;
            let band_code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
                .bind(band_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or(AppError::NotFound)?;
            let _mode = form
                .get("sevrage_mode")
                .map(|s| s.as_str())
                .unwrap_or("toute");
            let sel = form
                .get("sevrage_truies")
                .ok_or_else(|| AppError::Invalid("Truies manquantes".into()))?;
            let distrib = form
                .get("sevrage_distribution")
                .ok_or_else(|| AppError::Invalid("Répartition manquante".into()))?;
            let selected: Vec<serde_json::Value> = serde_json::from_str(sel)
                .map_err(|_| AppError::Invalid("Données truies invalides".into()))?;
            let dist: Vec<serde_json::Value> = serde_json::from_str(distrib)
                .map_err(|_| AppError::Invalid("Données répartition invalides".into()))?;
            // calculs
            let mut total_selected: i64 = 0;
            let mut sows: Vec<(i64, i64)> = Vec::new();
            for item in selected.iter() {
                if let Some(id) = item.get("id").and_then(|v| v.as_i64()) {
                    let nb = item.get("nb").and_then(|v| v.as_i64()).unwrap_or(0);
                    sows.push((id, nb));
                    total_selected += nb;
                }
            }
            let mut total_dist: i64 = 0;
            let mut dlines: Vec<(i64, i64)> = Vec::new();
            for item in dist.iter() {
                if let Some(case_id) = item.get("case_id").and_then(|v| v.as_i64()) {
                    let nb = item.get("nombre").and_then(|v| v.as_i64()).unwrap_or(0);
                    dlines.push((case_id, nb));
                    total_dist += nb;
                }
            }
            if total_dist != total_selected {
                return Err(AppError::Invalid(
                    "La somme des porcelets répartis doit correspondre au total sélectionné".into(),
                ));
            }
            let mut tx = state.pool.begin().await?;
            // pour chaque truie, enregistrer evenement sevrage (même si nb==0)
            // avant d'enregistrer, vérifier qu'aucune truie sélectionnée n'a déjà un sevrage à la même date ou après
            for (sow_id, nb) in sows.iter() {
                let last_sev: Option<String> = sqlx::query_scalar("SELECT date FROM evenement WHERE truie_id=? AND type='sevrage' ORDER BY date DESC,id DESC LIMIT 1").bind(sow_id).fetch_optional(&state.pool).await?;
                if let (Some(last), Some(given)) = (last_sev.as_deref(), parse_stored_date(&date)) {
                    if let Some(ld) = parse_stored_date(last) {
                        if ld >= given {
                            return Err(AppError::Invalid(format!(
                                "Truie {} déjà sevrée le {}",
                                sow_id, last
                            )));
                        }
                    }
                }
                let existing:Option<i64> = sqlx::query_scalar("SELECT id FROM evenement WHERE truie_id=? AND type='sevrage' AND bande_id IS ? ORDER BY id DESC LIMIT 1").bind(sow_id).bind(band_id).fetch_optional(&state.pool).await?;
                if let Some(event_id) = existing {
                    sqlx::query("UPDATE evenement SET date=?,nb_sevres=?,note=? WHERE id=?")
                        .bind(&date)
                        .bind(*nb)
                        .bind(form_text(&form, "note"))
                        .bind(event_id)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,note) VALUES('sevrage',?,?,?,?,?)").bind(&date).bind(sow_id).bind(band_id).bind(*nb).bind(form_text(&form,"note")).execute(&mut *tx).await?;
                }
                // mettre à jour la truie (perf_sevres + potentiellement nettoyage bande_code)
                sqlx::query(
                    "UPDATE truie SET perf_sevres=?,updated_at=CURRENT_TIMESTAMP WHERE id=?",
                )
                .bind(*nb as f64)
                .bind(sow_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE truie SET bande_code=CASE WHEN date(?)<=date('now') AND bande_code=(SELECT code FROM bande WHERE id=?) THEN NULL ELSE bande_code END,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(&date).bind(band_id).bind(sow_id).execute(&mut *tx).await?;
            }
            // Vérification de capacité : pour chaque case destination, s'assurer que la somme actuelle + nb ne dépasse pas nb_max_porcs si défini
            for (case_id, nb) in dlines.iter() {
                if *nb <= 0 {
                    continue;
                }
                // récupérer capacité
                let cap_opt: Option<i64> =
                    sqlx::query_scalar("SELECT nb_max_porcs FROM casesalle WHERE id=?")
                        .bind(case_id)
                        .fetch_optional(&mut *tx)
                        .await?;
                if let Some(cap) = cap_opt {
                    // calculer l'occupation approximative actuelle
                    let dest_str = format!("case:{}", case_id);
                    let occupancy: i64 = sqlx::query_scalar(
                        "SELECT COALESCE((SELECT SUM(COALESCE(nombre,0)) FROM mouvementstock WHERE destination=? AND est_stock=1),0) + COALESCE((SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM transfert WHERE case_dest_id=?),0) + COALESCE((SELECT nombre FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1),0)"
                    ).bind(&dest_str).bind(case_id).bind(case_id).fetch_one(&mut *tx).await?;
                    if occupancy + *nb > cap {
                        return Err(AppError::Invalid(format!(
                            "La case {} ne peut pas accueillir {} porcelet(s) (capacité {}, occupés {}).",
                            case_id, nb, cap, occupancy
                        )));
                    }
                }
            }
            // insert mouvementstock pour chaque destination
            for (case_id, nb) in dlines.iter() {
                if *nb > 0 {
                    sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,libelle,destination,type_saisie,est_stock) VALUES(?,?,?,?,?,'sevrage',0)")
                        .bind(&date).bind(&band_code).bind(*nb).bind("Sevrage").bind(format!("case:{}",case_id)).execute(&mut *tx).await?;
                }
            }
            // maj bande.cs_total_sevres
            if total_selected > 0 {
                sqlx::query("INSERT INTO bande(id,cs_total_sevres) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET cs_total_sevres=COALESCE(bande.cs_total_sevres,0)+excluded.cs_total_sevres")
                    .bind(band_id).bind(total_selected).execute(&mut *tx).await?;
            }
            tx.commit().await?;
            // remember updated band and cases for ajax response
            updated_band_id = Some(band_id);
            for (case_id, _nb) in dlines.iter() {
                updated_cases.push(*case_id);
            }
        }
        _ => return Err(AppError::Invalid("Type de saisie non reconnu".into())),
    }
    if ajax_mode {
        // return a JSON summary for client-side refresh
        let resp = json!({"ok": true, "band_id": updated_band_id, "cases": updated_cases});
        return Ok(axum::Json(resp).into_response());
    }
    Ok(Redirect::to("/?saisie=ok").into_response())
}

pub(super) async fn sauvegarde_restaurer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_admin(&session)?;
    let (form, file, _) = multipart_fields(multipart, "fichier").await?;
    verify_csrf(&session, &form)?;
    if form.get("confirmation").map(String::as_str) != Some("RESTAURER") {
        return Err(AppError::Invalid(
            "Tapez RESTAURER pour confirmer le remplacement de la base".into(),
        ));
    }
    let bytes = file.ok_or_else(|| AppError::Invalid("Sauvegarde SQLite obligatoire".into()))?;
    if !bytes.starts_with(b"SQLite format 3\0") {
        return Err(AppError::Invalid(
            "Le fichier n'est pas une base SQLite".into(),
        ));
    }
    let parent = data_parent(&state);
    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let candidate = parent.join(format!("restauration-validee-{timestamp}.db"));
    tokio::fs::write(&candidate, &bytes)
        .await
        .map_err(anyhow::Error::from)?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&candidate)
        .read_only(true);
    let check_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&check_pool)
        .await?;
    let tables:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN('truie','bande','utilisateur')").fetch_one(&check_pool).await?;
    let foreign_key_errors = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&check_pool)
        .await?;
    check_pool.close().await;
    if check != "ok" || tables != 3 || !foreign_key_errors.is_empty() {
        let _ = tokio::fs::remove_file(&candidate).await;
        return Err(AppError::Invalid(
            "Sauvegarde incomplète, corrompue ou contenant des références invalides".into(),
        ));
    }
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&state.pool)
        .await?;
    let backup = parent.join(format!("avant-restauration-{timestamp}.db"));
    tokio::fs::copy(&state.config.db_path, &backup)
        .await
        .map_err(anyhow::Error::from)?;
    db::journal(
        &state.pool,
        &session.nom,
        "restaurer",
        "sauvegarde",
        &backup.display().to_string(),
        "/sauvegarde/restaurer",
    )
    .await;
    let pool = state.pool.clone();
    let live = state.config.db_path.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        pool.close().await;
        for suffix in ["-wal", "-shm"] {
            let old = PathBuf::from(format!("{}{suffix}", live.display()));
            if tokio::fs::metadata(&old).await.is_ok() {
                let archived = PathBuf::from(format!("{}.ancien{suffix}", live.display()));
                let _ = tokio::fs::rename(old, archived).await;
            }
        }
        if tokio::fs::rename(candidate, live).await.is_ok() {
            std::process::exit(1);
        }
    });
    Ok(Html(String::from("<!doctype html><meta charset='utf-8'><h1>Restauration validée</h1><p>La base a été contrôlée et le service redémarre. Rechargez la page dans quelques secondes.</p>")).into_response())
}

#[derive(Clone)]
struct CommSettings {
    api_key: String,
    sender_email: String,
    sender_name: String,
    sms_sender: String,
}
async fn comm_settings(pool: &SqlitePool) -> AppResult<CommSettings> {
    let row:Option<(Option<String>,Option<String>,String,String)>=sqlx::query_as("SELECT brevo_api_key,sender_email,sender_name,sms_sender FROM reglagecommunicationventedirecte WHERE id=1").fetch_optional(pool).await?;
    let Some((key, email, name, sms)) = row else {
        return Err(AppError::Invalid(
            "Configurez d'abord les communications Brevo".into(),
        ));
    };
    let api_key = key
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::Invalid("Clé API Brevo absente".into()))?;
    let sender_email = email
        .filter(|v| v.contains('@'))
        .ok_or_else(|| AppError::Invalid("Adresse expéditeur absente".into()))?;
    Ok(CommSettings {
        api_key,
        sender_email,
        sender_name: name,
        sms_sender: sms,
    })
}

async fn brevo_send(
    settings: &CommSettings,
    channel: &str,
    to: &str,
    name: &str,
    subject: &str,
    content: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let (url, payload) = if channel == "email" {
        (
            "https://api.brevo.com/v3/smtp/email",
            json!({"sender":{"name":settings.sender_name,"email":settings.sender_email},"to":[{"email":to,"name":name}],"subject":subject,"htmlContent":content}),
        )
    } else {
        (
            "https://api.brevo.com/v3/transactionalSMS/sms",
            json!({"sender":settings.sms_sender,"recipient":to,"content":content,"type":"transactional","tag":"eo-suivi"}),
        )
    };
    let response = client
        .post(url)
        .header("api-key", &settings.api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        Err(format!("HTTP {status}: {body}"))
    }
}

async fn log_message(
    pool: &SqlitePool,
    client_id: Option<i64>,
    channel: &str,
    kind: &str,
    to: &str,
    content: &str,
    result: &Result<String, String>,
) -> AppResult<()> {
    sqlx::query("INSERT INTO messageventedirecte(client_id,canal,type_message,destinataire,contenu,succes,detail) VALUES(?,?,?,?,?,?,?)").bind(client_id).bind(channel).bind(kind).bind(to).bind(content).bind(result.is_ok()).bind(match result{Ok(v)|Err(v)=>v}).execute(pool).await?;
    Ok(())
}

pub(super) async fn communications(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let settings=generic_rows(&state.pool,"SELECT id,CASE WHEN trim(COALESCE(brevo_api_key,''))<>'' THEN '••••••••' ELSE '' END AS api_configuree,sender_email,sender_name,sms_sender,email_list_id,sms_list_id FROM reglagecommunicationventedirecte WHERE id=1").await?;
    let clients=generic_rows(&state.pool,"SELECT id,nom,email,telephone,newsletter_email,newsletter_sms,cree_le FROM clientventedirecte ORDER BY nom").await?;
    let messages=generic_rows(&state.pool,"SELECT id,cree_le,canal,type_message,destinataire,succes,detail FROM messageventedirecte ORDER BY id DESC LIMIT 200").await?;
    let mut ctx = context(&session);
    ctx.insert(
        "reglages".into(),
        settings.into_iter().next().unwrap_or_else(|| json!({})),
    );
    ctx.insert("clients".into(), Value::Array(clients));
    ctx.insert("messages".into(), Value::Array(messages));
    render(
        &state,
        "vente_directe_communications.html",
        Value::Object(ctx),
    )
}

pub(super) async fn communications_reglages(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_admin(&session)?;
    verify_csrf(&session, &form)?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT brevo_api_key FROM reglagecommunicationventedirecte WHERE id=1")
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    let key = form_text(&form, "brevo_api_key").or(existing);
    sqlx::query("INSERT INTO reglagecommunicationventedirecte(id,brevo_api_key,sender_email,sender_name,sms_sender,email_list_id,sms_list_id) VALUES(1,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET brevo_api_key=excluded.brevo_api_key,sender_email=excluded.sender_email,sender_name=excluded.sender_name,sms_sender=excluded.sms_sender,email_list_id=excluded.email_list_id,sms_list_id=excluded.sms_list_id").bind(key).bind(form_text(&form,"sender_email")).bind(form_text(&form,"sender_name").unwrap_or_else(||"EI ORY EMMANUEL".into())).bind(form_text(&form,"sms_sender").unwrap_or_else(||"ORYEMMANUEL".into())).bind(form_i64(&form,"email_list_id")).bind(form_i64(&form,"sms_list_id")).execute(&state.pool).await?;
    Ok(Redirect::to("/vente-directe/communications").into_response())
}

async fn send_test(
    state: &AppState,
    session: &SessionData,
    form: &HashMap<String, String>,
    channel: &str,
) -> AppResult<Response> {
    require_admin(session)?;
    verify_csrf(session, form)?;
    let settings = comm_settings(&state.pool).await?;
    let to = form_text(form, "destinataire")
        .ok_or_else(|| AppError::Invalid("Destinataire obligatoire".into()))?;
    let content = form_text(form, "contenu").unwrap_or_else(|| "Test EO-Suivi".into());
    let subject = form_text(form, "sujet").unwrap_or_else(|| "Test EO-Suivi".into());
    let result = brevo_send(&settings, channel, &to, "Test", &subject, &content).await;
    log_message(&state.pool, None, channel, "test", &to, &content, &result).await?;
    if let Err(error) = result {
        return Err(AppError::Invalid(format!("Échec de l'envoi: {error}")));
    }
    Ok(Redirect::to("/vente-directe/communications?test=ok").into_response())
}
pub(super) async fn test_email(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    send_test(&state, &session, &form, "email").await
}
pub(super) async fn test_sms(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    send_test(&state, &session, &form, "sms").await
}

async fn newsletter(
    state: &AppState,
    session: &SessionData,
    form: &HashMap<String, String>,
    channel: &str,
) -> AppResult<Response> {
    require_admin(session)?;
    verify_csrf(session, form)?;
    let settings = comm_settings(&state.pool).await?;
    let subject = form_text(form, "sujet").unwrap_or_else(|| "Actualités de la ferme".into());
    let content = form_text(form, "contenu")
        .ok_or_else(|| AppError::Invalid("Message obligatoire".into()))?;
    let sql = if channel == "email" {
        "SELECT id,nom,email FROM clientventedirecte WHERE newsletter_email=1 AND trim(COALESCE(email,''))<>''"
    } else {
        "SELECT id,nom,telephone FROM clientventedirecte WHERE newsletter_sms=1 AND trim(COALESCE(telephone,''))<>''"
    };
    let clients: Vec<(i64, String, Option<String>)> =
        sqlx::query_as(sql).fetch_all(&state.pool).await?;
    let mut sent = 0;
    for (id, name, to) in clients {
        let Some(to) = to else { continue };
        let result = brevo_send(&settings, channel, &to, &name, &subject, &content).await;
        log_message(
            &state.pool,
            Some(id),
            channel,
            "newsletter",
            &to,
            &content,
            &result,
        )
        .await?;
        if result.is_ok() {
            sent += 1;
        }
    }
    Ok(Redirect::to(&format!("/vente-directe/communications?envoyes={sent}")).into_response())
}
pub(super) async fn newsletter_email(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    newsletter(&state, &session, &form, "email").await
}
pub(super) async fn newsletter_sms(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    newsletter(&state, &session, &form, "sms").await
}

pub(super) async fn client_consentements(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE clientventedirecte SET newsletter_email=?,newsletter_sms=? WHERE id=?")
        .bind(form.contains_key("newsletter_email"))
        .bind(form.contains_key("newsletter_sms"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe/communications").into_response())
}

pub(super) async fn desinscription(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    if token.trim().is_empty() {
        return Err(AppError::NotFound);
    }
    let result=sqlx::query("UPDATE clientventedirecte SET newsletter_email=0,newsletter_sms=0 WHERE token_desinscription=? AND token_desinscription<>''").bind(token).execute(&state.pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Html("<!doctype html><meta charset='utf-8'><main><h1>Désinscription confirmée</h1><p>Vous ne recevrez plus les communications commerciales.</p></main>".into()))
}

pub(super) async fn recalculer_stocks(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    db::journal(
        &state.pool,
        &session.nom,
        "verifier",
        "stocks_vente_directe",
        "Aucune double déduction: les inventaires restent la référence",
        "/vente-directe/recalculer-stocks",
    )
    .await;
    Ok(Redirect::to("/vente-directe?stocks=verifies").into_response())
}

pub(super) async fn vente_session_detail(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let session_row=generic_rows(&state.pool,&format!("SELECT s.id,s.nom,s.date_creation,s.date_livraison,s.nb_porcs,s.bande_reference,s.active,s.notes,ROUND(COALESCE(SUM(CASE WHEN c.statut<>'annulee' THEN c.total ELSE 0 END),0),2) AS chiffre_affaires FROM sessionventedirecte s LEFT JOIN commandeventedirecte c ON c.session_vente_id=s.id WHERE s.id={id} GROUP BY s.id")).await?.into_iter().next().ok_or(AppError::NotFound)?;
    let commands=generic_rows(&state.pool,&format!("SELECT id,cree_le,nom_client,telephone,email,statut,total FROM commandeventedirecte WHERE session_vente_id={id} ORDER BY id DESC")).await?;
    let unattached=generic_rows(&state.pool,"SELECT id,cree_le,nom_client,total FROM commandeventedirecte WHERE session_vente_id IS NULL ORDER BY id DESC LIMIT 100").await?;
    let charges=generic_rows(&state.pool,&format!("SELECT id,categorie,libelle,montant,note FROM chargeventedirecte WHERE session_vente_id={id} ORDER BY id DESC")).await?;
    let mut ctx = context(&session);
    ctx.insert("vente_session".into(), session_row);
    ctx.insert("commandes".into(), Value::Array(commands));
    ctx.insert("sans_session".into(), Value::Array(unattached));
    ctx.insert("charges".into(), Value::Array(charges));
    render(&state, "vente_session_detail.html", Value::Object(ctx))
}

pub(super) async fn vente_session_commande_rattacher(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((id, command_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessionventedirecte WHERE id=?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE commandeventedirecte SET session_vente_id=? WHERE id=?")
        .bind(id)
        .bind(command_id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/vente-directe/session/{id}")).into_response())
}
