//! Contrôles de la navigation : aucun lien mort, aucune section orpheline.
//!
//! La barre de navigation est la carte du logiciel. Deux erreurs y sont
//! invisibles à la lecture et coûteuses à l'usage : un lien qui pointe vers une
//! route inexistante (l'éleveur tombe sur une page d'erreur), et une section
//! d'accès qui n'est atteignable par aucun menu (la page existe mais personne
//! ne la trouve). Ces tests interdisent les deux.

use std::collections::HashSet;

const BASE: &str = include_str!("../templates/base.html");
const ROUTES: &str = include_str!("../src/routes/mod.rs");
const AUTH: &str = include_str!("../src/auth.rs");

/// Tous les chemins déclarés en GET dans le routeur.
fn routes_get() -> HashSet<String> {
    let mut chemins = HashSet::new();
    let mut reste = ROUTES;
    while let Some(position) = reste.find(".route(") {
        reste = &reste[position + ".route(".len()..];
        let Some(debut) = reste.find('"') else { break };
        let Some(fin) = reste[debut + 1..].find('"') else {
            break;
        };
        let chemin = &reste[debut + 1..debut + 1 + fin];
        // La méthode suit le chemin : on ne retient que les routes lisibles.
        let suite = &reste[debut + 1 + fin..];
        let entete: String = suite.chars().take(40).collect();
        if entete.contains("get(") {
            chemins.insert(chemin.to_string());
        }
    }
    chemins
}

/// Tous les liens internes de la barre de navigation et de la barre du bas.
fn liens_navigation() -> Vec<String> {
    let debut = BASE
        .find("<nav class=\"rust-nav\"")
        .expect("barre de navigation présente");
    let fin = BASE[debut..]
        .find("</nav>")
        .map(|position| debut + position)
        .expect("fin de la barre de navigation");
    let mut zone = BASE[debut..fin].to_string();
    if let Some(pouce) = BASE.find("<nav class=\"nav-pouce\"") {
        let fin_pouce = BASE[pouce..]
            .find("</nav>")
            .map(|f| pouce + f)
            .unwrap_or(pouce);
        zone.push_str(&BASE[pouce..fin_pouce]);
    }
    let mut liens = Vec::new();
    let mut reste = zone.as_str();
    while let Some(position) = reste.find("href=\"/") {
        reste = &reste[position + "href=\"".len()..];
        let Some(fin) = reste.find('"') else { break };
        let lien = &reste[..fin];
        // Les gabarits minijinja ne produisent pas d'URL dynamique dans la nav.
        if !lien.contains('{') {
            liens.push(lien.to_string());
        }
    }
    liens
}

#[test]
fn chaque_lien_de_la_navigation_mene_a_une_route_existante() {
    let routes = routes_get();
    let manquants: Vec<_> = liens_navigation()
        .into_iter()
        .filter(|lien| {
            let sans_ancre = lien.split('#').next().unwrap_or(lien);
            !routes.contains(sans_ancre)
        })
        .collect();
    assert!(
        manquants.is_empty(),
        "liens de navigation sans route GET correspondante : {manquants:?}"
    );
}

/// La barre du bas reprend volontairement des écrans du menu (et propose des
/// replis selon les droits) : seul le menu déroulant doit être sans doublon,
/// sans quoi l'éleveur voit deux chemins pour le même écran.
#[test]
fn le_menu_deroulant_ne_propose_aucun_lien_en_double() {
    let debut = BASE.find("<nav class=\"rust-nav\"").unwrap();
    let fin = BASE[debut..].find("</nav>").map(|p| debut + p).unwrap();
    let mut vus = HashSet::new();
    let mut liens = Vec::new();
    let mut reste = &BASE[debut..fin];
    while let Some(position) = reste.find("href=\"/") {
        reste = &reste[position + "href=\"".len()..];
        let Some(f) = reste.find('"') else { break };
        if !reste[..f].contains('{') {
            liens.push(reste[..f].to_string());
        }
    }
    let doublons: Vec<_> = liens
        .iter()
        .filter(|lien| !vus.insert((*lien).clone()))
        .collect();
    assert!(
        doublons.is_empty(),
        "un même écran est proposé à deux endroits du menu : {doublons:?}"
    );
}

/// Chaque section d'accès (`ACCESS_PAGES`) doit être atteignable : si un
/// administrateur ouvre un droit, l'utilisateur doit trouver l'écran.
#[test]
fn chaque_section_dacces_est_atteignable_depuis_la_navigation() {
    let debut = AUTH
        .find("const ACCESS_PAGES")
        .expect("liste des sections présente");
    let fin = AUTH[debut..].find("];").expect("fin de liste") + debut;
    let mut sections = Vec::new();
    let mut reste = &AUTH[debut..fin];
    while let Some(position) = reste.find("(\"") {
        reste = &reste[position + 2..];
        let Some(f) = reste.find('"') else { break };
        sections.push(reste[..f].to_string());
    }
    assert!(sections.len() > 20, "sections mal extraites : {sections:?}");

    let debut_nav = BASE.find("<nav class=\"rust-nav\"").unwrap();
    let fin_nav = BASE.find("<main class=\"rust-main\">").unwrap();
    let nav = &BASE[debut_nav..fin_nav];
    let oubliees: Vec<_> = sections
        .iter()
        .filter(|section| !nav.contains(&format!("'{section}' in session.pages")))
        .collect();
    assert!(
        oubliees.is_empty(),
        "sections sans entrée dans la navigation : {oubliees:?}"
    );
}

#[test]
fn les_menus_souvrent_au_clic_et_pas_au_survol() {
    // Le survol est intenable au doigt et invisible au clavier.
    assert!(
        !BASE.contains(".rust-menu:hover .rust-sub"),
        "la navigation ne doit plus dépendre du survol"
    );
    assert!(BASE.contains("aria-expanded=true]+.rust-sub"));
    assert!(BASE.contains("<button type=\"button\" aria-expanded=\"false\">Aujourd’hui"));
    assert!(BASE.contains("if(event.key === 'Escape')"));
}

#[test]
fn la_barre_du_bas_et_le_fil_dariane_sont_en_place() {
    assert!(BASE.contains("id=\"fil-ariane\""));
    assert!(BASE.contains("id=\"onglets-section\""));
    assert!(BASE.contains("class=\"nav-pouce\""));
    // Le bouton Saisir réutilise la saisie rapide existante, sans la dupliquer.
    assert!(BASE.contains("saisir.addEventListener('click', function(){ fab.click(); })"));
}

#[test]
fn le_jargon_des_indicateurs_est_explique_dans_le_menu() {
    for (sigle, explication) in [
        ("GTTT", "Résultats de reproduction"),
        ("GTE", "Coûts, marges"),
        ("Références IFIP", "Comparaison à la moyenne nationale"),
    ] {
        assert!(
            BASE.contains(sigle) && BASE.contains(explication),
            "le menu doit expliquer {sigle}"
        );
    }
}

/// Sur téléphone, une page plus large que l'écran est affichée dézoomée et la
/// barre du bas, fixée au viewport de mise en page, sort de la zone visible.
/// Deux garde-fous : le rognage horizontal et l'enveloppement des tableaux.
#[test]
fn rien_ne_peut_elargir_la_page_sur_telephone() {
    assert!(BASE.contains("html,body{overflow-x:clip;max-width:100%}"));
    assert!(BASE.contains("if(table.closest('.rust-scroll')) return;"));
}
