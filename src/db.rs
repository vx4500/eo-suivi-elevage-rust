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

    auto_assign_economic_invoices(pool).await?;

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

/// Reprend les anciennes affectations puis propose une bande aux factures qui
/// n'en ont aucune. Une correction manuelle, y compris « aucune bande », est
/// verrouillée et ne sera jamais écrasée au prochain démarrage.
pub async fn auto_assign_economic_invoices(pool: &SqlitePool) -> anyhow::Result<u64> {
    let mut changed = 0;
    for (category, table) in [
        ("aliment", "livraisonaliment"),
        ("veto", "achatveto"),
        ("semence", "achatsemence"),
    ] {
        let sql = format!(
            "INSERT OR IGNORE INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) SELECT '{category}',id,bande_id,0 FROM {table} WHERE bande_id IS NOT NULL"
        );
        changed += sqlx::query(&sql).execute(pool).await?.rows_affected();
    }
    changed += sqlx::query("INSERT OR IGNORE INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) SELECT 'genetique',a.id,b.id,0 FROM achatgenetique a JOIN bande b ON b.code=a.bande_code WHERE a.bande_code IS NOT NULL AND trim(a.bande_code)<>''")
        .execute(pool).await?.rows_affected();
    for (category, table) in [("aliment", "livraisonaliment"), ("veto", "achatveto")] {
        let sql = format!("INSERT OR IGNORE INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) SELECT '{category}',x.id,CAST(j.value AS INTEGER),0 FROM {table} x,json_each('['||x.bandes||']') j WHERE x.bandes IS NOT NULL AND json_valid('['||x.bandes||']') AND EXISTS(SELECT 1 FROM bande b WHERE b.id=CAST(j.value AS INTEGER))");
        changed += sqlx::query(&sql).execute(pool).await?.rows_affected();
    }

    for (category, table, center, has_site) in [
        ("aliment", "livraisonaliment", 70_i64, true),
        ("veto", "achatveto", 55_i64, true),
        ("semence", "achatsemence", -114_i64, false),
        ("genetique", "achatgenetique", -180_i64, false),
    ] {
        let site_order = if has_site {
            "CASE WHEN trim(COALESCE(x.site,''))='' THEN 1 WHEN lower(trim(COALESCE(b.site,'')))=lower(trim(x.site)) THEN 0 ELSE 2 END,"
        } else {
            ""
        };
        let sql = format!(
            "WITH candidats AS (SELECT x.id AS facture_id,b.id AS bande_id,ABS((julianday(x.date)-julianday(b.date_mb))-({center})) AS score,ROW_NUMBER() OVER(PARTITION BY x.id ORDER BY {site_order} CASE WHEN x.date IS NULL THEN CASE WHEN b.active=1 THEN 0 ELSE 1 END ELSE ABS((julianday(x.date)-julianday(b.date_mb))-({center})) END,b.active DESC,b.date_mb DESC,b.id DESC) AS rang FROM {table} x CROSS JOIN bande b WHERE b.date_mb IS NOT NULL AND NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie='{category}' AND a.facture_id=x.id) AND NOT EXISTS(SELECT 1 FROM affectationfacturecontrole c WHERE c.categorie='{category}' AND c.facture_id=x.id AND c.verrou_manuel=1)) INSERT OR IGNORE INTO affectationfacturebande(categorie,facture_id,bande_id,automatique,score) SELECT '{category}',facture_id,bande_id,1,score FROM candidats WHERE rang=1"
        );
        changed += sqlx::query(&sql).execute(pool).await?.rows_affected();
    }

    sqlx::query("UPDATE livraisonaliment SET bande_id=(SELECT a.bande_id FROM affectationfacturebande a WHERE a.categorie='aliment' AND a.facture_id=livraisonaliment.id ORDER BY a.id LIMIT 1),bandes=(SELECT GROUP_CONCAT(a.bande_id) FROM affectationfacturebande a WHERE a.categorie='aliment' AND a.facture_id=livraisonaliment.id)").execute(pool).await?;
    sqlx::query("UPDATE achatveto SET bande_id=(SELECT a.bande_id FROM affectationfacturebande a WHERE a.categorie='veto' AND a.facture_id=achatveto.id ORDER BY a.id LIMIT 1),bandes=(SELECT GROUP_CONCAT(a.bande_id) FROM affectationfacturebande a WHERE a.categorie='veto' AND a.facture_id=achatveto.id)").execute(pool).await?;
    sqlx::query("UPDATE achatsemence SET bande_id=(SELECT a.bande_id FROM affectationfacturebande a WHERE a.categorie='semence' AND a.facture_id=achatsemence.id ORDER BY a.id LIMIT 1)").execute(pool).await?;
    sqlx::query("UPDATE achatgenetique SET bande_code=(SELECT b.code FROM affectationfacturebande a JOIN bande b ON b.id=a.bande_id WHERE a.categorie='genetique' AND a.facture_id=achatgenetique.id ORDER BY a.id LIMIT 1)").execute(pool).await?;
    Ok(changed)
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

#[cfg(test)]
mod economic_assignment_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn propose_la_bande_probable_et_respecte_un_retrait_manuel() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        let first =
            sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B1','2026-01-01',1)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        let second =
            sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('B2','2026-04-01',1)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        let invoice = sqlx::query("INSERT INTO livraisonaliment(date,produit,montant_ht) VALUES('2026-03-12','Aliment test',1000)")
            .execute(&pool).await?.last_insert_rowid();
        let removed = sqlx::query("INSERT INTO livraisonaliment(date,produit,montant_ht) VALUES('2026-03-12','Retrait manuel',500)")
            .execute(&pool).await?.last_insert_rowid();
        sqlx::query("INSERT INTO affectationfacturecontrole(categorie,facture_id,verrou_manuel) VALUES('aliment',?,1)")
            .bind(removed).execute(&pool).await?;

        auto_assign_economic_invoices(&pool).await?;
        let proposed: i64 = sqlx::query_scalar("SELECT bande_id FROM affectationfacturebande WHERE categorie='aliment' AND facture_id=?")
            .bind(invoice).fetch_one(&pool).await?;
        let removed_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM affectationfacturebande WHERE categorie='aliment' AND facture_id=?")
            .bind(removed).fetch_one(&pool).await?;
        assert_eq!(proposed, first);
        assert_eq!(removed_count, 0);
        sqlx::query("INSERT INTO affectationfacturebande(categorie,facture_id,bande_id,automatique) VALUES('aliment',?,?,0)")
            .bind(invoice).bind(second).execute(&pool).await?;
        let distributed_total: f64 = sqlx::query_scalar("SELECT CAST(SUM(x.montant_ht/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=x.id)) AS REAL) FROM livraisonaliment x JOIN affectationfacturebande a ON a.categorie='aliment' AND a.facture_id=x.id WHERE x.id=?")
            .bind(invoice).fetch_one(&pool).await?;
        assert!((distributed_total - 1000.0).abs() < 0.001);
        Ok(())
    }
}
