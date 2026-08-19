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

    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await?;
    assert_eq!(busy_timeout, 5_000);
    assert_eq!(foreign_keys, 1);

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
    // 60 depuis l'ajout de receptionachat (§1bis), lignee_genetique (§2),
    // silo_aliment/releve_silo (prévisions aliment, §5), et acterealiseverrat
    // (historique sanitaire des verrats, §3).
    assert_eq!(tables, 60);
    let objectives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM objectif")
        .fetch_one(&pool)
        .await?;
    let references: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM referenceifip")
        .fetch_one(&pool)
        .await?;
    assert_eq!(objectives, 15);
    assert_eq!(references, 10);

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
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,'2026-08-16',10,'Stock initial','test')")
        .bind(pen).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO venteapport(date,bande_id,nb_porcs,poids_total,montant_net) VALUES('2026-08-16',?,10,900,1800)")
        .bind(band).execute(&mut *tx).await?;
    let price: f64 = sqlx::query_scalar("SELECT SUM(montant_net)/SUM(poids_total) FROM venteapport WHERE bande_id=?")
        .bind(band).fetch_one(&mut *tx).await?;
    assert!((price - 2.0).abs() < 0.0001);
    sqlx::query("INSERT INTO valorisationapport(num_apport,date,libelle,montant,categorie) VALUES('AP-TEST','2026-08-16','Frais de groupement',-25,'retenue')")
        .execute(&mut *tx).await?;
    let retention: f64 = sqlx::query_scalar("SELECT CAST(SUM(ABS(montant)) AS REAL) FROM valorisationapport WHERE num_apport='AP-TEST' AND categorie='retenue'")
        .fetch_one(&mut *tx).await?;
    assert_eq!(retention, 25.0);
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,resume) VALUES('pdf-test','economique:aliment','facture.pdf','apercu','{\"ajouter\":1}')")
        .execute(&mut *tx).await?;
    sqlx::query("INSERT INTO importligne(token,numero_ligne,action,donnees_json) VALUES('pdf-test',1,'ajouter','{\"kind\":\"aliment\"}')")
        .execute(&mut *tx).await?;
    let preview_lines:i64=sqlx::query_scalar("SELECT COUNT(*) FROM importligne WHERE token='pdf-test' AND action='ajouter'")
        .fetch_one(&mut *tx).await?;
    assert_eq!(preview_lines,1);
    let sale_session = sqlx::query("INSERT INTO sessionventedirecte(nom,active) VALUES('Session test',1)")
        .execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence) VALUES(?,10)")
        .bind(sale_session).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO chargeventedirecte(session_vente_id,categorie,libelle,montant) VALUES(?,'découpe','Découpe',25)")
        .bind(sale_session).execute(&mut *tx).await?;
    let product = sqlx::query("INSERT INTO produitventedirecte(nom,prix,unite,actif,ordre,quantite_disponible) VALUES('Colis test',12.5,'kg',1,1,10)")
        .execute(&mut *tx).await?.last_insert_rowid();
    let order = sqlx::query("INSERT INTO commandeventedirecte(session_vente_id,nom_client,telephone,statut,total) VALUES(?,'Client test','0102030405','nouvelle',37.5)")
        .bind(sale_session).execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,'Colis test',12.5,'kg',3,37.5)")
        .bind(order).bind(product).execute(&mut *tx).await?;
    sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-3 WHERE id=?")
        .bind(product).execute(&mut *tx).await?;
    let reserved_stock: f64 = sqlx::query_scalar("SELECT quantite_disponible FROM produitventedirecte WHERE id=?")
        .bind(product).fetch_one(&mut *tx).await?;
    assert_eq!(reserved_stock, 7.0);
    // Une modification rend d'abord l'ancienne réservation, puis réserve la nouvelle.
    sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+3 WHERE id=?")
        .bind(product).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM lignecommandeventedirecte WHERE commande_id=?")
        .bind(order).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,'Colis test',12.5,'kg',5,62.5)")
        .bind(order).bind(product).execute(&mut *tx).await?;
    sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-5 WHERE id=?")
        .bind(product).execute(&mut *tx).await?;
    let edited_stock: f64 = sqlx::query_scalar("SELECT quantite_disponible FROM produitventedirecte WHERE id=?")
        .bind(product).fetch_one(&mut *tx).await?;
    assert_eq!(edited_stock, 5.0);
    let preparation: f64 = sqlx::query_scalar("SELECT CAST(SUM(l.quantite) AS REAL) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id=? AND c.statut<>'annulee' AND l.produit_id=?")
        .bind(sale_session).bind(product).fetch_one(&mut *tx).await?;
    assert_eq!(preparation, 5.0);
    tx.rollback().await?;

    let check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await?;
    assert_eq!(check, "ok");
    Ok(())
}

#[tokio::test]
async fn inventaire_est_un_point_de_depart_sans_double_comptage() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;
    let band = sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-STOCK','2026-08-01',1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,libelle,type_saisie,est_stock) VALUES('2026-08-16','B-STOCK',100,'stock porcs','inventaire',1)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO transfert(date,espece,bande_id,nombre) VALUES('2026-08-15','porc',?,20),('2026-08-16','porc',?,10),('2026-08-17','porc',?,15)")
        .bind(band)
        .bind(band)
        .bind(band)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO declarationmort(date,bande_code,nombre) VALUES('2026-08-16','B-STOCK',2),('2026-08-18','B-STOCK',3)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO venteapport(date,bande_id,nb_porcs,lots_json) VALUES('2026-08-16',?,5,NULL),('2026-08-19',?,6,NULL),(NULL,NULL,99,'json-invalide')")
        .bind(band)
        .bind(band)
        .execute(&pool)
        .await?;
    let transferred: i64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM transfert WHERE espece='porc' AND bande_id=? AND date>?")
        .bind(band)
        .bind("2026-08-16")
        .fetch_one(&pool)
        .await?;
    let deaths: i64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM declarationmort WHERE bande_code=? AND date>?")
        .bind("B-STOCK")
        .bind("2026-08-16")
        .fetch_one(&pool)
        .await?;
    let sold: i64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(CASE WHEN v.bande_id=? THEN COALESCE(v.nb_porcs,0) WHEN v.bande_id IS NULL AND json_type(CASE WHEN json_valid(v.lots_json) THEN v.lots_json ELSE 'null' END)='array' THEN (SELECT COALESCE(SUM(CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER)),0) FROM json_each(v.lots_json) j WHERE CAST(json_extract(j.value,'$.bande_id') AS INTEGER)=?) ELSE 0 END),0) AS INTEGER) FROM venteapport v WHERE date>?")
        .bind(band)
        .bind(band)
        .bind("2026-08-16")
        .fetch_one(&pool)
        .await?;
    assert_eq!((transferred, deaths, sold), (15, 3, 6));
    assert_eq!(100 - transferred - deaths - sold, 76);

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM mouvementstock WHERE est_stock=1 AND bande_code=? AND date=? AND lower(trim(COALESCE(libelle,'')))=lower(trim(?))")
        .bind("B-STOCK")
        .bind("2026-08-16")
        .bind("stock porcs")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,libelle,type_saisie,est_stock) VALUES('2026-08-16','B-STOCK',98,'stock porcs','inventaire',1)")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let corrected: (i64, i64) = sqlx::query_as("SELECT COUNT(*),CAST(SUM(nombre) AS INTEGER) FROM mouvementstock WHERE est_stock=1 AND bande_code='B-STOCK' AND date='2026-08-16' AND lower(trim(libelle))='stock porcs'")
        .fetch_one(&pool)
        .await?;
    assert_eq!(corrected, (1, 98));
    Ok(())
}

/// Vérifie en conditions réelles (base SQLite en mémoire) les contrôles
/// ajoutés à `/etat-donnees` pour le §3 « Fiabiliser les effectifs réels » :
/// même requête (WITH `effectif_case`/`effectif_case2`) que celle utilisée
/// par `etat_donnees` dans `src/routes/mod.rs`, pour attraper une régression
/// SQL — cette requête n'est exécutée nulle part ailleurs dans les tests.
#[tokio::test]
async fn etat_donnees_detecte_les_incoherences_deffectif() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;
    let site = sqlx::query("INSERT INTO site(code,nom) VALUES('TEST','Site test')")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let room = sqlx::query("INSERT INTO salle(site_id,nom,nb_cases,ordre) VALUES(?,'Engraissement',2,1)")
        .bind(site)
        .execute(&pool)
        .await?
        .last_insert_rowid();
    // Case avec un effectif calculé négatif : 5 déclarés présents, 8 morts
    // déclarées ensuite -> 5-8=-3.
    let pen_negative = sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case négative',20)")
        .bind(room)
        .execute(&pool)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,'2026-08-01',5,'Stock initial','test')")
        .bind(pen_negative)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO declarationmort(bande_code,date,stade,case_id,nombre) VALUES('B-TEST','2026-08-10','Engraissement',?,8)")
        .bind(pen_negative)
        .execute(&pool)
        .await?;
    // Case en dépassement de capacité : 3 présents pour 2 places.
    let pen_over = sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case pleine',2)")
        .bind(room)
        .execute(&pool)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,'2026-08-01',3,'Stock initial','test')")
        .bind(pen_over)
        .execute(&pool)
        .await?;
    // Mortalité sans stade renseigné.
    sqlx::query("INSERT INTO declarationmort(bande_code,date,nombre) VALUES('B-TEST','2026-08-11',1)")
        .execute(&pool)
        .await?;
    // Porc charcutier sans bande d'origine (donnée legacy).
    sqlx::query("INSERT INTO porccharcutier(rfid,date_naissance) VALUES('RFID-TEST','2026-05-01')")
        .execute(&pool)
        .await?;

    let sql = "WITH effectif_case AS (SELECT c.id,c.nb_max_porcs,(SELECT date FROM inventairecase WHERE case_id=c.id ORDER BY date DESC,id DESC LIMIT 1) AS inv_date,COALESCE((SELECT nombre FROM inventairecase WHERE case_id=c.id ORDER BY date DESC,id DESC LIMIT 1),0) AS base FROM casesalle c),effectif_case2 AS (SELECT e.id,e.nb_max_porcs,e.base+COALESCE((SELECT SUM(CASE WHEN t.case_dest_id=e.id THEN COALESCE(t.nombre,0) ELSE -COALESCE(t.nombre,0) END) FROM transfert t WHERE t.espece='porc' AND (t.case_dest_id=e.id OR t.case_source_id=e.id) AND (e.inv_date IS NULL OR t.date>e.inv_date)),0)-COALESCE((SELECT SUM(d.nombre) FROM declarationmort d WHERE d.case_id=e.id AND (e.inv_date IS NULL OR d.date>e.inv_date)),0) AS effectif FROM effectif_case e) SELECT 'Cases avec effectif calculé négatif' AS controle,COUNT(*) AS anomalies FROM effectif_case2 WHERE effectif<0 UNION ALL SELECT 'Cases dépassant leur capacité déclarée',COUNT(*) FROM effectif_case2 WHERE nb_max_porcs IS NOT NULL AND nb_max_porcs>0 AND effectif>nb_max_porcs UNION ALL SELECT 'Déclarations de mortalité sans stade renseigné',COUNT(*) FROM declarationmort WHERE stade IS NULL OR trim(stade)='' UNION ALL SELECT 'Porcs charcutiers sans bande d''origine',COUNT(*) FROM porccharcutier WHERE bande_code IS NULL OR trim(bande_code)=''";
    let rows: Vec<(String, i64)> = sqlx::query_as(sql).fetch_all(&pool).await?;
    assert_eq!(
        rows,
        vec![
            ("Cases avec effectif calculé négatif".to_string(), 1),
            ("Cases dépassant leur capacité déclarée".to_string(), 1),
            (
                "Déclarations de mortalité sans stade renseigné".to_string(),
                1
            ),
            ("Porcs charcutiers sans bande d'origine".to_string(), 1),
        ]
    );
    Ok(())
}

/// Vérifie qu'un acte réalisé sur un verrat (acterealiseverrat, §3
/// « Rappels sanitaires… avec historique ») rejoint bien l'historique
/// « Actes réalisés » aux côtés des actes par bande (acterealise), avec la
/// même requête UNION ALL que celle utilisée par `sanitaire` dans
/// `src/routes/mod.rs`.
#[tokio::test]
async fn historique_sanitaire_reunit_bandes_et_verrats() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;
    let band = sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-TEST','2026-08-01',1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let verrat = sqlx::query("INSERT INTO verrat(code,actif) VALUES('V-TEST',1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let acte_bande = sqlx::query("INSERT INTO acteprotocole(libelle,cible,reference,jour,actif) VALUES('Vermifuge','Bande','mise_bas',0,1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let acte_verrat = sqlx::query("INSERT INTO acteprotocole(libelle,cible,reference,jour,actif) VALUES('Bilan sanitaire','Verrat','mise_bas',0,1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO acterealise(acte_id,bande_id,date_realise) VALUES(?,?,'2026-08-10')")
        .bind(acte_bande)
        .bind(band)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO acterealiseverrat(acte_id,verrat_id,date_realise) VALUES(?,?,'2026-08-12')")
        .bind(acte_verrat)
        .bind(verrat)
        .execute(&pool)
        .await?;

    let sql = "SELECT ar.id AS id,ar.date_realise AS date_realise,b.code AS cible_nom,a.libelle,a.produit,ar.note FROM acterealise ar JOIN bande b ON b.id=ar.bande_id JOIN acteprotocole a ON a.id=ar.acte_id UNION ALL SELECT arv.id AS id,arv.date_realise AS date_realise,v.code AS cible_nom,a.libelle,a.produit,arv.note FROM acterealiseverrat arv JOIN verrat v ON v.id=arv.verrat_id JOIN acteprotocole a ON a.id=arv.acte_id ORDER BY date_realise DESC,id DESC LIMIT 250";
    #[allow(clippy::type_complexity)]
    let rows: Vec<(i64, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(sql).fetch_all(&pool).await?;
    let simplified: Vec<(i64, String, String, String)> = rows
        .into_iter()
        .map(|(id, date, cible, libelle, _, _)| (id, date, cible, libelle))
        .collect();
    assert_eq!(
        simplified,
        vec![
            (1, "2026-08-12".to_string(), "V-TEST".to_string(), "Bilan sanitaire".to_string()),
            (1, "2026-08-10".to_string(), "B-TEST".to_string(), "Vermifuge".to_string()),
        ]
    );
    Ok(())
}
