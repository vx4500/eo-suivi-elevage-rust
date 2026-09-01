use super::*;

/// La fiche courante lit les événements réels, sans les compléments de l'historique importé.
pub(super) async fn actuelle(pool: &SqlitePool, sow: i64) -> AppResult<Value> {
    let rows = sqlx::query("SELECT e.id,e.date,e.nes_vifs,e.heure_debut,e.heure_fin,e.delivrance_ok,e.suivi_actif,p.date_sevrage,p.cloturee,p.adoptes,p.retires,p.pertes,p.presents FROM evenement e JOIN portee_effectif p ON p.id=e.id WHERE e.truie_id=? AND e.date<=date('now') ORDER BY e.date DESC,e.id DESC LIMIT 1")
        .bind(sow).fetch_all(pool).await?;
    let mut row = rows_to_json(rows)?
        .into_iter()
        .next()
        .unwrap_or(Value::Null);
    if !row.is_null() {
        annoter(&mut row, true);
    }
    Ok(row)
}

/// La fin de mise-bas et la surveillance sont deux informations indépendantes.
pub(super) fn annoter(row: &mut Value, mise_bas: bool) {
    let (code, label, icon) = if !mise_bas {
        ("a_mettre_bas", "À mettre bas", "◷")
    } else if row["date_sevrage"].is_string() {
        ("sevree", "Portée sevrée", "↗")
    } else if row["cloturee"].as_i64() == Some(1) {
        ("archivee", "Ancienne portée", "▤")
    } else if row["delivrance_ok"].as_i64() == Some(1)
        || row["heure_fin"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
    {
        ("terminee", "Mise-bas terminée", "✓")
    } else {
        ("en_cours", "Mise-bas en cours", "◔")
    };
    let surveillance = mise_bas
        && !matches!(code, "sevree" | "archivee")
        && (row["suivi_actif"].as_i64() == Some(1) || row["delivrance_ok"].as_i64() == Some(0));
    row["statut_code"] = json!(code);
    row["statut_libelle"] = json!(label);
    row["statut_icone"] = json!(icon);
    row["surveillance"] = json!(surveillance);
}

/// Les boutons d'état ne réécrivent jamais les effectifs, dates ou observations.
pub(super) async fn changer_etat(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let action = form_text(&form, "action").unwrap_or_default();
    let sql = match action.as_str() {
        "terminer" => "UPDATE evenement SET delivrance_ok=1 WHERE id=?",
        "surveiller" => "UPDATE evenement SET suivi_actif=1 WHERE id=?",
        "arreter_surveillance" => "UPDATE evenement SET suivi_actif=0 WHERE id=?",
        _ => return Err(AppError::Invalid("Action de suivi inconnue.".into())),
    };
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let (sow, band, delivrance): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT e.truie_id,e.bande_id,e.delivrance_ok FROM evenement e JOIN portee_effectif p ON p.id=e.id WHERE e.id=? AND p.cloturee=0 AND e.date<=date('now')",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(|| AppError::Invalid("Cette portée n’est pas en cours de suivi.".into()))?;
    if action == "arreter_surveillance" && delivrance == Some(0) {
        return Err(AppError::Invalid(
            "La délivrance est encore notée NOK. Corrigez ce contrôle avant de lever l’alerte."
                .into(),
        ));
    }
    sqlx::query(sql).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    let destination = band
        .map(|id| format!("/maternite?bande_id={id}#truie-{sow}"))
        .unwrap_or_else(|| format!("/truie/{sow}"));
    Ok(Redirect::to(&destination).into_response())
}

/// Même validation et même calcul depuis la fiche truie ou le suivi de bande.
pub(super) async fn enregistrer_perte(
    pool: &SqlitePool,
    sow: i64,
    band: Option<i64>,
    form: &HashMap<String, String>,
    max_age: Option<i64>,
) -> AppResult<()> {
    let nombre = form_i64(form, "nb")
        .filter(|n| *n > 0)
        .ok_or_else(|| AppError::Invalid("Nombre entier positif obligatoire".into()))?;
    let cause =
        form_text(form, "cause").ok_or_else(|| AppError::Invalid("Cause obligatoire".into()))?;
    let date = form_date_or_today(form, "date")?;
    if date > today_iso() {
        return Err(AppError::Invalid(
            "Une perte ne peut pas être déclarée dans le futur.".into(),
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let known: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM causeperte WHERE lower(libelle)=lower(?)")
            .bind(&cause)
            .fetch_one(&mut *tx)
            .await?;
    if known == 0 {
        return Err(AppError::Invalid(
            "Sélectionnez une cause configurée.".into(),
        ));
    }
    let (band_id,birth,presents): (Option<i64>,String,i64) = sqlx::query_as(
        "SELECT bande_id,date,presents FROM portee_effectif WHERE truie_id=? AND (? IS NULL OR bande_id=?) AND cloturee=0 AND date<=date('now') ORDER BY date DESC,id DESC LIMIT 1"
    ).bind(sow).bind(band).bind(band).fetch_optional(&mut *tx).await?
        .ok_or_else(|| AppError::Invalid("Aucune portée non sevrée pour cette truie.".into()))?;
    let age = (parse_stored_date(&date)
        .ok_or_else(|| AppError::Invalid("Date invalide".into()))?
        - parse_stored_date(&birth)
            .ok_or_else(|| AppError::Invalid("Date de mise-bas invalide".into()))?)
    .num_days();
    if age < 0 || max_age.is_some_and(|max| age > max) {
        return Err(AppError::Invalid(
            "La perte doit être comprise dans la période de suivi de cette portée.".into(),
        ));
    }
    if nombre > presents {
        return Err(AppError::Invalid(format!(
            "Seulement {presents} porcelet(s) vivant(s) présents"
        )));
    }
    sqlx::query(
        "INSERT INTO perteporcelet(truie_id,bande_id,age_j,nb,cause,date) VALUES(?,?,?,?,?,?)",
    )
    .bind(sow)
    .bind(band_id)
    .bind(age)
    .bind(nombre)
    .bind(cause)
    .bind(date)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::documents::tests::{session, state};

    #[test]
    fn delivrance_ok_prime_sur_heure_fin_et_surveillance_reste_visible() {
        for (data, expected, surveillance) in [
            (
                json!({"heure_debut":"10:00","delivrance_ok":1}),
                "terminee",
                false,
            ),
            (
                json!({"heure_debut":"10:00","delivrance_ok":1,"suivi_actif":1}),
                "terminee",
                true,
            ),
            (
                json!({"heure_debut":"10:00","delivrance_ok":0}),
                "en_cours",
                true,
            ),
            (
                json!({"heure_fin":"12:00","delivrance_ok":0}),
                "terminee",
                true,
            ),
            (
                json!({"date_sevrage":"2026-01-01","suivi_actif":1}),
                "sevree",
                false,
            ),
            (json!({"cloturee":1,"suivi_actif":1}), "archivee", false),
        ] {
            let mut row = data;
            annoter(&mut row, true);
            assert_eq!(row["statut_code"], expected);
            assert_eq!(row["surveillance"], surveillance);
        }
        let mut absent = json!({});
        annoter(&mut absent, false);
        assert_eq!(absent["statut_code"], "a_mettre_bas");
    }

    #[tokio::test]
    async fn boutons_etat_ne_modifient_ni_effectifs_ni_dates_et_respectent_csrf(
    ) -> anyhow::Result<()> {
        let state = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'); INSERT INTO truie(id,num_travail,rang,bande_code) VALUES(1,'123',4,'B1'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs,heure_debut,delivrance_ok,suivi_actif,note) VALUES(1,'mise_bas','2026-01-01',1,1,12,'10:00',0,1,'Observation conservée');").execute(&state.pool).await?;
        let form = |action: &str| {
            HashMap::from([
                ("csrf_token".into(), "test".into()),
                ("action".into(), action.into()),
            ])
        };
        assert!(changer_etat(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(HashMap::from([("action".into(), "terminer".into())]))
        )
        .await
        .is_err());
        assert!(changer_etat(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("arreter_surveillance"))
        )
        .await
        .is_err());
        for _ in 0..2 {
            changer_etat(
                State(state.clone()),
                Extension(session()),
                Path(1),
                Form(form("terminer")),
            )
            .await?;
        }
        let values: (i64,String,Option<String>,String,i64,i64) = sqlx::query_as("SELECT nes_vifs,date,heure_fin,note,suivi_actif,delivrance_ok FROM evenement WHERE id=1").fetch_one(&state.pool).await?;
        assert_eq!(
            values,
            (
                12,
                "2026-01-01".into(),
                None,
                "Observation conservée".into(),
                1,
                1
            )
        );
        let Html(html) = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([("bande_id".into(), "1".into())])),
        )
        .await?;
        assert!(html.contains("data-status=\"terminee\" data-surveillance=\"1\""));
        assert!(html.contains("data-indicateur=\"presents\">12</b>"));
        changer_etat(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("arreter_surveillance")),
        )
        .await?;
        let active: i64 = sqlx::query_scalar("SELECT suivi_actif FROM evenement WHERE id=1")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(active, 0);
        let Html(html) = truie_detail(State(state.clone()), Extension(session()), Path(1)).await?;
        assert!(html.contains("✓ Mise-bas terminée"));
        assert!(html.contains("data-indicateur=\"presents\">12</b>"));
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres) VALUES('sevrage','2026-01-29',1,1,12)").execute(&state.pool).await?;
        assert!(changer_etat(
            State(state),
            Extension(session()),
            Path(1),
            Form(form("surveiller"))
        )
        .await
        .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn pertes_des_deux_pages_utilisent_la_portee_et_refusent_les_depassements(
    ) -> anyhow::Result<()> {
        let state = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'),(2,'B2'); INSERT INTO truie(id,num_travail,bande_code) VALUES(1,'123','B2'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs) VALUES(1,'mise_bas','2026-01-01',1,1,12); INSERT OR IGNORE INTO causeperte(libelle) VALUES('Test perte');").execute(&state.pool).await?;
        let form = |n: &str| {
            HashMap::from([
                ("csrf_token".into(), "test".into()),
                ("nb".into(), n.into()),
                ("cause".into(), "Test perte".into()),
                ("date".into(), "2026-01-05".into()),
            ])
        };
        truie_perte(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("2")),
        )
        .await?;
        maternite_perte(
            State(state.clone()),
            Extension(session()),
            Path((1, 1)),
            Form(form("1")),
        )
        .await?;
        let rows: Vec<(Option<i64>, i64)> =
            sqlx::query_as("SELECT bande_id,nb FROM perteporcelet ORDER BY id")
                .fetch_all(&state.pool)
                .await?;
        assert_eq!(rows, vec![(Some(1), 2), (Some(1), 1)]);
        assert!(truie_perte(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form("10"))
        )
        .await
        .is_err());
        let a = form("8");
        let b = form("8");
        let (a, b) = tokio::join!(
            enregistrer_perte(&state.pool, 1, None, &a, None),
            enregistrer_perte(&state.pool, 1, Some(1), &b, Some(28))
        );
        assert_ne!(a.is_ok(), b.is_ok());
        let presents: i64 = sqlx::query_scalar("SELECT presents FROM portee_effectif WHERE id=1")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(presents, 1);
        let mut late = form("1");
        late.insert("date".into(), "2026-01-30".into());
        assert!(enregistrer_perte(&state.pool, 1, Some(1), &late, Some(28))
            .await
            .is_err());
        late.insert("date".into(), "2026-01-29".into());
        enregistrer_perte(&state.pool, 1, Some(1), &late, Some(28)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn migration_vue_preserve_donnees_et_unifie_les_cycles() -> anyhow::Result<()> {
        let state = state().await;
        let migration = include_str!("../../migrations/0003_portee_effectif.sql");
        let definition = migration.split_once("CREATE VIEW").unwrap().1;
        assert!(include_str!("../../migrations/0001_schema.sql").contains(definition.trim()));
        sqlx::raw_sql("INSERT INTO truie(id,num_travail) VALUES(1,'123'); INSERT INTO evenement(id,type,date,truie_id,nes_vifs,nb_sevres) VALUES(1,'mise_bas','2025-01-01',1,12,NULL),(2,'sevrage','2025-01-29',1,NULL,10),(3,'mise_bas','2026-01-01',1,14,NULL); INSERT INTO perteporcelet(truie_id,date,nb) VALUES(1,'2025-01-05',2),(1,'2026-01-05',1); DROP VIEW portee_effectif; CREATE VIEW portee_effectif AS SELECT id,truie_id,bande_id,date,nes_vifs AS presents FROM evenement WHERE type='mise_bas';").execute(&state.pool).await?;
        for _ in 0..2 {
            sqlx::raw_sql(migration).execute(&state.pool).await?;
        }
        let rows: Vec<(i64, i64, i64)> =
            sqlx::query_as("SELECT id,presents,pertes FROM portee_effectif ORDER BY id")
                .fetch_all(&state.pool)
                .await?;
        assert_eq!(rows, vec![(1, 0, 2), (3, 13, 1)]);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM evenement")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(count, 3);
        Ok(())
    }
}
