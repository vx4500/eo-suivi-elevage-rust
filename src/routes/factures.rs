use super::*;

pub(super) async fn contexte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((category, id)): Path<(String, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let (table, sector) = match category.as_str() {
        "aliment" => ("livraisonaliment", "aliment"),
        "veto" => ("achatveto", "veterinaire"),
        _ => return Err(AppError::NotFound),
    };
    let date = form_date(&form, "date_reference")?
        .ok_or_else(|| AppError::Invalid("Date de référence obligatoire.".into()))?;
    let sites = site_ids(&state.pool, &form, "site_").await?;
    if category == "aliment" && sites.is_empty() {
        return Err(AppError::Invalid(
            "Choisissez au moins un site de livraison.".into(),
        ));
    }
    let stage = form
        .get("stade_aliment")
        .map(String::as_str)
        .unwrap_or("auto");
    if category == "aliment" && !crate::affectation::valid_stage(stage) {
        return Err(AppError::Invalid("Stade d'aliment invalide.".into()));
    }
    let mut tx = state.pool.begin().await?;
    let result=sqlx::query(&format!("UPDATE {table} SET date_reference=?,sites_json=?,site=NULL,bande_id=NULL,bandes=NULL WHERE id=?")).bind(date).bind(json!(sites).to_string()).bind(id).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM affectationfacturebande WHERE categorie=? AND facture_id=?")
        .bind(&category)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM affectationfacturecontrole WHERE categorie=? AND facture_id=?")
        .bind(&category)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if category == "aliment" {
        sqlx::query("UPDATE livraisonaliment SET stade_aliment=? WHERE id=?")
            .bind(stage)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    db::auto_assign_economic_invoices(&state.pool).await?;
    Ok(Redirect::to(&format!("/economique?secteur={sector}")).into_response())
}
pub(super) async fn site_ids(
    pool: &SqlitePool,
    form: &HashMap<String, String>,
    prefix: &str,
) -> AppResult<Vec<i64>> {
    let mut ids = Vec::new();
    for key in form.keys().filter(|key| key.starts_with(prefix)) {
        let id = key[prefix.len()..]
            .parse::<i64>()
            .map_err(|_| AppError::Invalid("Site invalide.".into()))?;
        if sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM site WHERE id=?")
            .bind(id)
            .fetch_one(pool)
            .await?
            != 1
        {
            return Err(AppError::Invalid("Site inconnu.".into()));
        }
        ids.push(id);
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}
pub(super) async fn ht(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let amount = form_f64(&form, "montant_ht")
        .filter(|x| x.is_finite())
        .ok_or_else(|| {
            AppError::Invalid("Montant HT valide obligatoire ; un avoir est négatif.".into())
        })?;
    if sqlx::query("UPDATE achatgenetique SET montant_ht=?,ht_manuel=1 WHERE id=?")
        .bind(amount)
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound);
    }
    db::journal(
        &state.pool,
        &session.identifiant,
        "correction HT",
        "genetique",
        &format!("Facture {id} : {amount:.2} HT"),
        "/economique",
    )
    .await;
    Ok(Redirect::to("/economique?secteur=genetique").into_response())
}
pub(super) fn suggest_sites(destination: Option<&str>, sites: &[Value]) -> String {
    let normalize = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_uppercase()
    };
    let destination = format!(" {} ", normalize(destination.unwrap_or_default()));
    let ids: Vec<String> = sites
        .iter()
        .filter(|s| {
            ["code", "nom"].iter().any(|key| {
                s[*key].as_str().is_some_and(|name| {
                    let name = normalize(name);
                    name.len() >= 4 && destination.contains(&format!(" {name} "))
                })
            })
        })
        .filter_map(|s| s["id"].as_i64().map(|id| id.to_string()))
        .collect();
    // Une adresse ambiguë n'est pas assimilée à une livraison multisite.
    if ids.len() == 1 {
        ids.join(",")
    } else {
        String::new()
    }
}

pub(super) async fn remove(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((category, id)): Path<(String, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let (table, sector) = match category.as_str() {
        "genetique" => ("achatgenetique", "genetique"),
        "semence" => ("achatsemence", "semence"),
        "aliment" => ("livraisonaliment", "aliment"),
        "veto" => ("achatveto", "veterinaire"),
        _ => return Err(AppError::NotFound),
    };
    let mut tx = state.pool.begin().await?;
    let (reference, supplier): (Option<String>, String) = sqlx::query_as(&format!(
        "SELECT num_facture,COALESCE(fournisseur,'') FROM {table} WHERE id=?"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    let ids: Vec<i64> = if let Some(r) = reference.as_deref().filter(|s| !s.trim().is_empty()) {
        sqlx::query_scalar(&format!(
            "SELECT id FROM {table} WHERE num_facture=? AND COALESCE(fournisseur,'')=?"
        ))
        .bind(r)
        .bind(&supplier)
        .fetch_all(&mut *tx)
        .await?
    } else {
        vec![id]
    };
    for row in &ids {
        sqlx::query("DELETE FROM affectationfacturebande WHERE categorie=? AND facture_id=?")
            .bind(&category)
            .bind(row)
            .execute(&mut *tx)
            .await?;
        remove_assignment_control(&mut tx, &category, *row).await?;
        sqlx::query(&format!("DELETE FROM {table} WHERE id=?"))
            .bind(row)
            .execute(&mut *tx)
            .await?;
    }
    let remaining: i64 =
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE num_facture=?"))
            .bind(&reference)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(reference) = reference
        .as_deref()
        .filter(|r| !r.trim().is_empty() && remaining == 0)
    {
        // Conserver la trace, mais autoriser une nouvelle importation du PDF
        // lorsque toutes les lignes du document ont été supprimées.
        sqlx::query("UPDATE importjournal SET statut='supprime',contenu_sha256=NULL WHERE statut='applique' AND type_import LIKE 'economique:%' AND EXISTS(SELECT 1 FROM importligne l WHERE l.token=importjournal.token AND json_valid(l.donnees_json) AND json_extract(l.donnees_json,'$.kind')=? AND json_extract(l.donnees_json,'$.reference')=?) AND NOT EXISTS(SELECT 1 FROM importligne l WHERE l.token=importjournal.token AND (NOT json_valid(l.donnees_json) OR COALESCE(json_extract(l.donnees_json,'$.kind'),'')<>? OR COALESCE(json_extract(l.donnees_json,'$.reference'),'')<>?))").bind(&category).bind(reference).bind(&category).bind(reference).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "suppression facture",
        &category,
        &format!(
            "{} : {} ligne(s)",
            reference.unwrap_or_else(|| id.to_string()),
            ids.len()
        ),
        "/economique",
    )
    .await;
    Ok(Redirect::to(&format!("/economique?secteur={sector}")).into_response())
}
async fn remove_assignment_control(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    category: &str,
    id: i64,
) -> AppResult<()> {
    sqlx::query("DELETE FROM affectationfacturecontrole WHERE categorie=? AND facture_id=?")
        .bind(category)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::documents::tests::{session, state};

    #[tokio::test]
    async fn import_repetitions_sites_et_non_affectation() -> anyhow::Result<()> {
        let s = state().await;
        sqlx::raw_sql("INSERT INTO site(id,code) VALUES(1,'S1'),(2,'S2'); INSERT INTO salle(id,site_id,nom) VALUES(1,1,'A'),(2,2,'B'); INSERT INTO bande(id,code) VALUES(1,'B1'),(2,'B2'); INSERT INTO transfert(date,bande_id,salle_dest_id,nombre) VALUES('2026-01-01',1,1,10),('2026-01-01',2,2,20); INSERT INTO importjournal(token,type_import,cree_par) VALUES('test','economique:aliment',1);").execute(&s.pool).await?;
        for n in 1..=2 {
            let line = ImportLine {
                kind: "aliment".into(),
                date: Some("2026-02-01".into()),
                reference: Some("FACT1".into()),
                label: "ALIMENT".into(),
                quantity: Some(1.0),
                unit_price: Some(100.0),
                amount: Some(100.0),
                details: json!({"produit":"ALIMENT","source_ligne":n,"tonnage":1,"pu_ht":100}),
            };
            sqlx::query("INSERT INTO importligne(token,numero_ligne,action,donnees_json) VALUES('test',?,'ajouter',?)").bind(n).bind(serde_json::to_string(&line)?).execute(&s.pool).await?;
        }
        let form = HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("site_ligne_1_1".into(), "on".into()),
            ("stade_aliment_1".into(), "tous".into()),
            ("site_ligne_1_2".into(), "on".into()),
            ("bande_ligne_2".into(), "none".into()),
        ]);
        economique_import_confirmer(
            State(s.clone()),
            Extension(session()),
            Path("test".into()),
            Form(form),
        )
        .await?;
        let total: (i64, f64) =
            sqlx::query_as("SELECT COUNT(*),SUM(montant_ht) FROM livraisonaliment")
                .fetch_one(&s.pool)
                .await?;
        assert_eq!(total, (2, 200.0));
        let assignments: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT facture_id,bande_id FROM affectationfacturebande ORDER BY facture_id,bande_id",
        )
        .fetch_all(&s.pool)
        .await?;
        assert_eq!(assignments, vec![(1, 1), (1, 2)]);
        let form = HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("date_reference".into(), "2026-02-01".into()),
            ("site_2".into(), "on".into()),
            ("stade_aliment".into(), "tous".into()),
        ]);
        contexte(
            State(s.clone()),
            Extension(session()),
            Path(("aliment".into(), 1)),
            Form(form),
        )
        .await?;
        let bands:Vec<i64>=sqlx::query_scalar("SELECT bande_id FROM affectationfacturebande WHERE facture_id=1 AND categorie='aliment'").fetch_all(&s.pool).await?;
        assert_eq!(bands, vec![2]);
        Ok(())
    }
    #[tokio::test]
    async fn ht_reparation_prouvee_et_correction_manuelle() -> anyhow::Result<()> {
        let s = state().await;
        sqlx::raw_sql("INSERT INTO achatgenetique(id,num_facture,montant_ht) VALUES(1,'GEN1',120),(2,'GEN2',120); INSERT INTO importjournal(token,type_import,statut) VALUES('gen','economique:genetique','applique');").execute(&s.pool).await?;
        for n in 1..=2 {
            let line = ImportLine {
                kind: "genetique".into(),
                date: None,
                reference: Some(format!("GEN{n}")),
                label: "GEN".into(),
                quantity: None,
                unit_price: None,
                amount: Some(100.0),
                details: json!({"montant_ht":100.0,"montant_net":120.0}),
            };
            sqlx::query("INSERT INTO importligne(token,numero_ligne,action,donnees_json) VALUES('gen',?,'ajouter',?)").bind(n).bind(serde_json::to_string(&line)?).execute(&s.pool).await?;
        }
        ht(
            State(s.clone()),
            Extension(session()),
            Path(2),
            Form(HashMap::from([
                ("csrf_token".into(), "test".into()),
                ("montant_ht".into(), "120".into()),
            ])),
        )
        .await?;
        assert_eq!(db::repair_genetic_ht(&s.pool).await?, 1);
        assert_eq!(db::repair_genetic_ht(&s.pool).await?, 0);
        let amounts: Vec<f64> =
            sqlx::query_scalar("SELECT montant_ht FROM achatgenetique ORDER BY id")
                .fetch_all(&s.pool)
                .await?;
        assert_eq!(amounts, vec![100.0, 120.0]);
        Ok(())
    }
    #[tokio::test]
    async fn suppression_facture_complete_et_affectations() -> anyhow::Result<()> {
        let s = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'); INSERT INTO livraisonaliment(id,num_facture,montant_ht) VALUES(1,'F1',100),(2,'F1',50),(3,'F2',20); INSERT INTO affectationfacturebande(categorie,facture_id,bande_id) VALUES('aliment',1,1),('aliment',2,1); INSERT INTO affectationfacturecontrole(categorie,facture_id,verrou_manuel) VALUES('aliment',1,1);").execute(&s.pool).await?;
        sqlx::query("INSERT INTO importjournal(token,type_import,statut,contenu_sha256) VALUES('deleted','economique:aliment','applique','digest')").execute(&s.pool).await?;
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,donnees_json) VALUES('deleted',1,'ajouter',?)").bind(json!({"kind":"aliment","reference":"F1"}).to_string()).execute(&s.pool).await?;
        assert!(remove(
            State(s.clone()),
            Extension(session()),
            Path(("aliment".into(), 1)),
            Form(HashMap::new())
        )
        .await
        .is_err());
        remove(
            State(s.clone()),
            Extension(session()),
            Path(("aliment".into(), 1)),
            Form(HashMap::from([("csrf_token".into(), "test".into())])),
        )
        .await?;
        let rows: Vec<i64> = sqlx::query_scalar("SELECT id FROM livraisonaliment")
            .fetch_all(&s.pool)
            .await?;
        assert_eq!(rows, vec![3]);
        let journal: (String, Option<String>) =
            sqlx::query_as("SELECT statut,contenu_sha256 FROM importjournal WHERE token='deleted'")
                .fetch_one(&s.pool)
                .await?;
        assert_eq!(journal, ("supprime".into(), None));

        let n:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM affectationfacturebande)+(SELECT COUNT(*) FROM affectationfacturecontrole)").fetch_one(&s.pool).await?;
        assert_eq!(n, 0);
        Ok(())
    }
    #[tokio::test]
    async fn suppression_ne_touche_pas_un_autre_fournisseur() -> anyhow::Result<()> {
        let s = state().await;
        sqlx::query("INSERT INTO achatgenetique(id,num_facture,fournisseur,montant_ht) VALUES(1,'001','A',100),(2,'001','B',200)").execute(&s.pool).await?;
        remove(
            State(s.clone()),
            Extension(session()),
            Path(("genetique".into(), 1)),
            Form(HashMap::from([("csrf_token".into(), "test".into())])),
        )
        .await?;
        let rows: Vec<i64> = sqlx::query_scalar("SELECT id FROM achatgenetique")
            .fetch_all(&s.pool)
            .await?;
        assert_eq!(rows, vec![2]);
        Ok(())
    }
    #[test]
    fn suggestion_site_unique_seulement() {
        let sites = vec![
            json!({"id":1,"code":"BERUE"}),
            json!({"id":2,"nom":"MELTIERE"}),
        ];
        assert_eq!(suggest_sites(Some("Livré chez ORY LA BERUE"), &sites), "1");
        assert_eq!(suggest_sites(Some("BERUE MELTIERE"), &sites), "");
        assert_eq!(suggest_sites(Some("INCONNU"), &sites), "");
    }
}
