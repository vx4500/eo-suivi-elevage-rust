use crate::auth;
use sqlx::{Row, SqlitePool};

pub async fn init(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
        .execute(pool)
        .await?;
    verify_sqlite_pragmas(pool).await?;

    // Migrations additives pour les bases anciennes 1.55–1.65. Une colonne déjà
    // présente est volontairement ignorée afin de rendre le démarrage idempotent.
    for (table, column, definition) in [
        ("utilisateur", "sections", "TEXT"),
        (
            "utilisateur",
            "doit_changer_mdp",
            "INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "utilisateur",
            "tentatives_echec",
            "INTEGER NOT NULL DEFAULT 0",
        ),
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
        ("bande", "site", "TEXT"),
        ("livraisonaliment", "site", "TEXT"),
        ("achatveto", "site", "TEXT"),
        ("entretien", "site", "TEXT"),
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
        ("evenement", "delivrance_ok", "INTEGER"),
        (
            "perteporcelet",
            "evenement_id",
            "INTEGER REFERENCES evenement(id) ON DELETE CASCADE",
        ),
        ("transfert", "vente_apport_id", "INTEGER"),
        ("declarationmort", "case_id", "INTEGER"),
        ("produitventedirecte", "quantite_disponible", "REAL"),
        ("produitventedirecte", "image_data", "BLOB"),
        ("produitventedirecte", "image_mime", "TEXT"),
        ("commandeventedirecte", "client_id", "INTEGER"),
        (
            "commandeventedirecte",
            "suivi_email",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        (
            "commandeventedirecte",
            "suivi_sms",
            "INTEGER NOT NULL DEFAULT 0",
        ),
        ("commandeventedirecte", "session_vente_id", "INTEGER"),
        (
            "commandeventedirecte",
            "nom_client",
            "TEXT NOT NULL DEFAULT ''",
        ),
        (
            "commandeventedirecte",
            "telephone",
            "TEXT NOT NULL DEFAULT ''",
        ),
        ("commandeventedirecte", "cree_le", "TEXT"),
        ("commandeventedirecte", "email", "TEXT"),
        ("commandeventedirecte", "notes", "TEXT"),
        (
            "commandeventedirecte",
            "statut",
            "TEXT NOT NULL DEFAULT 'nouvelle'",
        ),
        ("commandeventedirecte", "total", "REAL NOT NULL DEFAULT 0"),
        ("commandeventedirecte", "token_modification", "TEXT"),
        ("commandeventedirecte", "code_modification", "TEXT"),
        ("commandeventedirecte", "recap_envoye_le", "TEXT"),
        (
            "sessionventedirecte",
            "nom",
            "TEXT NOT NULL DEFAULT 'Session de vente'",
        ),
        ("sessionventedirecte", "date_creation", "TEXT"),
        ("sessionventedirecte", "date_livraison", "TEXT"),
        (
            "sessionventedirecte",
            "nb_porcs",
            "INTEGER NOT NULL DEFAULT 0",
        ),
        ("sessionventedirecte", "bande_reference", "TEXT"),
        (
            "sessionventedirecte",
            "active",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        ("sessionventedirecte", "notes", "TEXT"),
        ("sessionventedirecte", "date_cloture", "TEXT"),
        ("sessionventedirecte", "date_limite_commandes", "TEXT"),
        ("coutelevageventedirecte", "bande_id", "INTEGER"),
        ("coutelevageventedirecte", "nb_porcs_calcules", "INTEGER"),
        ("coutelevageventedirecte", "poids_moyen_kg", "REAL"),
        ("coutelevageventedirecte", "cout_par_porc", "REAL"),
        ("coutelevageventedirecte", "cout_par_kg", "REAL"),
        ("coutelevageventedirecte", "calcule_le", "TEXT"),
        (
            "reglageventedirecte",
            "commandes_ouvertes",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        ("reglageventedirecte", "message_fermeture", "TEXT"),
        (
            "releve_compteur",
            "remplacement_compteur",
            "INTEGER NOT NULL DEFAULT 0",
        ),
        ("releve_compteur", "prix_unitaire", "REAL"),
        ("venteapport", "montant_ht", "REAL"),
    ] {
        ensure_column(pool, table, column, definition).await?;
    }

    // Reprise 2.2.29 : les anciennes bases ne possédaient pas la colonne
    // `montant_ht` sur les ventes. Pour les apports Cooperl, le HT fiable est
    // déjà conservé dans le JSON de chaque lot. On ne reconstitue jamais un HT
    // depuis le net global : les lignes sans source HT restent volontairement
    // à NULL et sont signalées dans l'interface.
    sqlx::query(
        "UPDATE venteapport SET montant_ht=ROUND((SELECT SUM(CAST(json_extract(j.value,'$.montant_ht') AS REAL)) FROM json_each(venteapport.lots_json) j),2) WHERE montant_ht IS NULL AND json_valid(lots_json) AND json_type(lots_json)='array' AND EXISTS(SELECT 1 FROM json_each(venteapport.lots_json) j WHERE json_extract(j.value,'$.montant_ht') IS NOT NULL)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE venteapport SET montant_ht=ROUND(CAST(json_extract(lots_json,'$.montant_ht') AS REAL),2) WHERE montant_ht IS NULL AND json_valid(lots_json) AND json_type(lots_json)='object' AND json_extract(lots_json,'$.montant_ht') IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query("UPDATE commandeventedirecte SET token_modification=lower(hex(randomblob(32))) WHERE token_modification IS NULL OR trim(token_modification)=''")
        .execute(pool)
        .await?;
    sqlx::query("UPDATE commandeventedirecte SET code_modification=upper(substr(hex(randomblob(8)),1,8)) WHERE code_modification IS NULL OR trim(code_modification)=''")
        .execute(pool)
        .await?;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur")
        .fetch_one(pool)
        .await?;
    if count == 0 {
        let hash = auth::hash_password_async("admin".to_string()).await?;
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

async fn verify_sqlite_pragmas(pool: &SqlitePool) -> anyhow::Result<()> {
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(pool)
        .await?;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(pool)
        .await?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        journal_mode.eq_ignore_ascii_case("wal"),
        "SQLite doit fonctionner en WAL (mode obtenu: {journal_mode})"
    );
    anyhow::ensure!(
        busy_timeout >= 5_000,
        "SQLite busy_timeout inférieur à 5000 ms"
    );
    anyhow::ensure!(
        foreign_keys == 1,
        "les clés étrangères SQLite sont désactivées"
    );
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
    if rows
        .iter()
        .any(|row| row.get::<String, _>("name") == column)
    {
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
    if let Err(error) =
        sqlx::query("INSERT INTO journal(horodatage,utilisateur,action,objet,detail,chemin) VALUES(CURRENT_TIMESTAMP,?,?,?,?,?)")
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
