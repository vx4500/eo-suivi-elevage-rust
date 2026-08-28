use super::*;

/// Le verrou d'écriture couvre le contrôle d'effectif et les deux côtés du transfert.
pub(super) async fn transferer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(band_id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let source = form_i64(&form, "source_id")
        .ok_or_else(|| AppError::Invalid("Choisissez la truie donneuse".into()))?;
    let destination_raw = form_text(&form, "destination_id").ok_or_else(|| {
        AppError::Invalid("Choisissez une truie ou une case de nourrice artificielle".into())
    })?;
    let (destination, case_nourrice) = if let Some(id) = destination_raw.strip_prefix("case:") {
        (
            None,
            Some(
                id.parse::<i64>()
                    .map_err(|_| AppError::Invalid("Case de nourrice invalide".into()))?,
            ),
        )
    } else {
        (
            Some(
                destination_raw
                    .parse::<i64>()
                    .map_err(|_| AppError::Invalid("Truie receveuse invalide".into()))?,
            ),
            None,
        )
    };
    let nombre = form_i64(&form, "nombre")
        .filter(|n| *n > 0)
        .ok_or_else(|| AppError::Invalid("Nombre entier positif obligatoire".into()))?;
    if Some(source) == destination {
        return Err(AppError::Invalid(
            "La donneuse et la receveuse doivent être différentes".into(),
        ));
    }
    let date = form_date_or_today(&form, "date")?;
    if date > today_iso() {
        return Err(AppError::Invalid(
            "Une adoption ne peut pas être enregistrée dans le futur".into(),
        ));
    }
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    for id in std::iter::once(source).chain(destination) {
        let row: Option<(i64, i64, String)> = sqlx::query_as("SELECT p.presents,p.bande_id,p.date FROM portee_effectif p JOIN truie t ON t.id=p.truie_id WHERE p.id=? AND t.reformee=0 AND NOT EXISTS(SELECT 1 FROM evenement s WHERE s.type='sevrage' AND s.truie_id=p.truie_id AND s.bande_id=p.bande_id AND s.date>=p.date) AND p.id=(SELECT e.id FROM evenement e WHERE e.truie_id=p.truie_id AND e.type='mise_bas' ORDER BY e.date DESC,e.id DESC LIMIT 1)")
            .bind(id).fetch_optional(&mut *tx).await?;
        let (presents, bande, naissance) = row.ok_or_else(|| {
            AppError::Invalid("Choisissez deux portées en allaitement, non sevrées".into())
        })?;
        if id == source && bande != band_id {
            return Err(AppError::Invalid(
                "La donneuse doit appartenir à la bande affichée".into(),
            ));
        }
        if date < naissance {
            return Err(AppError::Invalid(
                "L’adoption doit avoir lieu après les deux mises-bas".into(),
            ));
        }
        let last: Option<String> = sqlx::query_scalar("SELECT MAX(date) FROM (SELECT date FROM adoptionporcelet WHERE source_id=? OR destination_id=? UNION ALL SELECT p.date FROM perteporcelet p JOIN evenement e ON e.truie_id=p.truie_id AND e.bande_id=p.bande_id WHERE e.id=?)")
            .bind(id).bind(id).bind(id).fetch_one(&mut *tx).await?;
        if last.is_some_and(|last| date < last) {
            return Err(AppError::Invalid("La date ne peut pas précéder une adoption ou une perte déjà enregistrée sur ces portées".into()));
        }
        if id == source && nombre > presents {
            return Err(AppError::Invalid(format!(
                "Effectif insuffisant : {presents} porcelet(s) vivant(s) chez la donneuse"
            )));
        }
    }
    if let Some(case_id) = case_nourrice {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE c.id=? AND (lower(COALESCE(s.type,'')) LIKE '%nourri%' OR lower(s.nom) LIKE '%nourri%' OR lower(COALESCE(s.type,'')) LIKE '%matern%' OR lower(s.nom) LIKE '%matern%')")
            .bind(case_id).fetch_one(&mut *tx).await?;
        if exists == 0 {
            return Err(AppError::Invalid(
                "Choisissez une case de maternité ou de nourrice artificielle".into(),
            ));
        }
    }
    sqlx::query("INSERT INTO adoptionporcelet(date,source_id,destination_id,case_nourrice_id,nombre,note) VALUES(?,?,?,?,?,?)")
        .bind(date).bind(source).bind(destination).bind(case_nourrice).bind(nombre).bind(form_text(&form,"note")).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!(
        "/maternite?bande_id={band_id}&vue=adoptions#adoptions"
    ))
    .into_response())
}

pub(super) async fn effectif(pool: &SqlitePool, sow_id: i64, band_id: i64) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT presents FROM portee_effectif WHERE truie_id=? AND bande_id=? ORDER BY date DESC,id DESC LIMIT 1")
        .bind(sow_id).bind(band_id).fetch_optional(pool).await?.unwrap_or(0))
}

/// Pertes et sevrages de nourrice gardent la bande d'origine et ne modifient
/// jamais la portée restée sous la mère.
pub(super) async fn sortie_nourrice(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((band_id, adoption_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "date")?;
    if date > today_iso() {
        return Err(AppError::Invalid(
            "La date ne peut pas être dans le futur".into(),
        ));
    }
    let nombre = form_i64(&form, "nombre")
        .filter(|n| *n > 0)
        .ok_or_else(|| AppError::Invalid("Nombre entier positif obligatoire".into()))?;
    let kind = form_text(&form, "type").unwrap_or_default();
    if kind != "perte" && kind != "sevrage" {
        return Err(AppError::Invalid(
            "Choisissez une perte ou un sevrage".into(),
        ));
    }
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let row: Option<(i64, i64, String)> = sqlx::query_as(
        "SELECT presents,case_nourrice_id,date FROM nourrice_effectif WHERE id=? AND bande_id=?",
    )
    .bind(adoption_id)
    .bind(band_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (present, source, entree) = row.ok_or(AppError::NotFound)?;
    if nombre > present {
        return Err(AppError::Invalid(format!(
            "Seulement {present} porcelet(s) vivant(s) dans ce lot de nourrice"
        )));
    }
    let last: Option<String> =
        sqlx::query_scalar("SELECT MAX(date) FROM sortienourrice WHERE adoption_id=?")
            .bind(adoption_id)
            .fetch_one(&mut *tx)
            .await?;
    if date < entree || last.is_some_and(|last| date < last) {
        return Err(AppError::Invalid(
            "La date doit suivre l’entrée et les dernières sorties de ce lot".into(),
        ));
    }
    let mut event_id: Option<i64> = None;
    let mut movement_id: Option<i64> = None;
    let mut cause = None;
    if kind == "perte" {
        cause = form_text(&form, "cause");
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM causeperte WHERE libelle=?")
            .bind(&cause)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            return Err(AppError::Invalid(
                "Sélectionnez une cause de perte configurée".into(),
            ));
        }
    } else {
        let destination = form_i64(&form, "case_dest_id")
            .ok_or_else(|| AppError::Invalid("Choisissez une case de post-sevrage".into()))?;
        let dest: Option<(i64,Option<i64>)> = sqlx::query_as("SELECT c.salle_id,c.nb_max_porcs FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE c.id=? AND (lower(COALESCE(s.type,'')) LIKE '%sevr%' OR lower(s.nom) LIKE '%sevr%')")
            .bind(destination).fetch_optional(&mut *tx).await?;
        let (room, capacity) = dest.ok_or_else(|| {
            AppError::Invalid("La destination doit être une case de post-sevrage".into())
        })?;
        if source == destination {
            return Err(AppError::Invalid(
                "La case de destination doit être différente de la nourrice".into(),
            ));
        }
        // Même calcul que case_pig_count, dans la transaction d'écriture.
        let occupancy: i64 = sqlx::query_scalar("SELECT COALESCE((SELECT nombre FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1),0)+COALESCE((SELECT SUM(CASE WHEN case_dest_id=? THEN COALESCE(nombre,0) WHEN id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(nombre,0) END) FROM transfert WHERE espece='porc' AND (case_dest_id=? OR case_source_id=?) AND date>COALESCE((SELECT date FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1),'')),0)-COALESCE((SELECT SUM(nombre) FROM declarationmort WHERE case_id=? AND date>COALESCE((SELECT date FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1),'')),0)")
            .bind(destination).bind(destination).bind(destination).bind(destination).bind(destination).bind(destination).bind(destination).fetch_one(&mut *tx).await?;
        if capacity.is_some_and(|max| nombre > max - occupancy.max(0)) {
            return Err(AppError::Invalid(
                "Capacité dépassée dans la case de post-sevrage".into(),
            ));
        }
        event_id = Some(sqlx::query("INSERT INTO evenement(type,date,bande_id,nb_sevres,case_id,note) VALUES('sevrage',?,?,?,?, 'Sevrage de nourrice artificielle')")
            .bind(&date).bind(band_id).bind(nombre).bind(source).execute(&mut *tx).await?.last_insert_rowid());
        movement_id = Some(sqlx::query("INSERT INTO transfert(date,espece,bande_id,salle_source_id,salle_dest_id,case_source_id,case_dest_id,nombre,note) VALUES(?,'porc',?,(SELECT salle_id FROM casesalle WHERE id=?),?,?,?,?, 'Sevrage de nourrice artificielle')")
            .bind(&date).bind(band_id).bind(source).bind(room).bind(source).bind(destination).bind(nombre).execute(&mut *tx).await?.last_insert_rowid());
    }
    sqlx::query("INSERT INTO sortienourrice(adoption_id,date,type,nombre,cause,evenement_id,transfert_id) VALUES(?,?,?,?,?,?,?)")
        .bind(adoption_id).bind(&date).bind(kind).bind(nombre).bind(cause).bind(event_id).bind(movement_id).execute(&mut *tx).await?;
    if event_id.is_some() {
        sqlx::query("UPDATE bande SET cs_total_sevres=(SELECT COALESCE(SUM(nb_sevres),0) FROM evenement WHERE bande_id=? AND type='sevrage'),updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(band_id).bind(band_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Redirect::to(&format!(
        "/maternite?bande_id={band_id}&vue=nourrices#nourrices"
    ))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> AppResult<AppState> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        sqlx::raw_sql("INSERT INTO bande(id,code,date_mb) VALUES(1,'B1','2026-08-01'),(2,'B2','2026-08-02'); INSERT INTO truie(id,num_travail,bande_code) VALUES(1,'101','B1'),(2,'102','B1'),(3,'103','B2'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs,mort_nes,momifies) VALUES(1,'mise_bas','2026-08-01',1,1,12,2,1),(2,'mise_bas','2026-08-01',2,1,10,0,0),(3,'mise_bas','2026-08-02',3,2,8,0,0);").execute(&pool).await?;
        Ok(AppState::new(
            Config {
                bind: "127.0.0.1:8080".parse().unwrap(),
                db_path: "/tmp/adoptions-test.db".into(),
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
    fn form(destination: &str, number: &str) -> HashMap<String, String> {
        [
            ("csrf_token", "test"),
            ("source_id", "1"),
            ("destination_id", destination),
            ("nombre", number),
            ("date", "2026-08-10"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    #[tokio::test]
    async fn adoption_conserve_vivants_entre_truies_et_autre_bande() -> AppResult<()> {
        let state = setup().await?;
        transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("2", "4")),
        )
        .await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 8);
        assert_eq!(effectif(&state.pool, 2, 1).await?, 14);
        transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("3", "3")),
        )
        .await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 5);
        assert_eq!(effectif(&state.pool, 3, 2).await?, 11);
        let total: i64 = sqlx::query_scalar("SELECT SUM(presents) FROM portee_effectif")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(total, 30);
        let html = maternite(
            State(state),
            Extension(session()),
            Query(HashMap::from([("bande_id".into(), "1".into())])),
        )
        .await?
        .0;
        assert!(!html.contains("Adoptions après mise-bas"));
        assert!(html.contains("&amp;vue=adoptions"));
        assert!(html.contains("nés vivants"));
        assert!(!html.contains("présents estimés"));
        Ok(())
    }
    #[tokio::test]
    async fn refus_ne_retire_jamais_de_porcelets() -> AppResult<()> {
        let state = setup().await?;
        for (dest, n) in [
            ("2", "13"),
            ("1", "2"),
            ("999", "2"),
            ("2", "0"),
            ("2", "-1"),
            ("", "2"),
        ] {
            assert!(transferer(
                State(state.clone()),
                Extension(session()),
                Path(1),
                Form(form(dest, n))
            )
            .await
            .is_err());
            assert_eq!(effectif(&state.pool, 1, 1).await?, 12);
        }
        let mut invalid = form("2", "2");
        invalid.insert("date".into(), "2026-07-31".into());
        assert!(transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(invalid)
        )
        .await
        .is_err());
        let mut invalid = form("2", "2");
        invalid.insert("csrf_token".into(), "incorrect".into());
        assert!(transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(invalid)
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM adoptionporcelet")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(count, 0);
        Ok(())
    }
    #[tokio::test]
    async fn pertes_excluent_mort_nes_et_momifies_et_sevrage_vide_la_portee() -> AppResult<()> {
        let state = setup().await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 12);
        sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,nb,date,cause) VALUES(1,1,2,'2026-08-05','Écrasement')").execute(&state.pool).await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 10);
        transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("2", "10")),
        )
        .await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 0);
        assert_eq!(effectif(&state.pool, 2, 1).await?, 20);
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres) VALUES('sevrage','2026-08-20',2,1,20)").execute(&state.pool).await?;
        assert_eq!(effectif(&state.pool, 2, 1).await?, 0);
        assert!(transferer(
            State(state),
            Extension(session()),
            Path(1),
            Form(form("2", "1"))
        )
        .await
        .is_err());
        Ok(())
    }
    #[tokio::test]
    async fn renommer_case_et_supprimer_limite_porcelets() -> AppResult<()> {
        let state = setup().await?;
        sqlx::raw_sql("INSERT INTO site(id,code) VALUES(1,'S'); INSERT INTO salle(id,site_id,nom) VALUES(1,1,'Maternité'); INSERT INTO casesalle(id,salle_id,nom,nb_max_porcelets) VALUES(1,1,'Ancien',10);").execute(&state.pool).await?;
        let f = HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("nom".into(), "Nouveau".into()),
        ]);
        structure_case_rfid(State(state.clone()), Extension(session()), Path(1), Form(f)).await?;
        let row: (String, Option<i64>) =
            sqlx::query_as("SELECT nom,nb_max_porcelets FROM casesalle WHERE id=1")
                .fetch_one(&state.pool)
                .await?;
        assert_eq!(row, ("Nouveau".into(), None));
        Ok(())
    }
    #[tokio::test]
    async fn transferts_concurrents_et_modifications_incoherentes_sont_refuses() -> AppResult<()> {
        let state = setup().await?;
        let (a, b) = tokio::join!(
            transferer(
                State(state.clone()),
                Extension(session()),
                Path(1),
                Form(form("2", "7"))
            ),
            transferer(
                State(state.clone()),
                Extension(session()),
                Path(1),
                Form(form("3", "7"))
            )
        );
        assert_ne!(a.is_ok(), b.is_ok());
        assert_eq!(effectif(&state.pool, 1, 1).await?, 5);
        assert!(sqlx::query("UPDATE evenement SET nes_vifs=1 WHERE id=1")
            .execute(&state.pool)
            .await
            .is_err());
        assert!(sqlx::query("DELETE FROM evenement WHERE id=1")
            .execute(&state.pool)
            .await
            .is_err());
        assert!(sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,nb,date,cause) VALUES(1,1,6,'2026-08-11','Écrasement')").execute(&state.pool).await.is_err());
        assert!(sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres) VALUES('sevrage','2026-08-20',1,1,4)").execute(&state.pool).await.is_err());
        assert_eq!(effectif(&state.pool, 1, 1).await?, 5);
        Ok(())
    }
    async fn nursery_setup() -> AppResult<AppState> {
        let state = setup().await?;
        sqlx::raw_sql("INSERT INTO site(id,code) VALUES(1,'S'); INSERT INTO salle(id,site_id,nom,type) VALUES(1,1,'Maternité','Maternité'),(2,1,'Machine à lait','Nourrice'),(3,1,'Post-sevrage','Post-sevrage'); INSERT INTO casesalle(id,salle_id,nom,nb_max_porcs) VALUES(11,1,'M1',NULL),(12,2,'Nourrice A',NULL),(13,3,'PS1',50); UPDATE truie SET case_id=11 WHERE id=1;").execute(&state.pool).await?;
        Ok(state)
    }
    fn sortie(kind: &str, number: &str) -> HashMap<String, String> {
        [
            ("csrf_token", "test"),
            ("type", kind),
            ("nombre", number),
            ("date", "2026-08-11"),
            ("cause", "Écrasement"),
            ("case_dest_id", "13"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    #[tokio::test]
    async fn nourrice_artificielle_pertes_et_sevrages_conservent_les_effectifs() -> AppResult<()> {
        let state = nursery_setup().await?;
        transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("case:12", "6")),
        )
        .await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 6);
        assert_eq!(case_litter_count(&state.pool, 11).await?, 6);
        assert_eq!(case_litter_count(&state.pool, 12).await?, 6);
        let html = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([("bande_id".into(), "1".into())])),
        )
        .await?
        .0;
        assert!(html.contains("<b>22</b><span>porcelets présents"));
        let html = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([
                ("bande_id".into(), "1".into()),
                ("vue".into(), "nourrices".into()),
            ])),
        )
        .await?
        .0;
        assert!(html.contains("Nourrice A"));
        assert!(html.contains("/nourrice/1/sortie"));
        sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(sortie("perte", "2")),
        )
        .await?;
        assert_eq!(effectif(&state.pool, 1, 1).await?, 6);
        assert_eq!(case_litter_count(&state.pool, 12).await?, 4);
        let html = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([("bande_id".into(), "1".into())])),
        )
        .await?
        .0;
        assert!(html.contains("<b>20</b><span>porcelets présents"));
        let html = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([
                ("bande_id".into(), "1".into()),
                ("vue".into(), "bilan".into()),
            ])),
        )
        .await?
        .0;
        assert!(html.contains("<b>2</b><span>pertes enregistrées"));
        sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(sortie("sevrage", "3")),
        )
        .await?;
        assert_eq!(case_litter_count(&state.pool, 12).await?, 1);
        assert_eq!(case_pig_count(&state.pool, 13).await?, 3);
        assert_eq!(case_pig_count_raw(&state.pool, 12).await?, 0);
        let alerts = farm_alerts(&state.pool).await?;
        assert!(!alerts
            .to_string()
            .contains("Cases avec un effectif incohérent"));

        assert_eq!(effectif(&state.pool, 1, 1).await?, 6);
        // Le sevrage de la mère ne fait pas disparaître le lot en nourrice.
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres) VALUES('sevrage','2026-08-12',1,1,6)").execute(&state.pool).await?;
        assert_eq!(case_litter_count(&state.pool, 12).await?, 1);
        let mut last = sortie("sevrage", "1");
        last.insert("date".into(), "2026-08-13".into());
        sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(last),
        )
        .await?;
        assert_eq!(case_litter_count(&state.pool, 12).await?, 0);
        assert_eq!(case_pig_count(&state.pool, 13).await?, 4);
        let total: i64 = sqlx::query_scalar("SELECT cs_total_sevres FROM bande WHERE id=1")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(total, 10);
        // Redémarrer le schéma conserve l'historique et les effectifs.
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&state.pool)
            .await?;
        assert_eq!(case_litter_count(&state.pool, 12).await?, 0);
        Ok(())
    }
    #[tokio::test]
    async fn nourrice_refuse_sorties_invalides_sans_mouvement_partiel() -> AppResult<()> {
        let state = nursery_setup().await?;
        for dest in ["case:999", "case:13", "case:invalide"] {
            assert!(transferer(
                State(state.clone()),
                Extension(session()),
                Path(1),
                Form(form(dest, "6"))
            )
            .await
            .is_err());
        }
        assert_eq!(effectif(&state.pool, 1, 1).await?, 12);
        transferer(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("case:12", "6")),
        )
        .await?;
        for (key, value) in [
            ("nombre", "7"),
            ("nombre", "-1"),
            ("date", "2026-08-01"),
            ("date", "2099-01-01"),
            ("case_dest_id", "11"),
            ("csrf_token", "bad"),
        ] {
            let mut f = sortie("sevrage", "2");
            f.insert(key.into(), value.into());
            assert!(sortie_nourrice(
                State(state.clone()),
                Extension(session()),
                Path((1, 1)),
                Form(f)
            )
            .await
            .is_err());
            assert_eq!(case_litter_count(&state.pool, 12).await?, 6);
            assert_eq!(case_pig_count(&state.pool, 13).await?, 0);
        }
        assert!(sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((2, 1)),
            Form(sortie("perte", "1"))
        )
        .await
        .is_err());
        let mut invalid_cause = sortie("perte", "1");
        invalid_cause.insert("cause".into(), "Mort-né".into());
        assert!(sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(invalid_cause)
        )
        .await
        .is_err());
        sqlx::query("UPDATE casesalle SET nb_max_porcs=1 WHERE id=13")
            .execute(&state.pool)
            .await?;
        assert!(sortie_nourrice(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(sortie("sevrage", "2"))
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sortienourrice")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(count, 0);
        assert!(sqlx::query("DELETE FROM casesalle WHERE id=12")
            .execute(&state.pool)
            .await
            .is_err());
        Ok(())
    }
}
