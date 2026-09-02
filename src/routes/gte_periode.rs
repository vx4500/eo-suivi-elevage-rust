//! GTE sur une période datée, à la structure du livret IFIP GT-Porc.
//!
//! L'écran `/gte` existant donne des indicateurs **par lot**. Le livret GT-Porc,
//! lui, raisonne sur **l'élevage entier entre deux dates** (« Période du
//! 01/01/24 au 31/12/24 ») et range ses indicateurs en quatre blocs :
//! résultats techniques, post-sevrage, engraissement, sevrage-vente. C'est
//! cette présentation qui permet de se comparer aux références nationales.
//!
//! Deux précautions sont prises et assumées partout dans ce fichier :
//!
//! 1. Les indicateurs IFIP dits « technique » ou « standardisé » (IC 8-30,
//!    GMQ 30-115, âge à 115 kg…) sont ramenés à des bornes de poids fixes par
//!    des courbes de croissance et de consommation que l'IFIP ne publie pas.
//!    On ne peut donc pas les reproduire. Ce qui est calculé ici, ce sont les
//!    valeurs **réelles** de l'élevage sur la période. Elles sont affichées à
//!    côté de la référence comme repère, jamais présentées comme équivalentes.
//! 2. Un indicateur dont la donnée manque en base renvoie `None` et s'affiche
//!    « — », avec la raison. Un chiffre inventé serait pire que pas de chiffre :
//!    la comparaison à une référence nationale n'a de sens que si elle est
//!    honnête.

use super::*;

/// Références nationales du livret, par orientation d'élevage.
const REFERENCES_IFIP: &str = include_str!("../../resources/gte-ifip-2024.json");

/// Rendement carcasse par défaut, en pourcentage. Le poids d'un apport
/// abattoir est un poids **carcasse** ; la GTE raisonne en **kilos vifs**.
/// Valeur forfaitaire retenue par l'IFIP dans ce même livret, donc cohérente
/// avec les références auxquelles l'éleveur se compare. Modifiable par le
/// réglage `rendement_carcasse_pct`.
const RENDEMENT_CARCASSE_DEFAUT: f64 = 76.5;

// --- Fonctions pures -------------------------------------------------------

/// Ramène une quantité observée sur `jours` à une base annuelle et à une
/// truie présente. `None` si la période est vide ou le cheptel nul : sur deux
/// jours d'observation, une extrapolation annuelle n'aurait aucun sens.
pub(super) fn par_truie_et_par_an(quantite: f64, truies_presentes: f64, jours: i64) -> Option<f64> {
    (truies_presentes > 0.0 && jours > 0)
        .then(|| quantite / truies_presentes * 365.0 / jours as f64)
}

/// Convertit un poids carcasse en poids vif. `None` si le rendement est
/// aberrant : un rendement nul ou négatif viendrait d'un réglage saisi de
/// travers, et produirait une division par zéro ou un poids négatif.
pub(super) fn poids_vif(poids_carcasse: f64, rendement_pct: f64) -> Option<f64> {
    (rendement_pct > 0.0 && rendement_pct <= 100.0).then(|| poids_carcasse * 100.0 / rendement_pct)
}

/// Taux de pertes et saisies (%), rapporté aux animaux entrés dans la phase.
pub(super) fn taux_pertes_pct(pertes: i64, entres: i64) -> Option<f64> {
    (entres > 0).then(|| 100.0 * pertes as f64 / entres as f64)
}

/// Consommation d'aliment par animal sorti de la phase (kg).
pub(super) fn aliment_par_animal(aliment_kg: f64, animaux: i64) -> Option<f64> {
    (animaux > 0).then(|| aliment_kg / animaux as f64)
}

/// Consommation d'aliment par porc et par jour de présence (kg).
/// `jours_presence` est la somme des journées-animal de la phase.
pub(super) fn aliment_par_porc_jour(aliment_kg: f64, jours_presence: f64) -> Option<f64> {
    (jours_presence > 0.0).then(|| aliment_kg / jours_presence)
}

/// Gain moyen quotidien réel (g/j) entre deux poids et une durée.
/// `None` si la durée est nulle ; une valeur négative est conservée telle
/// quelle — elle signale une saisie incohérente, la masquer la cacherait.
pub(super) fn gmq_reel(poids_entree_kg: f64, poids_sortie_kg: f64, jours: f64) -> Option<f64> {
    (jours > 0.0).then(|| (poids_sortie_kg - poids_entree_kg) * 1000.0 / jours)
}

/// Nombre de jours de la période, bornes incluses, comme le lit un éleveur :
/// du 1er au 31 janvier fait 31 jours, pas 30. `None` si les dates sont
/// invalides ou dans le désordre.
pub(super) fn jours_periode(debut: &str, fin: &str) -> Option<i64> {
    let debut = chrono::NaiveDate::parse_from_str(debut, "%Y-%m-%d").ok()?;
    let fin = chrono::NaiveDate::parse_from_str(fin, "%Y-%m-%d").ok()?;
    let jours = (fin - debut).num_days() + 1;
    (jours > 0).then_some(jours)
}

/// Période par défaut : les douze mois glissants qui précèdent aujourd'hui,
/// même repère que le taux de renouvellement déjà affiché sur cet écran.
pub(super) fn periode_par_defaut() -> (String, String) {
    let fin = Local::now().date_naive();
    let debut = fin - chrono::Duration::days(364);
    (
        debut.format("%Y-%m-%d").to_string(),
        fin.format("%Y-%m-%d").to_string(),
    )
}

// --- Requêtes --------------------------------------------------------------

/// Truies présentes au début et à la fin de la période. La GTE raisonne en
/// « truies présentes » (effectif moyen), pas en truies actives aujourd'hui :
/// un cheptel qui double en cours d'année fausserait tout le reste si on ne
/// prenait que la photo finale.
const TRUIES_PRESENTES_SQL: &str = "SELECT \
    (SELECT COUNT(*) FROM truie WHERE COALESCE(date_entree,'0000-01-01')<=?1 AND (reformee=0 OR COALESCE(date_reforme,'9999-12-31')>?1)),\
    (SELECT COUNT(*) FROM truie WHERE COALESCE(date_entree,'0000-01-01')<=?2 AND (reformee=0 OR COALESCE(date_reforme,'9999-12-31')>?2))";

/// Ventes à l'abattoir de la période : effectif, poids carcasse, et les deux
/// indicateurs de qualité que le livret affiche (TMP, % dans la gamme),
/// pondérés par le nombre de porcs de chaque apport plutôt que par apport —
/// sans quoi un apport de trois porcs pèserait autant qu'un apport de deux
/// cents dans la moyenne.
const VENTES_SQL: &str = "SELECT \
    CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER),\
    CAST(COALESCE(SUM(poids_total),0) AS REAL),\
    (SELECT SUM(tmp*nb_porcs)/NULLIF(SUM(nb_porcs),0) FROM venteapport WHERE date BETWEEN ?1 AND ?2 AND tmp IS NOT NULL),\
    (SELECT SUM(tx_qualification*nb_porcs)/NULLIF(SUM(nb_porcs),0) FROM venteapport WHERE date BETWEEN ?1 AND ?2 AND tx_qualification IS NOT NULL) \
    FROM venteapport WHERE date BETWEEN ?1 AND ?2";

/// Aliment livré sur la période, en kilos, réparti selon le stade de chaque
/// livraison. Les livraisons « tous stades », « auto » non tranché ou de
/// stade inconnu sont comptées à part : les noyer dans un des trois postes
/// fausserait l'indice de consommation de la phase concernée.
const ALIMENT_SQL: &str = "SELECT \
    CAST(COALESCE(SUM(CASE WHEN stade_aliment IN('gestation','lactation') THEN tonnage END),0)*1000 AS REAL),\
    CAST(COALESCE(SUM(CASE WHEN stade_aliment='ps' THEN tonnage END),0)*1000 AS REAL),\
    CAST(COALESCE(SUM(CASE WHEN stade_aliment IN('croissance','finition') THEN tonnage END),0)*1000 AS REAL),\
    CAST(COALESCE(SUM(CASE WHEN stade_aliment NOT IN('gestation','lactation','ps','croissance','finition') THEN tonnage END),0)*1000 AS REAL),\
    CAST(COALESCE(SUM(tonnage),0)*1000 AS REAL) \
    FROM livraisonaliment WHERE COALESCE(NULLIF(trim(date_reference),''),date) BETWEEN ?1 AND ?2";

/// Sevrages de la période : porcelets sevrés et poids moyen au sevrage, qui
/// est le poids d'entrée du post-sevrage. Le poids est pondéré par l'effectif
/// sevré, pour la même raison que les apports ci-dessus.
const SEVRAGES_SQL: &str = "SELECT \
    CAST(COALESCE(SUM(nb_sevres),0) AS INTEGER),\
    (SELECT SUM(poids_moyen*nb_sevres)/NULLIF(SUM(nb_sevres),0) FROM evenement WHERE type='sevrage' AND date BETWEEN ?1 AND ?2 AND poids_moyen IS NOT NULL AND nb_sevres>0) \
    FROM evenement WHERE type='sevrage' AND date BETWEEN ?1 AND ?2";

/// Mortalités déclarées de la période, par phase, plus les saisies à
/// l'abattoir : le livret parle bien de « taux de pertes **et saisies** ».
const PERTES_SQL: &str = "SELECT \
    CAST(COALESCE(SUM(CASE WHEN lower(COALESCE(stade,'')) LIKE '%post%sevrage%' OR lower(COALESCE(stade,''))='ps' THEN nombre END),0) AS INTEGER),\
    CAST(COALESCE(SUM(CASE WHEN lower(COALESCE(stade,'')) LIKE '%engrais%' OR lower(COALESCE(stade,'')) LIKE '%finition%' OR lower(COALESCE(stade,'')) LIKE '%croissance%' THEN nombre END),0) AS INTEGER),\
    (SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM saisieabattoir WHERE date BETWEEN ?1 AND ?2) \
    FROM declarationmort WHERE date BETWEEN ?1 AND ?2";

/// Données brutes d'une période, avant mise en forme.
#[derive(Debug, Default)]
pub(super) struct DonneesPeriode {
    pub truies_debut: i64,
    pub truies_fin: i64,
    pub porcs_vendus: i64,
    pub poids_carcasse_kg: f64,
    pub tmp: Option<f64>,
    pub pct_gamme: Option<f64>,
    pub aliment_truies_kg: f64,
    pub aliment_ps_kg: f64,
    pub aliment_engr_kg: f64,
    pub aliment_non_ventile_kg: f64,
    pub aliment_total_kg: f64,
    pub porcelets_sevres: i64,
    pub poids_sevrage_kg: Option<f64>,
    pub pertes_ps: i64,
    pub pertes_engr: i64,
    pub saisies: i64,
}

impl DonneesPeriode {
    /// Effectif moyen de truies présentes : moyenne des deux bornes. La base
    /// ne conserve pas d'inventaire quotidien du cheptel ; prendre les deux
    /// extrémités est la meilleure approximation disponible, et elle est
    /// exacte quand le cheptel est stable, ce qui est le cas courant.
    pub fn truies_presentes(&self) -> f64 {
        (self.truies_debut + self.truies_fin) as f64 / 2.0
    }

    /// Pertes totales du sevrage à la vente, saisies d'abattoir comprises.
    pub fn pertes_sevrage_vente(&self) -> i64 {
        self.pertes_ps + self.pertes_engr + self.saisies
    }
}

pub(super) async fn charger(
    pool: &SqlitePool,
    debut: &str,
    fin: &str,
) -> AppResult<DonneesPeriode> {
    let (truies_debut, truies_fin): (i64, i64) = sqlx::query_as(TRUIES_PRESENTES_SQL)
        .bind(debut)
        .bind(fin)
        .fetch_one(pool)
        .await?;
    let (porcs_vendus, poids_carcasse_kg, tmp, pct_gamme): (i64, f64, Option<f64>, Option<f64>) =
        sqlx::query_as(VENTES_SQL)
            .bind(debut)
            .bind(fin)
            .fetch_one(pool)
            .await?;
    let (
        aliment_truies_kg,
        aliment_ps_kg,
        aliment_engr_kg,
        aliment_non_ventile_kg,
        aliment_total_kg,
    ): (f64, f64, f64, f64, f64) = sqlx::query_as(ALIMENT_SQL)
        .bind(debut)
        .bind(fin)
        .fetch_one(pool)
        .await?;
    let (porcelets_sevres, poids_sevrage_kg): (i64, Option<f64>) = sqlx::query_as(SEVRAGES_SQL)
        .bind(debut)
        .bind(fin)
        .fetch_one(pool)
        .await?;
    let (pertes_ps, pertes_engr, saisies): (i64, i64, i64) = sqlx::query_as(PERTES_SQL)
        .bind(debut)
        .bind(fin)
        .fetch_one(pool)
        .await?;

    Ok(DonneesPeriode {
        truies_debut,
        truies_fin,
        porcs_vendus,
        poids_carcasse_kg,
        tmp,
        pct_gamme,
        aliment_truies_kg,
        aliment_ps_kg,
        aliment_engr_kg,
        aliment_non_ventile_kg,
        aliment_total_kg,
        porcelets_sevres,
        poids_sevrage_kg,
        pertes_ps,
        pertes_engr,
        saisies,
    })
}

/// Références du livret pour l'orientation active, et l'avertissement de
/// méthode qui les accompagne.
pub(super) fn references(type_elevage: &str) -> AppResult<(Value, Value, f64)> {
    let livret: Value =
        serde_json::from_str(REFERENCES_IFIP).map_err(|e| AppError::Internal(e.into()))?;
    let rendement = livret
        .get("rendement_carcasse_pct")
        .and_then(Value::as_f64)
        .unwrap_or(RENDEMENT_CARCASSE_DEFAUT);
    let orientation = livret
        .get("orientations")
        .and_then(|o| o.get(type_elevage))
        .cloned()
        .unwrap_or(Value::Null);
    let entete = json!({
        "source": livret.get("source"),
        "avertissement": livret.get("avertissement"),
        "rendement_source": livret.get("rendement_carcasse_source"),
    });
    Ok((orientation, entete, rendement))
}

/// Repères de conduite de l'élevage, lus dans les réglages existants : ils
/// donnent les durées de phase, faute de peser chaque animal à chaque
/// changement de salle.
#[derive(Debug, Clone, Copy)]
pub(super) struct Conduite {
    pub sevrage_j: i64,
    pub transfert_engraissement_j: i64,
    pub depart_j: i64,
}

impl Conduite {
    /// Durée d'engraissement, en jours. `None` si les réglages sont dans le
    /// désordre — mieux vaut une case vide qu'une consommation par jour
    /// calculée sur une durée négative.
    pub fn duree_engraissement(&self) -> Option<f64> {
        let jours = self.depart_j - self.transfert_engraissement_j;
        (jours > 0).then_some(jours as f64)
    }

    /// Durée du sevrage au départ, en jours.
    pub fn duree_sevrage_vente(&self) -> Option<f64> {
        let jours = self.depart_j - self.sevrage_j;
        (jours > 0).then_some(jours as f64)
    }
}

/// Une ligne du tableau : la valeur de l'élevage, la référence nationale, et
/// s'il y a lieu la raison pour laquelle la valeur est absente.
fn ligne(
    libelle: &str,
    valeur: Option<f64>,
    decimales: i64,
    reference: Option<f64>,
    raison: Option<&str>,
) -> Value {
    let arrondi = valeur.map(|v| {
        let facteur = 10_f64.powi(decimales as i32);
        (v * facteur).round() / facteur
    });
    json!({
        "libelle": libelle,
        "valeur": arrondi,
        "reference": reference,
        "decimales": decimales,
        "raison": raison,
    })
}

fn reference_de(orientation: &Value, cle: &str) -> Option<f64> {
    orientation.get("valeurs")?.get(cle)?.as_f64()
}

/// Raison unique et répétée pour tous les indicateurs « technique » du
/// livret : ils dépendent d'une standardisation que l'IFIP ne publie pas.
const RAISON_STANDARDISE: &str =
    "Indicateur IFIP standardisé sur des bornes de poids fixes, par des courbes non publiées : non reproductible ici.";

/// Raison des indicateurs qui demandent une pesée aux changements de salle.
const RAISON_PESEE: &str =
    "Aucune pesée enregistrée à ce changement de phase : à saisir pour que l'indicateur se calcule.";

/// Construit les quatre blocs du livret pour la période.
pub(super) fn tableau(
    donnees: &DonneesPeriode,
    jours: i64,
    rendement_pct: f64,
    conduite: Conduite,
    orientation: &Value,
    a_truies: bool,
) -> Vec<Value> {
    let truies = donnees.truies_presentes();
    let kg_vifs_vendus = poids_vif(donnees.poids_carcasse_kg, rendement_pct);
    let poids_vente_moyen = (donnees.porcs_vendus > 0)
        .then(|| kg_vifs_vendus.map(|kg| kg / donnees.porcs_vendus as f64))
        .flatten();
    // Entrées en engraissement : les porcelets sevrés qui ont passé le
    // post-sevrage. Faute de compteur dédié, c'est la meilleure estimation.
    let entres_engraissement = (donnees.porcelets_sevres - donnees.pertes_ps).max(0);
    let kg_sevres = donnees
        .poids_sevrage_kg
        .map(|poids| poids * donnees.porcelets_sevres as f64);

    let mut blocs = Vec::new();

    if a_truies {
        blocs.push(json!({
            "titre": "Résultats techniques",
            "lignes": [
                ligne("Truies présentes (moyenne de la période)", Some(truies), 1, orientation.get("truies_presentes").and_then(Value::as_f64), None),
                ligne(
                    "Porcs produits / truie présente / an",
                    par_truie_et_par_an(donnees.porcs_vendus as f64, truies, jours),
                    1,
                    reference_de(orientation, "porcs_truie_an"),
                    None,
                ),
                ligne(
                    "Kilos vifs produits / truie présente / an",
                    kg_vifs_vendus.and_then(|kg| par_truie_et_par_an(kg, truies, jours)),
                    0,
                    reference_de(orientation, "kg_vifs_truie_an"),
                    None,
                ),
                // Bug trouvé en comparant l'ordre de grandeur à la référence :
                // avec l'aliment total (truies + post-sevrage + engraissement)
                // le résultat sortait à plus de 7 000 kg pour une référence à
                // 1 258. Le livret ne compte ici que l'aliment des truies —
                // une truie mange environ 1,2 tonne par an. L'aliment total,
                // lui, reste au dénominateur de l'indice global juste dessous.
                ligne(
                    "Consommation aliment truies / truie présente / an (kg)",
                    par_truie_et_par_an(donnees.aliment_truies_kg, truies, jours),
                    0,
                    reference_de(orientation, "aliment_truie_an"),
                    None,
                ),
                ligne(
                    "Indice de consommation global",
                    kg_vifs_vendus.and_then(|kg| indice_consommation(donnees.aliment_total_kg, kg)),
                    2,
                    reference_de(orientation, "ic_global"),
                    None,
                ),
            ],
        }));
    }

    blocs.push(json!({
        "titre": "Post-sevrage",
        "lignes": [
            ligne("Poids moyen d'entrée (kg)", donnees.poids_sevrage_kg, 1, reference_de(orientation, "ps_poids_entree"), None),
            ligne("Poids moyen de sortie (kg)", None, 1, reference_de(orientation, "ps_poids_sortie"), Some(RAISON_PESEE)),
            ligne(
                "Taux de pertes et saisies (%)",
                taux_pertes_pct(donnees.pertes_ps, donnees.porcelets_sevres),
                1,
                reference_de(orientation, "ps_taux_pertes"),
                None,
            ),
            ligne(
                "Consommation d'aliment / porcelet sorti (kg)",
                aliment_par_animal(donnees.aliment_ps_kg, entres_engraissement),
                0,
                reference_de(orientation, "ps_aliment_par_porcelet"),
                None,
            ),
            ligne("Indice de consommation technique 8-30", None, 2, reference_de(orientation, "ps_ic_8_30"), Some(RAISON_STANDARDISE)),
            ligne("GMQ technique 8-30 (g/j)", None, 0, reference_de(orientation, "ps_gmq_8_30"), Some(RAISON_STANDARDISE)),
            ligne("Âge à 30 kg standardisé (j)", None, 0, reference_de(orientation, "ps_age_30kg"), Some(RAISON_STANDARDISE)),
        ],
    }));

    blocs.push(json!({
        "titre": "Engraissement",
        "lignes": [
            ligne("Poids moyen d'entrée (kg)", None, 1, reference_de(orientation, "engr_poids_entree"), Some(RAISON_PESEE)),
            ligne("Poids moyen de sortie (kg)", poids_vente_moyen, 1, reference_de(orientation, "engr_poids_sortie"), None),
            ligne(
                "Taux de pertes et saisies (%)",
                taux_pertes_pct(donnees.pertes_engr + donnees.saisies, entres_engraissement),
                1,
                reference_de(orientation, "engr_taux_pertes"),
                None,
            ),
            ligne(
                "Consommation d'aliment / porc / jour (kg)",
                conduite.duree_engraissement().and_then(|duree| {
                    aliment_par_porc_jour(donnees.aliment_engr_kg, donnees.porcs_vendus as f64 * duree)
                }),
                2,
                reference_de(orientation, "engr_aliment_porc_jour"),
                None,
            ),
            ligne("Indice de consommation technique 30-115", None, 2, reference_de(orientation, "engr_ic_30_115"), Some(RAISON_STANDARDISE)),
            ligne("GMQ technique 30-115 (g/j)", None, 0, reference_de(orientation, "engr_gmq_30_115"), Some(RAISON_STANDARDISE)),
        ],
    }));

    blocs.push(json!({
        "titre": "Sevrage-vente",
        "lignes": [
            ligne("Poids moyen d'entrée (kg)", donnees.poids_sevrage_kg, 1, reference_de(orientation, "sv_poids_entree"), None),
            ligne("Poids moyen de sortie (kg)", poids_vente_moyen, 1, reference_de(orientation, "sv_poids_sortie"), None),
            ligne(
                "Taux de pertes et saisies (%)",
                taux_pertes_pct(donnees.pertes_sevrage_vente(), donnees.porcelets_sevres),
                1,
                reference_de(orientation, "sv_taux_pertes"),
                None,
            ),
            ligne(
                "Indice de consommation réel sevrage-vente",
                kg_vifs_vendus.zip(kg_sevres).and_then(|(vendus, sevres)| {
                    indice_consommation(donnees.aliment_ps_kg + donnees.aliment_engr_kg, vendus - sevres)
                }),
                2,
                reference_de(orientation, "sv_ic_8_115"),
                None,
            ),
            ligne(
                "GMQ réel sevrage-vente (g/j)",
                donnees.poids_sevrage_kg.zip(poids_vente_moyen).and_then(|(entree, sortie)| {
                    conduite.duree_sevrage_vente().and_then(|duree| gmq_reel(entree, sortie, duree))
                }),
                0,
                reference_de(orientation, "sv_gmq_8_115"),
                None,
            ),
            ligne("Âge à 115 kg standardisé (j)", None, 0, reference_de(orientation, "sv_age_115kg"), Some(RAISON_STANDARDISE)),
            ligne("T.M.P. (%)", donnees.tmp, 1, reference_de(orientation, "sv_tmp"), None),
            ligne("% de porcs dans la gamme", donnees.pct_gamme, 1, reference_de(orientation, "sv_pct_gamme"), None),
        ],
    }));

    blocs
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn la_mise_a_lannee_refuse_une_periode_ou_un_cheptel_vide() {
        // 250 porcs, 100 truies, une demi-année → 5 porcs/truie/an.
        let valeur = par_truie_et_par_an(250.0, 100.0, 182).expect("période valide");
        assert!((valeur - 5.014).abs() < 0.01, "{valeur}");
        assert_eq!(par_truie_et_par_an(250.0, 0.0, 182), None);
        assert_eq!(par_truie_et_par_an(250.0, 100.0, 0), None);
    }

    #[test]
    fn le_poids_vif_se_deduit_du_rendement_carcasse() {
        // 100 kg de carcasse à 76,5 % de rendement = 130,7 kg vif.
        let vif = poids_vif(100.0, 76.5).expect("rendement valide");
        assert!((vif - 130.718).abs() < 0.01, "{vif}");
        // Un rendement saisi de travers ne doit pas produire un chiffre.
        assert_eq!(poids_vif(100.0, 0.0), None);
        assert_eq!(poids_vif(100.0, -5.0), None);
        assert_eq!(poids_vif(100.0, 150.0), None);
    }

    #[test]
    fn les_taux_et_ratios_ignorent_les_denominateurs_nuls() {
        assert_eq!(taux_pertes_pct(30, 1000), Some(3.0));
        assert_eq!(taux_pertes_pct(30, 0), None);
        assert_eq!(aliment_par_animal(41000.0, 1000), Some(41.0));
        assert_eq!(aliment_par_animal(41000.0, 0), None);
        assert_eq!(aliment_par_porc_jour(2270.0, 1000.0), Some(2.27));
        assert_eq!(aliment_par_porc_jour(2270.0, 0.0), None);
    }

    #[test]
    fn le_gmq_conserve_une_valeur_negative_qui_signale_une_saisie_fausse() {
        // 8 → 30 kg en 47 jours ≈ 468 g/j.
        let gmq = gmq_reel(8.0, 30.0, 47.0).expect("durée valide");
        assert!((gmq - 468.08).abs() < 0.1, "{gmq}");
        // Poids de sortie inférieur au poids d'entrée : anomalie visible.
        assert_eq!(gmq_reel(30.0, 8.0, 47.0), Some(-468.0851063829787));
        assert_eq!(gmq_reel(8.0, 30.0, 0.0), None);
    }

    #[test]
    fn la_periode_compte_les_deux_bornes() {
        assert_eq!(jours_periode("2024-01-01", "2024-01-31"), Some(31));
        assert_eq!(jours_periode("2024-01-01", "2024-12-31"), Some(366));
        assert_eq!(jours_periode("2024-01-01", "2024-01-01"), Some(1));
        // Dates inversées ou illisibles : aucun calcul.
        assert_eq!(jours_periode("2024-12-31", "2024-01-01"), None);
        assert_eq!(jours_periode("hier", "2024-01-01"), None);
    }

    #[test]
    fn la_periode_par_defaut_couvre_douze_mois() {
        let (debut, fin) = periode_par_defaut();
        assert_eq!(jours_periode(&debut, &fin), Some(365));
    }

    #[test]
    fn les_references_suivent_lorientation_et_restent_vides_si_inconnue() {
        let (orientation, entete, rendement) =
            references("naisseur_engraisseur").expect("livret lisible");
        assert_eq!(rendement, 76.5);
        assert_eq!(orientation["valeurs"]["porcs_truie_an"], json!(25.9));
        assert_eq!(orientation["valeurs"]["sv_tmp"], json!(61.2));
        assert!(entete["avertissement"]
            .as_str()
            .unwrap()
            .contains("standardisé"));

        let (engraisseur, _, _) = references("engraisseur").expect("livret lisible");
        // Un engraisseur n'a pas de truies : le livret ne publie pas ces lignes.
        assert!(engraisseur["valeurs"].get("porcs_truie_an").is_none());
        assert_eq!(engraisseur["valeurs"]["engr_ic_30_115"], json!(2.83));

        let (inconnue, _, _) = references("profil_inexistant").expect("livret lisible");
        assert!(inconnue.is_null());
    }

    /// L'écran doit se rendre réellement, avec des valeurs et des trous.
    /// Un test qui ne vérifie que le calcul laisserait passer un filtre
    /// inexistant ou une soustraction sur `none` — erreurs qui n'apparaissent
    /// qu'au rendu.
    #[test]
    fn lecran_se_rend_avec_des_valeurs_et_des_indicateurs_absents() {
        let (orientation, _, rendement) =
            references("naisseur_engraisseur").expect("livret lisible");
        let donnees = DonneesPeriode {
            truies_debut: 200,
            truies_fin: 220,
            porcs_vendus: 5000,
            poids_carcasse_kg: 470_000.0,
            tmp: Some(60.8),
            pct_gamme: Some(82.4),
            aliment_truies_kg: 260_000.0,
            aliment_ps_kg: 210_000.0,
            aliment_engr_kg: 1_150_000.0,
            aliment_non_ventile_kg: 12_000.0,
            aliment_total_kg: 1_632_000.0,
            porcelets_sevres: 5300,
            poids_sevrage_kg: Some(6.8),
            pertes_ps: 130,
            pertes_engr: 160,
            saisies: 20,
        };
        let conduite = Conduite {
            sevrage_j: 28,
            transfert_engraissement_j: 71,
            depart_j: 215,
        };
        let blocs = tableau(&donnees, 365, rendement, conduite, &orientation, true);
        assert_eq!(blocs.len(), 4, "quatre blocs comme le livret");

        let env = crate::templates::build().expect("modèles valides");
        let html = env
            .get_template("gte.html")
            .expect("gte.html enregistré")
            .render(minijinja::context! {
                session => json!({"peut_modifier": true, "csrf": "test", "a_truies": true, "recoit_achats": false}),
                blocs => &blocs,
                bandes => Vec::<Value>::new(),
                synthese_debut => "2024-01-01",
                synthese_fin => "2024-12-31",
                synthese_jours => 366,
                synthese_par_defaut => false,
                date_debut => "2024-01-01",
                date_fin => "2024-12-31",
                periode => 12,
                orientation => &orientation,
                livret => json!({"source": "IFIP GT-Porc 2024", "avertissement": "standardisé", "rendement_carcasse_source": "page 10"}),
                rendement => rendement,
                aliment_non_ventile_kg => 12000.0,
                renouvellement => 42.0,
                app_version => "test",
            })
            .expect("le rendu de gte.html ne doit pas échouer");

        assert!(html.contains("Post-sevrage"), "les blocs sont affichés");
        assert!(html.contains("T.M.P."), "le bloc sevrage-vente est affiché");
        // Un indicateur non calculable montre un tiret et sa raison, pas un zéro.
        assert!(
            html.contains("Indicateur IFIP standardisé"),
            "la raison est visible"
        );
        assert!(html.contains("—"), "les valeurs absentes sont des tirets");
    }

    /// Garde-fou d'ordre de grandeur. Un vrai bug a été trouvé ici : la
    /// consommation d'aliment par truie utilisait l'aliment **total** de
    /// l'élevage et sortait à plus de 7 000 kg pour une référence à 1 258 —
    /// le livret ne compte à cette ligne que l'aliment des truies. Le calcul
    /// « marchait » et passait tous les tests unitaires : seule la comparaison
    /// à la référence l'a révélé. Ce test fige la comparaison.
    #[test]
    fn les_indicateurs_restent_dans_lordre_de_grandeur_du_livret() {
        let (orientation, _, rendement) =
            references("naisseur_engraisseur").expect("livret lisible");
        // Jeu d'essai calé sur un élevage plausible de 210 truies.
        let donnees = DonneesPeriode {
            truies_debut: 200,
            truies_fin: 220,
            porcs_vendus: 5000,
            poids_carcasse_kg: 470_000.0,
            aliment_truies_kg: 260_000.0,
            aliment_ps_kg: 210_000.0,
            aliment_engr_kg: 1_150_000.0,
            aliment_total_kg: 1_632_000.0,
            porcelets_sevres: 5300,
            poids_sevrage_kg: Some(6.8),
            pertes_ps: 130,
            ..Default::default()
        };
        let conduite = Conduite {
            sevrage_j: 28,
            transfert_engraissement_j: 71,
            depart_j: 215,
        };
        let blocs = tableau(&donnees, 365, rendement, conduite, &orientation, true);

        let valeur = |titre: &str, libelle_debut: &str| -> f64 {
            blocs
                .iter()
                .find(|bloc| bloc["titre"] == titre)
                .and_then(|bloc| {
                    bloc["lignes"].as_array()?.iter().find(|ligne| {
                        ligne["libelle"]
                            .as_str()
                            .is_some_and(|l| l.starts_with(libelle_debut))
                    })
                })
                .and_then(|ligne| ligne["valeur"].as_f64())
                .unwrap_or_else(|| panic!("{titre} / {libelle_debut} doit être calculé"))
        };

        // Chaque valeur doit rester à moins de 15 % de la référence nationale :
        // au-delà, c'est une erreur d'unité ou d'assiette, pas une performance
        // d'élevage.
        let proche = |obtenu: f64, reference: f64, quoi: &str| {
            let ecart = (obtenu - reference).abs() / reference;
            assert!(
                ecart < 0.15,
                "{quoi} : {obtenu} contre une référence de {reference} ({:.0} % d'écart)",
                ecart * 100.0
            );
        };

        proche(
            valeur("Résultats techniques", "Porcs produits"),
            25.9,
            "porcs/truie/an",
        );
        proche(
            valeur("Résultats techniques", "Kilos vifs"),
            3089.0,
            "kg vifs/truie/an",
        );
        proche(
            valeur("Résultats techniques", "Consommation aliment truies"),
            1258.0,
            "aliment truies/truie/an",
        );
        proche(
            valeur("Résultats techniques", "Indice de consommation global"),
            2.74,
            "IC global",
        );
        proche(
            valeur("Post-sevrage", "Consommation d'aliment / porcelet"),
            41.0,
            "aliment/porcelet sorti",
        );
        proche(
            valeur("Engraissement", "Poids moyen de sortie"),
            122.4,
            "poids de vente vif",
        );
        proche(
            valeur("Sevrage-vente", "Indice de consommation réel"),
            2.41,
            "IC sevrage-vente",
        );
    }

    /// Les requêtes réelles, rejouées sur une base en mémoire avec un jeu de
    /// données daté : ce qui est hors de la période ne doit rien changer.
    #[tokio::test]
    async fn les_requetes_ne_retiennent_que_la_periode_demandee() -> AppResult<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(AppError::from)?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await
            .map_err(AppError::from)?;

        sqlx::raw_sql(
            "INSERT INTO truie(id,num_travail,date_entree,statut,reformee,rang,mere_cochette) VALUES\
             (1,'T1','2023-01-01','active',0,0,0),\
             (2,'T2','2023-01-01','active',0,0,0),\
             (3,'T3','2024-07-01','active',0,0,0);\
             UPDATE truie SET reformee=1,date_reforme='2024-03-01' WHERE id=2;\
             INSERT INTO bande(id,code,active,date_mb) VALUES(1,'B1',1,'2024-01-10');\
             INSERT INTO venteapport(date,bande_id,nb_porcs,poids_total,tmp,tx_qualification) VALUES\
             ('2024-06-15',1,100,9500,61.0,83.0),\
             ('2024-08-20',1,50,4800,62.0,85.0),\
             ('2025-02-01',1,999,99999,10.0,10.0);\
             INSERT INTO livraisonaliment(date,tonnage,stade_aliment) VALUES\
             ('2024-02-01',20,'gestation'),\
             ('2024-03-01',10,'lactation'),\
             ('2024-04-01',6,'ps'),\
             ('2024-05-01',30,'croissance'),\
             ('2024-06-01',20,'finition'),\
             ('2024-07-01',5,'tous'),\
             ('2025-06-01',999,'ps');\
             INSERT INTO evenement(type,date,bande_id,nb_sevres,poids_moyen,suivi_actif) VALUES\
             ('sevrage','2024-02-10',1,120,6.5,0),\
             ('sevrage','2024-03-10',1,80,7.5,0),\
             ('sevrage','2025-03-10',1,999,99.0,0);\
             INSERT INTO declarationmort(date,stade,nombre) VALUES\
             ('2024-04-05','Post-sevrage',4),\
             ('2024-09-05','Engraissement',6),\
             ('2025-04-05','Post-sevrage',999);\
             INSERT INTO saisieabattoir(date,morceau,nombre) VALUES\
             ('2024-06-16','Carcasse',2),\
             ('2025-06-16','Carcasse',999);",
        )
        .execute(&pool)
        .await
        .map_err(AppError::from)?;

        let d = charger(&pool, "2024-01-01", "2024-12-31").await?;

        // T2 est réformée au 1er mars : présente au début, absente à la fin.
        // T3 entre le 1er juillet : absente au début, présente à la fin.
        assert_eq!((d.truies_debut, d.truies_fin), (2, 2));
        assert_eq!(d.truies_presentes(), 2.0);

        // L'apport de 2025 est hors période.
        assert_eq!(d.porcs_vendus, 150);
        assert_eq!(d.poids_carcasse_kg, 14300.0);
        // TMP pondéré par l'effectif : (61×100 + 62×50) / 150.
        let tmp = d.tmp.expect("TMP présent");
        assert!((tmp - 61.3333).abs() < 0.001, "{tmp}");

        assert_eq!(d.aliment_truies_kg, 30_000.0);
        assert_eq!(d.aliment_ps_kg, 6_000.0);
        assert_eq!(d.aliment_engr_kg, 50_000.0);
        // « tous » n'est ventilé nulle part mais reste dans le total.
        assert_eq!(d.aliment_non_ventile_kg, 5_000.0);
        assert_eq!(d.aliment_total_kg, 91_000.0);

        assert_eq!(d.porcelets_sevres, 200);
        // Poids au sevrage pondéré : (6,5×120 + 7,5×80) / 200 = 6,9.
        let poids = d.poids_sevrage_kg.expect("poids de sevrage présent");
        assert!((poids - 6.9).abs() < 0.001, "{poids}");

        assert_eq!((d.pertes_ps, d.pertes_engr, d.saisies), (4, 6, 2));
        assert_eq!(d.pertes_sevrage_vente(), 12);

        // Une période réduite au seul mois de juin ne garde que ce mois.
        let juin = charger(&pool, "2024-06-01", "2024-06-30").await?;
        assert_eq!(juin.porcs_vendus, 100);
        assert_eq!(juin.aliment_engr_kg, 20_000.0);
        assert_eq!(juin.porcelets_sevres, 0);
        assert_eq!(juin.poids_sevrage_kg, None);
        Ok(())
    }
}
