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
