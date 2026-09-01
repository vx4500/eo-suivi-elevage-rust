use serde::Serialize;
use sqlx::FromRow;

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Utilisateur {
    pub id: i64,
    pub identifiant: String,
    pub nom: Option<String>,
    pub prenom: Option<String>,
    pub hash_mdp: String,
    pub role: String,
    pub actif: bool,
    pub sections: Option<String>,
    pub doit_changer_mdp: bool,
    pub tentatives_echec: i64,
    pub bloque_jusqu: Option<String>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Bande {
    pub id: i64,
    pub code: String,
    pub num_officiel: Option<String>,
    pub date_mb: Option<String>,
    pub site: Option<String>,
    pub note: Option<String>,
    pub active: bool,
    pub cs_truies_saillies: Option<i64>,
    pub cs_pleines: Option<i64>,
    pub cs_truies_mb: Option<i64>,
    pub cs_nt_portee: Option<f64>,
    pub cs_nv_portee: Option<f64>,
    pub cs_mn_portee: Option<f64>,
    pub cs_sevres_portee: Option<f64>,
    pub cs_total_sevres: Option<i64>,
    pub cs_tx_pertes_nv: Option<f64>,
    pub cs_poids_sevrage: Option<f64>,
    pub cs_gmq_ps: Option<f64>,
    pub cs_gmq_engr: Option<f64>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Truie {
    pub id: i64,
    pub num_travail: String,
    pub num_national: Option<String>,
    pub rfid: Option<String>,
    pub race: Option<String>,
    pub date_entree: Option<String>,
    pub statut: String,
    pub note: Option<String>,
    pub rang: i64,
    pub date_naissance: Option<String>,
    pub reformee: bool,
    pub date_reforme: Option<String>,
    pub motif_sortie: Option<String>,
    pub mere_cochette: bool,
    pub bande_code: Option<String>,
    pub salle_id: Option<i64>,
    pub case_id: Option<i64>,
    pub lignee_id: Option<i64>,
    pub perf_nt: Option<f64>,
    pub perf_nv: Option<f64>,
    pub perf_mn: Option<f64>,
    pub perf_sevres: Option<f64>,
    pub perf_tx_perte: Option<f64>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Evenement {
    pub id: i64,
    pub r#type: String,
    pub date: String,
    pub truie_id: Option<i64>,
    pub bande_id: Option<i64>,
    pub nes_totaux: Option<i64>,
    pub nes_vifs: Option<i64>,
    pub mort_nes: Option<i64>,
    pub momifies: Option<i64>,
    pub nb_sevres: Option<i64>,
    pub poids_moyen: Option<f64>,
    pub adoptes: Option<i64>,
    pub retires: Option<i64>,
    pub produit: Option<String>,
    pub motif: Option<String>,
    pub resultat: Option<String>,
    pub nb_doses: Option<i64>,
    pub creneaux_ia: Option<String>,
    pub case_id: Option<i64>,
    pub suivi_actif: bool,
    pub delivrance_ok: Option<i64>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct ProduitVenteDirecte {
    pub id: i64,
    pub nom: String,
    pub prix: f64,
    pub unite: String,
    pub actif: bool,
    pub ordre: i64,
    pub quantite_disponible: Option<f64>,
    pub image_mime: Option<String>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct CompteurEnergie {
    pub id: i64,
    pub nom: String,
    pub r#type: String,
    pub site_id: Option<i64>,
    pub unite: String,
    pub rappel_jours: Option<i64>,
    pub actif: bool,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct ReleveCompteur {
    pub id: i64,
    pub compteur_id: i64,
    pub date_releve: String,
    pub valeur_index: f64,
    pub bandes: Option<String>,
    pub note: Option<String>,
    pub remplacement_compteur: bool,
    pub prix_unitaire: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn les_colonnes_nullables_se_decodent_en_option() -> anyhow::Result<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;

        sqlx::query("INSERT INTO utilisateur(identifiant,hash_mdp,role,actif) VALUES('null-test','x','salarie',1)")
            .execute(&pool)
            .await?;
        let user = sqlx::query_as::<_, Utilisateur>("SELECT id,identifiant,nom,prenom,hash_mdp,role,actif,sections,doit_changer_mdp,tentatives_echec,bloque_jusqu FROM utilisateur WHERE identifiant='null-test'")
            .fetch_one(&pool)
            .await?;
        assert!(user.nom.is_none() && user.prenom.is_none() && user.sections.is_none());

        sqlx::query("INSERT INTO bande(code,active) VALUES('NULL-BAND',1)")
            .execute(&pool)
            .await?;
        let band = sqlx::query_as::<_, Bande>("SELECT id,code,num_officiel,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr FROM bande WHERE code='NULL-BAND'")
            .fetch_one(&pool)
            .await?;
        assert!(band.date_mb.is_none() && band.cs_nv_portee.is_none());

        sqlx::query("INSERT INTO truie(num_travail,statut,rang,reformee,mere_cochette) VALUES('NULL-SOW','active',0,0,0)")
            .execute(&pool)
            .await?;
        let sow = sqlx::query_as::<_, Truie>("SELECT id,num_travail,num_national,rfid,race,date_entree,statut,note,rang,date_naissance,reformee,date_reforme,motif_sortie,mere_cochette,bande_code,salle_id,case_id,lignee_id,perf_nt,perf_nv,perf_mn,perf_sevres,perf_tx_perte FROM truie WHERE num_travail='NULL-SOW'")
            .fetch_one(&pool)
            .await?;
        assert!(sow.date_reforme.is_none() && sow.perf_nv.is_none() && sow.lignee_id.is_none());

        sqlx::query("INSERT INTO evenement(type,date,suivi_actif) VALUES('note','2026-08-17',0)")
            .execute(&pool)
            .await?;
        let event = sqlx::query_as::<_, Evenement>("SELECT id,type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,nb_sevres,poids_moyen,adoptes,retires,produit,motif,resultat,nb_doses,creneaux_ia,case_id,suivi_actif,delivrance_ok,note FROM evenement WHERE type='note'")
            .fetch_one(&pool)
            .await?;
        assert!(event.truie_id.is_none() && event.poids_moyen.is_none() && event.note.is_none());
        Ok(())
    }
}
