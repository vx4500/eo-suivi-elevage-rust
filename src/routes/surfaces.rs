//! Repères de surface : aucune conclusion de conformité globale ou de bien-être.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SurfaceConfig {
    secteur: String,
    logement: String,
    surface: Option<f64>,
    longueur: Option<f64>,
    largeur: Option<f64>,
    exclue: Option<f64>,
    poids: Option<f64>,
    objectif: Option<f64>,
    date_saisie: String,
}

impl SurfaceConfig {
    fn utile(&self) -> Option<f64> {
        self.surface
            .or_else(|| Some(self.longueur? * self.largeur?))
            .map(|s| s - self.exclue.unwrap_or(0.0))
    }
    fn validate(&self) -> AppResult<()> {
        if ![
            "",
            "verraterie",
            "gestantes",
            "maternite",
            "ps",
            "pe",
            "engraissement",
        ]
        .contains(&self.secteur.as_str())
            || ![
                "",
                "groupe",
                "reproductrices",
                "individuel",
                "liberte",
                "bloquee",
            ]
            .contains(&self.logement.as_str())
        {
            return Err(AppError::Invalid("Secteur ou logement inconnu".into()));
        }
        for v in [
            self.surface,
            self.longueur,
            self.largeur,
            self.poids,
            self.objectif,
        ]
        .into_iter()
        .flatten()
        {
            if !v.is_finite() || v <= 0.0 || v > 100_000.0 {
                return Err(AppError::Invalid("Les surfaces, dimensions, poids et objectifs doivent être des nombres positifs finis (maximum 100 000)".into()));
            }
        }
        if self.longueur.is_some() != self.largeur.is_some() {
            return Err(AppError::Invalid(
                "Renseigner ensemble longueur et largeur".into(),
            ));
        }
        if self.surface.is_some() && self.longueur.is_some() {
            return Err(AppError::Invalid(
                "Choisir les m² OU longueur × largeur, pas les deux".into(),
            ));
        }
        if let Some(e) = self.exclue {
            if !e.is_finite() || e < 0.0 || self.utile().is_none_or(|s| s <= 0.0) {
                return Err(AppError::Invalid("La surface exclue doit être positive ou nulle et inférieure à la surface totale".into()));
            }
        }
        let compatible = match self.secteur.as_str() {
            "ps" | "pe" | "engraissement" => ["", "groupe"].contains(&self.logement.as_str()),
            "verraterie" | "gestantes" => {
                ["", "reproductrices", "individuel"].contains(&self.logement.as_str())
            }
            "maternite" => ["", "liberte", "bloquee"].contains(&self.logement.as_str()),
            _ => self.logement.is_empty(),
        };
        if !compatible {
            return Err(AppError::Invalid(
                "Le mode de logement ne correspond pas au secteur choisi".into(),
            ));
        }
        Ok(())
    }
}

fn number(form: &HashMap<String, String>, key: &str) -> AppResult<Option<f64>> {
    match form.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => s
            .replace(',', ".")
            .parse()
            .map(Some)
            .map_err(|_| AppError::Invalid(format!("Valeur numérique invalide : {key}"))),
    }
}

pub(super) async fn save(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let config = SurfaceConfig {
        secteur: form_text(&form, "secteur").unwrap_or_default(),
        logement: form_text(&form, "logement").unwrap_or_default(),
        surface: number(&form, "surface")?,
        longueur: number(&form, "longueur")?,
        largeur: number(&form, "largeur")?,
        exclue: number(&form, "exclue")?,
        poids: number(&form, "poids")?,
        objectif: number(&form, "objectif")?,
        date_saisie: Local::now().format("%d/%m/%Y").to_string(),
    };
    config.validate()?;
    let encoded = serde_json::to_string(&config).map_err(anyhow::Error::from)?;
    let result = sqlx::query("UPDATE casesalle SET surface_config=? WHERE id=?")
        .bind(encoded)
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to("/structure").into_response())
}

fn minimum_porc(poids: f64) -> f64 {
    for (limit, surface) in [
        (10.0, 0.15),
        (20.0, 0.20),
        (30.0, 0.30),
        (50.0, 0.40),
        (85.0, 0.55),
        (110.0, 0.65),
    ] {
        if poids <= limit {
            return surface;
        }
    }
    1.0
}

fn besoin_reproductrices(total: i64, cochettes: i64) -> f64 {
    let coefficient = if total < 6 {
        1.1
    } else if total >= 40 {
        0.9
    } else {
        1.0
    };
    ((total - cochettes) as f64 * 2.25 + cochettes as f64 * 1.64) * coefficient
}

fn diagnostic(c: &SurfaceConfig, effectif: i64, cochettes: Option<i64>) -> Value {
    let mut d = json!({"statut":"À renseigner", "niveau":"inconnu", "effectif":effectif,
        "message":"Renseigner le secteur, le logement et la surface utile. Aucune conformité globale n’est évaluée."});
    let Some(surface) = c.utile().filter(|s| *s > 0.0 && s.is_finite()) else {
        return d;
    };
    d["utile"] = json!(format!("{surface:.2}"));
    if effectif < 0 {
        d["message"] =
            json!("Effectif calculé négatif : corriger l’inventaire avant tout diagnostic.");
        return d;
    }
    if effectif == 0 {
        d["statut"] = json!("Aucun animal enregistré");
        d["message"] = json!("Vérifier les affectations et l’inventaire : un effectif nul ne prouve pas que la case est vide.");
        return d;
    }
    d["par_animal"] = json!(format!("{:.3}", surface / effectif as f64));
    let (minimum, reference, message) = match (c.secteur.as_str(), c.logement.as_str()) {
        ("ps" | "pe" | "engraissement", "groupe") => {
            let Some(poids) = c.poids else {
                d["message"] = json!("Saisir le poids vif de contrôle. Pour anticiper, utiliser le poids de sortie. Pour un lot hétérogène, utiliser le poids des plus lourds (estimation prudente).");
                return d;
            };
            (Some(minimum_porc(poids)), c.objectif,
             "Scénario au poids saisi, pas une pesée en temps réel. Minimum France conventionnel, surface seule. Actualiser le poids après croissance ou changement de lot.")
        },
        ("verraterie" | "gestantes", "reproductrices") => {
            let Some(gilts) = cochettes else {
                d["message"] = json!("Rang des reproductrices incomplet : impossible de distinguer truies et cochettes.");
                return d;
            };
            (Some(besoin_reproductrices(effectif, gilts) / effectif as f64), c.objectif,
             "Groupe après saillie déclaré : rang 0 = cochette, rang ≥ 1 = truie. Vérifier ces rangs et la saillie. Correction +10 % si moins de 6 ; −10 % dès 40. Les sols, côtés de case et périodes de logement restent à contrôler.")
        },
        ("maternite", "liberte") => (None, Some(c.objectif.unwrap_or(6.6)),
             "Repère scientifique EFSA 2022 : 6,6 m² accessibles à la truie en maternité liberté. Exclure le nid réservé aux porcelets et les obstacles. Ce n’est pas un minimum légal. Pour une maternité collective, expertise spécifique nécessaire."),
        ("maternite", "bloquee") | ("verraterie" | "gestantes", "individuel") => {
            d["statut"] = json!("Évaluation spécifique nécessaire");
            d["message"] = json!("Les m² ne suffisent pas en contention : dimensions corporelles, liberté de mouvement, durée et stade physiologique à vérifier. Verrats : règles distinctes de 6 m², ou 10 m² pour la monte naturelle, non calculées ici.");
            return d;
        },
        _ => return d,
    };
    if c.secteur == "maternite" && effectif != 1 {
        d["statut"] = json!("Maternité collective à examiner");
        d["message"] = json!("Le repère de case individuelle EFSA ne doit pas être multiplié automatiquement par le nombre de truies.");
        return d;
    }
    d["message"] = json!(message);
    let per = surface / effectif as f64;
    if let Some(min) = minimum {
        d["minimum"] = json!(format!("{min:.3}"));
        d["requis"] = json!(format!("{:.2}", min * effectif as f64));
        // Pas de capacité de groupe de reproductrices : le coefficient varie avec l’effectif.
        if matches!(c.secteur.as_str(), "ps" | "pe" | "engraissement") {
            let capacity = ((surface + 1e-9) / min).floor() as i64;
            d["capacite"] = json!(capacity);
            d["excedent"] = json!((effectif - capacity).max(0));
        }
        if per + 1e-9 < min {
            d["statut"] = json!("Surcharge selon les données saisies");
            d["niveau"] = json!("danger");
            d["manque"] = json!(format!("{:.2}", min * effectif as f64 - surface));
            return d;
        }
        d["statut"] = json!("Minimum de surface atteint");
        d["niveau"] = json!("attention");
    }
    if let Some(target) = reference {
        // Un objectif utilisateur ne peut jamais assouplir le minimum légal.
        let target = target.max(minimum.unwrap_or(0.0));
        d["objectif"] = json!(format!("{target:.3}"));
        d["origine_objectif"] = json!(if c.objectif.is_some() {
            "Objectif personnalisé (relevé au minimum si nécessaire)"
        } else {
            "Repère scientifique EFSA 2022"
        });
        if per + 1e-9 >= target {
            d["statut"] = json!("Objectif de surface atteint");
            d["niveau"] = json!("favorable");
        } else {
            d["statut"] = json!("Sous l’objectif de surface");
            d["niveau"] = json!("attention");
        }
    }
    d
}

pub(super) async fn enrich(pool: &SqlitePool, object: &mut Map<String, Value>) -> AppResult<()> {
    let id = object["id"].as_i64().unwrap_or_default();
    let config = match object.get("surface_config").and_then(Value::as_str) {
        Some(raw) => serde_json::from_str::<SurfaceConfig>(raw).map_err(anyhow::Error::from)?,
        None => SurfaceConfig::default(),
    };
    let porcs = case_pig_count_raw(pool, id).await?;
    let sows = object["truies_presentes"].as_i64().unwrap_or(0);
    let (gilts, unknown): (i64,i64) = sqlx::query_as("SELECT COALESCE(SUM(CASE WHEN rang=0 THEN 1 ELSE 0 END),0),COALESCE(SUM(CASE WHEN rang IS NULL OR rang<0 THEN 1 ELSE 0 END),0) FROM truie WHERE case_id=? AND reformee=0")
        .bind(id).fetch_one(pool).await?;
    let count = if matches!(
        config.secteur.as_str(),
        "verraterie" | "gestantes" | "maternite"
    ) {
        sows
    } else {
        porcs
    };
    let result = diagnostic(
        &config,
        count,
        if unknown == 0 { Some(gilts) } else { None },
    );
    object.insert("porcs_presents".into(), json!(porcs));
    object.insert(
        "surface".into(),
        serde_json::to_value(config).map_err(anyhow::Error::from)?,
    );
    object.insert("densite".into(), result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seuils_legaux_et_bornes() {
        for (weight, min) in [
            (10., 0.15),
            (10.01, 0.20),
            (20., 0.20),
            (20.01, 0.30),
            (30., 0.30),
            (30.01, 0.40),
            (50., 0.40),
            (50.01, 0.55),
            (85., 0.55),
            (85.01, 0.65),
            (110., 0.65),
            (110.01, 1.0),
        ] {
            assert_eq!(minimum_porc(weight), min);
        }
        assert!((besoin_reproductrices(5, 0) - 12.375).abs() < 1e-9);
        assert!((besoin_reproductrices(6, 2) - 12.28).abs() < 1e-9);
        assert!((besoin_reproductrices(40, 0) - 81.0).abs() < 1e-9);
        assert!((besoin_reproductrices(39, 0) - 87.75).abs() < 1e-9);
    }
    #[test]
    fn dimensions_et_donnees_invalides() {
        let mut c = SurfaceConfig {
            longueur: Some(4.),
            largeur: Some(3.),
            exclue: Some(2.),
            ..Default::default()
        };
        assert_eq!(c.utile(), Some(10.));
        assert!(c.validate().is_ok());
        c.surface = Some(12.);
        assert!(c.validate().is_err());
        c.surface = None;
        c.largeur = None;
        assert!(c.validate().is_err());
        c.largeur = Some(3.);
        c.exclue = Some(12.);
        assert!(c.validate().is_err());
        c.exclue = None;
        c.poids = Some(f64::NAN);
        assert!(c.validate().is_err());
        let f = HashMap::from([("surface".into(), "12,5".into())]);
        assert_eq!(number(&f, "surface").unwrap(), Some(12.5));
    }
    #[test]
    fn surcharge_et_objectif_ne_masquent_pas_le_minimum() {
        let mut c = SurfaceConfig {
            secteur: "engraissement".into(),
            logement: "groupe".into(),
            surface: Some(13.),
            poids: Some(110.),
            objectif: Some(0.1),
            ..Default::default()
        };
        let d = diagnostic(&c, 20, Some(0));
        assert_eq!(d["niveau"], "favorable");
        assert_eq!(d["capacite"], 20);
        c.poids = Some(111.);
        let d = diagnostic(&c, 20, Some(0));
        assert_eq!(d["niveau"], "danger");
        assert_eq!(d["excedent"], 7);
        c.poids = None;
        assert_eq!(diagnostic(&c, 20, Some(0))["niveau"], "inconnu");
        assert_eq!(diagnostic(&c, -1, Some(0))["niveau"], "inconnu");
    }
    #[test]
    fn pas_de_fausse_conformite_maternite_ou_vide() {
        let mut c = SurfaceConfig {
            secteur: "maternite".into(),
            logement: "liberte".into(),
            surface: Some(6.6),
            ..Default::default()
        };
        let d = diagnostic(&c, 1, Some(0));
        assert_eq!(d["niveau"], "favorable");
        assert!(d.get("minimum").is_none());
        assert_eq!(diagnostic(&c, 2, Some(0))["niveau"], "inconnu");
        assert_eq!(diagnostic(&c, 0, Some(0))["niveau"], "inconnu");
        c.logement = "bloquee".into();
        assert_eq!(diagnostic(&c, 1, Some(0))["niveau"], "inconnu");
    }
}
