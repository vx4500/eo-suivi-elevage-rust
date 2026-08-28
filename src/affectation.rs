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

pub async fn refresh(pool: &SqlitePool, category: &str, table: &str) -> anyhow::Result<u64> {
    let rows:Vec<(i64,Option<String>,String,Option<String>)>=sqlx::query_as(&format!("SELECT id,COALESCE(date_reference,date),sites_json,site FROM {table} x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturecontrole c WHERE c.categorie=? AND c.facture_id=x.id AND c.verrou_manuel=1) AND NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie=? AND a.facture_id=x.id AND a.automatique=0)")).bind(category).bind(category).fetch_all(pool).await?;
    let mut count = 0;
    for (id, date, raw, site) in rows {
        let sites = selected_sites(pool, &raw, site.as_deref()).await?;
        let bands = if let Some(date) =
            date.filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        {
            if category == "aliment" && sites.is_empty() {
                vec![]
            } else {
                present(pool, &date, &sites).await?
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

#[cfg(test)]
mod tests {
    use super::*;
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
        sqlx::query("INSERT INTO livraisonaliment(id,date,montant_ht,sites_json) VALUES(1,'2026-01-15',300,'[1,2]'),(2,'2026-01-15',100,'[]')").execute(&p).await?;
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
