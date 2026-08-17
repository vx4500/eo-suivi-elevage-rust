const PYTHON_V165_ROUTES: [&str; 68] = [
    "/abattoir/saisie",
    "/attente",
    "/bande/{id}/engraisseur",
    "/bande/{id}/inventaire",
    "/bande/{id}/mortalite/{declaration_id}/supprimer",
    "/bande/{id}/transfert-porcs",
    "/bande/{id}/truie/{truie_id}/portee",
    "/cahiers/ajouter",
    "/cause/ajouter",
    "/cause/{id}/supprimer",
    "/desinscription/{token}",
    "/economique/aliment/{id}/bandes",
    "/economique/aliment/{id}/site",
    "/economique/auto-lier",
    "/economique/genetique/{id}/bande",
    "/economique/rattacher-auto",
    "/economique/semence/{id}/bande",
    "/economique/semence/{id}/montant",
    "/economique/vente/{id}/bande",
    "/economique/vente/{id}/lot/{lot_index}/bande",
    "/economique/veto/{id}/bande",
    "/economique/veto/{id}/bandes",
    "/economique/veto/{id}/site",
    "/entretien/ajouter",
    "/export/mise-bas-pdf/{id}",
    "/export/registre-pdf",
    "/import",
    "/import-pdf",
    "/journal/{id}/supprimer",
    "/logo",
    "/maj",
    "/maj/lancer",
    "/maj/zip",
    "/parametres/aliment/ajouter",
    "/parametres/aliment/{id}/modifier",
    "/parametres/aliment/{id}/supprimer",
    "/parametres/demo",
    "/parametres/demo-actif",
    "/parametres/maj",
    "/qr/truie/{file}",
    "/reglages/maj",
    "/saisie-rapide",
    "/salle/{id}/lavage",
    "/sanitaire/generer-protocole",
    "/sauvegarde/restaurer",
    "/scan",
    "/scan/lookup",
    "/stock/doses",
    "/template/truies.csv",
    "/truie/{id}/chaleur",
    "/truie/{id}/echo",
    "/truie/{id}/ia",
    "/truie/{id}/misebas",
    "/truie/{id}/reclasser-verrat",
    "/truie/{id}/sevrage",
    "/truie/{id}/sortie",
    "/truie/{id}/traitement",
    "/truies/transfert",
    "/vente-directe/client/{id}/consentements",
    "/vente-directe/communications",
    "/vente-directe/communications/newsletter-email",
    "/vente-directe/communications/newsletter-sms",
    "/vente-directe/communications/reglages",
    "/vente-directe/communications/test-email",
    "/vente-directe/communications/test-sms",
    "/vente-directe/recalculer-stocks",
    "/vente-directe/session/{id}",
    "/vente-directe/session/{id}/commande/{commande_id}/rattacher",
];

#[test]
fn les_68_routes_python_sont_explicitement_portees() {
    let router = include_str!("../src/routes/mod.rs");
    let missing: Vec<_> = PYTHON_V165_ROUTES
        .iter()
        .filter(|route| !router.contains(&format!("\"{route}\"")))
        .collect();
    assert!(missing.is_empty(), "routes absentes: {missing:?}");
}

#[test]
fn la_version_220_contient_les_ecrans_de_parite() {
    let templates = include_str!("../src/templates.rs");
    for name in [
        "attente.html",
        "planning.html",
        "stock.html",
        "journal.html",
        "scan.html",
        "maj.html",
        "parametres.html",
        "vente_directe_communications.html",
        "vente_session_detail.html",
    ] {
        assert!(templates.contains(name), "écran non enregistré: {name}");
    }
}

#[test]
fn la_saisie_rapide_est_accessible_depuis_toutes_les_pages_connectees() {
    let base = include_str!("../templates/base.html");
    for marker in [
        "id=\"fab\"",
        "id=\"fab-panel\"",
        "action=\"/saisie-rapide\"",
        "fetch('/api/truies')",
        "fetch('/api/bandes-actives')",
        "fetch('/api/cases')",
    ] {
        assert!(base.contains(marker), "élément de saisie rapide absent: {marker}");
    }
}

#[test]
fn la_version_222_couvre_les_demandes_de_suivi() {
    let base = include_str!("../templates/base.html");
    let band = include_str!("../templates/bande.html");
    let sanitary = include_str!("../templates/sanitaire.html");
    assert!(base.contains("data-type=\"mouvement\""));
    assert!(base.contains("/transferts/porcs"));
    assert!(base.contains("rust-sortable"));
    assert!(band.contains("N° marquage"));
    assert!(band.contains("Avance / retard"));
    assert!(sanitary.contains("Porcs traités hors vaccins"));
}

#[test]
fn la_version_223_permet_plusieurs_imports_pdf_securises() {
    let economic = include_str!("../templates/economique.html");
    let routes = include_str!("../src/routes/mod.rs");
    assert!(economic.contains("multiple required"));
    assert!(economic.contains("Jusqu’à 5 PDF"));
    assert!(economic.contains("Ouvrir l’aperçu"));
    assert!(routes.contains("files.len() > 5"));
    assert!(routes.contains("total_size > 40 * 1024 * 1024"));
}
