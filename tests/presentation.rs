#[test]
fn version_affichee_et_artifact_sont_synchronises() {
    let version = env!("CARGO_PKG_VERSION");
    let readme = include_str!("../README.md");
    let state = include_str!("../Etat-projet-eo-suivi-rust.md");
    let changelog = include_str!("../templates/correctifs.html");
    let workflow = include_str!("../.github/workflows/rust.yml");

    assert!(readme.starts_with(&format!("# EO-Suivi Élevage — portage Rust {version}")));
    assert!(state.contains(&format!("Version actuelle : **{version}**")));
    assert!(state.contains(&format!("Version Rust {version}")));
    assert!(changelog.contains(&format!("<h2>{version} —")));
    assert!(workflow.contains(&format!("eo-suivi-elevage-{version}-linux-x86_64-musl")));
}

#[test]
fn documentation_ne_renvoie_plus_vers_les_fichiers_supprimes() {
    let documents = [
        include_str!("../README.md"),
        include_str!("../Etat-projet-eo-suivi-rust.md"),
        include_str!("../templates/correctifs.html"),
    ];
    let deleted = [
        "VERSIONS-RUST.md",
        "MISE-A-JOUR-DEBIAN13.md",
        "AUDIT-PORTAGE-RUST-2.1.2.md",
    ];

    for document in documents {
        for filename in deleted {
            assert!(
                !document.contains(filename),
                "référence obsolète : {filename}"
            );
        }
    }
}

#[test]
fn saisie_rapide_prepare_le_sevrage_avant_lenvoi() {
    let template = include_str!("../templates/base.html");
    let submit = template
        .find("form.addEventListener('submit'")
        .expect("gestionnaire d'envoi de la saisie rapide");
    let script = &template[submit..];
    let serialisation = script
        .find("sevrage_truies').value=JSON.stringify(selected)")
        .expect("sérialisation des truies sélectionnées");
    let form_data = script
        .find("var fd = new FormData(form)")
        .expect("construction des données du formulaire");

    assert!(
        serialisation < form_data,
        "les truies doivent être sérialisées avant FormData"
    );
    assert!(template.contains("sow.disabled=movement||sevrage"));
    assert!(template.contains("selected=sevrageWholeSelection.slice()"));
    assert!(!template.contains("inp.previousSibling.checked"));
}

#[test]
fn accueil_visualise_le_cycle_et_saisie_rapide_propose_la_mise_bas() {
    let dashboard = include_str!("../templates/dashboard.html");
    let base = include_str!("../templates/base.html");
    let styles = include_str!("../static/style.css");
    let routes = include_str!("../src/routes/parity.rs");

    assert!(dashboard.contains("Avancement du cycle"));
    assert!(dashboard.contains("class=\"band-progress\""));
    assert!(dashboard.contains("{% for e in l.etapes %}"));
    assert!(base.contains("data-type=\"mise_bas\""));
    assert!(base.contains("data-for=\"mise_bas\""));
    assert!(base.contains("Perte porc et porcelet"));
    assert!(routes.contains("\"mise_bas\" =>"));
    assert!(routes.contains("perf_nt=?"));
    assert!(dashboard.contains("Conduite des bandes"));
    assert!(dashboard.contains("class=\"band-stage-code\""));
    assert!(base.contains("/static/style.css?v={{ app_version }}"));
    assert!(styles.contains("v2.2.29 — coûts d'élevage"));
}

#[test]
fn version_227_calcule_les_couts_et_permet_la_modification_client() {
    let routes = include_str!("../src/routes/mod.rs");
    let communications = include_str!("../src/routes/parity.rs");
    let sessions = include_str!("../templates/vente_sessions.html");
    let confirmation = include_str!("../templates/commande_confirmation.html");
    let edit = include_str!("../templates/commande_client_modifier.html");
    let schema = include_str!("../migrations/0001_schema.sql");

    assert!(routes.contains("/vente-directe/session/{id}/cout-calculer"));
    assert!(routes.contains("/commande/modifier/{token}"));
    assert!(routes.contains("date_limite_commandes IS NULL OR date('now')"));
    assert!(sessions.contains("coût d’élevage"));
    assert!(sessions.contains("par porc"));
    assert!(sessions.contains("par kg"));
    assert!(confirmation.contains("code personnel"));
    assert!(edit.contains("Enregistrer toute la commande"));
    assert!(communications.contains("envoyer_recap_commande"));
    assert!(schema.contains("token_modification TEXT"));
    assert!(schema.contains("cout_par_porc REAL"));
    assert!(schema.contains("cout_par_kg REAL"));
}

#[test]
fn version_226_imprime_un_bon_de_commande_client_complet() {
    let routes = include_str!("../src/routes/mod.rs");
    let orders = include_str!("../templates/vente_directe_commandes.html");
    let print = include_str!("../templates/vente_commande_impression.html");

    assert!(orders.contains(">Bon de commande</a>"));
    assert!(orders.contains("kg commandés"));
    assert!(routes.contains("AS poids_kg"));
    assert!(routes.contains("adresse_elevage"));
    assert!(print.contains("Produit commandé"));
    assert!(print.contains("Poids commandé"));
    assert!(print.contains("Prix unitaire"));
    assert!(print.contains("TOTAL À RÉGLER"));
    assert!(print.contains("window.print()"));
}

#[test]
fn version_225_affiche_les_flux_et_les_alertes_elevage() {
    let routes = include_str!("../src/routes/mod.rs");
    let base = include_str!("../templates/base.html");
    let dashboard = include_str!("../templates/dashboard.html");
    let bands = include_str!("../templates/bandes.html");
    let band = include_str!("../templates/bande.html");

    assert!(routes.contains("/api/alertes-elevage"));
    assert!(routes.contains("band_flow_summary"));
    assert!(routes.contains("Relevés d’eau attendus"));
    assert!(routes.contains("Relevés d’électricité attendus"));
    assert!(base.contains("id=\"farm-alert-button\""));
    assert!(base.contains("fetch('/api/alertes-elevage'"));
    assert!(dashboard.contains("Truies → verraterie"));
    assert!(bands.contains("{{ l.flux.sevres }} sevrés"));
    assert!(band.contains("{% for vente in flux.ventes %}"));
}

#[test]
fn version_224_corrige_effectifs_pertes_et_recherche() {
    let routes = include_str!("../src/routes/mod.rs");
    let parity = include_str!("../src/routes/parity.rs");
    let band = include_str!("../templates/bande.html");
    let sow = include_str!("../templates/truie.html");
    let list = include_str!("../templates/liste.html");
    let migration = include_str!("../migrations/0001_schema.sql");

    assert!(band.contains("action=\"/effectifs/inventaire-case\""));
    assert!(!band.contains("Derniers emplacements enregistrés"));
    assert!(routes.contains("synchroniser_pertes_mise_bas"));
    assert!(parity.contains("synchroniser_pertes_mise_bas"));
    assert!(migration.contains("evenement_id INTEGER REFERENCES evenement"));
    assert!(sow.contains("{% for c in causes %}"));
    assert!(sow.contains("name=\"tues_truie\""));
    assert!(routes.contains("'/truie/'||id AS lien"));
    assert!(list.contains("{% if row.lien %}"));
}

#[test]
fn mise_a_jour_debian_controle_aussi_la_feuille_de_style() {
    let script = include_str!("../scripts/mettre-a-jour-debian13.sh");

    assert!(script.contains("STYLE_URL="));
    assert!(script.contains("current_style=$(curl"));
    assert!(script.contains("grep -Fq \"v$version\""));
    assert!(script.contains("static_backup="));
}

#[test]
fn vente_directe_classe_les_produits_et_peut_fermer_les_commandes() {
    let routes = include_str!("../src/routes/mod.rs");
    let management = include_str!("../templates/vente_directe.html");
    let customer = include_str!("../templates/commande.html");
    let migration = include_str!("../migrations/0001_schema.sql");

    assert!(routes.contains("/vente-directe/commandes-ouverture"));
    assert!(routes.contains("AS kg_vendus"));
    assert!(routes.contains("AS chiffre_affaires"));
    assert!(management.contains("Classement des produits vendus"));
    assert!(management.contains("⏹ Fermer les commandes"));
    assert!(customer.contains("⏹ Fin de la vente"));
    assert!(customer.contains("{% if not reglage.commandes_ouvertes %}"));
    assert!(migration.contains("commandes_ouvertes INTEGER NOT NULL DEFAULT 1"));
}

#[test]
fn version_220_gere_bandes_causes_assistant_et_images() {
    let routes = include_str!("../src/routes/mod.rs");
    let bands = include_str!("../templates/bandes.html");
    let settings = include_str!("../templates/parametres.html");
    let quick = include_str!("../templates/base.html");
    let assistant = include_str!("../templates/resolution_problemes.html");
    let products = include_str!("../templates/vente_directe.html");

    assert!(bands.contains("Mise-bas {{ l.date_mb|date_fr }}"));
    assert!(settings.contains("/parametres/conduite-bandes"));
    assert!(settings.contains("/parametres/aliment/{{ p.id }}/modifier"));
    assert!(quick.contains("/api/causes-perte"));
    assert!(assistant.contains("Question '+(index+1)+' sur '"));
    assert!(routes.contains("image_data,image_mime"));
    assert!(products.contains("accept=\"image/jpeg,image/png,image/webp\""));
}

#[test]
fn sauvegarde_propose_telechargement_et_restauration_confirmee() {
    let router = include_str!("../src/routes/mod.rs");
    let restoration = include_str!("../src/routes/parity.rs");
    let templates = include_str!("../src/templates.rs");
    let page = include_str!("../templates/maj.html");

    assert!(router.contains("/sauvegarde/telecharger"));
    assert!(router.contains("/sauvegarde/restaurer"));
    assert!(templates.contains("sauvegarde.html"));
    assert!(page.contains("Tapez RESTAURER"));
    assert!(page.contains("enctype=\"multipart/form-data\""));
    assert!(restoration.contains("Some(\"RESTAURER\")"));
    assert!(restoration.contains("PRAGMA foreign_key_check"));
}

#[test]
fn version_221_corrige_commandes_et_suit_la_mise_a_jour() {
    let database = include_str!("../src/db.rs");
    let routes = include_str!("../src/routes/mod.rs");
    let update = include_str!("../scripts/mettre-a-jour-debian13.sh");
    let update_page = include_str!("../templates/maj.html");
    let report = include_str!("../templates/vente_directe_bilan.html");

    assert!(database.contains("\"commandeventedirecte\", \"session_vente_id\""));
    assert!(routes.contains("/vente-directe/session/{id}/cloturer"));
    assert!(routes.contains("AS prix_revient_kg"));
    assert!(report.contains("Prix de revient total"));
    assert!(report.contains("⏹ Fin de vente"));
    assert!(update.contains("status_update \"compilation\""));
    assert!(update_page.contains("fetch('/maj/statut'"));
    assert!(update_page.contains("id=\"sauvegardes\""));
}

#[test]
fn version_222_ordonne_bandes_et_rend_vente_facultative() {
    let auth = include_str!("../src/auth.rs");
    let routes = include_str!("../src/routes/mod.rs");
    let database = include_str!("../src/db.rs");
    let settings = include_str!("../templates/parametres.html");
    let navigation = include_str!("../templates/base.html");

    assert!(routes.contains("ORDER BY date_mb IS NULL,date_mb DESC,id DESC"));
    assert!(routes.contains("affichage de secours"));
    assert!(database.contains("\"commandeventedirecte\", \"email\""));
    assert!(auth.contains("pub module_vente_directe: bool"));
    assert!(settings.contains("name=\"module_vente_directe\""));
    assert!(settings.contains("class=\"module-option\""));
    assert!(settings.contains("Informations de l’élevage"));
    assert!(navigation.contains("{% if session.module_vente_directe %}"));
}

#[test]
fn commandes_clients_est_enregistree_et_journalise_avec_date() {
    let templates = include_str!("../src/templates.rs");
    let database = include_str!("../src/db.rs");

    assert!(templates.contains("\"vente_directe_commandes.html\","));
    assert!(templates.contains("include_str!(\"../templates/vente_directe_commandes.html\")"));
    assert!(database.contains("journal(horodatage,utilisateur"));
    assert!(database.contains("VALUES(CURRENT_TIMESTAMP,?,?,?,?,?)"));
}
