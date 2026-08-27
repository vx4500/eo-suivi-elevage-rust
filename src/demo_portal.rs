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
    prune(pool).await?;
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
