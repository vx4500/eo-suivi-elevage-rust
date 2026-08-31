//! Démonstration autonome : aucune ouverture d'une base réelle n'est permise.
use sqlx::SqlitePool;

pub fn enabled() -> bool {
    std::env::var("EO_DEMO_PORTAL").as_deref() == Ok("1")
}

pub async fn verify_database(pool: &SqlitePool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(pool)
    .await?;
    if count > 0 {
        let marker =
            sqlx::query_scalar::<_, String>("SELECT valeur FROM parametre WHERE cle='demo_portal'")
                .fetch_optional(pool)
                .await;
        anyhow::ensure!(matches!(marker,Ok(Some(ref s)) if s=="1"),"Refus : cette base n'est pas une démonstration. Utilisez un dossier de données vide et indépendant.");
    }
    Ok(())
}
pub async fn init(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::raw_sql("CREATE TABLE IF NOT EXISTS demo_acces(utilisateur_id INTEGER PRIMARY KEY REFERENCES utilisateur(id),expire INTEGER NOT NULL,cree INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS demo_suggestion(id INTEGER PRIMARY KEY,utilisateur_id INTEGER NOT NULL REFERENCES utilisateur(id),message TEXT NOT NULL,page TEXT,cree INTEGER NOT NULL);").execute(pool).await?;
    let marker: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='demo_portal'")
            .fetch_optional(pool)
            .await?;
    if marker.is_none() {
        let password = std::env::var("EO_DEMO_ADMIN_PASSWORD").unwrap_or_default();
        anyhow::ensure!(password.len()>=16,"Définissez EO_DEMO_ADMIN_PASSWORD avec au moins 16 caractères pour initialiser la démonstration.");
        let hash = crate::auth::hash_password_async(password).await?;
        let mut tx = pool.begin().await?;
        crate::demo::activer(&mut tx).await?;
        sqlx::query("UPDATE utilisateur SET actif=0")
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE utilisateur SET hash_mdp=?,actif=1,doit_changer_mdp=0 WHERE identifiant='admin'").bind(hash).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('demo_portal','1'),('module_genetique','1') ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").execute(&mut *tx).await?;
        tx.commit().await?;
    }
    completer_economie(pool).await?;
    prune(pool).await?;
    Ok(())
}

/// Complément versionné réservé au portail fictif. Ne modifie aucune saisie
/// existante et ignore les bandes dont l'économie a déjà été renseignée.
async fn completer_economie(pool: &SqlitePool) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let demo: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='demo_portal'")
            .fetch_optional(&mut *tx)
            .await?;
    anyhow::ensure!(
        demo.as_deref() == Some("1"),
        "Complément économique réservé à la démonstration"
    );
    // L'écriture acquiert le verrou avant toute lecture du jeu à compléter.
    let inserted =
        sqlx::query("INSERT OR IGNORE INTO parametre(cle,valeur) VALUES('demo_economie_v1','1')")
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if inserted == 0 {
        tx.commit().await?;
        return Ok(());
    }
    // Remonter plus loin que cinq ans de naissances : les ventes arrivent
    // six mois plus tard. Les sept bandes actives restent inchangées.
    loop {
        let ancienne: Option<(i64, String)> = sqlx::query_as(
            "SELECT b.id,b.date_mb FROM bande b JOIN demoobjet d ON d.table_name='bande' AND d.row_id=b.id
             WHERE b.code LIKE 'DEMO-%' AND date(b.date_mb) IS NOT NULL ORDER BY b.date_mb LIMIT 1"
        ).fetch_optional(&mut *tx).await?;
        let Some((id, date)) = ancienne else { break };
        let suffisant: bool = sqlx::query_scalar(
            "SELECT date(?) <= date('now','start of month','-5 years','-180 days')",
        )
        .bind(&date)
        .fetch_one(&mut *tx)
        .await?;
        if suffisant {
            break;
        }
        let archive = sqlx::query("INSERT INTO bande(code,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr,cs_gmq_nv)
            SELECT 'DEMO-HIST-'||date(date_mb,'-21 days'),date(date_mb,'-21 days'),site,'Historique fictif — conduite en 7 bandes, cycle tous les 21 jours',0,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr,cs_gmq_nv FROM bande WHERE id=?")
            .bind(id).execute(&mut *tx).await?.last_insert_rowid();
        sqlx::query("INSERT INTO demoobjet(table_name,row_id) VALUES('bande',?)")
            .bind(archive)
            .execute(&mut *tx)
            .await?;
    }
    let bandes: Vec<(i64, String, String, i64, i64)> = sqlx::query_as(
        "SELECT b.id,b.code,b.date_mb,CAST(ROUND(b.cs_total_sevres*0.96) AS INTEGER),CAST(julianday(date('now'))-julianday(b.date_mb) AS INTEGER) FROM bande b
         WHERE b.id IN(SELECT row_id FROM demoobjet WHERE table_name='bande')
         AND b.code LIKE 'DEMO-%' AND date(b.date_mb)<=date('now') AND b.cs_total_sevres>0
         AND NOT EXISTS(SELECT 1 FROM ventelot v WHERE v.bande_id=b.id)
         AND NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.bande_id=b.id)
         AND NOT EXISTS(SELECT 1 FROM livraisonaliment a WHERE a.bande_id=b.id)
         AND NOT EXISTS(SELECT 1 FROM achatveto a WHERE a.bande_id=b.id)
         AND NOT EXISTS(SELECT 1 FROM achatsemence a WHERE a.bande_id=b.id)
         AND NOT EXISTS(SELECT 1 FROM achatgenetique a WHERE a.bande_code=b.code) ORDER BY b.id"
    ).fetch_all(&mut *tx).await?;
    for (id, code, date, porcs, age) in bandes {
        // Hypothèses illustratives uniquement, pas des références de marché.
        let prix = 1.85 + (id % 7) as f64 * 0.05;
        let poids = porcs as f64 * 95.0;
        let recette = (poids * prix * 100.0).round() / 100.0;
        let mut objets = Vec::new();
        let mut factures = Vec::new();
        if age >= 180 {
            let vente = sqlx::query("INSERT INTO venteapport(date,num_apport,bande_id,frappe,nb_porcs,poids_total,poids_moyen,prix_moyen,montant_ht,montant_net,tmp,lots_json) VALUES(date(?,'+180 days'),?,?,?, ?,?,95,?,?,?,61,'[]')")
            .bind(&date).bind(format!("FICTIF-{code}")).bind(id).bind(&code).bind(porcs).bind(poids).bind(prix).bind(recette).bind(recette)
            .execute(&mut *tx).await?.last_insert_rowid();
            objets.push(("venteapport", vente));
        }
        for (stade, decalage, ration, tarif) in [
            ("gestation", -60, 0.035, 290.0),
            ("lactation", 7, 0.025, 340.0),
            ("ps", 35, 0.025, 450.0),
            ("croissance", 75, 0.09, 320.0),
            ("finition", 130, 0.14, 295.0),
        ] {
            if age < decalage {
                continue;
            }
            let tonnes = (porcs as f64 * ration * 1000.0).round() / 1000.0;
            let aliment = sqlx::query("INSERT INTO livraisonaliment(date,fournisseur,produit,stade_aliment,tonnage,pu_ht,montant_ht,num_facture,bande_id) VALUES(date(?,?),'Fournisseur fictif',?,?, ?,?,?,?,?)")
                .bind(&date).bind(format!("{decalage:+} days")).bind(format!("Aliment {stade} fictif")).bind(stade).bind(tonnes).bind(tarif).bind((tonnes*tarif*100.0).round()/100.0).bind(format!("FICTIF-AL-{stade}-{code}")).bind(id)
                .execute(&mut *tx).await?.last_insert_rowid();
            objets.push(("livraisonaliment", aliment));
            factures.push(("aliment", aliment));
        }
        if age >= 30 {
            let veto = sqlx::query("INSERT INTO achatveto(date,fournisseur,produit,quantite,pu_ht,montant_ht,num_facture,bande_id) VALUES(date(?,'+30 days'),'Fournisseur fictif','Frais sanitaires fictifs',1,?,?,?,?)")
            .bind(&date).bind(porcs as f64*4.0).bind(porcs as f64*4.0).bind(format!("FICTIF-VE-{code}")).bind(id)
            .execute(&mut *tx).await?.last_insert_rowid();
            objets.push(("achatveto", veto));
            factures.push(("veto", veto));
        }
        let semence = sqlx::query("INSERT INTO achatsemence(date,num_facture,fournisseur,designation,nb_doses,montant_ht,bande_id,note) VALUES(date(?),?,'Fournisseur fictif','Semence démonstration',250,1750,?,'Données fictives, sans valeur comptable')")
            .bind(&date).bind(format!("FICTIF-SE-{code}")).bind(id)
            .execute(&mut *tx).await?.last_insert_rowid();
        let genetique = sqlx::query("INSERT INTO achatgenetique(date,num_facture,fournisseur,designation,nb_animaux,poids_total,prix_moyen,montant_ht,montant_net,bande_code,note) VALUES(date(?),?,'Fournisseur fictif','Cochettes de renouvellement fictives',20,2600,4,10400,10400,?,'Données fictives, sans valeur comptable')")
            .bind(&date).bind(format!("FICTIF-GE-{code}")).bind(&code).execute(&mut *tx).await?.last_insert_rowid();
        objets.extend([("achatsemence", semence), ("achatgenetique", genetique)]);
        factures.extend([("semence", semence), ("genetique", genetique)]);
        for (table, row) in objets {
            sqlx::query("INSERT INTO demoobjet(table_name,row_id) VALUES(?,?)")
                .bind(table)
                .bind(row)
                .execute(&mut *tx)
                .await?;
        }
        for (categorie, facture) in factures {
            let lien = sqlx::query(
                "INSERT INTO affectationfacturebande(categorie,facture_id,bande_id) VALUES(?,?,?)",
            )
            .bind(categorie)
            .bind(facture)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
            sqlx::query(
                "INSERT INTO demoobjet(table_name,row_id) VALUES('affectationfacturebande',?)",
            )
            .bind(lien)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
pub async fn valid(pool: &SqlitePool, id: i64, now: i64) -> bool {
    sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM utilisateur u LEFT JOIN demo_acces d ON d.utilisateur_id=u.id WHERE u.id=? AND u.actif=1 AND ((u.identifiant='admin' AND u.role='admin') OR d.expire>?)").bind(id).bind(now).fetch_one(pool).await.unwrap_or(0)==1
}
pub fn blocked(path: &str) -> bool {
    [
        "/maj",
        "/sauvegarde",
        "/restaurer",
        "/reglages/maj",
        "/parametres",
        "/utilisateurs",
        "/import",
        "/economique/import",
        "/contact/envoyer",
        "/notifications",
        "/vente-directe/notifier",
        "/vente-directe/communications",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}
pub async fn prune(pool: &SqlitePool) -> anyhow::Result<()> {
    let cutoff = chrono::Utc::now().timestamp() - 90 * 86400;
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM demo_suggestion WHERE cree<?")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE controlequotidien SET utilisateur='Ancien testeur' WHERE utilisateur IN (SELECT u.nom FROM utilisateur u JOIN demo_acces d ON d.utilisateur_id=u.id WHERE d.cree<?)").bind(cutoff).execute(&mut *tx).await?;
    sqlx::query("UPDATE utilisateur SET actif=0,nom='Ancien testeur',prenom=NULL,identifiant='expire-'||id,hash_mdp='' WHERE id IN(SELECT utilisateur_id FROM demo_acces WHERE cree<?)").bind(cutoff).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM demo_acces WHERE cree<?")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn economie_additive_idempotente_et_reservee_a_la_demo() -> anyhow::Result<()> {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0002_ventelot.sql"))
            .execute(&p)
            .await?;
        assert!(completer_economie(&p).await.is_err());
        sqlx::raw_sql("INSERT INTO parametre(cle,valeur) VALUES('demo_portal','1');
          INSERT INTO utilisateur(identifiant,nom,hash_mdp,role) VALUES('testeur','Testeur','hash-conserve','eleveur');
          INSERT INTO bande(id,code,date_mb,cs_total_sevres) VALUES
          (1,'DEMO-B1',date('now','-5 years','-210 days'),1200),
          (2,'DEMO-B2',date('now','-10 days'),1200),
          (3,'DEMO-B3',date('now','-250 days'),1200),
          (4,'AUTRE',date('now','-300 days'),1200);
          INSERT INTO demoobjet(table_name,row_id) VALUES('bande',1),('bande',2),('bande',3);
          INSERT INTO achatsemence(bande_id,montant_ht,note) VALUES(3,42,'Saisie à conserver');").execute(&p).await?;
        completer_economie(&p).await?;
        completer_economie(&p).await?;
        let ventes: (i64, i64, f64) =
            sqlx::query_as("SELECT COUNT(*),SUM(nb_porcs),SUM(montant_ht) FROM ventelot")
                .fetch_one(&p)
                .await?;
        assert_eq!(ventes.0, 1);
        assert_eq!(ventes.1, 1152);
        assert!(ventes.2 > 0.0);
        let liens: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM affectationfacturebande WHERE bande_id=1")
                .fetch_one(&p)
                .await?;
        assert_eq!(liens, 8);
        let depenses: f64 = sqlx::query_scalar("SELECT (SELECT SUM(montant_ht) FROM livraisonaliment WHERE bande_id=1)+(SELECT SUM(montant_ht) FROM achatveto WHERE bande_id=1)+(SELECT SUM(montant_ht) FROM achatsemence WHERE bande_id=1)+(SELECT SUM(montant_ht) FROM achatgenetique WHERE bande_code='DEMO-B1')").fetch_one(&p).await?;
        assert!(depenses > 0.0 && depenses < ventes.2);
        let conserve: (f64, String) =
            sqlx::query_as("SELECT montant_ht,note FROM achatsemence WHERE bande_id=3")
                .fetch_one(&p)
                .await?;
        assert_eq!(conserve, (42.0, "Saisie à conserver".into()));
        let hash: String =
            sqlx::query_scalar("SELECT hash_mdp FROM utilisateur WHERE identifiant='testeur'")
                .fetch_one(&p)
                .await?;
        assert_eq!(hash, "hash-conserve");
        let erreurs = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&p)
            .await?;
        assert!(erreurs.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn cinq_ans_economiques_sept_bandes_et_aucune_date_future() -> anyhow::Result<()> {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0002_ventelot.sql"))
            .execute(&p)
            .await?;
        let mut tx = p.begin().await?;
        crate::demo::activer(&mut tx).await?;
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('demo_portal','1')")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        completer_economie(&p).await?;
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE active=1")
            .fetch_one(&p)
            .await?;
        assert_eq!(active, 7);
        let cycles: Vec<String> = sqlx::query_scalar("SELECT date_mb FROM bande ORDER BY date_mb")
            .fetch_all(&p)
            .await?;
        for pair in cycles.windows(2) {
            let a = chrono::NaiveDate::parse_from_str(&pair[0], "%Y-%m-%d")?;
            let b = chrono::NaiveDate::parse_from_str(&pair[1], "%Y-%m-%d")?;
            assert_eq!((b - a).num_days(), 21);
        }
        for table in [
            "venteapport",
            "livraisonaliment",
            "achatveto",
            "achatsemence",
            "achatgenetique",
        ] {
            let sql = format!("SELECT MIN(date)<=date('now','start of month','-5 years'),SUM(CASE WHEN date>date('now') THEN 1 ELSE 0 END) FROM {table}");
            let (cinq_ans, futur): (bool, i64) = sqlx::query_as(&sql).fetch_one(&p).await?;
            assert!(cinq_ans, "historique insuffisant : {table}");
            assert_eq!(futur, 0, "date future : {table}");
        }
        for table in [
            "venteapport",
            "livraisonaliment",
            "achatveto",
            "achatsemence",
            "achatgenetique",
        ] {
            let query = format!("SELECT COUNT(DISTINCT substr(date,1,7)) FROM {table} WHERE date>=date('now','start of month','-5 years') AND date<date('now','start of month')");
            let mois: i64 = sqlx::query_scalar(&query).fetch_one(&p).await?;
            assert_eq!(mois, 60, "mois manquant : {table}");
        }
        let sans_lien: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM livraisonaliment a WHERE NOT EXISTS(SELECT 1 FROM affectationfacturebande f WHERE f.categorie='aliment' AND f.facture_id=a.id AND f.bande_id=a.bande_id)").fetch_one(&p).await?;
        assert_eq!(sans_lien, 0);
        let avant: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet")
            .fetch_one(&p)
            .await?;
        completer_economie(&p).await?;
        let apres: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet")
            .fetch_one(&p)
            .await?;
        assert_eq!(avant, apres);
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&p)
            .await?
            .is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn economie_annule_tout_si_insertion_echoue() -> anyhow::Result<()> {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0002_ventelot.sql"))
            .execute(&p)
            .await?;
        sqlx::raw_sql("INSERT INTO parametre(cle,valeur) VALUES('demo_portal','1');
          INSERT INTO bande(id,code,date_mb,cs_total_sevres) VALUES(1,'DEMO-B1',date('now','-200 days'),1200);
          INSERT INTO demoobjet(table_name,row_id) VALUES('bande',1);
          CREATE TRIGGER panne BEFORE INSERT ON achatveto BEGIN SELECT RAISE(ABORT,'panne simulée'); END;").execute(&p).await?;
        assert!(completer_economie(&p).await.is_err());
        let n: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM venteapport)+(SELECT COUNT(*) FROM livraisonaliment)+(SELECT COUNT(*) FROM parametre WHERE cle='demo_economie_v1')").fetch_one(&p).await?;
        assert_eq!(n, 0);
        sqlx::query("DROP TRIGGER panne").execute(&p).await?;
        completer_economie(&p).await?;
        Ok(())
    }
    #[tokio::test]
    async fn refuse_base_reelle() {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        verify_database(&p).await.unwrap();
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await
            .unwrap();
        assert!(verify_database(&p).await.is_err());
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('demo_portal','1')")
            .execute(&p)
            .await
            .unwrap();
        verify_database(&p).await.unwrap();
    }
    #[tokio::test]
    async fn expiration_exacte_et_revocation() {
        let p = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&p)
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE demo_acces(utilisateur_id INTEGER,expire INTEGER,cree INTEGER); INSERT INTO utilisateur(id,identifiant,nom,hash_mdp,role,actif) VALUES(1,'test','Test','unused','eleveur',1); INSERT INTO demo_acces VALUES(1,172800,0)").execute(&p).await.unwrap();
        assert!(valid(&p, 1, 172799).await);
        assert!(!valid(&p, 1, 172800).await);
        sqlx::query("UPDATE utilisateur SET actif=0 WHERE id=1")
            .execute(&p)
            .await
            .unwrap();
        assert!(!valid(&p, 1, 1).await);
        assert!(!valid(&p, 99, 1).await);
    }
    #[test]
    fn catalogue_fidele_et_operations_protegees() {
        let data: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/danbred-2026-08.json")).unwrap();
        assert_eq!(data.len(), 40);
        assert_eq!(data.iter().filter(|v| v["statut"] == "Actif").count(), 27);
        assert_eq!(
            data.iter().filter(|v| v["index_actuel"].is_null()).count(),
            5
        );
        assert!(blocked("/sauvegarde/restaurer"));
        assert!(blocked("/vente-directe/communications/test-email"));
        assert!(!blocked("/quotidien/note"));
    }
}
