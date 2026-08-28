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
    let empty_sales: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport")
            .fetch_one(&pool)
            .await?;
    assert_eq!(empty_sales, 0.0);

    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&pool)
    .await?;
    // 65 depuis l'ajout de soinportee, receptionachat (§1bis), lignee_genetique (§2),
    // silo_aliment/releve_silo (prévisions aliment, §5), acterealiseverrat
    // (historique sanitaire des verrats, §3) et consommationsoupe (import
    // machine à soupe, § « aliment et stock »).
    // affectationfacturebande et affectationfacturecontrole assurent la
    // ventilation multi-bandes des factures sans double comptage. La liste
    // numeromarquage normalise les numéros proposés aux bandes.
    // adoptionporcelet conserve les deux extrémités de chaque transfert.
    assert_eq!(tables, 69);
    let objectives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM objectif")
        .fetch_one(&pool)
        .await?;
    let references: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM referenceifip")
        .fetch_one(&pool)
        .await?;
    assert_eq!(objectives, 15);
    assert_eq!(references, 10);
    let sales_settings: (i64, Option<String>) = sqlx::query_as(
        "SELECT commandes_ouvertes,message_fermeture FROM reglageventedirecte WHERE id=1",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(sales_settings, (1, None));

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
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    let room =
        sqlx::query("INSERT INTO salle(site_id,nom,nb_cases,ordre) VALUES(?,'Engraissement',1,1)")
            .bind(site)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
    let pen = sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case 1',20)")
        .bind(room)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    let band =
        sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-TEST','2026-08-01',1)")
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
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
    let price: f64 = sqlx::query_scalar(
        "SELECT SUM(montant_net)/SUM(poids_total) FROM venteapport WHERE bande_id=?",
    )
    .bind(band)
    .fetch_one(&mut *tx)
    .await?;
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
    let preview_lines: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM importligne WHERE token='pdf-test' AND action='ajouter'",
    )
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(preview_lines, 1);
    let sale_session = sqlx::query(
        "INSERT INTO sessionventedirecte(nom,active,date_limite_commandes) VALUES('Session test',1,'2026-08-31')",
    )
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
    sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence,bande_id,nb_porcs_calcules,poids_moyen_kg,cout_par_porc,cout_par_kg,calcule_le) VALUES(?,10,?,10,90,1,0.011111,CURRENT_TIMESTAMP)")
        .bind(sale_session)
        .bind(band)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO chargeventedirecte(session_vente_id,categorie,libelle,montant) VALUES(?,'découpe','Découpe',25)")
        .bind(sale_session).execute(&mut *tx).await?;
    let product = sqlx::query("INSERT INTO produitventedirecte(nom,prix,unite,actif,ordre,quantite_disponible) VALUES('Colis test',12.5,'kg',1,1,10)")
        .execute(&mut *tx).await?.last_insert_rowid();
    let order = sqlx::query("INSERT INTO commandeventedirecte(session_vente_id,nom_client,telephone,statut,total,token_modification,code_modification) VALUES(?,'Client test','0102030405','nouvelle',37.5,'token-client-test','ABC12345')")
        .bind(sale_session).execute(&mut *tx).await?.last_insert_rowid();
    let client_access: (String, String, String) = sqlx::query_as(
        "SELECT token_modification,code_modification,(SELECT date_limite_commandes FROM sessionventedirecte WHERE id=session_vente_id) FROM commandeventedirecte WHERE id=?",
    )
    .bind(order)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(
        client_access,
        (
            "token-client-test".into(),
            "ABC12345".into(),
            "2026-08-31".into()
        )
    );
    let unit_costs: (f64, f64) = sqlx::query_as(
        "SELECT cout_par_porc,cout_par_kg FROM coutelevageventedirecte WHERE session_vente_id=?",
    )
    .bind(sale_session)
    .fetch_one(&mut *tx)
    .await?;
    assert_eq!(unit_costs.0, 1.0);
    assert!((unit_costs.1 - 0.011111).abs() < 0.000001);
    sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,'Colis test',12.5,'kg',3,37.5)")
        .bind(order).bind(product).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-3 WHERE id=?",
    )
    .bind(product)
    .execute(&mut *tx)
    .await?;
    let reserved_stock: f64 =
        sqlx::query_scalar("SELECT quantite_disponible FROM produitventedirecte WHERE id=?")
            .bind(product)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(reserved_stock, 7.0);
    // Une modification rend d'abord l'ancienne réservation, puis réserve la nouvelle.
    sqlx::query(
        "UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+3 WHERE id=?",
    )
    .bind(product)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM lignecommandeventedirecte WHERE commande_id=?")
        .bind(order)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,'Colis test',12.5,'kg',5,62.5)")
        .bind(order).bind(product).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-5 WHERE id=?",
    )
    .bind(product)
    .execute(&mut *tx)
    .await?;
    let edited_stock: f64 =
        sqlx::query_scalar("SELECT quantite_disponible FROM produitventedirecte WHERE id=?")
            .bind(product)
            .fetch_one(&mut *tx)
            .await?;
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
    let band =
        sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-STOCK','2026-08-01',1)")
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
    let room =
        sqlx::query("INSERT INTO salle(site_id,nom,nb_cases,ordre) VALUES(?,'Engraissement',2,1)")
            .bind(site)
            .execute(&pool)
            .await?
            .last_insert_rowid();
    // Case avec un effectif calculé négatif : 5 déclarés présents, 8 morts
    // déclarées ensuite -> 5-8=-3.
    let pen_negative = sqlx::query(
        "INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case négative',20)",
    )
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
    let pen_over =
        sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Case pleine',2)")
            .bind(room)
            .execute(&pool)
            .await?
            .last_insert_rowid();
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,'2026-08-01',3,'Stock initial','test')")
        .bind(pen_over)
        .execute(&pool)
        .await?;
    // Mortalité sans stade renseigné.
    sqlx::query(
        "INSERT INTO declarationmort(bande_code,date,nombre) VALUES('B-TEST','2026-08-11',1)",
    )
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
    let band =
        sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B-TEST','2026-08-01',1)")
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
    sqlx::query(
        "INSERT INTO acterealiseverrat(acte_id,verrat_id,date_realise) VALUES(?,?,'2026-08-12')",
    )
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
            (
                1,
                "2026-08-12".to_string(),
                "V-TEST".to_string(),
                "Bilan sanitaire".to_string()
            ),
            (
                1,
                "2026-08-10".to_string(),
                "B-TEST".to_string(),
                "Vermifuge".to_string()
            ),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn imports_refusent_les_doublons_en_base() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;

    let compteur = sqlx::query("INSERT INTO compteur_energie(nom,type) VALUES('Eau','eau')")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index) VALUES(?,'2026-08-26',10)")
        .bind(compteur).execute(&pool).await?;
    assert!(sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index) VALUES(?,'2026-08-26',11)")
        .bind(compteur).execute(&pool).await.is_err());

    sqlx::query("INSERT INTO consommationsoupe(date,heure_debut,produit_machine,quantite_recue) VALUES('2026-08-26','08:00','Blé',10)")
        .execute(&pool).await?;
    assert!(sqlx::query("INSERT INTO consommationsoupe(date,heure_debut,produit_machine,quantite_recue) VALUES('2026-08-26','08:00',' blé ',12)")
        .execute(&pool).await.is_err());
    Ok(())
}

/// Alerte « délai d'attente en cours » du tableau de bord : un traitement
/// (truie ou porc charcutier) dont le délai n'est pas écoulé doit remonter,
/// un traitement déjà terminé ou un animal déjà mort ne doit pas apparaître.
/// Mêmes requêtes que celles exécutées par le handler `dashboard`
/// (`src/routes/mod.rs`) — dupliquées ici volontairement pour vérifier leur
/// comportement réel plutôt que de supposer qu'elles fonctionnent.
#[tokio::test]
async fn dashboard_alerte_delai_attente_ignore_les_traitements_termines() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;

    let truie_en_cours = sqlx::query(
        "INSERT INTO truie(num_travail,bande_code,statut,reformee) VALUES('T-ENCOURS',NULL,'active',0)",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();
    let truie_terminee = sqlx::query(
        "INSERT INTO truie(num_travail,bande_code,statut,reformee) VALUES('T-TERMINE',NULL,'active',0)",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO evenement(type,date,truie_id,produit,delai_attente) VALUES('traitement',date('now','-1 day'),?,'Toujours en attente',5)")
        .bind(truie_en_cours).execute(&pool).await?;
    sqlx::query("INSERT INTO evenement(type,date,truie_id,produit,delai_attente) VALUES('traitement',date('now','-30 day'),?,'Depuis longtemps fini',5)")
        .bind(truie_terminee).execute(&pool).await?;

    let porc_vivant =
        sqlx::query("INSERT INTO porccharcutier(rfid,bande_code) VALUES('P-VIVANT',NULL)")
            .execute(&pool)
            .await?
            .last_insert_rowid();
    let porc_mort = sqlx::query(
        "INSERT INTO porccharcutier(rfid,bande_code,date_mort) VALUES('P-MORT',NULL,date('now'))",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO traitementcharcutier(charcutier_id,date,produit,delai_attente) VALUES(?,date('now'),'En attente',10)")
        .bind(porc_vivant).execute(&pool).await?;
    // Même en plein délai d'attente, un animal déjà déclaré mort ne doit pas
    // apparaître : il ne partira ni à l'abattoir ni en vente directe.
    sqlx::query("INSERT INTO traitementcharcutier(charcutier_id,date,produit,delai_attente) VALUES(?,date('now'),'Animal mort entre-temps',10)")
        .bind(porc_mort).execute(&pool).await?;

    let truies: Vec<(String,)> = sqlx::query_as(
        "SELECT t.num_travail AS reference FROM evenement e JOIN truie t ON t.id=e.truie_id WHERE e.type='traitement' AND e.delai_attente IS NOT NULL AND e.delai_attente>0 AND date(e.date,'+'||e.delai_attente||' day')>=date('now')",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(truies, vec![("T-ENCOURS".to_string(),)]);

    let charcutiers: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(NULLIF(p.rfid,''),'Porc #'||p.id) AS reference FROM traitementcharcutier tc JOIN porccharcutier p ON p.id=tc.charcutier_id WHERE tc.delai_attente IS NOT NULL AND tc.delai_attente>0 AND date(tc.date,'+'||tc.delai_attente||' day')>=date('now') AND p.date_mort IS NULL",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(charcutiers, vec![("P-VIVANT".to_string(),)]);
    Ok(())
}

/// Vrai bug corrigé : la « Consommation aliment par bande » de
/// `/aliment-previsions` rejoignait `livraisonaliment.bande_id`, la seule
/// bande historique « principale » d'une facture, alors qu'une livraison
/// est très souvent affectée à plusieurs bandes à la fois via
/// `affectationfacturebande` (comme les coûts économiques). Une facture
/// partagée entre deux bandes ne montrait donc rien pour la seconde.
#[tokio::test]
async fn consommation_aliment_par_bande_repartit_une_facture_a_deux_bandes() -> anyhow::Result<()> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(&pool)
        .await?;

    let bande1 = sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B1','2026-08-01',1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let bande2 = sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B2','2026-08-01',1)")
        .execute(&pool)
        .await?
        .last_insert_rowid();
    let livraison =
        sqlx::query("INSERT INTO livraisonaliment(date,tonnage) VALUES(date('now'),10)")
            .execute(&pool)
            .await?
            .last_insert_rowid();
    for bande in [bande1, bande2] {
        sqlx::query(
            "INSERT INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) VALUES('aliment',?,?,0)",
        )
        .bind(livraison)
        .bind(bande)
        .execute(&pool)
        .await?;
    }

    let rows: Vec<(String, f64)> = sqlx::query_as(
        "SELECT b.code,CAST(COALESCE(SUM(l.tonnage/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=l.id)),0) AS REAL) AS tonnage_90j FROM bande b JOIN affectationfacturebande af ON af.categorie='aliment' AND af.bande_id=b.id JOIN livraisonaliment l ON l.id=af.facture_id WHERE b.active=1 AND l.date>=date('now','-90 days') GROUP BY b.id,b.code ORDER BY b.code",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows, vec![("B1".to_string(), 5.0), ("B2".to_string(), 5.0)]);
    Ok(())
}
