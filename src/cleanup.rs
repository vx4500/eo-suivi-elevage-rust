use sqlx::{Row, SqlitePool};

fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Suppression ciblée des bandes fantômes v1.14. Chaque ligne touchée est
/// sauvegardée avant modification ; les événements à lien facultatif restent.
pub async fn historical_bands(pool: &SqlitePool) -> anyhow::Result<()> {
    let ghosts:Vec<(i64,String)>=sqlx::query_as("SELECT id,code FROM bande WHERE note LIKE 'Bande historique recréée automatiquement en v1.14%'").fetch_all(pool).await?;
    if ghosts.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name<>'archive_nettoyage'").fetch_all(&mut *tx).await?;
    for (band, code) in ghosts {
        // Ne jamais détacher une portée dont les transferts reposent sur la bande.
        let linked: i64=sqlx::query_scalar("SELECT COUNT(*) FROM adoptionporcelet a JOIN evenement e ON e.id=a.source_id OR e.id=a.destination_id WHERE e.bande_id=?").bind(band).fetch_one(&mut *tx).await?;
        if linked > 0 {
            tracing::warn!(bande=%code,"Bande historique conservée : des adoptions nécessitent encore ce lien");
            continue;
        }
        for table in &tables {
            let cols = sqlx::query(&format!("PRAGMA table_info({})", ident(table)))
                .fetch_all(&mut *tx)
                .await?;
            let fields = cols
                .iter()
                .map(|c| {
                    let n: String = c.get("name");
                    format!(
                        "'{}',CASE WHEN typeof({})='blob' THEN hex({}) ELSE {} END",
                        n.replace('\'', "''"),
                        ident(&n),
                        ident(&n),
                        ident(&n)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let mut changes = Vec::new();
            if table == "bande" {
                changes.push(("id".to_string(), false, true));
            }
            for c in &cols {
                let name: String = c.get("name");
                if name == "bande_id" || name == "bande_code" || name == "bandes" {
                    changes.push((name, c.get::<i64, _>("notnull") == 1, false));
                }
            }
            for (column, required, is_band) in changes {
                let numeric = column == "bande_id" || is_band;
                let predicate = if numeric {
                    format!("{}=?", ident(&column))
                } else {
                    format!(
                        "instr(','||replace(COALESCE({},''),' ','')||',',','||?||',')>0",
                        ident(&column)
                    )
                };
                let value = if numeric {
                    band.to_string()
                } else {
                    code.clone()
                };
                sqlx::query(&format!("INSERT OR IGNORE INTO archive_nettoyage(table_source,ligne_id,donnees,motif) SELECT ?,rowid,json_object({fields}),'Bandes fantômes v1.14' FROM {} WHERE {predicate}",ident(table))).bind(table).bind(&value).execute(&mut *tx).await?;
                if is_band {
                    continue;
                }
                if numeric {
                    let statement = if required {
                        format!("DELETE FROM {} WHERE {predicate}", ident(table))
                    } else {
                        format!(
                            "UPDATE {} SET {}=NULL WHERE {predicate}",
                            ident(table),
                            ident(&column)
                        )
                    };
                    sqlx::query(&statement)
                        .bind(&value)
                        .execute(&mut *tx)
                        .await?;
                } else {
                    let rows: Vec<(i64, String)> = sqlx::query_as(&format!(
                        "SELECT rowid,{} FROM {} WHERE {predicate}",
                        ident(&column),
                        ident(table)
                    ))
                    .bind(&value)
                    .fetch_all(&mut *tx)
                    .await?;
                    for (id, old) in rows {
                        let remaining = old
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty() && *s != code)
                            .collect::<Vec<_>>()
                            .join(",");
                        sqlx::query(&format!(
                            "UPDATE {} SET {}=? WHERE rowid=?",
                            ident(table),
                            ident(&column)
                        ))
                        .bind(if remaining.is_empty() && !required {
                            None
                        } else {
                            Some(remaining)
                        })
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    }
                }
            }
        }
        sqlx::query("DELETE FROM bande WHERE id=?")
            .bind(band)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn nettoyage_archive_sans_effacer_les_evenements() -> anyhow::Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        sqlx::raw_sql("INSERT INTO bande(id,code,note) VALUES(1,'20243801','Bande historique recréée automatiquement en v1.14 pour préserver les liens d’import.'),(2,'B1','Bande normale'); INSERT INTO truie(id,num_travail,bande_code) VALUES(1,'100','20243801'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs) VALUES(1,'mise_bas','2025-01-08',1,1,12); INSERT INTO compteur_energie(id,nom,type) VALUES(1,'Eau','eau'); INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index,bandes) VALUES(1,'2025-01-08',100,'20243801,B1');").execute(&pool).await?;
        historical_bands(&pool).await?;
        historical_bands(&pool).await?;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bande")
                .fetch_one(&pool)
                .await?,
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<i64>>("SELECT bande_id FROM evenement WHERE id=1")
                .fetch_one(&pool)
                .await?,
            None
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT bandes FROM releve_compteur")
                .fetch_one(&pool)
                .await?,
            "B1"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM archive_nettoyage")
                .fetch_one(&pool)
                .await?,
            4
        );
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await?
            .is_empty());
        Ok(())
    }
}
