use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn schema_complet_et_ecritures_compatibles() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;

    // SQLite renvoie normalement un INTEGER pour COALESCE(SUM(...), 0)
    // lorsque la table est vide. Le CAST garantit le décodage Rust en f64.
    let empty_sales: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(empty_sales, 0.0);

    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(tables, 53);

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO truie(num_travail,statut,reformee,rang,mere_cochette) VALUES('TEST-RUST','active',0,0,0)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO evenement(type,date,suivi_actif) VALUES('chaleur','2026-08-16',0)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO controlequotidien(date,horodatage,categorie,statut,note) VALUES('2026-08-16',CURRENT_TIMESTAMP,'note_libre','ok','RAS')")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO clientventedirecte(nom,newsletter_email,newsletter_sms,cree_le,token_desinscription) VALUES('Test',0,0,CURRENT_TIMESTAMP,'test-token')")
        .execute(&mut *tx)
        .await?;
    let site = sqlx::query("INSERT INTO site(code,nom) VALUES('TEST','Site test')")
        .execute(&mut *tx).await?.last_insert_rowid();
    let room = sqlx::query("INSERT INTO salle(site_id,nom,nb_cases,ordre) VALUES(?,'Engraissement',1,1)")
        .bind(site).execute(&mut *tx).await?.last_insert_rowid();
    let pen = sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case 1',20)")
        .bind(room).execute(&mut *tx).await?.last_insert_rowid();
    let band = sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-TEST','2026-08-01',1)")
        .execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,libelle,type_saisie,est_stock) VALUES('2026-08-16','B-TEST',12,'stock porcs','inventaire',1)")
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO transfert(date,espece,bande_id,salle_dest_id,case_dest_id,nombre) VALUES('2026-08-16','porc',?,?,?,10)")
        .bind(band).bind(room).bind(pen).execute(&mut *tx).await?;
    let present: i64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(CASE WHEN case_dest_id=? THEN nombre ELSE -nombre END),0) AS INTEGER) FROM transfert WHERE espece='porc' AND (case_dest_id=? OR case_source_id=?)")
        .bind(pen).bind(pen).bind(pen).fetch_one(&mut *tx).await?;
    assert_eq!(present, 10);
    sqlx::query("INSERT INTO venteapport(date,bande_id,nb_porcs,poids_total,montant_net) VALUES('2026-08-16',?,10,900,1800)")
        .bind(band).execute(&mut *tx).await?;
    let price: f64 = sqlx::query_scalar("SELECT SUM(montant_net)/SUM(poids_total) FROM venteapport WHERE bande_id=?")
        .bind(band).fetch_one(&mut *tx).await?;
    assert!((price - 2.0).abs() < 0.0001);
    let sale_session = sqlx::query("INSERT INTO sessionventedirecte(nom,active) VALUES('Session test',1)")
        .execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence) VALUES(?,10)")
        .bind(sale_session).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chargeventedirecte(session_vente_id,categorie,libelle,montant) VALUES(?,'découpe','Découpe',25)")
        .bind(sale_session).execute(&mut *tx).await?;
    tx.rollback().await?;

    let check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await?;
    assert_eq!(check, "ok");
    Ok(())
}
