//! Générateur de données de démonstration étendues (§ « mode démo » des
//! demandes en attente) : remplace le geste symbolique précédent (une
//! bande, une truie, un événement — voir l'historique git de
//! `routes/parity.rs::demo_basculer`) par un jeu de données permettant de
//! vraiment se faire une idée de l'application sur la durée : plus de 850
//! truies actives, engraissement réparti sur place et chez un prestataire
//! extérieur, et 5 ans d'historique de bandes.
//!
//! Choix de conception pour rester exécutable en une seule transaction sans
//! ralentir un vrai serveur :
//! - Seules les bandes des ~5 derniers mois (« actives ») ont des truies et
//!   des porcs charcutiers individuels — ce sont les seules visibles sur les
//!   écrans de suivi courant (`/truies`, `/bandes`, `/engraissement`).
//! - Les bandes plus anciennes (jusqu'à 5 ans en arrière) n'ont qu'une ligne
//!   `bande` avec ses agrégats de production (`cs_*`) déjà remplis — c'est
//!   exactement ce que lisent les écrans GTTT/productivité historiques
//!   (`bande.cs_truies_mb`, `cs_nt_portee`, etc., pas un événement par
//!   truie), donc l'historique est réellement exploitable sans avoir à
//!   fabriquer des dizaines de milliers de lignes inutiles.
//! - Chaque ligne insérée est tracée dans `demoobjet` pour un retrait propre
//!   (même mécanisme que le mode démo existant), y compris les tables
//!   ajoutées ici (site, utilisateur prestataire, porccharcutier).

use crate::auth;
use chrono::{Duration, NaiveDate};
use rand::{Rng, SeedableRng};
use sqlx::{Sqlite, Transaction};

/// Nombre de bandes « actives » (truies et charcutiers individuels réels) —
/// couvre un peu plus d'un cycle complet de reproduction (gestation +
/// lactation ≈ 150 jours) pour que l'effectif actif soit cohérent avec des
/// bandes qui se chevauchent réellement, pas artificiellement gonflé.
const BANDES_ACTIVES: i64 = 7;
/// Truies par bande active — 7×125 = 875 truies actives, au-delà des « plus
/// de 850 » demandées.
const TRUIES_PAR_BANDE: i64 = 125;
/// Espacement entre deux mises-bas (conduite en bandes classique à 3
/// semaines).
const INTERVALLE_JOURS: i64 = 21;
/// Bandes archivées supplémentaires pour couvrir 5 ans en arrière
/// (5×365 / 21 ≈ 87 bandes au total, moins les actives).
const HORIZON_JOURS: i64 = 5 * 365;

/// Dates de mise-bas des bandes, de la plus récente (aujourd'hui) à la plus
/// ancienne (~5 ans), espacées de `intervalle_jours`. Fonction pure pour
/// pouvoir vérifier le nombre de bandes et l'espacement sans base de
/// données.
fn dates_bandes(aujourdhui: NaiveDate, intervalle_jours: i64, horizon_jours: i64) -> Vec<NaiveDate> {
    if intervalle_jours <= 0 {
        return vec![aujourdhui];
    }
    let nb = (horizon_jours / intervalle_jours).max(1);
    (0..nb).map(|i| aujourdhui - Duration::days(i * intervalle_jours)).collect()
}

async fn marquer(tx: &mut Transaction<'_, Sqlite>, table: &str, id: i64) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO demoobjet(table_name,row_id) VALUES(?,?)")
        .bind(table)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Génère le jeu de données étendu. Doit être appelé dans une transaction
/// déjà ouverte (le·la responsable de `demoobjet` reste `demo_basculer`,
/// qui décide aussi de la suppression symétrique).
pub async fn activer(tx: &mut Transaction<'_, Sqlite>) -> anyhow::Result<()> {
    // `ThreadRng` n'est pas `Send` et ne peut donc pas être conservé à
    // travers les nombreux `.await` de cette fonction (le handler axum
    // exige un futur `Send`) — `StdRng` l'est.
    let mut rng = rand::rngs::StdRng::from_entropy();
    let today = chrono::Local::now().date_naive();

    let site_principal = sqlx::query("INSERT INTO site(code,nom) VALUES('DEMO-SP','Site principal (démo)')")
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
    marquer(tx, "site", site_principal).await?;
    let site_exterieur = sqlx::query("INSERT INTO site(code,nom) VALUES('DEMO-SE','Site extérieur (démo)')")
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
    marquer(tx, "site", site_exterieur).await?;

    let hash = auth::hash_password_async("demo".to_string()).await?;
    let engraisseur_id = sqlx::query(
        "INSERT INTO utilisateur(identifiant,nom,hash_mdp,role,actif,doit_changer_mdp) VALUES('demo_prestataire','Prestataire (démo)',?,'engraisseur',1,1)",
    )
    .bind(&hash)
    .execute(&mut **tx)
    .await?
    .last_insert_rowid();
    marquer(tx, "utilisateur", engraisseur_id).await?;

    let dates = dates_bandes(today, INTERVALLE_JOURS, HORIZON_JOURS);
    let races = ["Large White", "Landrace", "Piétrain"];

    for (index, date_mb) in dates.iter().enumerate() {
        let numero = index + 1;
        let code = format!("DEMO-B{numero}");
        let active = (index as i64) < BANDES_ACTIVES;
        // Un peu plus d'un tiers des bandes actives part chez le
        // prestataire extérieur, le reste reste sur place — reflète la
        // demande « une partie de l'engraissement sur place, l'autre à
        // l'extérieur » sans faire de l'extérieur la majorité.
        let externe = active && numero % 3 == 0;

        // Chiffres de production réalistes avec un peu de bruit, pour que
        // l'historique 5 ans ne soit pas une ligne plate à l'écran.
        let saillies = rng.gen_range(118..=134_i64);
        let pleines = (saillies as f64 * rng.gen_range(0.86..=0.94)).round() as i64;
        let truies_mb = (pleines as f64 * rng.gen_range(0.94..=0.99)).round() as i64;
        let nt_portee = rng.gen_range(13.5..=15.8_f64);
        let nv_portee = nt_portee - rng.gen_range(0.8..=1.6);
        let mn_portee = rng.gen_range(0.7..=1.4_f64);
        let sevres_portee = (nv_portee - rng.gen_range(1.0..=2.0)).max(8.0);
        let total_sevres = (truies_mb as f64 * sevres_portee).round() as i64;
        let tx_pertes_nv = ((nv_portee - sevres_portee) / nv_portee * 100.0).max(0.0);
        let poids_sevrage = rng.gen_range(6.8..=7.6_f64);
        let gmq_ps = rng.gen_range(420.0..=480.0_f64);
        let gmq_engr = rng.gen_range(760.0..=870.0_f64);
        let gmq_nv = rng.gen_range(200.0..=260.0_f64);

        let site_nom = if externe { "Site extérieur (démo)" } else { "Site principal (démo)" };
        let engraisseur_bind = externe.then_some(engraisseur_id);
        let band_id = sqlx::query(
            "INSERT INTO bande(code,date_mb,site,note,active,engraisseur_id,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr,cs_gmq_nv) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&code)
        .bind(date_mb.format("%Y-%m-%d").to_string())
        .bind(site_nom)
        .bind("Donnée de démonstration")
        .bind(active)
        .bind(engraisseur_bind)
        .bind(saillies)
        .bind(pleines)
        .bind(truies_mb)
        .bind(nt_portee)
        .bind(nv_portee)
        .bind(mn_portee)
        .bind(sevres_portee)
        .bind(total_sevres)
        .bind(tx_pertes_nv)
        .bind(poids_sevrage)
        .bind(gmq_ps)
        .bind(gmq_engr)
        .bind(gmq_nv)
        .execute(&mut **tx)
        .await?
        .last_insert_rowid();
        marquer(tx, "bande", band_id).await?;

        if !active {
            continue;
        }

        // Bandes actives seulement : vraies truies et charcutiers
        // individuels, pour que /truies, /bande/{id} et /engraissement
        // aient quelque chose à montrer, pas seulement des agrégats.
        for rang_truie in 0..TRUIES_PAR_BANDE {
            let num_travail = format!("DEMO-{numero}-{rang_truie:03}");
            let race = races[rng.gen_range(0..races.len())];
            let rang = rng.gen_range(1..=7_i64);
            let sow_id = sqlx::query(
                "INSERT INTO truie(num_travail,race,statut,rang,bande_code,note,reformee) VALUES(?,?,'active',?,?,?,0)",
            )
            .bind(&num_travail)
            .bind(race)
            .bind(rang)
            .bind(&code)
            .bind("Donnée de démonstration")
            .execute(&mut **tx)
            .await?
            .last_insert_rowid();
            marquer(tx, "truie", sow_id).await?;

            let nes_totaux = rng.gen_range(11..=18_i64);
            let mort_nes = rng.gen_range(0..=2_i64);
            let momifies = rng.gen_range(0..=1_i64);
            let nes_vifs = (nes_totaux - mort_nes - momifies).max(1);
            let mb_id = sqlx::query(
                "INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,note) VALUES('mise_bas',?,?,?,?,?,?,?,?)",
            )
            .bind(date_mb.format("%Y-%m-%d").to_string())
            .bind(sow_id)
            .bind(band_id)
            .bind(nes_totaux)
            .bind(nes_vifs)
            .bind(mort_nes)
            .bind(momifies)
            .bind("Donnée de démonstration")
            .execute(&mut **tx)
            .await?
            .last_insert_rowid();
            marquer(tx, "evenement", mb_id).await?;

            let nb_sevres = (nes_vifs - rng.gen_range(0..=2_i64)).max(1);
            let date_sevrage = *date_mb + Duration::days(28);
            if date_sevrage <= today {
                let sevrage_id = sqlx::query(
                    "INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,poids_moyen,note) VALUES('sevrage',?,?,?,?,?,?)",
                )
                .bind(date_sevrage.format("%Y-%m-%d").to_string())
                .bind(sow_id)
                .bind(band_id)
                .bind(nb_sevres)
                .bind(poids_sevrage)
                .bind("Donnée de démonstration")
                .execute(&mut **tx)
                .await?
                .last_insert_rowid();
                marquer(tx, "evenement", sevrage_id).await?;

                for porcelet in 0..nb_sevres {
                    let sexe = if (porcelet + rang_truie) % 2 == 0 { "M" } else { "F" };
                    let age_jours = (today - date_sevrage).num_days();
                    let poids1 = poids_sevrage;
                    let poids2 = (age_jours >= 60).then(|| poids_sevrage + gmq_ps / 1000.0 * 32.0);
                    let poids3 = (age_jours >= 150).then(|| poids2.unwrap_or(poids1) + gmq_engr / 1000.0 * 90.0);
                    let destination = if externe { "Extérieur (prestataire)" } else { "Sur place" };
                    let charcutier_id = sqlx::query(
                        "INSERT INTO porccharcutier(date_naissance,bande_code,sexe,poids1,poids2,poids3,destination,note) VALUES(?,?,?,?,?,?,?,?)",
                    )
                    .bind(date_mb.format("%Y-%m-%d").to_string())
                    .bind(&code)
                    .bind(sexe)
                    .bind(poids1)
                    .bind(poids2)
                    .bind(poids3)
                    .bind(destination)
                    .bind("Donnée de démonstration")
                    .execute(&mut **tx)
                    .await?
                    .last_insert_rowid();
                    marquer(tx, "porccharcutier", charcutier_id).await?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_bandes_couvre_lhorizon_avec_le_bon_espacement() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        let dates = dates_bandes(today, 21, 5 * 365);
        // ~87 bandes pour couvrir 5 ans à 21 jours d'intervalle.
        assert_eq!(dates.len(), (5 * 365) / 21);
        assert_eq!(dates[0], today);
        assert_eq!(dates[1], today - Duration::days(21));
        let derniere = *dates.last().unwrap();
        let nb = dates.len() as i64;
        assert_eq!(today - derniere, Duration::days((nb - 1) * 21));
        // La dernière bande reste dans les 21 jours de l'horizon visé (la
        // division entière peut arrondir légèrement en dessous, jamais au-delà).
        assert!(today - derniere <= Duration::days(5 * 365));
        assert!(today - derniere > Duration::days(5 * 365 - 42));
    }

    #[test]
    fn dates_bandes_ne_plante_pas_avec_un_intervalle_nul() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        assert_eq!(dates_bandes(today, 0, 1000), vec![today]);
    }

    /// Vérifie contre une vraie base SQLite (pas seulement en théorie) que
    /// le générateur : respecte les clés étrangères, atteint bien plus de
    /// 850 truies actives, répartit une partie de l'engraissement chez le
    /// prestataire extérieur, couvre 5 ans d'historique de bandes, et que
    /// la suppression symétrique (même logique que `demo_basculer`) retire
    /// tout sans rien laisser derrière ni violer une contrainte.
    #[tokio::test]
    async fn activer_puis_retirer_respecte_les_contraintes_et_les_volumes() -> anyhow::Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;

        let mut tx = pool.begin().await?;
        activer(&mut tx).await?;
        tx.commit().await?;

        let truies_actives: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE reformee=0")
            .fetch_one(&pool)
            .await?;
        assert!(truies_actives > 850, "attendu plus de 850 truies actives, obtenu {truies_actives}");

        let bandes_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande").fetch_one(&pool).await?;
        assert!(bandes_total > 80, "attendu un historique de bandes sur ~5 ans, obtenu {bandes_total}");

        let plus_ancienne: String = sqlx::query_scalar("SELECT MIN(date_mb) FROM bande").fetch_one(&pool).await?;
        let plus_recente: String = sqlx::query_scalar("SELECT MAX(date_mb) FROM bande").fetch_one(&pool).await?;
        let ecart = NaiveDate::parse_from_str(&plus_recente, "%Y-%m-%d")?
            - NaiveDate::parse_from_str(&plus_ancienne, "%Y-%m-%d")?;
        assert!(ecart >= Duration::days(4 * 365), "attendu au moins ~5 ans d'écart, obtenu {ecart}");

        let bandes_externes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE engraisseur_id IS NOT NULL")
            .fetch_one(&pool)
            .await?;
        assert!(bandes_externes > 0, "au moins une bande doit être confiée au prestataire extérieur");
        let bandes_sur_place: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE active=1 AND engraisseur_id IS NULL")
            .fetch_one(&pool)
            .await?;
        assert!(bandes_sur_place > 0, "au moins une bande active doit rester sur place");

        let charcutiers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM porccharcutier").fetch_one(&pool).await?;
        assert!(charcutiers > 0, "des porcs charcutiers doivent être générés pour les bandes déjà sevrées");
        let destinations: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT destination) FROM porccharcutier WHERE destination IS NOT NULL",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(destinations, 2, "les deux destinations (sur place / extérieur) doivent apparaître");

        let total_traces: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet").fetch_one(&pool).await?;
        let total_reel: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM bande)+(SELECT COUNT(*) FROM truie)+(SELECT COUNT(*) FROM evenement)+(SELECT COUNT(*) FROM porccharcutier)+(SELECT COUNT(*) FROM site)+(SELECT COUNT(*) FROM utilisateur)",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(total_traces, total_reel, "chaque ligne générée doit être tracée dans demoobjet");

        // Suppression symétrique (même ordre et mêmes tables que
        // `parity::demo_basculer`) : ne doit rien laisser et ne violer
        // aucune clé étrangère.
        let mut tx = pool.begin().await?;
        for table in ["evenement", "porccharcutier", "truie", "bande", "utilisateur", "site"] {
            let sql = format!("DELETE FROM {table} WHERE id IN(SELECT row_id FROM demoobjet WHERE table_name=?)");
            sqlx::query(&sql).bind(table).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM demoobjet").execute(&mut *tx).await?;
        tx.commit().await?;

        for table in ["bande", "truie", "evenement", "porccharcutier", "site", "utilisateur", "demoobjet"] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&pool).await?;
            assert_eq!(count, 0, "{table} doit être vide après retrait de la démo");
        }
        Ok(())
    }
}
