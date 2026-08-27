use super::*;

pub(super) async fn selection_truie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let mut values = Vec::new();
    for key in ["nb_tetines", "splayleg"] {
        let value = match form_text(&form, key) {
            None => None,
            Some(s) => Some(s.parse::<i64>().ok().filter(|v| *v >= 0).ok_or_else(|| {
                AppError::Invalid("Saisissez un nombre entier positif ou zéro".into())
            })?),
        };
        values.push(value);
    }
    sqlx::query("UPDATE truie SET nb_tetines=?,splayleg=? WHERE id=?")
        .bind(values[0])
        .bind(values[1])
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

pub(super) async fn mise_bas_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date =
        form_date(&form, "date")?.ok_or_else(|| AppError::Invalid("Date obligatoire".into()))?;
    let mut counts = Vec::new();
    for key in [
        "nes_vifs",
        "mort_nes",
        "momifies",
        "chetifs",
        "ecrases",
        "tues_truie",
    ] {
        counts.push(
            form_i64(&form, key)
                .filter(|v| *v >= 0)
                .ok_or_else(|| AppError::Invalid(format!("Nombre invalide : {key}")))?,
        );
    }
    let losses = counts[3]
        .checked_add(counts[4])
        .and_then(|n| n.checked_add(counts[5]))
        .ok_or_else(|| AppError::Invalid("Pertes trop élevées".into()))?;
    if losses > counts[0] {
        return Err(AppError::Invalid(
            "Les pertes ne peuvent pas dépasser les nés vivants".into(),
        ));
    }
    let total = counts[0]
        .checked_add(counts[1])
        .and_then(|n| n.checked_add(counts[2]))
        .ok_or_else(|| AppError::Invalid("Total trop élevé".into()))?;
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let (sow, band): (i64, Option<i64>) =
        sqlx::query_as("SELECT truie_id,bande_id FROM evenement WHERE id=? AND type='mise_bas'")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound)?;
    sqlx::query("UPDATE evenement SET date=?,nes_totaux=?,nes_vifs=?,mort_nes=?,momifies=?,chetifs=?,ecrases=?,tues_truie=?,heure_debut=?,heure_fin=?,note=? WHERE id=?")
        .bind(&date).bind(total).bind(counts[0]).bind(counts[1]).bind(counts[2]).bind(counts[3]).bind(counts[4]).bind(counts[5]).bind(form_text(&form,"heure_debut")).bind(form_text(&form,"heure_fin")).bind(form_text(&form,"note")).bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM perteporcelet WHERE evenement_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (n, cause) in [
        (counts[3], "Chétif / non conforme"),
        (counts[4], "Écrasement"),
        (counts[5], "Tué par la truie"),
    ] {
        if n > 0 {
            sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,age_j,nb,cause,date,evenement_id) VALUES(?,?,0,?,?,?,?)").bind(sow).bind(band).bind(n).bind(cause).bind(&date).bind(id).execute(&mut *tx).await?;
        }
    }
    let available: i64 = sqlx::query_scalar("SELECT COALESCE(nes_vifs,0)+COALESCE(adoptes,0)-COALESCE(retires,0)+(SELECT COALESCE(SUM(nombre),0) FROM adoptionporcelet WHERE destination_id=e.id)-(SELECT COALESCE(SUM(nombre),0) FROM adoptionporcelet WHERE source_id=e.id)-(SELECT COALESCE(SUM(nb),0) FROM perteporcelet WHERE truie_id=e.truie_id AND (bande_id=e.bande_id OR evenement_id=e.id))-(SELECT COALESCE(SUM(nb_sevres),0) FROM evenement WHERE type='sevrage' AND truie_id=e.truie_id AND bande_id=e.bande_id) FROM evenement e WHERE id=?").bind(id).fetch_one(&mut *tx).await?;
    if available < 0 {
        return Err(AppError::Invalid("Cette correction est incompatible avec les pertes, adoptions ou sevrages déjà enregistrés".into()));
    }
    sqlx::query("UPDATE soinportee SET date_prevue=date(?,printf('%+d day',(SELECT jour FROM acteprotocole WHERE id=soinportee.protocole_id))) WHERE evenement_id=? AND date_realisee IS NULL").bind(&date).bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE truie SET perf_nt=?,perf_nv=?,perf_mn=?,perf_mo=? WHERE id=? AND ?=(SELECT id FROM evenement WHERE truie_id=? AND type='mise_bas' ORDER BY date DESC,id DESC LIMIT 1)").bind(total).bind(counts[0]).bind(counts[1]).bind(counts[2]).bind(sow).bind(id).bind(sow).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/truie/{sow}#mises-bas")).into_response())
}

pub(super) async fn note_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let note =
        form_text(&form, "note").ok_or_else(|| AppError::Invalid("Note obligatoire".into()))?;
    let day =
        form_date(&form, "date")?.ok_or_else(|| AppError::Invalid("Date obligatoire".into()))?;
    sqlx::query("UPDATE controlequotidien SET date=?,note=? WHERE id=? AND categorie='note_libre'")
        .bind(&day)
        .bind(note)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/quotidien?jour={day}#historique-notes")).into_response())
}
pub(super) async fn note_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM controlequotidien WHERE id=? AND categorie='note_libre'")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/quotidien#historique-notes").into_response())
}
pub(super) async fn tache_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let titre =
        form_text(&form, "titre").ok_or_else(|| AppError::Invalid("Titre obligatoire".into()))?;
    sqlx::query(
        "UPDATE tache SET titre=?,type=?,echeance=?,salle=?,bande_code=?,note=? WHERE id=?",
    )
    .bind(titre)
    .bind(form_text(&form, "type"))
    .bind(form_date(&form, "echeance")?)
    .bind(form_text(&form, "salle"))
    .bind(form_text(&form, "bande_code"))
    .bind(form_text(&form, "note"))
    .bind(id)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/taches").into_response())
}
pub(super) async fn entretien_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name =
        form_text(&form, "nom").ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let frequency = form_i64(&form, "frequence_jours")
        .filter(|n| *n > 0)
        .ok_or_else(|| AppError::Invalid("Fréquence positive obligatoire".into()))?;
    sqlx::query("UPDATE entretien SET nom=?,type=?,site=?,frequence_jours=?,note=? WHERE id=?")
        .bind(name)
        .bind(form_text(&form, "type"))
        .bind(form_text(&form, "site"))
        .bind(frequency)
        .bind(form_text(&form, "note"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/taches#entretiens").into_response())
}
pub(super) async fn bandes_releve(
    pool: &SqlitePool,
    meter: i64,
    date: &str,
    form: &HashMap<String, String>,
) -> AppResult<Option<String>> {
    let site: Option<String> = sqlx::query_scalar(
        "SELECT s.code FROM compteur_energie c LEFT JOIN site s ON s.id=c.site_id WHERE c.id=?",
    )
    .bind(meter)
    .fetch_optional(pool)
    .await?
    .flatten();
    if form.get("mode_bandes").map(String::as_str) != Some("manuel") {
        // Sans site connu, aucune déduction plutôt qu'un mélange entre sites.
        return if let Some(site) = site {
            Ok(Some(
                present_bands(pool, Some(&site), date).await?.join(","),
            ))
        } else {
            Ok(None)
        };
    }
    let mut codes = Vec::new();
    for code in form_text(form, "bandes")
        .unwrap_or_default()
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM bande b WHERE code=? AND (? IS NULL OR EXISTS(SELECT 1 FROM site s WHERE s.code=? AND lower(trim(COALESCE(b.site,''))) IN (lower(trim(s.code)),lower(trim(COALESCE(s.nom,''))))))").bind(code).bind(&site).bind(&site).fetch_one(pool).await?;
        if exists == 0 {
            return Err(AppError::Invalid(format!(
                "Bande {code} inconnue ou située sur un autre site"
            )));
        }
        if !codes.contains(&code.to_string()) {
            codes.push(code.to_string());
        }
    }
    Ok(Some(codes.join(",")))
}
pub(super) async fn releve_bandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let (meter, date): (i64, String) =
        sqlx::query_as("SELECT compteur_id,date_releve FROM releve_compteur WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let bands = bandes_releve(&state.pool, meter, &date, &form).await?;
    sqlx::query("UPDATE releve_compteur SET bandes=? WHERE id=?")
        .bind(bands)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/energie#compteur-{meter}")).into_response())
}
pub(super) async fn compteur_site(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE compteur_energie SET site_id=? WHERE id=?")
        .bind(form_i64(&form, "site_id"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/energie#compteur-{id}")).into_response())
}

pub(super) async fn semence_produit(
    pool: &SqlitePool,
    form: &HashMap<String, String>,
    date: &str,
    band: Option<i64>,
) -> AppResult<Option<String>> {
    match form
        .get("mode_semence")
        .map(String::as_str)
        .unwrap_or("manuel")
    {
        "facture" => {
            let id = form_i64(form, "achatsemence_id")
                .ok_or_else(|| AppError::Invalid("Choisissez une facture de semence".into()))?;
            let p:Option<String>=sqlx::query_scalar("SELECT designation FROM achatsemence WHERE id=? AND date<=? AND trim(COALESCE(designation,''))<>''").bind(id).bind(date).fetch_optional(pool).await?;
            p.map(Some)
                .ok_or_else(|| AppError::Invalid("Facture absente ou postérieure à l’IA".into()))
        }
        "auto" => {
            let p:Option<String>=sqlx::query_scalar("SELECT a.designation FROM achatsemence a WHERE a.date<=? AND trim(COALESCE(a.designation,''))<>'' AND (? IS NULL OR a.bande_id=? OR EXISTS(SELECT 1 FROM affectationfacturebande af WHERE af.categorie='semence' AND af.facture_id=a.id AND af.bande_id=?)) ORDER BY a.date DESC,a.id DESC LIMIT 1").bind(date).bind(band).bind(band).bind(band).fetch_optional(pool).await?;
            p.map(Some).ok_or_else(||AppError::Invalid("Aucune facture de semence adaptée : sélectionnez une facture ou saisissez manuellement".into()))
        }
        _ => Ok(form_text(form, "produit")),
    }
}

// Neutralise les formules lors de l’ouverture du CSV dans un tableur.
fn inventory_cell(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r', '\n', '\'']) {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

pub(super) async fn inventaire_export(State(state): State<AppState>) -> AppResult<Response> {
    let rows=generic_rows(&state.pool,"SELECT id,produit,COALESCE(unite,'') AS unite,COALESCE(stock_actuel,0) AS stock_actuel FROM produitpharmacie ORDER BY produit").await?;
    let mut w = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(vec![]);
    w.write_record([
        "type",
        "id",
        "produit",
        "unite",
        "stock_reference",
        "quantite_comptee",
    ])
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        w.write_record([
            "produit".into(),
            csv_value(r.get("id")),
            inventory_cell(&csv_value(r.get("produit"))),
            inventory_cell(&csv_value(r.get("unite"))),
            csv_value(r.get("stock_actuel")),
            String::new(),
        ])
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    let silos=generic_rows(&state.pool,"SELECT s.id,s.nom,COALESCE((SELECT niveau_tonnes FROM releve_silo r WHERE r.silo_id=s.id ORDER BY date DESC,id DESC LIMIT 1),0) AS niveau FROM silo_aliment s WHERE actif=1 ORDER BY nom").await?;
    for r in silos {
        w.write_record([
            "silo".into(),
            csv_value(r.get("id")),
            inventory_cell(&csv_value(r.get("nom"))),
            "tonnes".into(),
            csv_value(r.get("niveau")),
            String::new(),
        ])
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    let bytes = w.into_inner().map_err(|e| AppError::Internal(e.into()))?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=inventaire-stocks.csv",
            ),
        ],
        bytes,
    )
        .into_response())
}
pub(super) async fn inventaire_import(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let (form, file, _) = parity::multipart_fields(multipart, "fichier").await?;
    verify_csrf(&session, &form)?;
    let bytes = file.ok_or_else(|| AppError::Invalid("Fichier CSV obligatoire".into()))?;
    appliquer_inventaire(&state.pool, &bytes, &session.nom).await?;
    Ok(Redirect::to("/stock").into_response())
}
pub(super) async fn appliquer_inventaire(
    pool: &SqlitePool,
    bytes: &[u8],
    user: &str,
) -> AppResult<()> {
    let mut reader = csv::ReaderBuilder::new().delimiter(b';').from_reader(bytes);
    let headers = reader
        .headers()
        .map_err(|e| AppError::Invalid(e.to_string()))?
        .clone();
    if headers.iter().collect::<Vec<_>>()
        != [
            "type",
            "id",
            "produit",
            "unite",
            "stock_reference",
            "quantite_comptee",
        ]
    {
        return Err(AppError::Invalid(
            "Utilisez le fichier exporté sans modifier les colonnes".into(),
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut seen = HashSet::new();
    let mut count = 0;
    for r in reader.records() {
        let r = r.map_err(|e| AppError::Invalid(e.to_string()))?;
        if r[5].trim().is_empty() {
            continue;
        }
        let id = r[1]
            .parse::<i64>()
            .map_err(|_| AppError::Invalid("Identifiant invalide".into()))?;
        if !seen.insert((r[0].to_string(), id)) {
            return Err(AppError::Invalid("Ligne en double".into()));
        }
        let n = parse_french_number(&r[5])
            .filter(|n| n.is_finite() && *n >= 0.)
            .ok_or_else(|| AppError::Invalid("Quantité comptée invalide".into()))?;
        let reference = parse_french_number(&r[4])
            .filter(|n| n.is_finite())
            .ok_or_else(|| AppError::Invalid("Stock de référence invalide".into()))?;
        if &r[0] == "produit" {
            let (name,unit,old):(String,String,f64)=sqlx::query_as("SELECT produit,COALESCE(unite,''),CAST(COALESCE(stock_actuel,0) AS REAL) FROM produitpharmacie WHERE id=?").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
            if inventory_cell(&name) != r[2]
                || inventory_cell(&unit) != r[3]
                || (old - reference).abs() > 0.00001
            {
                return Err(AppError::Invalid(
                    "Produit modifié depuis l’export : réexportez l’inventaire".into(),
                ));
            }
            sqlx::query("UPDATE produitpharmacie SET stock_actuel=?,maj=? WHERE id=?")
                .bind(n)
                .bind(today_iso())
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO mouvementpharmacie(produit,date,type,quantite,note) VALUES(?,?,'inventaire',?,?)").bind(name).bind(today_iso()).bind(n-old).bind(format!("Inventaire importé par {user} : {old} → {n}")).execute(&mut *tx).await?;
        } else if &r[0] == "silo" {
            let (name,old):(String,f64)=sqlx::query_as("SELECT nom,CAST(COALESCE((SELECT niveau_tonnes FROM releve_silo WHERE silo_id=s.id ORDER BY date DESC,id DESC LIMIT 1),0) AS REAL) FROM silo_aliment s WHERE id=? AND actif=1").bind(id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
            if inventory_cell(&name) != r[2]
                || &r[3] != "tonnes"
                || (old - reference).abs() > 0.00001
            {
                return Err(AppError::Invalid(
                    "Silo modifié depuis l’export : réexportez l’inventaire".into(),
                ));
            }
            sqlx::query("INSERT INTO releve_silo(silo_id,date,niveau_tonnes,note) VALUES(?,?,?,?)")
                .bind(id)
                .bind(today_iso())
                .bind(n)
                .bind(format!("Inventaire importé par {user}"))
                .execute(&mut *tx)
                .await?;
        } else {
            return Err(AppError::Invalid("Type de stock inconnu".into()));
        }
        count += 1;
    }
    if count == 0 {
        return Err(AppError::Invalid(
            "Renseignez au moins une quantité comptée".into(),
        ));
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn pool() -> SqlitePool {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&p)
            .await
            .unwrap();
        p
    }
    fn session() -> SessionData {
        SessionData {
            uid: 1,
            identifiant: "test".into(),
            nom: "Test".into(),
            role: "admin".into(),
            sections: vec![],
            csrf: "test".into(),
            doit_changer_mdp: false,
            type_elevage: "naisseur_engraisseur".into(),
            module_genetique: false,
            module_prestataires: true,
            module_charcutiers_rfid: false,
            module_vente_directe: true,
        }
    }
    async fn state() -> AppState {
        AppState::new(
            Config {
                bind: "127.0.0.1:8080".parse().unwrap(),
                db_path: "/tmp/ameliorations-test.db".into(),
                secure_cookies: false,
            },
            pool().await,
            crate::templates::build().unwrap(),
        )
    }
    fn form(items: &[(&str, &str)]) -> HashMap<String, String> {
        std::iter::once(("csrf_token", "test"))
            .chain(items.iter().copied())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    #[tokio::test]
    async fn quotidien_historique_pagine_et_note_vide() {
        let s = state().await;
        sqlx::raw_sql("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<65) INSERT INTO controlequotidien(date,categorie,note) SELECT '2026-08-27','note_libre',CASE WHEN x=65 THEN NULL ELSE 'Note '||x END FROM n").execute(&s.pool).await.unwrap();
        let Html(body) =
            super::super::quotidien(State(s), Extension(session()), Query(HashMap::new()))
                .await
                .unwrap();
        assert_eq!(body.matches("Enregistrer la modification").count(), 30);
        assert!(body.contains("Plus anciennes"));
        assert!(body.contains("65 notes"));
    }
    #[tokio::test]
    async fn routeur_et_pages_modernisees() {
        let s = state().await;
        let _router = super::super::router(s.clone());
        let pages = [
            super::super::cochettes(State(s.clone()), Extension(session())).await,
            super::super::reformes(State(s.clone()), Extension(session())).await,
            super::super::entretien(State(s.clone()), Extension(session())).await,
            super::super::sanitaire(State(s.clone()), Extension(session())).await,
            super::super::inseminations(State(s.clone()), Extension(session())).await,
        ];
        for page in pages {
            let Html(body) = page.unwrap();
            assert!(body.contains("class=\"workflow\""));
            assert!(!body.split("</title>").next().unwrap().contains("<script>"));
        }
    }
    #[tokio::test]
    async fn correction_naissance_et_notes() {
        let s = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'A'); INSERT INTO truie(id,num_travail) VALUES(1,'100'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs,mort_nes,momifies) VALUES(1,'mise_bas','2026-08-01',1,1,12,1,1); INSERT INTO controlequotidien(id,date,categorie,note) VALUES(1,'2026-08-01','note_libre','Ancienne note');").execute(&s.pool).await.unwrap();
        let good = form(&[
            ("date", "2026-08-02"),
            ("nes_vifs", "14"),
            ("mort_nes", "2"),
            ("momifies", "1"),
            ("chetifs", "1"),
            ("ecrases", "1"),
            ("tues_truie", "0"),
        ]);
        mise_bas_modifier(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Form(good.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT SUM(nb) FROM perteporcelet")
                .fetch_one(&s.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT nes_totaux FROM evenement WHERE id=1")
                .fetch_one(&s.pool)
                .await
                .unwrap(),
            17
        );
        mise_bas_modifier(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Form(good.clone()),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT SUM(nb) FROM perteporcelet")
                .fetch_one(&s.pool)
                .await
                .unwrap(),
            2
        );
        let mut bad = good.clone();
        bad.insert("nes_vifs".into(), "1".into());
        assert!(
            mise_bas_modifier(State(s.clone()), Extension(session()), Path(1), Form(bad))
                .await
                .is_err()
        );
        let mut readonly = session();
        readonly.role = "visiteur".into();
        assert!(
            mise_bas_modifier(State(s.clone()), Extension(readonly), Path(1), Form(good))
                .await
                .is_err()
        );
        note_modifier(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Form(form(&[("date", "2026-08-03"), ("note", "Nouvelle note")])),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT note FROM controlequotidien WHERE id=1")
                .fetch_one(&s.pool)
                .await
                .unwrap(),
            "Nouvelle note"
        );
        note_supprimer(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Form(form(&[])),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM controlequotidien")
                .fetch_one(&s.pool)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn inventaire_atomique_et_reference_perimee() {
        let p = pool().await;
        sqlx::query("INSERT INTO produitpharmacie(id,produit,unite,stock_actuel) VALUES(1,'Savon','litres',10),(2,'Test','doses',4)").execute(&p).await.unwrap();
        let header = "type;id;produit;unite;stock_reference;quantite_comptee\n";
        let bad = format!("{header}produit;1;Savon;litres;10;7,5\nproduit;2;Test;doses;4;-1\n");
        assert!(appliquer_inventaire(&p, bad.as_bytes(), "Test")
            .await
            .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, f64>("SELECT stock_actuel FROM produitpharmacie WHERE id=1")
                .fetch_one(&p)
                .await
                .unwrap(),
            10.
        );
        let good = format!("{header}produit;1;Savon;litres;10;7,5\nproduit;2;Test;doses;4;\n");
        appliquer_inventaire(&p, good.as_bytes(), "Test")
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, f64>("SELECT quantite FROM mouvementpharmacie")
                .fetch_one(&p)
                .await
                .unwrap(),
            -2.5
        );
        assert!(appliquer_inventaire(&p, good.as_bytes(), "Test")
            .await
            .is_err());
    }
    #[tokio::test]
    async fn eau_isole_sites_et_accepte_correction() {
        let p = pool().await;
        sqlx::raw_sql("INSERT INTO site(id,code,nom) VALUES(1,'A','Ferme A'),(2,'B','Ferme B'); INSERT INTO bande(code,site,date_mb) VALUES('B1','Ferme A','2026-08-01'),('B2','B','2026-08-01'); INSERT INTO compteur_energie(id,nom,type,site_id) VALUES(1,'Eau A','eau',1),(2,'Sans site','eau',NULL)").execute(&p).await.unwrap();
        let mut f = HashMap::new();
        assert_eq!(
            bandes_releve(&p, 1, "2026-08-27", &f).await.unwrap(),
            Some("B1".into())
        );
        assert_eq!(bandes_releve(&p, 2, "2026-08-27", &f).await.unwrap(), None);
        f.insert("mode_bandes".into(), "manuel".into());
        f.insert("bandes".into(), "B2".into());
        assert!(bandes_releve(&p, 1, "2026-08-27", &f).await.is_err());
        f.insert("bandes".into(), "B1,B1".into());
        assert_eq!(
            bandes_releve(&p, 1, "2026-08-27", &f).await.unwrap(),
            Some("B1".into())
        );
    }
    #[tokio::test]
    async fn semence_auto_respecte_bande_et_date() {
        let p = pool().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'A'),(2,'B'); INSERT INTO achatsemence(id,date,designation,bande_id) VALUES(1,'2026-08-01','Verrat A',1),(2,'2026-08-02','Verrat B',2),(3,'2026-09-01','Futur',1)").execute(&p).await.unwrap();
        let f = HashMap::from([("mode_semence".into(), "auto".into())]);
        assert_eq!(
            semence_produit(&p, &f, "2026-08-27", Some(1))
                .await
                .unwrap(),
            Some("Verrat A".into())
        );
        assert!(semence_produit(&p, &f, "2026-07-01", Some(1))
            .await
            .is_err());
    }
}
