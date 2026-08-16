use crate::auth;
use sqlx::{Row, SqlitePool};

pub async fn init(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(pool)
        .await?;

    // Migrations additives pour les bases anciennes 1.55–1.65. Une colonne déjà
    // présente est volontairement ignorée afin de rendre le démarrage idempotent.
    for (table, column, definition) in [
        ("utilisateur", "sections", "TEXT"),
        ("utilisateur", "doit_changer_mdp", "INTEGER NOT NULL DEFAULT 0"),
        ("utilisateur", "tentatives_echec", "INTEGER NOT NULL DEFAULT 0"),
        ("utilisateur", "bloque_jusqu", "TEXT"),
        ("truie", "num_national", "TEXT"),
        ("truie", "bande_code", "TEXT"),
        ("truie", "perf_tx_perte", "REAL"),
        ("truie", "salle_id", "INTEGER"),
        ("truie", "case_id", "INTEGER"),
        ("truie", "source_import_id", "TEXT"),
        ("bande", "cs_mn_portee", "REAL"),
        ("bande", "cs_adoptes", "INTEGER"),
        ("bande", "cs_retires", "INTEGER"),
        ("evenement", "resultat", "TEXT"),
        ("evenement", "chetifs", "INTEGER"),
        ("evenement", "ecrases", "INTEGER"),
        ("evenement", "tues_truie", "INTEGER"),
        ("evenement", "adoptes", "INTEGER"),
        ("evenement", "retires", "INTEGER"),
        ("evenement", "delai_attente", "INTEGER"),
        ("evenement", "nb_doses", "INTEGER"),
        ("evenement", "heure_debut", "TEXT"),
        ("evenement", "heure_fin", "TEXT"),
        ("evenement", "suivi_actif", "INTEGER NOT NULL DEFAULT 0"),
        ("declarationmort", "case_id", "INTEGER"),
        ("produitventedirecte", "quantite_disponible", "REAL"),
        ("releve_compteur", "remplacement_compteur", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        ensure_column(pool, table, column, definition).await?;
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur")
        .fetch_one(pool)
        .await?;
    if count == 0 {
        let hash = auth::hash_password("admin");
        sqlx::query(
            "INSERT INTO utilisateur(identifiant,nom,hash_mdp,role,actif,doit_changer_mdp) VALUES('admin','Administrateur',?,'admin',1,1)",
        )
        .bind(hash)
        .execute(pool)
        .await?;
        tracing::warn!("Compte admin initial créé; mot de passe temporaire: admin");
    }
    Ok(())
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', ""));
    let rows = sqlx::query(&sql).fetch_all(pool).await?;
    if rows.iter().any(|row| row.get::<String, _>("name") == column) {
        return Ok(());
    }
    let alter = format!(
        "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
        table.replace('"', ""),
        column.replace('"', ""),
        definition
    );
    sqlx::query(&alter).execute(pool).await?;
    Ok(())
}

pub async fn journal(
    pool: &SqlitePool,
    utilisateur: &str,
    action: &str,
    objet: &str,
    detail: &str,
    chemin: &str,
) {
    if let Err(error) = sqlx::query(
        "INSERT INTO journal(utilisateur,action,objet,detail,chemin) VALUES(?,?,?,?,?)",
    )
    .bind(utilisateur)
    .bind(action)
    .bind(objet)
    .bind(detail)
    .bind(chemin)
    .execute(pool)
    .await
    {
        tracing::warn!(%error, "journalisation impossible");
    }
}
