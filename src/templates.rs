use chrono::NaiveDate;
use minijinja::{Environment, Value};

pub fn build() -> anyhow::Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_template(
        "surface_case.html",
        include_str!("../templates/surface_case.html"),
    )?;
    env.add_template(
        "surface_references.html",
        include_str!("../templates/surface_references.html"),
    )?;
    env.add_filter("truncate", |value: String, length: usize| {
        let mut chars = value.chars();
        let text: String = chars.by_ref().take(length).collect();
        if chars.next().is_some() {
            format!("{text}…")
        } else {
            text
        }
    });
    env.add_template(
        "workflow_ui.html",
        include_str!("../templates/workflow_ui.html"),
    )?;
    env.add_template(
        "feuille_mise_bas.html",
        include_str!("../templates/feuille_mise_bas.html"),
    )?;
    env.add_global("demo_portal", crate::demo_portal::enabled());
    env.add_template(
        "demo_acces.html",
        include_str!("../templates/demo_acces.html"),
    )?;
    env.add_template(
        "demo_feedback.html",
        include_str!("../templates/demo_feedback.html"),
    )?;
    env.add_template(
        "solutions_terrain.html",
        include_str!("../templates/solutions_terrain.html"),
    )?;
    env.add_template(
        "documents_obligatoires.html",
        include_str!("../templates/documents_obligatoires.html"),
    )?;
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
        "genetique_import_apercu.html",
        include_str!("../templates/genetique_import_apercu.html"),
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
        "commande_confirmation.html",
        include_str!("../templates/commande_confirmation.html"),
    )?;
    env.add_template(
        "commande_client_modifier.html",
        include_str!("../templates/commande_client_modifier.html"),
    )?;
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
        "import_historique_apercu.html",
        include_str!("../templates/import_historique_apercu.html"),
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
        "machine_soupe_apercu.html",
        include_str!("../templates/machine_soupe_apercu.html"),
    )?;
    env.add_template(
        "fiche_mise_bas.html",
        include_str!("../templates/fiche_mise_bas.html"),
    )?;
    env.add_template(
        "maternite.html",
        include_str!("../templates/maternite.html"),
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
    env.add_filter("id_in_csv", id_in_csv);
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

fn id_in_csv(value: Value, id: i64) -> bool {
    value
        .as_str()
        .unwrap_or_default()
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .any(|candidate| candidate == id)
}

#[cfg(test)]
mod tests {
    use minijinja::Value;
    use std::fs;

    #[test]
    fn tous_les_modeles_html_sont_valides() {
        assert!(
            super::build().is_ok(),
            "les modèles MiniJinja doivent être valides"
        );
    }

    #[test]
    fn tous_les_fichiers_html_sont_enregistres() {
        let templates_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let registry = include_str!("templates.rs");

        for entry in fs::read_dir(templates_dir).expect("dossier templates lisible") {
            let path = entry.expect("entrée de template lisible").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("html") {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("nom de template UTF-8");
            let include = format!("include_str!(\"../templates/{filename}\")");
            assert!(
                registry.contains(&include),
                "template non enregistré dans build() : {filename}"
            );
        }
    }

    #[test]
    fn verrat_toutes_bandes_reste_lisible() {
        let html=super::build().unwrap().get_template("economique.html").unwrap().render(minijinja::context! {
            secteur=>"genetique", session=>serde_json::json!({"role":"admin","peut_modifier":true,"csrf":"test"}),
            genetiques=>serde_json::json!([{"id":1,"toutes_bandes":1,"montant_ht":697.44,"bandes_affectees":"B1, B2, B3","bandes_ids":"1,2,3"}])
        }).unwrap();
        assert!(html.contains("Toutes les bandes · verrat"));
        assert!(html.contains("Supprimer la facture"));
        assert!(!html.contains("<b>B1, B2, B3</b>"));
    }

    #[test]
    fn documents_pdf_complets_et_rendus_sans_erreur() {
        let groups: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../resources/documents-elevage.json")).unwrap();
        assert_eq!(groups.len(), 10);
        assert_eq!(
            groups
                .iter()
                .map(|g| g["items"].as_array().unwrap().len())
                .sum::<usize>(),
            64
        );
        let html = super::build()
            .unwrap()
            .get_template("documents_obligatoires.html")
            .unwrap()
            .render(minijinja::context! { document_groups => groups })
            .unwrap();
        assert_eq!(html.matches("class=\"doc-label\"").count(), 64);
        assert!(html.contains("Justificatif de dérogation TATOUPA"));
        assert!(html.contains("Bordereau de pesée du groupement"));
        assert!(!html.contains("Ce que cette liste couvre"));
        assert!(!html.contains("Registre de visites"));
    }

    #[test]
    fn maternite_affiche_une_seule_rubrique_et_preserve_la_saisie() {
        let env = super::build().unwrap();
        for vue in ["mises-bas", "adoptions", "nourrices", "sevrage", "bilan"] {
            for editable in [true, false] {
                let html = env.get_template("maternite.html").unwrap().render(minijinja::context! {
                    vue => vue,
                    truies_sevrage => serde_json::json!([]),
                    session => serde_json::json!({"peut_modifier":editable,"csrf":"test"}),
                    bande => serde_json::json!({"id":1,"code":"B1"}),
                    totaux => serde_json::json!({"truies":1,"mises_bas":1,"restantes":0,"en_cours":1,"presents":12,"nourrice":0}),
                    truies => serde_json::json!([{"id":7,"num_travail":"123","mise_bas_id":8,"statut_code":"en_cours","statut_libelle":"En cours","porcelets_presents":12,"delivrance_ok":0,"soins_attendus":1}])
                }).unwrap();
                assert!(html.contains(&format!("name=\"vue\" value=\"{vue}\"")));
                assert_eq!(
                    html.split("aria-label=\"Rubriques maternité\"")
                        .nth(1)
                        .unwrap()
                        .split("</nav>")
                        .next()
                        .unwrap()
                        .matches("aria-current=\"page\"")
                        .count(),
                    1
                );
                assert_eq!(html.contains("id=\"mat-search\""), vue == "mises-bas");
                for panel in ["adoptions", "nourrices", "sevrage"] {
                    assert_eq!(html.contains(&format!("id=\"{panel}\"")), vue == panel);
                }
                assert_eq!(
                    html.contains("action=\"/truie/7/misebas\""),
                    vue == "mises-bas" && editable
                );
                if vue == "mises-bas" {
                    assert!(html.contains("<details class=\"mat-sow\" id=\"truie-7\""));
                    assert!(html.contains("Délivrance NOK"));
                    assert!(html.contains("1 soin(s) attendu(s)"));
                    assert!(html.contains("hashchange"));
                }
            }
        }
    }

    #[test]
    fn une_date_absente_ou_invalide_ne_casse_pas_le_rendu() {
        assert_eq!(super::date_fr(Value::UNDEFINED), "—");
        assert_eq!(super::date_fr(Value::from("31/12/2026")), "—");
        assert_eq!(super::date_fr(Value::from("2026-12-31")), "31/12/2026");
    }
}
