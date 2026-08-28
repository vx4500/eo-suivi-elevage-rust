use super::*;
type SaleAssignmentRow = (
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

fn normalize(reference: &str) -> String {
    reference
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}
/// Match exact du code ou du numéro officiel. Un marquage réutilisé reste
/// ambigu : aucune déduction de bande à partir de la seule date d'abattage.
pub(super) fn suggestion(reference: Option<&str>, bands: &[Value]) -> (Option<i64>, &'static str) {
    let reference = normalize(reference.unwrap_or_default());
    if reference.is_empty() {
        return (None, "Numéro de lot absent : choisissez la bande.");
    }
    let matches: HashSet<i64> = bands
        .iter()
        .filter(|b| {
            ["code", "num_officiel"]
                .iter()
                .any(|k| b[*k].as_str().is_some_and(|v| normalize(v) == reference))
        })
        .filter_map(|b| b["id"].as_i64())
        .collect();
    match matches.len() {
        1 => (
            matches.into_iter().next(),
            "Bande proposée d’après le numéro de lot / marquage.",
        ),
        0 => (None, "Aucune correspondance exacte : choisissez la bande."),
        _ => (
            None,
            "Ce numéro correspond à plusieurs bandes : choisissez la bonne bande.",
        ),
    }
}
pub(super) async fn bands(pool: &SqlitePool) -> AppResult<Vec<Value>> {
    generic_rows(pool,"SELECT id,code,num_officiel,site,date_mb FROM bande ORDER BY active DESC,date_mb DESC,id DESC").await
}
pub(super) async fn rows(pool: &SqlitePool) -> AppResult<Vec<Value>> {
    let bands = bands(pool).await?;
    let mut rows=generic_rows(pool,"SELECT v.*,b.code AS bande_code,b.site AS bande_site,ROUND(v.montant_ht/NULLIF(v.poids_total,0),3) AS prix_ht_kg FROM ventelot v LEFT JOIN bande b ON b.id=v.bande_id ORDER BY v.date DESC,v.id DESC,v.lot_index LIMIT 250").await?;
    for row in &mut rows {
        let (id, message) = suggestion(row["lot_ref"].as_str(), &bands);
        row["suggested_band"] = json!(id);
        row["suggestion_note"] = json!(message);
    }
    Ok(rows)
}

pub(super) async fn assign(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    index: i64,
    band: Option<i64>,
) -> AppResult<()> {
    if let Some(band) = band {
        if sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bande WHERE id=?")
            .bind(band)
            .fetch_one(&mut **tx)
            .await?
            != 1
        {
            return Err(AppError::Invalid("Bande inconnue.".into()));
        }
    }
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ventelot WHERE id=? AND lot_index=?")
            .bind(id)
            .bind(index)
            .fetch_one(&mut **tx)
            .await?;
    if exists != 1 {
        return Err(AppError::NotFound);
    }
    let (date, reference, old_band, number, raw): SaleAssignmentRow = sqlx::query_as(
        "SELECT date,num_apport,bande_id,nb_porcs,lots_json FROM venteapport WHERE id=?",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    let mut raw = raw;
    let primary;
    if index >= 0 {
        let mut lots: Vec<Value> = serde_json::from_str(raw.as_deref().unwrap_or("[]"))
            .map_err(|e| AppError::Internal(e.into()))?;
        // Préserver l'ancienne affectation globale des autres lots avant
        // de passer à une ventilation explicite lot par lot.
        for lot in &mut lots {
            if lot["bande_id"].is_null() {
                lot["bande_id"] = json!(old_band);
            }
        }
        let lot = lots.get_mut(index as usize).ok_or(AppError::NotFound)?;
        lot["bande_id"] = json!(band);
        raw = Some(json!(lots).to_string());
        primary = None;
        sqlx::query("UPDATE venteapport SET bande_id=NULL,lots_json=? WHERE id=?")
            .bind(&raw)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    } else {
        primary = band;
        sqlx::query("UPDATE venteapport SET bande_id=? WHERE id=?")
            .bind(band)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    synchronise_sortie_abattoir(
        tx,
        id,
        date.as_deref(),
        reference.as_deref().unwrap_or(""),
        primary,
        number,
        raw.as_deref(),
    )
    .await
}
fn selected(form: &HashMap<String, String>) -> AppResult<Option<i64>> {
    match form_text(form, "bande_id") {
        None => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Some)
            .ok_or_else(|| AppError::Invalid("Bande invalide.".into())),
    }
}
pub(super) async fn direct(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let mut tx = state.pool.begin().await?;
    assign(&mut tx, id, -1, selected(&form)?).await?;
    rename_lot(&mut tx, id, -1, &form).await?;
    tx.commit().await?;
    Ok(Redirect::to("/economique?secteur=abattoir").into_response())
}
pub(super) async fn lot(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((id, index)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    if index < 0 {
        return Err(AppError::NotFound);
    }
    let mut tx = state.pool.begin().await?;
    assign(&mut tx, id, index, selected(&form)?).await?;
    rename_lot(&mut tx, id, index, &form).await?;
    tx.commit().await?;
    Ok(Redirect::to("/economique?secteur=abattoir").into_response())
}

pub(super) async fn auto_assign(pool: &SqlitePool) -> AppResult<u64> {
    let bands = bands(pool).await?;
    let mut tx = pool.begin().await?;
    let pending: Vec<(i64, i64, Option<String>)> =
        sqlx::query_as("SELECT id,lot_index,lot_ref FROM ventelot WHERE bande_id IS NULL")
            .fetch_all(&mut *tx)
            .await?;
    let mut count = 0;
    for (id, index, reference) in pending {
        if let (Some(band), _) = suggestion(reference.as_deref(), &bands) {
            assign(&mut tx, id, index, Some(band)).await?;
            count += 1;
        }
    }
    tx.commit().await?;
    Ok(count)
}

async fn rename_lot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    index: i64,
    form: &HashMap<String, String>,
) -> AppResult<()> {
    let Some(reference) = form.get("lot_ref") else {
        return Ok(());
    };
    let reference = reference.trim().to_uppercase();
    if reference.is_empty() || reference.len() > 80 {
        return Err(AppError::Invalid(
            "Numéro de lot obligatoire (80 caractères maximum).".into(),
        ));
    }
    let old: String =
        sqlx::query_scalar("SELECT COALESCE(lot_ref,'') FROM ventelot WHERE id=? AND lot_index=?")
            .bind(id)
            .bind(index)
            .fetch_one(&mut **tx)
            .await?;
    if normalize(&old) == normalize(&reference) {
        return Ok(());
    }
    let duplicate:i64=sqlx::query_scalar("SELECT COUNT(*) FROM ventelot WHERE num_apport=(SELECT num_apport FROM venteapport WHERE id=?) AND upper(trim(lot_ref))=? AND NOT(id=? AND lot_index=?)").bind(id).bind(&reference).bind(id).bind(index).fetch_one(&mut **tx).await?;
    if duplicate > 0 {
        return Err(AppError::Invalid(
            "Ce numéro de lot existe déjà dans cet apport.".into(),
        ));
    }
    if index < 0 {
        sqlx::query("UPDATE venteapport SET frappe=? WHERE id=?")
            .bind(&reference)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    } else {
        let raw: String = sqlx::query_scalar("SELECT lots_json FROM venteapport WHERE id=?")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        let mut lots: Vec<Value> =
            serde_json::from_str(&raw).map_err(|e| AppError::Internal(e.into()))?;
        let lot = lots.get_mut(index as usize).ok_or(AppError::NotFound)?;
        if lot["ref_originale"].is_null() {
            lot["ref_originale"] = json!(old);
        }
        lot["ref"] = json!(reference);
        sqlx::query("UPDATE venteapport SET lots_json=? WHERE id=?")
            .bind(json!(lots).to_string())
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub(super) async fn remove_apport(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let mut tx = state.pool.begin().await?;
    let reference: Option<String> =
        sqlx::query_scalar("SELECT num_apport FROM venteapport WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound)?;
    let ids: Vec<i64> = if let Some(reference) = reference.as_ref().filter(|r| !r.trim().is_empty())
    {
        sqlx::query_scalar("SELECT id FROM venteapport WHERE num_apport=?")
            .bind(reference)
            .fetch_all(&mut *tx)
            .await?
    } else {
        vec![id]
    };
    for sale in &ids {
        sqlx::query("DELETE FROM transfert WHERE vente_apport_id=?")
            .bind(sale)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM venteapport WHERE id=?")
            .bind(sale)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(reference) = reference.as_ref().filter(|r| !r.trim().is_empty()) {
        sqlx::query("DELETE FROM valorisationapport WHERE num_apport=?")
            .bind(reference)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "suppression",
        "apport",
        &format!(
            "Apport {} : {} lots et sorties annulés",
            reference.unwrap_or_default(),
            ids.len()
        ),
        "/economique",
    )
    .await;
    Ok(Redirect::to("/economique?secteur=abattoir").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn import_propose_deux_bandes_et_respecte_les_corrections() -> anyhow::Result<()> {
        use super::super::documents::tests::{session, state};
        let state = state().await;
        sqlx::query(
            "INSERT INTO bande(id,code,num_officiel) VALUES(1,'B1','LOT1'),(2,'B2','LOT2')",
        )
        .execute(&state.pool)
        .await?;
        sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,resume) VALUES('test','economique:apport','test.pdf','apercu',1,'{\"ajouter\":2}')").execute(&state.pool).await?;
        let lots = json!([{"ref":"LOT1","nb_porcs":10,"poids":1000,"montant_ht":2000},{"ref":"LOT2","nb_porcs":20,"poids":2000,"montant_ht":4000}]);
        for i in 1..=2 {
            let line = ImportLine {
                kind: "vente".into(),
                date: Some("2026-08-01".into()),
                reference: Some("AP-TEST".into()),
                label: format!("Lot LOT{i}"),
                quantity: Some((i * 10) as f64),
                unit_price: Some(2.0),
                amount: Some((i * 2000) as f64),
                details: json!({"frappe":format!("LOT{i}"),"nb_porcs":i*10,"poids_total":i*1000,"montant_ht":i*2000,"lots_json":lots}),
            };
            sqlx::query("INSERT INTO importligne(token,numero_ligne,action,donnees_json) VALUES('test',?,'ajouter',?)").bind(i).bind(serde_json::to_string(&line)?).execute(&state.pool).await?;
        }
        let Html(preview) = economique_import_apercu(
            State(state.clone()),
            Extension(session()),
            Path("test".into()),
        )
        .await?;
        assert!(preview.contains("value=\"1\" selected"));
        assert!(preview.contains("value=\"2\" selected"));
        assert!(preview.contains("LOT1"));
        assert!(preview.contains("LOT2"));
        // Une ligne explicitement laissée sans bande doit ignorer le défaut.
        let form = HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("bande_id".into(), "1".into()),
            ("bande_ligne_1".into(), "none".into()),
            ("bande_ligne_2".into(), "2".into()),
        ]);
        economique_import_confirmer(
            State(state.clone()),
            Extension(session()),
            Path("test".into()),
            Form(form),
        )
        .await?;
        let rows: Vec<(Option<i64>, i64, f64)> = sqlx::query_as(
            "SELECT bande_id,nb_porcs,CAST(montant_ht AS REAL) FROM ventelot ORDER BY id",
        )
        .fetch_all(&state.pool)
        .await?;
        assert_eq!(rows, vec![(None, 10, 2000.0), (Some(2), 20, 4000.0)]);
        let mut tx = state.pool.begin().await?;
        assign(&mut tx, 1, -1, Some(1)).await?;
        assign(&mut tx, 1, -1, Some(1)).await?;
        tx.commit().await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT SUM(nombre) FROM transfert WHERE vente_apport_id IS NOT NULL",
        )
        .fetch_one(&state.pool)
        .await?;
        assert_eq!(total, 30);
        let Html(page) = economique(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([("secteur".into(), "abattoir".into())])),
        )
        .await?;
        assert!(page.contains("Bande du lot"));
        assert!(page.contains("B1"));
        assert!(page.contains("B2"));
        Ok(())
    }
    #[tokio::test]
    async fn reclassement_et_suppression_restituent_les_effectifs() -> anyhow::Result<()> {
        use super::super::documents::tests::{session, state};
        let state = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'A'),(2,'B'); INSERT INTO site(id,code) VALUES(1,'S'); INSERT INTO salle(id,site_id,nom) VALUES(1,1,'R'); INSERT INTO casesalle(id,salle_id,nom) VALUES(1,1,'C1'),(2,1,'C2'); INSERT INTO transfert(date,bande_id,case_dest_id,nombre) VALUES('2026-01-01',1,1,50),('2026-01-01',2,2,40); INSERT INTO venteapport(id,date,num_apport,frappe,nb_porcs,montant_ht) VALUES(1,'2026-02-01','AP','ERREUR',10,2000),(2,'2026-02-01','AP','LOT2',5,1000);").execute(&state.pool).await?;
        let mut tx = state.pool.begin().await?;
        assign(&mut tx, 1, -1, Some(1)).await?;
        assign(&mut tx, 2, -1, Some(2)).await?;
        tx.commit().await?;
        let form = HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("bande_id".into(), "2".into()),
            ("lot_ref".into(), "LOT1".into()),
        ]);
        direct(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(form),
        )
        .await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transfert WHERE vente_apport_id=1 AND bande_id=1",
        )
        .fetch_one(&state.pool)
        .await?;
        assert_eq!(count, 0);
        let sold: i64 = sqlx::query_scalar(
            "SELECT SUM(nombre) FROM transfert WHERE vente_apport_id IS NOT NULL AND bande_id=2",
        )
        .fetch_one(&state.pool)
        .await?;
        assert_eq!(sold, 15);
        let reference: String = sqlx::query_scalar("SELECT frappe FROM venteapport WHERE id=1")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(reference, "LOT1");
        assert!(remove_apport(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(HashMap::new())
        )
        .await
        .is_err());
        remove_apport(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(HashMap::from([("csrf_token".into(), "test".into())])),
        )
        .await?;
        let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(sales, 0);
        let out: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transfert WHERE vente_apport_id IS NOT NULL")
                .fetch_one(&state.pool)
                .await?;
        assert_eq!(out, 0);
        let stock:i64=sqlx::query_scalar("SELECT SUM(CASE WHEN case_dest_id=2 THEN nombre ELSE -nombre END) FROM transfert WHERE case_source_id=2 OR case_dest_id=2").fetch_one(&state.pool).await?;
        assert_eq!(stock, 40);
        Ok(())
    }
    #[test]
    fn correspondance_exacte_et_ambiguites() {
        let bands = vec![
            json!({"id":1,"code":"B1.01","num_officiel":"DA915"}),
            json!({"id":2,"code":"B2.01","num_officiel":"DA9"}),
        ];
        assert_eq!(suggestion(Some(" da915 "), &bands).0, Some(1));
        assert_eq!(suggestion(Some("B2.01"), &bands).0, Some(2));
        assert_eq!(suggestion(Some("DA"), &bands).0, None);
        let mut repeated = bands.clone();
        repeated.push(json!({"id":3,"num_officiel":"DA915"}));
        assert_eq!(suggestion(Some("DA915"), &repeated).0, None);
        assert_eq!(suggestion(None, &bands).0, None);
    }
    #[tokio::test]
    async fn ventilation_multilot_et_mouvements_sans_doublon() -> anyhow::Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0002_ventelot.sql"))
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO bande(id,code) VALUES(1,'B1'),(2,'B2')")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO venteapport(id,date,num_apport,nb_porcs,poids_total,montant_ht,lots_json) VALUES(1,'2026-08-01','TEST',30,3000,6000,?)").bind(json!([{"ref":"A","nb_porcs":10,"poids":1000,"montant_ht":2000},{"ref":"B","nb_porcs":20,"poids":2000,"montant_ht":4000}]).to_string()).execute(&pool).await?;
        let mut tx = pool.begin().await?;
        assign(&mut tx, 1, 0, Some(1)).await?;
        assign(&mut tx, 1, 1, Some(2)).await?;
        assign(&mut tx, 1, 1, Some(2)).await?;
        assert!(assign(&mut tx, 1, 1, Some(999)).await.is_err());
        tx.commit().await?;
        let totals:Vec<(i64,i64,f64)>=sqlx::query_as("SELECT bande_id,SUM(nb_porcs),CAST(SUM(montant_ht) AS REAL) FROM ventelot GROUP BY bande_id ORDER BY bande_id").fetch_all(&pool).await?;
        assert_eq!(totals, vec![(1, 10, 2000.0), (2, 20, 4000.0)]);
        let moved: i64 =
            sqlx::query_scalar("SELECT SUM(nombre) FROM transfert WHERE vente_apport_id=1")
                .fetch_one(&pool)
                .await?;
        assert_eq!(moved, 30);
        let mut tx = pool.begin().await?;
        assign(&mut tx, 1, 0, None).await?;
        tx.commit().await?;
        let moved: i64 =
            sqlx::query_scalar("SELECT SUM(nombre) FROM transfert WHERE vente_apport_id=1")
                .fetch_one(&pool)
                .await?;
        assert_eq!(moved, 20);
        Ok(())
    }
}
