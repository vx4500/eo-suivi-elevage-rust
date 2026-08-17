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
}
