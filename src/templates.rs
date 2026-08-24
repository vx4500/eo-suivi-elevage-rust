use chrono::NaiveDate;
use minijinja::{Environment, Value};

pub fn build() -> anyhow::Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_global("app_version", env!("CARGO_PKG_VERSION"));
    env.add_template("base.html", include_str!("../templates/base.html"))?;
    env.add_template("login.html", include_str!("../templates/login.html"))?;
    env.add_template(
        "dashboard.html",
        include_str!("../templates/dashboard.html"),
    )?;
    env.add_template("bandes.html", include_str!("../templates/bandes.html"))?;
    env.add_template("bande.html", include_str!("../templates/bande.html"))?;
    env.add_template("truies.html", include_str!("../templates/truies.html"))?;
    env.add_template("truie.html", include_str!("../templates/truie.html"))?;
    env.add_template("energie.html", include_str!("../templates/energie.html"))?;
    env.add_template(
        "economique.html",
        include_str!("../templates/economique.html"),
    )?;
    env.add_template(
        "economique_import_apercu.html",
        include_str!("../templates/economique_import_apercu.html"),
    )?;
    env.add_template(
        "vente_directe.html",
        include_str!("../templates/vente_directe.html"),
    )?;
    env.add_template(
        "vente_directe_commandes.html",
        include_str!("../templates/vente_directe_commandes.html"),
    )?;
    env.add_template(
        "vente_commande_modifier.html",
        include_str!("../templates/vente_commande_modifier.html"),
    )?;
    env.add_template(
        "vente_commande_impression.html",
        include_str!("../templates/vente_commande_impression.html"),
    )?;
    env.add_template(
        "vente_preparation.html",
        include_str!("../templates/vente_preparation.html"),
    )?;
    env.add_template("commande.html", include_str!("../templates/commande.html"))?;
    env.add_template(
        "sauvegarde.html",
        include_str!("../templates/sauvegarde.html"),
    )?;
    env.add_template(
        "utilisateurs.html",
        include_str!("../templates/utilisateurs.html"),
    )?;
    env.add_template(
        "mot_de_passe.html",
        include_str!("../templates/mot_de_passe.html"),
    )?;
    env.add_template("liste.html", include_str!("../templates/liste.html"))?;
    env.add_template(
        "entretien.html",
        include_str!("../templates/entretien.html"),
    )?;
    env.add_template("abattoir.html", include_str!("../templates/abattoir.html"))?;
    env.add_template(
        "quotidien.html",
        include_str!("../templates/quotidien.html"),
    )?;
    env.add_template(
        "inseminations.html",
        include_str!("../templates/inseminations.html"),
    )?;
    env.add_template("gttt.html", include_str!("../templates/gttt.html"))?;
    env.add_template(
        "import_apercu.html",
        include_str!("../templates/import_apercu.html"),
    )?;
    env.add_template(
        "prestataire.html",
        include_str!("../templates/prestataire.html"),
    )?;
    env.add_template(
        "reception.html",
        include_str!("../templates/reception.html"),
    )?;
    env.add_template("gte.html", include_str!("../templates/gte.html"))?;
    env.add_template(
        "genetique.html",
        include_str!("../templates/genetique.html"),
    )?;
    env.add_template(
        "aliment_previsions.html",
        include_str!("../templates/aliment_previsions.html"),
    )?;
    env.add_template(
        "fiche_mise_bas.html",
        include_str!("../templates/fiche_mise_bas.html"),
    )?;
    env.add_template(
        "structure.html",
        include_str!("../templates/structure.html"),
    )?;
    env.add_template(
        "sanitaire.html",
        include_str!("../templates/sanitaire.html"),
    )?;
    env.add_template(
        "pharmacie.html",
        include_str!("../templates/pharmacie.html"),
    )?;
    env.add_template(
        "charcutiers.html",
        include_str!("../templates/charcutiers.html"),
    )?;
    env.add_template(
        "charcutier.html",
        include_str!("../templates/charcutier.html"),
    )?;
    env.add_template("ifip.html", include_str!("../templates/ifip.html"))?;
    env.add_template(
        "productivite.html",
        include_str!("../templates/productivite.html"),
    )?;
    env.add_template("reformes.html", include_str!("../templates/reformes.html"))?;
    env.add_template(
        "cochettes.html",
        include_str!("../templates/cochettes.html"),
    )?;
    env.add_template(
        "correctifs.html",
        include_str!("../templates/correctifs.html"),
    )?;
    env.add_template("apropos.html", include_str!("../templates/apropos.html"))?;
    env.add_template("contact.html", include_str!("../templates/contact.html"))?;
    env.add_template(
        "transferts.html",
        include_str!("../templates/transferts.html"),
    )?;
    env.add_template(
        "effectifs.html",
        include_str!("../templates/effectifs.html"),
    )?;
    env.add_template(
        "vente_sessions.html",
        include_str!("../templates/vente_sessions.html"),
    )?;
    env.add_template(
        "vente_directe_bilan.html",
        include_str!("../templates/vente_directe_bilan.html"),
    )?;
    env.add_template(
        "impression.html",
        include_str!("../templates/impression.html"),
    )?;
    env.add_template("attente.html", include_str!("../templates/attente.html"))?;
    env.add_template("planning.html", include_str!("../templates/planning.html"))?;
    env.add_template("stock.html", include_str!("../templates/stock.html"))?;
    env.add_template("journal.html", include_str!("../templates/journal.html"))?;
    env.add_template("imports.html", include_str!("../templates/imports.html"))?;
    env.add_template(
        "resolution_problemes.html",
        include_str!("../templates/resolution_problemes.html"),
    )?;
    env.add_template("maj.html", include_str!("../templates/maj.html"))?;
    env.add_template(
        "parametres.html",
        include_str!("../templates/parametres.html"),
    )?;
    env.add_template(
        "vente_directe_communications.html",
        include_str!("../templates/vente_directe_communications.html"),
    )?;
    env.add_template(
        "vente_session_detail.html",
        include_str!("../templates/vente_session_detail.html"),
    )?;
    env.add_filter("date_fr", date_fr);
    env.add_filter("euro", euro);
    env.add_filter("decimal1", decimal1);
    env.add_filter("decimal2", decimal2);
    env.add_filter("decimal3", decimal3);
    Ok(env)
}

fn date_fr(value: Value) -> String {
    let raw = value.as_str().unwrap_or_default();
    if raw.is_empty() {
        return "—".to_string();
    }
    let prefix = raw.get(..10).unwrap_or(raw);
    NaiveDate::parse_from_str(prefix, "%Y-%m-%d")
        .map(|date| date.format("%d/%m/%Y").to_string())
        .unwrap_or_else(|_| "—".to_string())
}

fn euro(value: Value) -> String {
    let number = f64::try_from(value).unwrap_or(0.0);
    format!("{number:.2} €").replace('.', ",")
}

fn decimal1(value: Value) -> String {
    format!("{:.1}", f64::try_from(value).unwrap_or(0.0)).replace('.', ",")
}

fn decimal2(value: Value) -> String {
    format!("{:.2}", f64::try_from(value).unwrap_or(0.0)).replace('.', ",")
}

fn decimal3(value: Value) -> String {
    format!("{:.3}", f64::try_from(value).unwrap_or(0.0)).replace('.', ",")
}

#[cfg(test)]
mod tests {
    use minijinja::Value;

    #[test]
    fn tous_les_modeles_html_sont_valides() {
        assert!(
            super::build().is_ok(),
            "les modèles MiniJinja doivent être valides"
        );
    }

    #[test]
    fn une_date_absente_ou_invalide_ne_casse_pas_le_rendu() {
        assert_eq!(super::date_fr(Value::UNDEFINED), "—");
        assert_eq!(super::date_fr(Value::from("31/12/2026")), "—");
        assert_eq!(super::date_fr(Value::from("2026-12-31")), "31/12/2026");
    }
}
