use sqlx::SqlitePool;

/// Présence historique prouvée par les entrées/sorties, par espèce et site.
/// Une bande.active actuelle ou une date de mise-bas ne prouve pas sa présence passée.
pub async fn present(pool: &SqlitePool, date: &str, sites: &[i64]) -> anyhow::Result<Vec<i64>> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    let json = serde_json::to_string(sites)?;
    Ok(sqlx::query_scalar(r#"WITH mouvements AS (
SELECT t.bande_id,t.espece,t.nombre AS n,sd.site_id FROM transfert t LEFT JOIN casesalle cd ON cd.id=t.case_dest_id JOIN salle sd ON sd.id=COALESCE(cd.salle_id,t.salle_dest_id) WHERE date(t.date)<=date(?)
UNION ALL
SELECT t.bande_id,t.espece,-t.nombre,ss.site_id FROM transfert t LEFT JOIN casesalle cs ON cs.id=t.case_source_id JOIN salle ss ON ss.id=COALESCE(cs.salle_id,t.salle_source_id) WHERE date(t.date)<=date(?)
UNION ALL
SELECT b.id,'porc',-d.nombre,s.site_id FROM declarationmort d JOIN bande b ON b.code=d.bande_code JOIN casesalle c ON c.id=d.case_id JOIN salle s ON s.id=c.salle_id WHERE date(d.date)<=date(?)
), presents AS (SELECT bande_id,site_id,espece FROM mouvements WHERE bande_id IS NOT NULL GROUP BY bande_id,site_id,espece HAVING SUM(COALESCE(n,0))>0)
SELECT DISTINCT bande_id FROM presents WHERE json_array_length(?)=0 OR site_id IN(SELECT value FROM json_each(?)) ORDER BY bande_id"#).bind(date).bind(date).bind(date).bind(&json).bind(&json).fetch_all(pool).await?)
}

pub async fn selected_sites(
    pool: &SqlitePool,
    raw: &str,
    legacy: Option<&str>,
) -> anyhow::Result<Vec<i64>> {
    let mut ids: Vec<i64> = serde_json::from_str(raw).unwrap_or_default();
    if ids.is_empty() {
        if let Some(name) = legacy.filter(|s| !s.trim().is_empty()) {
            ids=sqlx::query_scalar("SELECT id FROM site WHERE lower(trim(code))=lower(trim(?)) OR lower(trim(nom))=lower(trim(?))").bind(name).bind(name).fetch_all(pool).await?;
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

pub fn feed_stage(product: &str) -> &'static str {
    let p = product
        .to_uppercase()
        .replace(['É', 'È', 'Ê', 'Ë'], "E")
        .replace(['À', 'Â'], "A");
    let p = p
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let stages = [
        ("gestation", p.contains("GESTA")),
        ("lactation", p.contains("LACTA")),
        ("croissance", p.contains("CROISSANCE")),
        ("finition", p.contains("FINITION")),
        (
            "ps",
            p.contains("POST SEVRAGE")
                || p.contains("2EME AGE")
                || p.contains("2E AGE")
                || p.split_whitespace().any(|word| word == "PS"),
        ),
    ];
    let found: Vec<_> = stages.into_iter().filter(|(_, yes)| *yes).collect();
    if found.len() == 1 {
        found[0].0
    } else {
        "inconnu"
    }
}
pub fn valid_stage(stage: &str) -> bool {
    matches!(
        stage,
        "auto" | "gestation" | "lactation" | "ps" | "croissance" | "finition" | "tous" | "inconnu"
    )
}
fn stage_matches(stage: &str, age: i64, s: crate::routes::BandSchedule) -> bool {
    match stage {
        "gestation" => (-s.gestation..-s.maternity_before_farrowing).contains(&age),
        "lactation" => (-s.maternity_before_farrowing..s.weaning).contains(&age),
        "ps" => (s.weaning..s.transfer_finishing).contains(&age),
        "croissance" => (s.transfer_finishing..s.finishing_feed).contains(&age),
        "finition" => (s.finishing_feed..s.departure).contains(&age),
        "tous" => true,
        _ => false,
    }
}
/// Les mouvements priment ; à défaut, le cycle daté et le site de la bande
/// donnent une présence prévisionnelle. Aucun rapprochement entre sites par similarité.
pub async fn cycle_present(
    pool: &SqlitePool,
    date: &str,
    sites: &[i64],
    stage: &str,
) -> anyhow::Result<Vec<i64>> {
    let day = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
    let schedule = crate::routes::load_band_schedule(pool).await?;
    let mut result = present(pool, date, sites).await?;
    let rows:Vec<(i64,Option<String>,i64,i64)>=sqlx::query_as("SELECT b.id,b.date_mb,EXISTS(SELECT 1 FROM transfert t WHERE t.bande_id=b.id AND date(t.date)<=date(?)),EXISTS(SELECT 1 FROM site s WHERE s.id IN(SELECT value FROM json_each(?)) AND (lower(trim(b.site))=lower(trim(s.code)) OR lower(trim(b.site))=lower(trim(s.nom)))) FROM bande b").bind(date).bind(serde_json::to_string(sites)?).fetch_all(pool).await?;
    for (id, mb, has_history, site_matches) in rows {
        let age = mb
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .map(|d| (day - d).num_days());
        if has_history == 0
            && (sites.is_empty() || site_matches == 1)
            && age.is_some_and(|a| (-schedule.gestation..schedule.departure).contains(&a))
        {
            result.push(id);
        }
        if stage != "tous" && !age.is_some_and(|a| stage_matches(stage, a, schedule)) {
            result.retain(|x| *x != id);
        }
    }
    result.sort_unstable();
    result.dedup();
    Ok(result)
}
pub async fn boars(pool: &SqlitePool) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM affectationfacturebande WHERE categorie='genetique' AND facture_id IN(SELECT id FROM achatgenetique WHERE toutes_bandes=1)").execute(&mut *tx).await?;
    let changed=sqlx::query("INSERT INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) SELECT 'genetique',g.id,b.id,1 FROM achatgenetique g CROSS JOIN bande b WHERE g.toutes_bandes=1").execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    Ok(changed)
}
pub async fn refresh(pool: &SqlitePool, category: &str, table: &str) -> anyhow::Result<u64> {
    let extra = if category == "aliment" {
        "stade_aliment"
    } else {
        "'tous'"
    };
    type InvoiceContext = (
        i64,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
    );
    let rows:Vec<InvoiceContext>=sqlx::query_as(&format!("SELECT id,COALESCE(NULLIF(trim(date_reference),''),trim(date)),sites_json,site,produit,{extra} FROM {table} x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturecontrole c WHERE c.categorie=? AND c.facture_id=x.id AND c.verrou_manuel=1) AND NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie=? AND a.facture_id=x.id AND a.automatique=0)")).bind(category).bind(category).fetch_all(pool).await?;
    let mut count = 0;
    for (id, date, raw, site, product, stage) in rows {
        let sites = selected_sites(pool, &raw, site.as_deref()).await?;
        let stage = if stage == "auto" {
            feed_stage(product.as_deref().unwrap_or_default())
        } else {
            &stage
        };
        let bands = if let Some(date) =
            date.filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        {
            if category == "aliment" && (sites.is_empty() || stage == "inconnu") {
                vec![]
            } else {
                cycle_present(pool, &date, &sites, stage).await?
            }
        } else {
            vec![]
        };
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM affectationfacturebande WHERE categorie=? AND facture_id=? AND automatique=1").bind(category).bind(id).execute(&mut *tx).await?;
        for band in bands {
            count+=sqlx::query("INSERT OR IGNORE INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) VALUES(?,?,?,1)").bind(category).bind(id).bind(band).execute(&mut *tx).await?.rows_affected();
        }
        tx.commit().await?;
    }
    Ok(count)
}

/// Explique les prérequis manquants sans modifier les choix de l'éleveur.
pub async fn explain_unassigned(
    pool: &SqlitePool,
    category: &str,
    invoices: &mut [serde_json::Value],
) -> anyhow::Result<()> {
    let locked: Vec<i64> = sqlx::query_scalar(
        "SELECT facture_id FROM affectationfacturecontrole WHERE categorie=? AND verrou_manuel=1",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    for invoice in invoices {
        let sites = selected_sites(
            pool,
            invoice["sites_json"].as_str().unwrap_or("[]"),
            invoice["site"].as_str(),
        )
        .await?;
        // Les anciens sites textuels doivent aussi être cochés dans le formulaire.
        invoice["sites_ids"] = serde_json::json!(sites
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(","));
        if !invoice["bandes_ids"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
        {
            continue;
        }
        let mut reasons = Vec::new();
        if invoice["id"]
            .as_i64()
            .is_some_and(|id| locked.contains(&id))
        {
            reasons.push("Sans bande par choix manuel. Le recalcul automatique conserve ce choix.");
        } else {
            if invoice["date_reference"]
                .as_str()
                .is_none_or(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
            {
                reasons.push("Date manquante ou invalide : renseignez la date de référence.");
            }
            if category == "aliment" {
                if sites.is_empty() {
                    reasons.push(
                        "Site de livraison manquant ou non reconnu : cochez le site concerné.",
                    );
                }
                let stage = invoice["stade_aliment"].as_str().unwrap_or("auto");
                if stage == "inconnu"
                    || (stage == "auto"
                        && feed_stage(invoice["produit"].as_str().unwrap_or_default()) == "inconnu")
                {
                    reasons.push(
                        "Stade d’aliment non reconnu : choisissez la destination de l’aliment.",
                    );
                }
            }
            if reasons.is_empty() {
                reasons.push("Aucune affectation enregistrée : vérifiez les sites, les dates de cycle et les mouvements des bandes, puis relancez le recalcul.");
            }
        }
        invoice["affectation_message"] = serde_json::json!(reasons.join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn libelles_post_sevrage_et_ambiguite() {
        for label in [
            "POST-SEVRAGE",
            "Post–sevrage",
            "POST  SEVRAGE",
            "2ème âge",
            "2e âge",
            "ALIMENT PS",
        ] {
            assert_eq!(feed_stage(label), "ps", "{label}");
        }
        for label in ["ACTI PLUS B", "PS FINITION", "GESTA LACTA", "CAPSULE"] {
            assert_eq!(feed_stage(label), "inconnu", "{label}");
        }
    }

    #[tokio::test]
    async fn date_reference_vide_et_diagnostic_sans_ecraser_choix_manuel() -> anyhow::Result<()> {
        let state = crate::routes::documents::tests::state().await;
        let p = &state.pool;
        sqlx::raw_sql("INSERT INTO site(id,code) VALUES(1,'S1'); INSERT INTO bande(id,code,site,date_mb) VALUES(1,'PS','S1','2026-05-15'); INSERT INTO livraisonaliment(id,date,date_reference,site,produit) VALUES(1,'2026-07-01','  ','S1','2e âge'),(2,'2026-07-01',NULL,NULL,'ACTI PLUS B'),(3,'2026-07-01',NULL,'S1','2e âge'); INSERT INTO affectationfacturecontrole(categorie,facture_id,verrou_manuel) VALUES('aliment',3,1);").execute(p).await?;
        for _ in 0..2 {
            crate::db::auto_assign_economic_invoices(p).await?;
            let assigned: Vec<(i64, i64)> = sqlx::query_as("SELECT facture_id,bande_id FROM affectationfacturebande WHERE categorie='aliment' ORDER BY facture_id").fetch_all(p).await?;
            assert_eq!(assigned, vec![(1, 1)]);
        }
        let mut invoices = vec![
            serde_json::json!({"id":1,"site":"S1","bandes_ids":"1"}),
            serde_json::json!({"id":2,"date_reference":"2026-07-01","stade_aliment":"auto","produit":"ACTI PLUS B"}),
            serde_json::json!({"id":3,"site":"S1"}),
        ];
        explain_unassigned(p, "aliment", &mut invoices).await?;
        assert_eq!(invoices[0]["sites_ids"], "1");
        assert!(invoices[0]["affectation_message"].is_null());
        let message = invoices[1]["affectation_message"].as_str().unwrap();
        assert!(message.contains("Site de livraison manquant"));
        assert!(message.contains("Stade d’aliment non reconnu"));
        assert!(invoices[2]["affectation_message"]
            .as_str()
            .unwrap()
            .contains("choix manuel"));
        Ok(())
    }

    #[tokio::test]
    async fn stades_et_sites_selon_le_cycle() -> anyhow::Result<()> {
        let state = crate::routes::documents::tests::state().await;
        let p = &state.pool;
        sqlx::raw_sql("INSERT INTO site(id,code,nom) VALUES(1,'A','Site A'),(2,'B','Site B'); INSERT INTO bande(id,code,site,date_mb) VALUES(1,'GEST','A','2026-08-01'),(2,'LACT','A','2026-06-20'),(3,'PS','A','2026-05-15'),(4,'CROISS','A','2026-04-01'),(5,'FIN','A','2026-01-15'),(6,'AUTRESITE','B','2026-08-01');").execute(p).await?;
        for (product, id) in [
            ("GESTA PLUS FE", 1),
            ("LACTA SAFE FE", 2),
            ("POST SEVRAGE", 3),
            ("MULTI BE CROISSANCE B", 4),
            ("MULTI BE FINITION C", 5),
        ] {
            assert_eq!(
                cycle_present(p, "2026-07-01", &[1], feed_stage(product)).await?,
                vec![id]
            );
        }
        assert_eq!(
            cycle_present(p, "2026-07-01", &[1, 2], "gestation").await?,
            vec![1, 6]
        );
        assert_eq!(
            cycle_present(p, "2026-07-01", &[], "tous").await?,
            vec![1, 2, 3, 4, 5, 6]
        );
        assert!(
            cycle_present(p, "2026-07-01", &[1], feed_stage("ACTI PLUS B"))
                .await?
                .is_empty()
        );
        sqlx::query("INSERT OR REPLACE INTO reglage(cle,valeur,libelle) VALUES('aliment_finition',80,'Finition')")
            .execute(p)
            .await?;
        assert_eq!(
            cycle_present(p, "2026-07-01", &[1], "finition").await?,
            vec![4, 5]
        );
        // Un départ enregistré prime sur le calendrier prévisionnel.
        sqlx::raw_sql("INSERT INTO salle(id,site_id,nom) VALUES(1,1,'R'); INSERT INTO transfert(date,bande_id,salle_dest_id,nombre) VALUES('2026-06-01',4,1,10); INSERT INTO transfert(date,bande_id,salle_source_id,nombre) VALUES('2026-06-30',4,1,10);").execute(p).await?;
        assert_eq!(
            cycle_present(p, "2026-07-01", &[1], "finition").await?,
            vec![5]
        );
        Ok(())
    }
    #[tokio::test]
    async fn verrat_reparti_sans_multiplier_le_ht() -> anyhow::Result<()> {
        let state = crate::routes::documents::tests::state().await;
        let p = &state.pool;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'),(2,'B2'); INSERT INTO achatgenetique(id,num_facture,montant_ht,toutes_bandes) VALUES(1,'VERRAT',697.44,1);").execute(p).await?;
        crate::db::auto_assign_economic_invoices(p).await?;
        crate::db::auto_assign_economic_invoices(p).await?;
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM affectationfacturebande WHERE categorie='genetique'",
        )
        .fetch_one(p)
        .await?;
        assert_eq!(n, 2);
        sqlx::query("INSERT INTO bande(id,code) VALUES(3,'B3')")
            .execute(p)
            .await?;
        boars(p).await?;
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM affectationfacturebande WHERE categorie='genetique'",
        )
        .fetch_one(p)
        .await?;
        assert_eq!(n, 3);
        let total:f64=sqlx::query_scalar("SELECT SUM(g.montant_ht/(SELECT COUNT(*) FROM affectationfacturebande z WHERE z.categorie='genetique' AND z.facture_id=g.id)) FROM achatgenetique g JOIN affectationfacturebande a ON a.categorie='genetique' AND a.facture_id=g.id").fetch_one(p).await?;
        assert!((total - 697.44).abs() < 0.001);
        Ok(())
    }
    #[tokio::test]
    async fn plusieurs_sites_presence_historique_et_sortie() -> anyhow::Result<()> {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await?;
        sqlx::raw_sql("INSERT INTO site(id,code) VALUES(1,'S1'),(2,'S2'); INSERT INTO salle(id,site_id,nom) VALUES(1,1,'A'),(2,2,'B'); INSERT INTO bande(id,code) VALUES(1,'B1'),(2,'B2'),(3,'B3'); INSERT INTO transfert(date,bande_id,salle_dest_id,nombre) VALUES('2026-01-01',1,1,10),('2026-01-01',2,1,20),('2026-01-01',3,2,30); INSERT INTO transfert(date,bande_id,salle_source_id,salle_dest_id,nombre) VALUES('2026-02-01',1,1,2,10); INSERT INTO transfert(date,bande_id,salle_source_id,nombre) VALUES('2026-03-01',1,2,10);").execute(&p).await?;
        assert_eq!(present(&p, "2026-01-15", &[1]).await?, vec![1, 2]);
        assert_eq!(present(&p, "2026-01-15", &[1, 2]).await?, vec![1, 2, 3]);
        assert_eq!(present(&p, "2026-02-15", &[1]).await?, vec![2]);
        assert_eq!(present(&p, "2026-02-15", &[2]).await?, vec![1, 3]);
        assert_eq!(present(&p, "2026-03-15", &[]).await?, vec![2, 3]);
        assert!(present(&p, "2025-12-31", &[]).await?.is_empty());
        sqlx::query("INSERT INTO livraisonaliment(id,date,montant_ht,sites_json,stade_aliment) VALUES(1,'2026-01-15',300,'[1,2]','tous'),(2,'2026-01-15',100,'[]','tous')").execute(&p).await?;
        crate::db::auto_assign_economic_invoices(&p).await?;
        crate::db::auto_assign_economic_invoices(&p).await?;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM affectationfacturebande WHERE categorie='aliment' AND facture_id=1 AND automatique=1").fetch_one(&p).await?;
        assert_eq!(count, 3);
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM affectationfacturebande WHERE categorie='aliment' AND facture_id=2").fetch_one(&p).await?;
        assert_eq!(count, 0);
        sqlx::raw_sql("INSERT INTO casesalle(id,salle_id,nom) VALUES(1,1,'C1'); INSERT INTO declarationmort(bande_code,date,case_id,nombre) VALUES('B2','2026-04-01',1,20);").execute(&p).await?;
        assert_eq!(present(&p, "2026-04-02", &[]).await?, vec![3]);
        Ok(())
    }
}
