use crate::auth::{self, SessionData};
use crate::config::Config;
use crate::db;
use crate::economic_import::{self, ImportLine};
use crate::error::{AppError, AppResult};
use crate::machine_soupe;
use crate::models::{
    Bande, CompteurEnergie, Evenement, ProduitVenteDirecte, ReleveCompteur, Truie, Utilisateur,
};
use axum::extract::{DefaultBodyLimit, Extension, Form, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Local, NaiveDate};
use dashmap::DashMap;
use minijinja::Environment;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

mod adoptions;
mod ameliorations;
mod demo_portal;
pub(crate) mod documents;
mod factures;
mod feuille_mise_bas;
mod historique_truie;
mod import_historique;
mod maternite_suivi;
mod parity;
mod surfaces;
mod ventes;

fn contenu_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn refuser_fichier_deja_importe(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    digest: &str,
) -> AppResult<()> {
    let existe: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM importjournal WHERE contenu_sha256=?")
            .bind(digest)
            .fetch_one(&mut **transaction)
            .await?;
    if existe > 0 {
        return Err(AppError::Invalid(
            "Ce fichier a déjà été importé, même sous un autre nom".into(),
        ));
    }
    Ok(())
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: SqlitePool,
    pub templates: Arc<Environment<'static>>,
    pub sessions: Arc<DashMap<String, SessionData>>,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool, templates: Environment<'static>) -> Self {
        Self {
            config,
            pool,
            templates: Arc::new(templates),
            sessions: Arc::new(DashMap::new()),
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/login", get(login_page).post(login_post))
        .route(
            "/demo/acces",
            get(demo_portal::acces).post(demo_portal::creer),
        )
        .route("/demo/acces/{id}/revoquer", post(demo_portal::revoquer))
        .route("/demo/suggestion", post(demo_portal::suggestion))
        .route("/logout", get(logout))
        .route("/mon-compte/mdp", get(password_page).post(password_post))
        .route("/bandes", get(bandes))
        .route("/bandes/ajouter", post(bande_ajouter))
        .route("/marquages/ajouter", post(marquage_ajouter))
        .route(
            "/maternite/mise-bas/{id}/etat",
            post(maternite_suivi::changer_etat),
        )
        .route("/bandes/{id}/modifier-rapide", post(bande_modifier_rapide))
        .route("/bande/{id}", get(bande_detail))
        .route("/bande/{id}/marquage", post(bande_marquage))
        .route("/bande/{id}/imprimer", get(bande_imprimer))
        .route("/export/mise-bas/{id}", get(export_mise_bas))
        .route("/bande/{id}/fiche-mise-bas", get(fiche_mise_bas))
        .route("/bande/{id}/feuille-mise-bas", get(feuille_mise_bas::page))
        .route("/maternite", get(maternite))
        .route(
            "/maternite/bande/{band_id}/adoption",
            post(adoptions::transferer),
        )
        .route(
            "/maternite/bande/{band_id}/nourrice/{adoption_id}/sortie",
            post(adoptions::sortie_nourrice),
        )
        .route(
            "/maternite/bande/{band_id}/sevrage",
            post(maternite_sevrage),
        )
        .route(
            "/maternite/bande/{band_id}/truie/{sow_id}/perte",
            post(maternite_perte),
        )
        .route(
            "/maternite/bande/{band_id}/perte/{loss_id}/supprimer",
            post(maternite_perte_supprimer),
        )
        .route("/bande/{id}/archiver", post(bande_archiver))
        .route("/bande/{id}/desarchiver", post(bande_desarchiver))
        .route("/bande/{id}/supprimer", post(bande_supprimer))
        .route("/archives", get(archives))
        .route("/truies", get(truies))
        .route("/truies/ajouter", post(truie_ajouter))
        .route("/truies/modele.csv", get(truies_modele_csv))
        .route("/truies/import", post(truies_import))
        .route("/truies/import/confirmer", post(truies_import_confirmer))
        .route("/truies/import/annuler", post(truies_import_annuler))
        .route(
            "/truies/import-historique/modele.csv",
            get(import_historique::modele_csv),
        )
        .route(
            "/truies/import-historique",
            post(import_historique::importer).layer(DefaultBodyLimit::max(15 * 1024 * 1024)),
        )
        .route(
            "/truies/import-historique/confirmer",
            post(import_historique::confirmer),
        )
        .route(
            "/truies/import-historique/annuler",
            post(import_historique::annuler),
        )
        .route("/truies/affecter-bande", post(truies_affecter_bande))
        .route("/truie/{id}", get(truie_detail))
        .route("/truie/{id}/imprimer", get(truie_imprimer))
        .route("/truie/{id}/bande", post(truie_bande))
        .route("/truie/{id}/lignee", post(truie_lignee))
        .route("/truie/{id}/emplacement", post(truie_emplacement))
        .route("/truie/{id}/reformer", post(truie_reformer))
        .route("/truie/{id}/annuler-sortie", post(truie_annuler_sortie))
        .route("/truie/{id}/mesure", post(truie_mesure))
        .route("/mesure/{id}/modifier", post(mesure_modifier))
        .route("/mesure/{id}/supprimer", post(mesure_supprimer))
        .route("/truie/{id}/perte", post(truie_perte))
        .route("/perte/{id}/supprimer", post(perte_supprimer))
        .route("/truie/{id}/cochette", post(truie_cochette))
        .route("/evenement/ajouter", post(evenement_ajouter))
        .route("/evenement/{id}/supprimer", post(evenement_supprimer))
        .route("/evenement/{id}/modifier", post(evenement_modifier))
        .route("/inseminations", get(inseminations))
        .route(
            "/inseminations/enregistrer",
            post(inseminations_enregistrer),
        )
        .route("/truies/imprimer", post(truies_imprimer))
        .route("/soin-portee/{id}/realiser", post(soin_portee_realiser))
        .route("/recherche", get(recherche))
        .route("/gttt", get(gttt))
        .route("/productivite", get(productivite))
        .route("/objectif/maj", post(objectifs_maj))
        .route("/objectif/ajouter", post(objectif_ajouter))
        .route("/objectif/{id}/supprimer", post(objectif_supprimer))
        .route("/reformes", get(reformes))
        .route("/reformes/seuils", post(reformes_seuils))
        .route("/reformes/criteres", post(reformes_criteres))
        .route("/cochettes", get(cochettes))
        .route("/cochettes/criteres", post(cochettes_criteres))
        .route("/ifip", get(ifip))
        .route("/ifip/maj", post(ifip_maj))
        .route("/charcutiers", get(charcutiers))
        .route("/charcutier/{id}", get(charcutier_detail))
        .route("/charcutier/{id}/traitement", post(charcutier_traitement))
        .route(
            "/traitement-charc/{id}/supprimer",
            post(charcutier_traitement_supprimer),
        )
        .route("/transferts", get(transferts))
        .route("/transferts/porcs", post(transferts_porcs))
        .route("/transferts/truies", post(transferts_truies))
        .route("/transfert/{id}/supprimer", post(transfert_supprimer))
        .route(
            "/truie/{id}/selection",
            post(ameliorations::selection_truie),
        )
        .route(
            "/evenement/{id}/mise-bas",
            post(ameliorations::mise_bas_modifier),
        )
        .route(
            "/quotidien/{id}/modifier",
            post(ameliorations::note_modifier),
        )
        .route(
            "/quotidien/{id}/supprimer",
            post(ameliorations::note_supprimer),
        )
        .route("/taches/{id}/modifier", post(ameliorations::tache_modifier))
        .route(
            "/entretien/{id}/modifier",
            post(ameliorations::entretien_modifier),
        )
        .route(
            "/energie/releve/{id}/bandes",
            post(ameliorations::releve_bandes),
        )
        .route(
            "/energie/compteur/{id}/site",
            post(ameliorations::compteur_site),
        )
        .route(
            "/stock/inventaire.csv",
            get(ameliorations::inventaire_export),
        )
        .route(
            "/stock/inventaire/importer",
            post(ameliorations::inventaire_import),
        )
        .route("/effectifs", get(effectifs))
        .route("/effectifs/inventaire", post(effectifs_inventaire))
        .route(
            "/effectifs/inventaire-case",
            post(effectifs_inventaire_case),
        )
        .route("/etat-donnees", get(etat_donnees))
        .route("/api/bandes-actives", get(api_bandes_actives))
        .route("/api/bandes", get(api_bandes))
        .route("/api/truies", get(api_truies))
        .route("/api/truies-sevrage", get(api_truies_sevrage))
        .route(
            "/api/bande/{id}/sevrage-estimate",
            get(api_bande_sevrage_estimate),
        )
        .route("/api/bande/{id}", get(api_bande_json))
        .route("/api/cases", get(api_cases))
        .route("/api/cases-capacity", get(api_cases_capacity))
        .route("/api/causes-perte", get(api_causes_perte))
        .route("/api/alertes-elevage", get(api_alertes_elevage))
        .route("/energie", get(energie))
        .route("/aliment-previsions", get(aliment_previsions))
        .route("/aliment-previsions/silo", post(silo_ajouter))
        .route(
            "/aliment-previsions/silo/{id}/releve",
            post(silo_releve_ajouter),
        )
        .route(
            "/aliment-previsions/silo/{id}/supprimer",
            post(silo_supprimer),
        )
        .route(
            "/aliment-previsions/machine-soupe",
            post(machine_soupe_import).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route(
            "/aliment-previsions/machine-soupe/confirmer",
            post(machine_soupe_import_confirmer),
        )
        .route(
            "/aliment-previsions/machine-soupe/annuler",
            post(machine_soupe_import_annuler),
        )
        .route("/energie/compteur", post(energie_compteur))
        .route("/energie/releve", post(energie_releve))
        .route("/energie/compteur/{id}/rappel", post(energie_rappel))
        .route(
            "/energie/releve/{id}/supprimer",
            post(energie_releve_supprimer),
        )
        .route("/energie/modele.csv", get(energie_modele_csv))
        .route("/energie/import", post(energie_import))
        .route("/economique", get(economique))
        .route(
            "/economique/{category}/{id}/contexte",
            post(factures::contexte),
        )
        .route("/economique/genetique/{id}/ht", post(factures::ht))
        .route("/gte", get(gte))
        .route(
            "/economique/import-pdf",
            post(economique_import_pdf).layer(DefaultBodyLimit::max(42 * 1024 * 1024)),
        )
        .route("/economique/import/{token}", get(economique_import_apercu))
        .route(
            "/economique/import/{token}/confirmer",
            post(economique_import_confirmer),
        )
        .route(
            "/economique/import/{token}/annuler",
            post(economique_import_annuler),
        )
        .route("/economique/aliment", post(economique_aliment))
        .route("/economique/veto", post(economique_veto))
        .route("/economique/vente", post(economique_vente))
        .route("/economique/semence", post(economique_semence))
        .route("/economique/genetique", post(economique_genetique))
        .route("/economique/valorisation", post(economique_valorisation))
        .route(
            "/economique/valorisation/{id}/supprimer",
            post(economique_valorisation_supprimer),
        )
        .route(
            "/economique/aliment/{id}/supprimer",
            post(economique_aliment_supprimer),
        )
        .route(
            "/economique/veto/{id}/supprimer",
            post(economique_veto_supprimer),
        )
        .route(
            "/economique/vente/{id}/supprimer",
            post(ventes::remove_apport),
        )
        .route(
            "/economique/semence/{id}/supprimer",
            post(economique_semence_supprimer),
        )
        .route(
            "/economique/genetique/{id}/supprimer",
            post(economique_genetique_supprimer),
        )
        .route("/vente-directe", get(vente_directe))
        .route("/vente-directe/commandes", get(vente_directe_commandes))
        .route("/vente-directe/bilan", get(vente_directe_bilan))
        .route("/vente-directe/produit-ajouter", post(produit_ajouter))
        .route("/vente-directe/produit/{id}", post(produit_modifier))
        .route(
            "/vente-directe/produit/{id}/image",
            get(produit_image).post(produit_image_maj),
        )
        .route(
            "/vente-directe/enseigne-logo",
            get(vente_enseigne_logo).post(vente_enseigne_logo_maj),
        )
        .route(
            "/vente-directe/produit/{id}/inventaire",
            post(produit_inventaire),
        )
        .route(
            "/vente-directe/produit/{id}/deplacer",
            post(produit_deplacer),
        )
        .route(
            "/vente-directe/reglage-livraison",
            post(vente_reglage_livraison),
        )
        .route(
            "/vente-directe/commandes-ouverture",
            post(vente_commandes_ouverture),
        )
        .route("/vente-directe/session/creer", post(vente_session_creer))
        .route(
            "/vente-directe/session/{id}/activer",
            post(vente_session_activer),
        )
        .route(
            "/vente-directe/session/{id}/cloturer",
            post(vente_session_cloturer),
        )
        .route(
            "/vente-directe/session/{id}/modifier",
            post(vente_session_modifier),
        )
        .route(
            "/vente-directe/session/{id}/couts",
            post(vente_session_couts),
        )
        .route(
            "/vente-directe/session/{id}/cout-calculer",
            post(vente_session_cout_calculer),
        )
        .route(
            "/vente-directe/session/{id}/charge-ajouter",
            post(vente_session_charge_ajouter),
        )
        .route(
            "/vente-directe/session/{id}/charge/{charge_id}/supprimer",
            post(vente_session_charge_supprimer),
        )
        .route(
            "/vente-directe/commande/{id}/session",
            post(vente_commande_session),
        )
        .route(
            "/vente-directe/commande/{id}",
            get(vente_commande_modifier_page),
        )
        .route(
            "/vente-directe/commande/{id}/modifier",
            post(vente_commande_modifier),
        )
        .route(
            "/vente-directe/commande/{id}/imprimer",
            get(vente_commande_imprimer),
        )
        .route(
            "/vente-directe/preparation/imprimer",
            get(vente_preparation_imprimer),
        )
        .route("/vente-directe/commande/{id}/statut", post(commande_statut))
        .route(
            "/vente-directe/commande/{id}/supprimer",
            post(commande_supprimer),
        )
        .route("/commande", get(commande_page).post(commande_post))
        .route("/commande/confirmation/{token}", get(commande_confirmation))
        .route(
            "/commande/modifier/{token}",
            get(commande_client_modifier_page).post(commande_client_modifier),
        )
        .route("/utilisateurs", get(utilisateurs))
        .route("/utilisateurs/creer", post(utilisateur_creer))
        .route("/utilisateurs/{id}/actif", post(utilisateur_actif))
        .route("/utilisateurs/{id}/sections", post(utilisateur_sections))
        .route("/utilisateurs/{id}/mdp", post(utilisateur_mdp))
        .route("/sauvegarde", get(sauvegarde))
        .route("/sauvegarde/telecharger", get(sauvegarde_telecharger))
        .route("/structure", get(structure))
        .route("/structure/site", post(structure_site))
        .route(
            "/structure/site/{id}/modifier",
            post(structure_site_modifier),
        )
        .route("/structure/salle", post(structure_salle))
        .route("/structure/case", post(structure_case))
        .route("/structure/case/{id}/surface", post(surfaces::save))
        .route(
            "/structure/salle/{id}/modifier",
            post(structure_salle_modifier),
        )
        .route("/structure/salle/{id}/ordre", post(structure_salle_ordre))
        .route(
            "/structure/salle/{id}/supprimer",
            post(structure_salle_supprimer),
        )
        .route("/structure/case/{id}/rfid", post(structure_case_rfid))
        .route(
            "/structure/case/{id}/supprimer",
            post(structure_case_supprimer),
        )
        .route(
            "/structure/site/{id}/supprimer",
            post(structure_site_supprimer),
        )
        .route("/taches", get(taches))
        .route("/taches/ajouter", post(tache_ajouter))
        .route("/taches/{id}/fait", post(tache_fait))
        .route("/taches/{id}/supprimer", post(tache_supprimer))
        .route("/sanitaire", get(sanitaire))
        .route("/resolution-problemes", get(resolution_problemes))
        .route("/pharmacie", get(pharmacie))
        .route("/sanitaire/acte/ajouter", post(sanitaire_acte_ajouter))
        .route("/sanitaire/acte/modifier", post(sanitaire_acte_modifier))
        .route("/sanitaire/acte/supprimer", post(sanitaire_acte_supprimer))
        .route("/sanitaire/fait", post(sanitaire_fait))
        .route("/sanitaire/fait-verrat", post(sanitaire_fait_verrat))
        .route("/pharmacie/mouvement", post(pharmacie_mouvement))
        .route("/pharmacie/regler", post(pharmacie_regler))
        .route("/planning", get(planning))
        .route("/calendrier.ics", get(calendrier_ics))
        .route("/imports", get(imports_page))
        .route("/stock", get(stock))
        .route("/journal", get(journal))
        .route("/entretien", get(entretien).post(entretien_ajouter))
        .route("/entretien/{id}/date", post(entretien_date))
        .route("/entretien/{id}/supprimer", post(entretien_supprimer))
        .route("/engraissement", get(engraissement))
        .route("/declaration", post(declaration_ajouter))
        .route("/declaration/{id}/supprimer", post(declaration_supprimer))
        .route("/reception", get(reception).post(reception_ajouter))
        .route("/reception/{id}/supprimer", post(reception_supprimer))
        .route("/genetique", get(genetique).post(genetique_ajouter))
        .route("/genetique/{id}/supprimer", post(genetique_supprimer))
        .route("/abattoir", get(abattoir).post(abattoir_saisie))
        .route(
            "/abattoir/saisie/{id}/supprimer",
            post(abattoir_saisie_supprimer),
        )
        .route("/cahiers", get(cahiers).post(cahier_ajouter))
        .route("/cahiers/{id}/maj", post(cahier_maj))
        .route("/cahiers/{id}/supprimer", post(cahier_supprimer))
        .route("/quotidien", get(quotidien))
        .route("/quotidien/note", post(quotidien_note))
        .route("/quotidien/ras", post(quotidien_ras))
        .route("/vente-directe/sessions", get(vente_sessions))
        .route("/reglages", get(reglages))
        .route("/parametres", get(parametres))
        .route("/parametres/conduite-bandes", post(conduite_bandes_maj))
        .route("/correctifs", get(correctifs))
        .route("/documents-obligatoires", get(documents::page))
        .route(
            "/documents-obligatoires/{key}/importer",
            post(documents::upload).layer(DefaultBodyLimit::max(9 * 1024 * 1024)),
        )
        .route(
            "/documents-obligatoires/fichier/{id}",
            get(documents::download),
        )
        .route(
            "/documents-obligatoires/fichier/{id}/supprimer",
            post(documents::delete),
        )
        .route("/apropos", get(apropos))
        .route("/contact", get(contact))
        // Compatibilité complète avec les URL de la version Python 1.65.
        .route("/abattoir/saisie", post(abattoir_saisie))
        .route("/attente", get(|| async { Redirect::to("/inseminations") }))
        .route("/bande/{id}/engraisseur", post(parity::bande_engraisseur))
        .route("/bande/{id}/inventaire", post(parity::bande_inventaire))
        .route(
            "/bande/{id}/mortalite/{declaration_id}/supprimer",
            post(parity::mortalite_supprimer),
        )
        .route(
            "/bande/{id}/transfert-porcs",
            post(parity::bande_transfert_porcs),
        )
        .route(
            "/bande/{id}/truie/{truie_id}/portee",
            post(parity::portee_bande_truie),
        )
        .route("/cahiers/ajouter", post(cahier_ajouter))
        .route("/cause/ajouter", post(parity::cause_ajouter))
        .route("/cause/{id}/supprimer", post(parity::cause_supprimer))
        .route("/desinscription/{token}", get(parity::desinscription))
        .route(
            "/economique/aliment/{id}/bandes",
            post(parity::economique_aliment_bandes),
        )
        .route(
            "/economique/aliment/{id}/site",
            post(parity::economique_aliment_site),
        )
        .route("/economique/auto-lier", post(parity::economique_auto_lier))
        .route(
            "/economique/{categorie}/{id}/affectations",
            post(parity::economique_facture_affectations),
        )
        .route(
            "/economique/genetique/{id}/bande",
            post(parity::economique_genetique_bande),
        )
        .route(
            "/economique/rattacher-auto",
            post(parity::economique_rattacher_auto),
        )
        .route(
            "/economique/semence/{id}/bande",
            post(parity::economique_semence_bande),
        )
        .route(
            "/economique/semence/{id}/montant",
            post(parity::economique_semence_montant),
        )
        .route("/economique/vente/{id}/bande", post(ventes::direct))
        .route(
            "/economique/vente/{id}/lot/{lot_index}/bande",
            post(ventes::lot),
        )
        .route(
            "/economique/veto/{id}/bande",
            post(parity::economique_veto_bande),
        )
        .route(
            "/economique/veto/{id}/bandes",
            post(parity::economique_veto_bandes),
        )
        .route(
            "/economique/veto/{id}/site",
            post(parity::economique_veto_site),
        )
        .route("/entretien/ajouter", post(entretien_ajouter))
        .route(
            "/export/mise-bas-pdf/{id}",
            get(parity::export_mise_bas_pdf),
        )
        .route("/export/registre-pdf", get(parity::export_registre_pdf))
        .route(
            "/import",
            post(truies_import).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route(
            "/import-pdf",
            post(economique_import_pdf).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route("/journal/{id}/supprimer", post(parity::journal_supprimer))
        .route("/logo", get(parity::logo))
        .route("/maj", get(parity::maj))
        .route("/maj/lancer", post(parity::maj_lancer))
        .route("/maj/statut", get(parity::maj_statut))
        .route(
            "/maj/zip",
            post(parity::maj_zip).layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
        .route("/parametres/aliment/ajouter", post(parity::aliment_ajouter))
        .route(
            "/parametres/aliment/{id}/modifier",
            post(parity::aliment_modifier),
        )
        .route(
            "/parametres/aliment/{id}/supprimer",
            post(parity::aliment_supprimer),
        )
        .route("/parametres/demo", post(parity::demo_basculer))
        .route("/parametres/demo-actif", get(parity::demo_actif))
        .route("/parametres/maj", post(parity::parametres_maj))
        // Le paramètre reçoit aussi le suffixe .png (ex. /qr/truie/42.png).
        .route("/qr/truie/{file}", get(parity::qr_truie))
        .route("/reglages/maj", post(parity::reglages_maj))
        .route("/saisie-rapide", post(parity::saisie_rapide))
        .route("/salle/{id}/lavage", post(parity::salle_lavage))
        .route(
            "/sanitaire/generer-protocole",
            post(parity::sanitaire_generer_protocole),
        )
        .route(
            "/sauvegarde/restaurer",
            post(parity::sauvegarde_restaurer).layer(DefaultBodyLimit::max(128 * 1024 * 1024)),
        )
        .route("/stock/doses", post(parity::stock_doses))
        .route("/template/truies.csv", get(truies_modele_csv))
        .route("/truie/{id}/chaleur", post(parity::truie_chaleur))
        .route("/truie/{id}/echo", post(parity::truie_echo))
        .route("/truie/{id}/ia", post(parity::truie_ia))
        .route("/truie/{id}/misebas", post(parity::truie_mise_bas))
        .route(
            "/truie/{id}/reclasser-verrat",
            post(parity::truie_reclasser_verrat),
        )
        .route("/truie/{id}/sevrage", post(parity::truie_sevrage))
        .route("/truie/{id}/sortie", post(parity::truie_sortie))
        .route("/truie/{id}/traitement", post(parity::truie_traitement))
        .route("/truies/transfert", post(transferts_truies))
        .route(
            "/vente-directe/client/{id}/consentements",
            post(parity::client_consentements),
        )
        .route("/vente-directe/communications", get(parity::communications))
        .route(
            "/vente-directe/communications/newsletter-email",
            post(parity::newsletter_email),
        )
        .route(
            "/vente-directe/communications/newsletter-sms",
            post(parity::newsletter_sms),
        )
        .route(
            "/vente-directe/communications/reglages",
            post(parity::communications_reglages),
        )
        .route(
            "/vente-directe/communications/test-email",
            post(parity::test_email),
        )
        .route(
            "/vente-directe/communications/test-sms",
            post(parity::test_sms),
        )
        .route(
            "/vente-directe/recalculer-stocks",
            post(parity::recalculer_stocks),
        )
        .route(
            "/vente-directe/session/{id}",
            get(parity::vente_session_detail),
        )
        .route(
            "/vente-directe/session/{id}/commande/{commande_id}/rattacher",
            post(parity::vente_session_commande_rattacher),
        )
        .fallback(compatibility_fallback)
        .with_state(state)
}

fn render(state: &AppState, name: &str, context: Value) -> AppResult<Html<String>> {
    let template = state
        .templates
        .get_template(name)
        .map_err(|error| AppError::Internal(error.into()))?;
    let body = template
        .render(context)
        .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Html(body))
}

fn session_value(session: &SessionData) -> Value {
    json!({
        "uid": session.uid,
        "identifiant": session.identifiant,
        "nom": session.nom,
        "role": session.role,
        "sections": session.sections,
        "csrf": session.csrf,
        "doit_changer_mdp": session.doit_changer_mdp,
        "peut_modifier": session.peut_modifier(),
        "est_admin": session.est_admin(),
        "type_elevage": session.type_elevage,
        "a_truies": session.a_truies(),
        "engraisse": session.engraisse(),
        "recoit_achats": session.recoit_achats(),
        "module_genetique": session.module_genetique,
        "module_prestataires": session.module_prestataires,
        "module_charcutiers_rfid": session.module_charcutiers_rfid,
        "module_vente_directe": session.module_vente_directe,
    })
}

fn context(session: &SessionData) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("session".into(), session_value(session));
    map
}

fn form_text(form: &HashMap<String, String>, key: &str) -> Option<String> {
    form.get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn form_i64(form: &HashMap<String, String>, key: &str) -> Option<i64> {
    form_text(form, key)?.parse().ok()
}

fn ia_slots(form: &HashMap<String, String>) -> Vec<&'static str> {
    [
        ("ia_matin", "matin"),
        ("ia_midi", "midi"),
        ("ia_soir", "soir"),
    ]
    .into_iter()
    .filter_map(|(key, label)| form.contains_key(key).then_some(label))
    .collect()
}

fn form_f64(form: &HashMap<String, String>, key: &str) -> Option<f64> {
    parse_french_number(&form_text(form, key)?)
}

fn today_iso() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

async fn synchroniser_pertes_mise_bas(
    pool: &SqlitePool,
    evenement_id: i64,
    truie_id: i64,
    bande_id: Option<i64>,
    date: &str,
    nombres: (i64, i64, i64),
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM perteporcelet WHERE evenement_id=?")
        .bind(evenement_id)
        .execute(&mut *tx)
        .await?;
    for (nombre, cause) in [
        (nombres.0, "Chétif / non conforme"),
        (nombres.1, "Écrasement"),
        (nombres.2, "Tué par la truie"),
    ] {
        if nombre > 0 {
            sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,age_j,nb,cause,date,evenement_id) VALUES(?,?,0,?,?,?,?)")
                .bind(truie_id)
                .bind(bande_id)
                .bind(nombre)
                .bind(cause)
                .bind(date)
                .bind(evenement_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn synchroniser_soins_portee(
    pool: &SqlitePool,
    evenement_id: i64,
    truie_id: i64,
    bande_id: Option<i64>,
    date_mise_bas: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO soinportee(protocole_id,evenement_id,truie_id,bande_id,date_prevue) SELECT a.id,?,?,?,date(?,printf('%+d day',a.jour)) FROM acteprotocole a WHERE a.actif=1 AND lower(trim(a.cible)) IN ('porcelet','porcelets') AND lower(trim(a.reference))='mise_bas' ON CONFLICT(protocole_id,evenement_id) DO UPDATE SET date_prevue=excluded.date_prevue,bande_id=excluded.bande_id WHERE soinportee.date_realisee IS NULL",
    )
    .bind(evenement_id)
    .bind(truie_id)
    .bind(bande_id)
    .bind(date_mise_bas)
    .execute(pool)
    .await?;
    Ok(())
}

async fn synchroniser_protocole_portees(pool: &SqlitePool, protocole_id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM soinportee WHERE protocole_id=? AND date_realisee IS NULL")
        .bind(protocole_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO soinportee(protocole_id,evenement_id,truie_id,bande_id,date_prevue) SELECT a.id,e.id,e.truie_id,e.bande_id,date(e.date,printf('%+d day',a.jour)) FROM acteprotocole a JOIN evenement e ON e.type='mise_bas' AND e.truie_id IS NOT NULL WHERE a.id=? AND a.actif=1 AND lower(trim(a.cible)) IN ('porcelet','porcelets') AND lower(trim(a.reference))='mise_bas'")
        .bind(protocole_id).execute(pool).await?;
    Ok(())
}

fn parse_iso_date(raw: &str) -> Option<String> {
    if raw.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .map(|date| date.format("%Y-%m-%d").to_string())
}

fn parse_stored_date(raw: &str) -> Option<NaiveDate> {
    let prefix = raw.get(..10).unwrap_or(raw);
    NaiveDate::parse_from_str(prefix, "%Y-%m-%d").ok()
}

fn form_date(form: &HashMap<String, String>, key: &str) -> AppResult<Option<String>> {
    form_text(form, key)
        .map(|raw| {
            parse_iso_date(&raw).ok_or_else(|| {
                AppError::Invalid(format!(
                    "Date invalide pour {key} : utilise le format AAAA-MM-JJ"
                ))
            })
        })
        .transpose()
}

fn form_date_or_today(form: &HashMap<String, String>, key: &str) -> AppResult<String> {
    Ok(form_date(form, key)?.unwrap_or_else(today_iso))
}

fn json_object_mut<'a>(value: &'a mut Value, label: &str) -> AppResult<&'a mut Map<String, Value>> {
    value.as_object_mut().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "structure JSON inattendue pendant la préparation de {label}"
        ))
    })
}

fn parse_french_number(input: &str) -> Option<f64> {
    let mut normalized = input
        .trim()
        .replace('−', "-")
        .replace(',', ".")
        .chars()
        .filter(|character| !character.is_whitespace() && *character != '\u{a0}')
        .collect::<String>();
    let negative = normalized.starts_with('-')
        || normalized.ends_with('-')
        || (normalized.starts_with('(') && normalized.ends_with(')'));
    normalized = normalized
        .trim_matches(|character| matches!(character, '-' | '(' | ')'))
        .to_string();
    let value = normalized.parse::<f64>().ok()?.abs();
    if !value.is_finite() {
        return None;
    }
    Some(if negative { -value } else { value })
}

fn economic_amount(form: &HashMap<String, String>, key: &str) -> Option<f64> {
    let amount = form_f64(form, key)?;
    Some(if form.get("nature").map(String::as_str) == Some("avoir") {
        -amount.abs()
    } else {
        amount
    })
}

fn form_selected_ids(form: &HashMap<String, String>, prefix: &str) -> Vec<i64> {
    let mut ids: Vec<i64> = form
        .keys()
        .filter_map(|key| key.strip_prefix(prefix))
        .filter_map(|value| value.parse::<i64>().ok())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn verify_csrf(session: &SessionData, form: &HashMap<String, String>) -> AppResult<()> {
    match form.get("csrf_token") {
        Some(token) if token == &session.csrf => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

// API: fournir la liste des truies avec dernier "mise_bas" et dernier "sevrage" connus
async fn api_truies_sevrage(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    // retourne : id, num_travail, rfid, bande_code, dernier_nes_vifs, dernier_nb_sevres
    let rows = generic_rows(&state.pool, "SELECT t.id,t.num_travail,t.rfid,t.bande_code, (SELECT nes_vifs FROM evenement WHERE type='mise_bas' AND truie_id=t.id ORDER BY date DESC,id DESC LIMIT 1) AS dernier_nes_vifs, (SELECT nb_sevres FROM evenement WHERE type='sevrage' AND truie_id=t.id ORDER BY date DESC,id DESC LIMIT 1) AS dernier_nb_sevres FROM truie t WHERE t.reformee=0 ORDER BY t.num_travail")
        .await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_bande_sevrage_estimate(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> AppResult<axum::Json<Value>> {
    // récupère la liste des truies attachées à la bande (par code) et renvoie leurs estimations et le total
    let rows = generic_rows(&state.pool, &format!("SELECT t.id,t.num_travail, (SELECT nes_vifs FROM evenement WHERE type='mise_bas' AND truie_id=t.id ORDER BY date DESC,id DESC LIMIT 1) AS dernier_nes_vifs, (SELECT nb_sevres FROM evenement WHERE type='sevrage' AND truie_id=t.id ORDER BY date DESC,id DESC LIMIT 1) AS dernier_nb_sevres FROM truie t WHERE t.reformee=0 AND t.bande_code=(SELECT code FROM bande WHERE id={}) ORDER BY t.num_travail", id)).await?;
    let mut total: i64 = 0;
    let mut list = Vec::new();
    for v in &rows {
        if let Some(obj) = v.as_object() {
            let id = obj.get("id").and_then(Value::as_i64).unwrap_or(0);
            let num = obj
                .get("num_travail")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let nes = obj
                .get("dernier_nes_vifs")
                .and_then(Value::as_f64)
                .map(|f| f as i64)
                .or_else(|| obj.get("dernier_nes_vifs").and_then(Value::as_i64));
            let sev = obj
                .get("dernier_nb_sevres")
                .and_then(Value::as_f64)
                .map(|f| f as i64)
                .or_else(|| obj.get("dernier_nb_sevres").and_then(Value::as_i64));
            let est = nes.or(sev).unwrap_or(0);
            total += est;
            let mut entry = Map::new();
            entry.insert("id".into(), json!(id));
            entry.insert("num_travail".into(), json!(num));
            entry.insert("estimate".into(), json!(est));
            list.push(Value::Object(entry));
        }
    }
    // also return band id for clients that may want to display it
    Ok(axum::Json(
        json!({"band_id": id, "total_expected": total, "truies": list}),
    ))
}

async fn api_causes_perte(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let causes = generic_rows(
        &state.pool,
        "SELECT id,libelle FROM causeperte ORDER BY libelle COLLATE NOCASE",
    )
    .await?;
    Ok(axum::Json(json!({"causes": causes})))
}

fn push_farm_alert(alerts: &mut Vec<Value>, count: i64, message: &str, url: &str, severity: &str) {
    if count > 0 {
        alerts.push(json!({
            "nombre": count,
            "message": message,
            "url": url,
            "niveau": severity,
        }));
    }
}

async fn api_alertes_elevage(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    Ok(axum::Json(farm_alerts(&state.pool).await?))
}

async fn farm_alerts(pool: &SqlitePool) -> AppResult<Value> {
    let mut alerts = Vec::new();
    for (kind, label) in [
        ("eau", "Relevés d’eau attendus"),
        ("electricite", "Relevés d’électricité attendus"),
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compteur_energie c WHERE c.actif=1 AND c.type=? AND COALESCE(c.rappel_jours,0)>0 AND date(COALESCE((SELECT MAX(r.date_releve) FROM releve_compteur r WHERE r.compteur_id=c.id),'1900-01-01'),printf('+%d day',c.rappel_jours))<=date('now')",
        )
        .bind(kind)
        .fetch_one(pool)
        .await?;
        push_farm_alert(&mut alerts, count, label, "/energie", "attention");
    }
    for (condition, message) in [
        (
            "date_mb IS NULL OR trim(date_mb)=''",
            "Bandes sans date de mise-bas",
        ),
        (
            "site IS NULL OR trim(site)=''",
            "Bandes sans site renseigné",
        ),
        (
            "num_officiel IS NULL OR trim(num_officiel)=''",
            "Bandes sans numéro de marquage",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM bande WHERE active=1 AND ({condition})"
        ))
        .fetch_one(pool)
        .await?;
        push_farm_alert(&mut alerts, count, message, "/bandes", "information");
    }
    let schedule = load_band_schedule(pool).await?;
    let missing_weaning: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bande b WHERE b.active=1 AND b.date_mb IS NOT NULL AND date(b.date_mb,printf('+%d day',?))<date('now') AND NOT EXISTS(SELECT 1 FROM evenement e WHERE e.bande_id=b.id AND e.type='sevrage')",
    )
    .bind(schedule.weaning)
    .fetch_one(pool)
    .await?;
    push_farm_alert(
        &mut alerts,
        missing_weaning,
        "Bandes arrivées au sevrage sans information",
        "/bandes",
        "attention",
    );
    let overdue_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tache WHERE fait=0 AND echeance IS NOT NULL AND date(echeance)<date('now')",
    )
    .fetch_one(pool)
    .await?;
    push_farm_alert(
        &mut alerts,
        overdue_tasks,
        "Tâches d’élevage en retard",
        "/taches",
        "urgent",
    );
    let farrowing_follow_up: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM evenement WHERE type='mise_bas' AND (suivi_actif=1 OR delivrance_ok=0)",
    )
    .fetch_one(pool)
    .await?;
    push_farm_alert(
        &mut alerts,
        farrowing_follow_up,
        "Mises-bas nécessitant un suivi",
        "/planning",
        "urgent",
    );
    let case_anomalies: i64 = sqlx::query_scalar(
        "WITH depart AS (SELECT c.id,c.nb_max_porcs,(SELECT i.date FROM inventairecase i WHERE i.case_id=c.id ORDER BY i.date DESC,i.id DESC LIMIT 1) AS inv_date,COALESCE((SELECT i.nombre FROM inventairecase i WHERE i.case_id=c.id ORDER BY i.date DESC,i.id DESC LIMIT 1),0) AS base FROM casesalle c),effectifs AS (SELECT d.id,d.nb_max_porcs,d.base+COALESCE((SELECT SUM(CASE WHEN t.case_dest_id=d.id THEN COALESCE(t.nombre,0) WHEN t.id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(t.nombre,0) END) FROM transfert t WHERE t.espece='porc' AND (t.case_dest_id=d.id OR t.case_source_id=d.id) AND (d.inv_date IS NULL OR t.date>d.inv_date)),0)-COALESCE((SELECT SUM(m.nombre) FROM declarationmort m WHERE m.case_id=d.id AND (d.inv_date IS NULL OR m.date>d.inv_date)),0) AS effectif FROM depart d) SELECT COUNT(*) FROM effectifs WHERE effectif<0 OR (nb_max_porcs>0 AND effectif>nb_max_porcs)",
    )
    .fetch_one(pool)
    .await?;
    push_farm_alert(
        &mut alerts,
        case_anomalies,
        "Cases avec un effectif incohérent",
        "/structure",
        "urgent",
    );
    let total = alerts
        .iter()
        .filter_map(|alert| alert.get("nombre").and_then(Value::as_i64))
        .sum::<i64>();
    Ok(json!({"total": total, "alertes": alerts}))
}

// Fournit la capacité et l'occupation actuelle par case (pour affichage côté client)
async fn api_cases_capacity(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    // retourne: id, nb_max_porcs, occupancy, remaining
    let cases = generic_rows(&state.pool, "SELECT c.id,COALESCE(si.nom,si.code) AS site,s.nom AS salle,c.nom,c.nb_max_porcs FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY site,salle,c.nom").await?;
    let mut out = Vec::new();
    for c in cases {
        if let Some(obj) = c.as_object() {
            let id = obj.get("id").and_then(Value::as_i64).unwrap_or(0);
            let nb_max = obj
                .get("nb_max_porcs")
                .and_then(Value::as_f64)
                .map(|f| f as i64)
                .or_else(|| obj.get("nb_max_porcs").and_then(Value::as_i64));
            // calculer occupation approximative
            let dest = format!("case:{}", id);
            let occupancy: i64 = sqlx::query_scalar(
                "SELECT COALESCE((SELECT SUM(COALESCE(nombre,0)) FROM mouvementstock WHERE destination=? AND est_stock=1),0) + COALESCE((SELECT COALESCE(SUM(nombre),0) FROM transfert WHERE case_dest_id=?),0) + COALESCE((SELECT nombre FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1),0)"
            ).bind(&dest).bind(id).bind(id).fetch_one(&state.pool).await?;
            let remaining = nb_max.map(|cap| cap - occupancy);
            let mut entry = Map::new();
            entry.insert("id".into(), json!(id));
            entry.insert("nb_max_porcs".into(), json!(nb_max));
            entry.insert("occupancy".into(), json!(occupancy));
            entry.insert("remaining".into(), json!(remaining));
            out.push(Value::Object(entry));
        }
    }
    Ok(axum::Json(Value::Array(out)))
}

// API: bande detail minimal JSON for partial client refresh
async fn api_bande_json(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> AppResult<axum::Json<Value>> {
    // mirror key parts of bande_detail but return JSON only
    let sql = format!("SELECT {BAND_FIELDS} FROM bande WHERE id=?");
    let band = sqlx::query_as::<_, Bande>(&sql)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let sows = sqlx::query_as::<_, Truie>(TRUIE_SELECT_BY_BAND)
        .bind(&band.code)
        .fetch_all(&state.pool)
        .await?;
    let events = sqlx::query_as::<_, Evenement>(EVENT_SELECT_BY_BAND)
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
    let litters = load_gttt_litters(&state.pool, Some(&band.code)).await?;
    let technical_summary = if litters.is_empty() {
        gttt_band_fallback(&band, &events)
    } else {
        gttt_summary(&litters)
    };
    let schedule = load_band_schedule(&state.pool).await?;
    let porcs_presents = total_band_pigs(&state.pool, band.id, &band.code).await?;
    let emplacements = generic_rows(
        &state.pool,
        &format!("SELECT t.date,si.code AS batiment,s.nom AS salle,c.nom AS unite,t.nombre FROM transfert t JOIN casesalle c ON c.id=t.case_dest_id JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id WHERE t.espece='porc' AND t.bande_id={} ORDER BY t.date DESC,t.id DESC LIMIT 10", band.id),
    )
    .await?;
    let vente_reelle: Option<String> = sqlx::query_scalar(
        "SELECT MAX(date) FROM venteapport v WHERE v.bande_id=? OR (json_type(CASE WHEN json_valid(v.lots_json) THEN v.lots_json ELSE 'null' END)='array' AND EXISTS(SELECT 1 FROM json_each(v.lots_json) j WHERE CAST(json_extract(j.value,'$.bande_id') AS INTEGER)=?))",
    )
    .bind(band.id)
    .bind(band.id)
    .fetch_one(&state.pool)
    .await?;
    let depart_prevu = band
        .date_mb
        .as_deref()
        .and_then(parse_stored_date)
        .map(|date| date + Duration::days(schedule.departure));
    let reference = vente_reelle
        .as_deref()
        .and_then(parse_stored_date)
        .unwrap_or_else(|| Local::now().date_naive());
    let ecart_vente = depart_prevu.map(|date| (reference - date).num_days());
    let statut_vente = match (vente_reelle.is_some(), ecart_vente) {
        (true, Some(value)) if value < 0 => format!("Vendu avec {} jour(s) d'avance", -value),
        (true, Some(value)) if value > 0 => format!("Vendu avec {value} jour(s) de retard"),
        (true, Some(_)) => "Vendu à la date prévue".to_string(),
        (false, Some(value)) if value > 0 && porcs_presents > 0 => {
            format!("En retard de {value} jour(s)")
        }
        (false, Some(value)) if value <= 0 => format!("Départ prévu dans {} jour(s)", -value),
        (false, Some(_)) => "Date de départ dépassée, aucun porc présent".to_string(),
        _ => "Date de mise-bas à renseigner".to_string(),
    };
    // build resume object similar to template
    let resume = json!({
        "truies": sows.len(),
        "portees": technical_summary.portees,
        "nv": technical_summary.total_nes_vifs,
        "sevres": technical_summary.total_sevres,
        "pertes": technical_summary.mortalite_allaitement,
        "nv_portee": technical_summary.nes_vifs_moy,
        "sevres_portee": technical_summary.sevres_moy,
        "mortnes": technical_summary.taux_mortnes,
    });
    let suivi_porcs = json!({
        "presents": porcs_presents,
        "depart_prevu": depart_prevu.map(|d| d.format("%Y-%m-%d").to_string()),
        "vente_reelle": vente_reelle,
        "statut": statut_vente,
        "ecart_jours": ecart_vente,
    });
    let resp = json!({
        "band": band,
        "resume": resume,
        "emplacements": emplacements,
        "suivi_porcs": suivi_porcs,
    });
    Ok(axum::Json(resp))
}

#[cfg(test)]
mod reception_tests {
    use super::*;

    #[test]
    fn quarantaine_active_couvre_la_date_de_fin_incluse() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        assert!(quarantaine_active(Some("2026-08-19"), today));
        assert!(quarantaine_active(Some("2026-08-25"), today));
        assert!(!quarantaine_active(Some("2026-08-18"), today));
    }

    #[test]
    fn quarantaine_active_faux_sans_date_ou_avec_une_date_invalide() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        assert!(!quarantaine_active(None, today));
        assert!(!quarantaine_active(Some("pas une date"), today));
    }
}

#[cfg(test)]
mod capacite_tests {
    use super::*;

    #[test]
    fn places_disponibles_ne_descend_jamais_sous_zero() {
        assert_eq!(places_disponibles(31, 20), 11);
        assert_eq!(places_disponibles(31, 31), 0);
        assert_eq!(places_disponibles(31, 45), 0);
    }

    #[test]
    fn stade_pour_type_salle_reconnait_les_memes_motifs_que_les_capacites() {
        assert_eq!(
            stade_pour_type_salle("Verraterie A"),
            Some("Verraterie".to_string())
        );
        assert_eq!(
            stade_pour_type_salle("Maternité 2"),
            Some("Maternité".to_string())
        );
        assert_eq!(
            stade_pour_type_salle("Post-sevrage"),
            Some("Post-sevrage".to_string())
        );
        assert_eq!(
            stade_pour_type_salle("Engraissement B"),
            Some("Engraissement".to_string())
        );
        assert_eq!(
            stade_pour_type_salle("Salle de finition"),
            Some("Engraissement".to_string())
        );
        assert_eq!(stade_pour_type_salle("Local technique"), None);
        assert_eq!(stade_pour_type_salle(""), None);
    }
}

#[cfg(test)]
mod sanitaire_tests {
    use super::*;

    #[test]
    fn rappel_en_retard_inclut_le_jour_de_lecheance() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        assert!(rappel_en_retard(
            NaiveDate::from_ymd_opt(2026, 8, 19).unwrap(),
            today
        ));
        assert!(rappel_en_retard(
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap(),
            today
        ));
        assert!(!rappel_en_retard(
            NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(),
            today
        ));
    }
}

#[cfg(test)]
mod energie_tests {
    use super::*;

    #[test]
    fn cout_consommation_exige_les_deux_valeurs_et_une_conso_positive() {
        assert_eq!(cout_consommation(Some(120.0), Some(1.5)), Some(180.0));
        assert_eq!(cout_consommation(None, Some(1.5)), None);
        assert_eq!(cout_consommation(Some(120.0), None), None);
        assert_eq!(cout_consommation(Some(0.0), Some(1.5)), None);
        assert_eq!(cout_consommation(Some(-5.0), Some(1.5)), None);
    }

    #[test]
    fn repartir_cout_par_bande_divise_a_parts_egales() {
        let bandes = vec![
            "B1.26".to_string(),
            "B2.26".to_string(),
            "B3.26".to_string(),
        ];
        let parts = repartir_cout_par_bande(100.0, &bandes);
        assert_eq!(
            parts,
            vec![
                ("B1.26".to_string(), 33.33),
                ("B2.26".to_string(), 33.33),
                ("B3.26".to_string(), 33.33),
            ]
        );
    }

    #[test]
    fn repartir_cout_par_bande_reste_vide_sans_bande_ou_sans_cout() {
        assert!(repartir_cout_par_bande(100.0, &[]).is_empty());
        assert!(repartir_cout_par_bande(0.0, &["B1.26".to_string()]).is_empty());
    }
}

#[cfg(test)]
mod aliment_tests {
    use super::*;

    #[test]
    fn consommation_quotidienne_fait_le_bilan_de_matiere() {
        // 20 t restantes, 10 t livrées entre les deux relevés, 18 t restantes
        // 7 jours plus tard : 20+10-18 = 12 t consommées sur 7 jours.
        assert_eq!(
            consommation_quotidienne_tonnes(20.0, 10.0, 18.0, 7),
            Some(12.0 / 7.0)
        );
        assert_eq!(consommation_quotidienne_tonnes(20.0, 0.0, 18.0, 0), None);
    }

    #[test]
    fn consommation_quotidienne_ne_descend_jamais_sous_zero() {
        // Erreur de saisie plausible (niveau remonté sans livraison notée) :
        // on affiche 0 plutôt qu'une consommation négative absurde.
        assert_eq!(
            consommation_quotidienne_tonnes(10.0, 0.0, 15.0, 5),
            Some(0.0)
        );
    }

    #[test]
    fn jours_avant_rupture_ignore_consommation_nulle() {
        assert_eq!(jours_avant_rupture(21.0, 3.0), Some(7.0));
        assert_eq!(jours_avant_rupture(21.0, 0.0), None);
    }

    #[test]
    fn quantite_a_commander_ramene_a_la_capacite_sans_jamais_etre_negative() {
        assert_eq!(quantite_a_commander(8.0, Some(20.0)), Some(12.0));
        // Niveau déjà au-dessus de la capacité déclarée (erreur de saisie
        // plausible) : rien à commander, pas une quantité négative.
        assert_eq!(quantite_a_commander(25.0, Some(20.0)), Some(0.0));
        assert_eq!(quantite_a_commander(8.0, None), None);
    }

    #[test]
    fn commande_urgente_compare_au_delai_de_livraison() {
        assert!(commande_urgente(Some(3.0), 5));
        assert!(commande_urgente(Some(5.0), 5));
        assert!(!commande_urgente(Some(6.0), 5));
        assert!(!commande_urgente(None, 5));
    }
}

#[cfg(test)]
mod gte_tests {
    use super::*;

    #[test]
    fn indice_consommation_divise_aliment_par_poids_produit() {
        assert_eq!(indice_consommation(2800.0, 1000.0), Some(2.8));
        assert_eq!(indice_consommation(2800.0, 0.0), None);
    }

    #[test]
    fn cout_alimentaire_par_porc_ignore_effectif_nul() {
        assert_eq!(cout_alimentaire_par_porc(1000.0, 200), Some(5.0));
        assert_eq!(cout_alimentaire_par_porc(1000.0, 0), None);
    }

    #[test]
    fn msa_est_les_recettes_moins_le_seul_cout_aliment() {
        assert_eq!(marge_sur_cout_alimentaire(12000.0, 4500.0), 7500.0);
    }

    #[test]
    fn marge_brute_par_truie_ignore_lot_sans_truies() {
        assert_eq!(marge_brute_par_truie(7500.0, 30), Some(250.0));
        assert_eq!(marge_brute_par_truie(7500.0, 0), None);
    }

    #[test]
    fn taux_renouvellement_ignore_cheptel_vide() {
        assert_eq!(taux_renouvellement_pct(9, 60), Some(15.0));
        assert_eq!(taux_renouvellement_pct(9, 0), None);
    }

    #[test]
    fn cout_achat_par_animal_ignore_lot_sans_reception() {
        assert_eq!(cout_achat_par_animal_entre(3000.0, 60), Some(50.0));
        assert_eq!(cout_achat_par_animal_entre(0.0, 0), None);
    }

    /// La requête réelle doit remonter le coût d'achat et l'effectif entré du
    /// lot : sans cela l'écran afficherait une marge après achat identique à la
    /// MSA pour un profil acheteur, c'est-à-dire surestimée.
    #[tokio::test]
    async fn la_requete_gte_impute_les_receptions_dachat_au_lot() -> AppResult<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(AppError::from)?;
        for migration in [
            include_str!("../../migrations/0001_schema.sql"),
            include_str!("../../migrations/0002_ventelot.sql"),
            include_str!("../../migrations/0003_portee_effectif.sql"),
        ] {
            sqlx::raw_sql(migration)
                .execute(&pool)
                .await
                .map_err(AppError::from)?;
        }
        sqlx::query("INSERT INTO bande(code,active) VALUES('B-ACHAT',1)")
            .execute(&pool)
            .await?;
        // Deux réceptions sur le même lot : les coûts et les effectifs
        // s'additionnent, ils ne sont pas écrasés par la dernière ligne.
        sqlx::query(
            "INSERT INTO receptionachat(date,bande_code,effectif,prix_total) VALUES('2026-01-10','B-ACHAT',40,2000.0),('2026-02-10','B-ACHAT',20,1000.0)",
        )
        .execute(&pool)
        .await?;
        // Une réception sans lot ne doit être imputée à aucune bande.
        sqlx::query(
            "INSERT INTO receptionachat(date,bande_code,effectif,prix_total) VALUES('2026-03-10','',10,500.0)",
        )
        .execute(&pool)
        .await?;

        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            i64,
            String,
            Option<String>,
            i64,
            f64,
            f64,
            f64,
            f64,
            i64,
            f64,
            i64,
        )> = sqlx::query_as(GTE_LOTS_SQL).fetch_all(&pool).await?;
        assert_eq!(rows.len(), 1, "le lot acheteur doit apparaître sans vente");
        let (_, code, _, _, _, recettes, _, cout_aliment, _, cout_achat, entres) = &rows[0];
        assert_eq!(code, "B-ACHAT");
        assert_eq!(*cout_achat, 3000.0);
        assert_eq!(*entres, 60);
        // Sans vente ni aliment, la marge après achat est bien négative du
        // montant payé — le lot n'est pas silencieusement à l'équilibre.
        let msa = marge_sur_cout_alimentaire(*recettes, *cout_aliment);
        assert_eq!(marge_apres_cout_achat(msa, *cout_achat), -3000.0);
        assert_eq!(
            cout_achat_par_animal_entre(*cout_achat, *entres),
            Some(50.0)
        );
        Ok(())
    }

    #[test]
    fn marge_apres_achat_deduit_la_charge_dentree() {
        // Profil acheteur : la MSA seule surestime la marge du lot.
        assert_eq!(marge_apres_cout_achat(7500.0, 3000.0), 4500.0);
        // Profil naisseur : aucune réception, la valeur reste la MSA.
        assert_eq!(marge_apres_cout_achat(7500.0, 0.0), 7500.0);
    }
}

#[cfg(test)]
mod sevrage_tests {
    use super::*;
    use crate::config::Config;
    use minijinja::Environment;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn api_truies_sevrage_and_estimate_return_expected_values() -> AppResult<()> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        // create a band
        let band_id =
            sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('BTEST','2026-08-01',1)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        // create truies
        let sow1 = sqlx::query("INSERT INTO truie(num_travail,bande_code,statut,reformee) VALUES('T1','BTEST','active',0)").execute(&pool).await?.last_insert_rowid();
        let sow2 = sqlx::query("INSERT INTO truie(num_travail,bande_code,statut,reformee) VALUES('T2','BTEST','active',0)").execute(&pool).await?.last_insert_rowid();
        // add events: mise_bas for sow1 (nes_vifs 10), sow2 no mise_bas but prior sevrage of 8
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nes_vifs) VALUES('mise_bas','2026-08-01',?,?,10)").bind(sow1).bind(band_id).execute(&pool).await?;
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres) VALUES('sevrage','2026-08-02',?,?,8)").bind(sow2).bind(band_id).execute(&pool).await?;
        // build AppState
        let config = Config {
            bind: "0.0.0.0:8080".parse().unwrap(),
            db_path: std::path::PathBuf::from("data/test.db"),
            secure_cookies: false,
        };
        let env = Environment::new();
        let state = AppState::new(config, pool.clone(), env);
        // call api_truies_sevrage
        let json = api_truies_sevrage(State(state.clone())).await?;
        let arr = json.0.as_array().cloned().unwrap_or_default();
        assert!(arr.len() >= 2, "expected at least 2 truies returned");
        // call estimate
        let estimate = api_bande_sevrage_estimate(Path(band_id), State(state)).await?;
        let obj = estimate.0.as_object().cloned().unwrap();
        let total = obj
            .get("total_expected")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        assert_eq!(total, 10 + 8);
        Ok(())
    }
}

fn require_writer(session: &SessionData) -> AppResult<()> {
    if session.peut_modifier() {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn login_page(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let error = match query.get("err").map(String::as_str) {
        Some("expire") => "Accès de démonstration expiré ou révoqué. Contactez Emmanuel ORY.",
        Some("bloque") => "Compte temporairement verrouillé après plusieurs échecs.",
        Some(_) => "Identifiant ou mot de passe incorrect.",
        None => "",
    };
    render(&state, "login.html", json!({"error": error}))
}

async fn login_post(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    let identifiant = form_text(&form, "identifiant").unwrap_or_default();
    let password = form.get("mdp").cloned().unwrap_or_default();
    let user = sqlx::query_as::<_, Utilisateur>(
        "SELECT id,identifiant,nom,prenom,hash_mdp,role,actif,sections,doit_changer_mdp,tentatives_echec,bloque_jusqu FROM utilisateur WHERE identifiant=? AND actif=1 LIMIT 1",
    )
    .bind(&identifiant)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(user) = user {
        if crate::demo_portal::enabled()
            && !crate::demo_portal::valid(&state.pool, user.id, chrono::Utc::now().timestamp())
                .await
        {
            return Ok(Redirect::to("/login?err=expire").into_response());
        }
        if user.bloque_jusqu.as_deref().is_some_and(|until| {
            until
                > chrono::Utc::now()
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
                    .as_str()
        }) {
            return Ok(Redirect::to("/login?err=bloque").into_response());
        }
        if auth::verify_password_async(password.clone(), user.hash_mdp.clone()).await? {
            sqlx::query("UPDATE utilisateur SET tentatives_echec=0,bloque_jusqu=NULL WHERE id=?")
                .bind(user.id)
                .execute(&state.pool)
                .await?;
            let session_id = auth::new_secure_token();
            let full_name = format!(
                "{} {}",
                user.prenom.clone().unwrap_or_default(),
                user.nom.clone().unwrap_or_else(|| user.identifiant.clone())
            )
            .trim()
            .to_string();
            let sections = user
                .sections
                .as_deref()
                .unwrap_or("")
                .split(',')
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect();
            let type_elevage = type_elevage_actif(&state.pool).await?;
            let module_genetique = module_actif(&state.pool, "module_genetique", false).await?;
            let module_prestataires =
                module_actif(&state.pool, "module_prestataires", true).await?;
            let module_charcutiers_rfid =
                module_actif(&state.pool, "module_charcutiers_rfid", false).await?;
            let module_vente_directe =
                module_actif(&state.pool, "module_vente_directe", true).await?;
            state.sessions.insert(
                session_id.clone(),
                SessionData {
                    uid: user.id,
                    identifiant: user.identifiant,
                    nom: full_name,
                    role: user.role.clone(),
                    sections,
                    csrf: auth::new_csrf(),
                    doit_changer_mdp: user.doit_changer_mdp,
                    type_elevage,
                    module_genetique,
                    module_prestataires,
                    module_charcutiers_rfid,
                    module_vente_directe,
                },
            );
            let cookie = Cookie::build(("eo_session", session_id))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
                .secure(state.config.secure_cookies)
                .build();
            let target = if user.doit_changer_mdp {
                "/mon-compte/mdp?force=1"
            } else if user.role == "engraisseur" {
                "/engraissement"
            } else {
                "/"
            };
            return Ok((jar.add(cookie), Redirect::to(target)).into_response());
        }
        sqlx::query(
            "UPDATE utilisateur SET tentatives_echec=tentatives_echec+1, bloque_jusqu=CASE WHEN tentatives_echec+1>=5 THEN datetime('now','+15 minutes') ELSE bloque_jusqu END WHERE id=?",
        )
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    }
    Ok(Redirect::to("/login?err=1").into_response())
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get("eo_session") {
        state.sessions.remove(cookie.value());
    }
    let cookie = Cookie::build(("eo_session", ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.secure_cookies)
        .build();
    (jar.remove(cookie), Redirect::to("/login")).into_response()
}

async fn password_page(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let mut ctx = context(&session);
    ctx.insert("force".into(), json!(query.contains_key("force")));
    ctx.insert(
        "error".into(),
        json!(query.get("err").cloned().unwrap_or_default()),
    );
    render(&state, "mot_de_passe.html", Value::Object(ctx))
}

async fn password_post(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session, &form)?;
    let password = form.get("mdp").cloned().unwrap_or_default();
    if password.len() < 8 || form.get("mdp2") != Some(&password) {
        return Ok(Redirect::to("/mon-compte/mdp?err=confirmation-ou-longueur").into_response());
    }
    let hash = auth::hash_password_async(password).await?;
    sqlx::query("UPDATE utilisateur SET hash_mdp=?,doit_changer_mdp=0 WHERE id=?")
        .bind(hash)
        .bind(session.uid)
        .execute(&state.pool)
        .await?;
    // Un changement de mot de passe doit rendre inutilisable tout cookie de
    // session éventuellement volé. L'utilisateur se reconnecte ensuite avec
    // son nouveau mot de passe.
    state.sessions.retain(|_, active| active.uid != session.uid);
    Ok(Redirect::to("/login").into_response())
}

#[derive(serde::Serialize)]
struct BandStageView {
    nom: String,
    date: Option<String>,
    repere: String,
    terminee: bool,
    actuelle: bool,
}

#[derive(serde::Serialize)]
struct BandView {
    id: i64,
    code: String,
    num_officiel: Option<String>,
    date_mb: Option<String>,
    site: Option<String>,
    age: Option<i64>,
    stade: String,
    prochaine: String,
    prochaine_date: Option<String>,
    prochaine_delai: Option<String>,
    urgence: String,
    truies: i64,
    progression: i64,
    etapes: Vec<BandStageView>,
    flux: Value,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BandSchedule {
    pub(crate) gestation: i64,
    pub(crate) echo_after_ia: i64,
    pub(crate) maternity_before_farrowing: i64,
    pub(crate) weaning: i64,
    pub(crate) transfer_finishing: i64,
    pub(crate) finishing_feed: i64,
    pub(crate) departure: i64,
}

impl Default for BandSchedule {
    fn default() -> Self {
        Self {
            gestation: 115,
            echo_after_ia: 28,
            maternity_before_farrowing: 5,
            weaning: 28,
            transfer_finishing: 71,
            finishing_feed: 140,
            departure: 215,
        }
    }
}

impl BandSchedule {
    fn stages(self) -> [(&'static str, i64); 8] {
        [
            ("Insémination", -self.gestation),
            ("Échographie", -self.gestation + self.echo_after_ia),
            ("Entrée maternité", -self.maternity_before_farrowing),
            ("Mise-bas", 0),
            ("Sevrage", self.weaning),
            ("Transfert engraissement", self.transfer_finishing),
            ("Aliment finition", self.finishing_feed),
            ("Départ abattoir", self.departure),
        ]
    }

    fn stage(self, age: i64) -> (&'static str, &'static str) {
        let echo_day = -self.gestation + self.echo_after_ia;
        if age < -self.gestation {
            ("Planifiée", "Insémination")
        } else if age < echo_day {
            ("Verraterie", "Échographie")
        } else if age < -self.maternity_before_farrowing {
            ("Gestante", "Entrée maternité")
        } else if age < 0 {
            ("Maternité (préparation)", "Mise-bas")
        } else if age < self.weaning {
            ("Maternité", "Sevrage")
        } else if age < self.transfer_finishing {
            ("Post-sevrage", "Transfert engraissement")
        } else if age < self.departure {
            ("Engraissement", "Départ abattoir")
        } else {
            ("Départ / terminé", "Cycle terminé")
        }
    }

    fn stage_index(self, age: i64) -> usize {
        self.stages()
            .iter()
            .rposition(|(_, offset)| age >= *offset)
            .unwrap_or(0)
    }

    fn progression(self, age: i64) -> i64 {
        let total = self.departure + self.gestation;
        (age + self.gestation).clamp(0, total) * 100 / total
    }
}

pub(crate) async fn load_band_schedule(pool: &SqlitePool) -> AppResult<BandSchedule> {
    let mut schedule = BandSchedule::default();
    for row in sqlx::query(
        "SELECT cle,valeur FROM reglage WHERE cle IN ('gestation','echo_j','passage_maternite_j','sevrage','transfert_engr','aliment_finition','depart')",
    )
    .fetch_all(pool)
    .await?
    {
        let key: String = row.try_get("cle")?;
        let value: i64 = row.try_get("valeur")?;
        match key.as_str() {
            "gestation" if (90..=140).contains(&value) => schedule.gestation = value,
            "echo_j" if (1..=60).contains(&value) => schedule.echo_after_ia = value,
            "passage_maternite_j" if (1..=21).contains(&value) => {
                schedule.maternity_before_farrowing = value
            }
            "sevrage" if (14..=49).contains(&value) => schedule.weaning = value,
            "transfert_engr" if (30..=140).contains(&value) => {
                schedule.transfer_finishing = value
            }
            "aliment_finition" if (60..=210).contains(&value) => {
                schedule.finishing_feed = value
            }
            "depart" if (100..=365).contains(&value) => schedule.departure = value,
            _ => {}
        }
    }
    schedule.transfer_finishing = schedule.transfer_finishing.max(schedule.weaning + 1);
    schedule.departure = schedule.departure.max(schedule.transfer_finishing + 2);
    schedule.finishing_feed = schedule
        .finishing_feed
        .clamp(schedule.transfer_finishing + 1, schedule.departure - 1);
    Ok(schedule)
}

fn band_view(band: &Bande, sow_count: i64, schedule: BandSchedule, flux: Value) -> BandView {
    let today = Local::now().date_naive();
    let date = band.date_mb.as_deref().and_then(parse_stored_date);
    let age = date.map(|date| (today - date).num_days());
    let (stade, default_next) = age
        .map(|age| schedule.stage(age))
        .unwrap_or(("À renseigner", "Renseigner la date de mise-bas"));
    let stages = schedule.stages();
    let current_stage = age.map(|age| schedule.stage_index(age)).unwrap_or(0);
    let cycle_complete = age.is_some_and(|age| age >= schedule.departure);
    let next_stage = age.and_then(|age| stages.iter().copied().find(|(_, offset)| *offset > age));
    let prochaine = next_stage.map(|(name, _)| name).unwrap_or(default_next);
    let prochaine_date = match (date, next_stage) {
        (Some(date), Some((_, offset))) => Some(
            (date + Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string(),
        ),
        _ => None,
    };
    let jours_prochaine = match (age, next_stage) {
        (Some(age), Some((_, offset))) => Some(offset - age),
        _ => None,
    };
    let prochaine_delai = jours_prochaine.map(|days| {
        if days == 1 {
            "demain".to_string()
        } else {
            format!("dans {days} jours")
        }
    });
    let urgence = match jours_prochaine {
        Some(days) if days <= 7 => "urgent",
        Some(days) if days <= 21 => "proche",
        Some(_) => "planifie",
        None if cycle_complete => "termine",
        None => "incomplet",
    };
    let markers = ["IA", "ÉCHO", "MAT.", "MB", "SEV.", "TRANS.", "FIN.", "DÉP."];
    let etapes = stages
        .iter()
        .enumerate()
        .map(|(index, (name, offset))| BandStageView {
            nom: (*name).to_string(),
            date: date.map(|date| {
                (date + Duration::days(*offset))
                    .format("%Y-%m-%d")
                    .to_string()
            }),
            repere: markers[index].to_string(),
            terminee: age.is_some() && (cycle_complete || index < current_stage),
            actuelle: age
                .is_some_and(|age| !cycle_complete && age >= stages[0].1 && index == current_stage),
        })
        .collect();
    BandView {
        id: band.id,
        code: band.code.clone(),
        num_officiel: band.num_officiel.clone(),
        date_mb: band.date_mb.clone(),
        site: band.site.clone(),
        age,
        stade: stade.to_string(),
        prochaine: prochaine.to_string(),
        prochaine_date,
        prochaine_delai,
        urgence: urgence.to_string(),
        truies: sow_count,
        progression: age.map(|age| schedule.progression(age)).unwrap_or(0),
        etapes,
        flux,
    }
}

async fn band_flow_summary(pool: &SqlitePool, band: &Bande) -> AppResult<Value> {
    let sow_flow = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "WITH cycle_truies AS (SELECT DISTINCT truie_id FROM evenement WHERE bande_id=? AND truie_id IS NOT NULL UNION SELECT id FROM truie WHERE bande_code=?),sevragees AS (SELECT DISTINCT truie_id FROM evenement WHERE bande_id=? AND type='sevrage' AND truie_id IS NOT NULL),vers_verraterie AS (SELECT DISTINCT w.truie_id FROM sevragees w JOIN truie t ON t.id=w.truie_id WHERE t.reformee=0 AND (EXISTS(SELECT 1 FROM transfert m LEFT JOIN casesalle c ON c.id=m.case_dest_id JOIN salle s ON s.id=COALESCE(m.salle_dest_id,c.salle_id) WHERE m.truie_id=t.id AND m.date>=(SELECT MAX(e.date) FROM evenement e WHERE e.bande_id=? AND e.truie_id=t.id AND e.type='sevrage') AND (lower(COALESCE(s.type,'')) LIKE '%verrater%' OR lower(COALESCE(s.nom,'')) LIKE '%verrater%')) OR EXISTS(SELECT 1 FROM salle s WHERE s.id=COALESCE(t.salle_id,(SELECT c.salle_id FROM casesalle c WHERE c.id=t.case_id)) AND (lower(COALESCE(s.type,'')) LIKE '%verrater%' OR lower(COALESCE(s.nom,'')) LIKE '%verrater%')))) SELECT (SELECT COUNT(*) FROM cycle_truies),(SELECT COUNT(*) FROM vers_verraterie),(SELECT COUNT(*) FROM sevragees w JOIN truie t ON t.id=w.truie_id WHERE t.reformee=0 AND (t.bande_code IS NULL OR trim(t.bande_code)='' OR NOT EXISTS(SELECT 1 FROM bande nouvelle WHERE nouvelle.code=t.bande_code AND nouvelle.active=1)) AND NOT EXISTS(SELECT 1 FROM vers_verraterie v WHERE v.truie_id=t.id)),(SELECT COUNT(*) FROM cycle_truies c JOIN truie t ON t.id=c.truie_id WHERE t.reformee=1),(SELECT CAST(COALESCE(SUM(nb_sevres),0) AS INTEGER) FROM evenement WHERE bande_id=? AND type='sevrage')",
    )
    .bind(band.id)
    .bind(&band.code)
    .bind(band.id)
    .bind(band.id)
    .bind(band.id)
    .fetch_one(pool)
    .await?;
    let sales = generic_rows(
        pool,
        &format!(
            "SELECT date,CAST(SUM(nb_porcs) AS INTEGER) AS nombre FROM (SELECT date,nb_porcs FROM venteapport WHERE bande_id={0} AND date IS NOT NULL AND nb_porcs IS NOT NULL UNION ALL SELECT v.date,CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER) FROM venteapport v,json_each(v.lots_json) j WHERE v.bande_id IS NULL AND json_valid(v.lots_json) AND json_type(v.lots_json)='array' AND CAST(json_extract(j.value,'$.bande_id') AS INTEGER)={0} AND v.date IS NOT NULL) GROUP BY date ORDER BY date",
            band.id
        ),
    )
    .await?;
    let vendus = sales
        .iter()
        .filter_map(|row| row.get("nombre").and_then(Value::as_i64))
        .sum::<i64>();
    let derniere_vente = sales
        .last()
        .and_then(|row| row.get("date"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(json!({
        "truies_cycle": sow_flow.0,
        "verraterie": sow_flow.1,
        "attente": sow_flow.2,
        "reformees": sow_flow.3,
        "sevres": sow_flow.4,
        "vendus": vendus,
        "derniere_vente": derniere_vente,
        "ventes": sales,
    }))
}

async fn dashboard(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Response> {
    if session.role == "engraisseur" {
        return Ok(Redirect::to("/engraissement").into_response());
    }
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let schedule = load_band_schedule(&state.pool).await?;
    let mut views = Vec::new();
    for band in &bands {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE bande_code=? AND reformee=0")
                .bind(&band.code)
                .fetch_one(&state.pool)
                .await?;
        let flux = band_flow_summary(&state.pool, band).await?;
        views.push(band_view(band, count, schedule, flux));
    }
    let truies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE reformee=0")
        .fetch_one(&state.pool)
        .await?;
    let sevres: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(nb_sevres),0) FROM evenement WHERE type='sevrage'")
            .fetch_one(&state.pool)
            .await?;
    let vente: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
    let aliment: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM livraisonaliment",
    )
    .fetch_one(&state.pool)
    .await?;
    let veto: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatveto")
            .fetch_one(&state.pool)
            .await?;
    let semence: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatsemence")
            .fetch_one(&state.pool)
            .await?;
    let genetique: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatgenetique")
            .fetch_one(&state.pool)
            .await?;
    let year = Local::now().format("%Y").to_string();
    let year_sales = sqlx::query_as::<_, (i64, f64, f64)>(
        "SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER),CAST(COALESCE(SUM(poids_total),0) AS REAL),CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM venteapport WHERE substr(date,1,4)=?",
    )
    .bind(&year)
    .fetch_one(&state.pool)
    .await?;
    let year_deaths: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM declarationmort WHERE substr(date,1,4)=?",
    )
    .bind(&year)
    .fetch_one(&state.pool)
    .await?;
    let price_trend = generic_rows(
        &state.pool,
        "SELECT substr(date,1,7) AS mois,CAST(SUM(COALESCE(nb_porcs,0)) AS INTEGER) AS porcs,ROUND(SUM(COALESCE(montant_ht,0))/NULLIF(SUM(COALESCE(poids_total,0)),0),3) AS prix_ht_kg FROM venteapport WHERE date IS NOT NULL GROUP BY substr(date,1,7) ORDER BY mois DESC LIMIT 12",
    )
    .await?;
    let latest_sales = generic_rows(
        &state.pool,
        "WITH recent AS (SELECT id,date,num_apport,nb_porcs,poids_total,montant_ht,ROUND(montant_ht/NULLIF(poids_total,0),3) AS prix_ht_kg FROM venteapport WHERE poids_total>0 AND montant_ht IS NOT NULL ORDER BY date DESC,id DESC LIMIT 10), bounds AS (SELECT MIN(prix_ht_kg) AS mini,MAX(prix_ht_kg) AS maxi FROM recent) SELECT recent.*,ROUND(CASE WHEN bounds.maxi=bounds.mini THEN 70 ELSE 30+90*(recent.prix_ht_kg-bounds.mini)/(bounds.maxi-bounds.mini) END,0) AS hauteur FROM recent CROSS JOIN bounds ORDER BY recent.date,recent.id",
    )
    .await?;
    let latest_average = sqlx::query_as::<_, (f64, f64)>(
        "SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL),CAST(COALESCE(SUM(poids_total),0) AS REAL) FROM (SELECT montant_ht,poids_total FROM venteapport WHERE poids_total>0 AND montant_ht IS NOT NULL ORDER BY date DESC,id DESC LIMIT 5)",
    )
    .fetch_one(&state.pool)
    .await?;
    let taches = generic_rows(
        &state.pool,
        "SELECT id,titre,type,bande_code,salle,echeance,note FROM tache WHERE fait=0 ORDER BY echeance LIMIT 8",
    )
    .await?;
    let inseminations = generic_rows(
        &state.pool,
        "SELECT t.num_travail,date(e.date,'+1 day') AS date_prevue FROM evenement e JOIN truie t ON t.id=e.truie_id WHERE e.type='chaleur' AND NOT EXISTS(SELECT 1 FROM evenement ia WHERE ia.truie_id=e.truie_id AND ia.type='ia' AND ia.date>=e.date) ORDER BY e.date DESC LIMIT 12",
    )
    .await?;
    // Alertes « délai d'attente en cours » : un animal (truie ou porc
    // charcutier) ayant reçu un traitement dont le délai d'attente n'est
    // pas encore écoulé ne doit pas partir à l'abattoir ni en vente directe
    // — une information de sécurité sanitaire qui n'était affichée nulle
    // part avant la fiche de l'animal concerné. `date(date,'+N day')`
    // reproduit le même calcul d'échéance que les rappels sanitaires
    // (`printf('%+d day', jour)` ailleurs dans le fichier), ici avec un
    // décalage toujours positif ou nul.
    let delais_attente_truies = generic_rows(
        &state.pool,
        "SELECT t.num_travail AS reference,e.produit,e.date AS date_traitement,date(e.date,'+'||e.delai_attente||' day') AS fin_attente FROM evenement e JOIN truie t ON t.id=e.truie_id WHERE e.type='traitement' AND e.delai_attente IS NOT NULL AND e.delai_attente>0 AND date(e.date,'+'||e.delai_attente||' day')>=date('now') ORDER BY fin_attente",
    )
    .await?;
    let delais_attente_charcutiers = generic_rows(
        &state.pool,
        "SELECT COALESCE(NULLIF(p.rfid,''),'Porc #'||p.id) AS reference,tc.produit,tc.date AS date_traitement,date(tc.date,'+'||tc.delai_attente||' day') AS fin_attente FROM traitementcharcutier tc JOIN porccharcutier p ON p.id=tc.charcutier_id WHERE tc.delai_attente IS NOT NULL AND tc.delai_attente>0 AND date(tc.date,'+'||tc.delai_attente||' day')>=date('now') AND p.date_mort IS NULL ORDER BY fin_attente",
    )
    .await?;
    let alertes_delai_attente: i64 =
        delais_attente_truies.len() as i64 + delais_attente_charcutiers.len() as i64;
    // Commandes de vente directe non traitées (statut initial « nouvelle »),
    // uniquement si le module est actif — un compteur simple sur le tableau
    // de bord, à côté des autres alertes structurelles.
    let commandes_vente_ouvertes: i64 =
        if module_actif(&state.pool, "module_vente_directe", true).await? {
            sqlx::query_scalar("SELECT COUNT(*) FROM commandeventedirecte WHERE statut='nouvelle'")
                .fetch_one(&state.pool)
                .await?
        } else {
            0
        };
    let capacites = capacites_par_etape(&state.pool, &session).await?;
    let mut ctx = context(&session);
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(views).unwrap_or_default(),
    );
    ctx.insert("taches".into(), Value::Array(taches));
    ctx.insert("inseminations".into(), Value::Array(inseminations));
    ctx.insert("prix_tendance".into(), Value::Array(price_trend));
    ctx.insert("dernieres_ventes".into(), Value::Array(latest_sales));
    ctx.insert("annee".into(), json!(year));
    ctx.insert("aujourd_hui".into(), json!(today_iso()));
    ctx.insert("capacites".into(), Value::Array(capacites));
    ctx.insert(
        "alertes".into(),
        json!({
            "delai_attente_truies": delais_attente_truies,
            "delai_attente_charcutiers": delais_attente_charcutiers,
            "delai_attente_total": alertes_delai_attente,
            "commandes_vente_ouvertes": commandes_vente_ouvertes,
        }),
    );
    ctx.insert(
        "stats".into(),
        json!({"band_active": bands.len(), "truies": truies, "sevres": sevres, "marge": vente-aliment-veto-semence-genetique,"porcs_vendus_annee":year_sales.0,"prix_ht_kg":if year_sales.1>0.0{Some(year_sales.2/year_sales.1)}else{None},"prix_dernieres_ventes":if latest_average.1>0.0{Some(latest_average.0/latest_average.1)}else{None},"morts_annee":year_deaths}),
    );
    Ok(render(&state, "dashboard.html", Value::Object(ctx))?.into_response())
}

const BAND_FIELDS: &str = "id,code,num_officiel,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr";
const BAND_SELECT_ACTIVE: &str = "SELECT id,code,num_officiel,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr FROM bande WHERE active=1 ORDER BY date_mb IS NULL,date_mb DESC,id DESC";

async fn bandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let schedule = load_band_schedule(&state.pool).await?;
    let mut views = Vec::new();
    for band in bands {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE bande_code=? AND reformee=0")
                .bind(&band.code)
                .fetch_one(&state.pool)
                .await?;
        let flux = band_flow_summary(&state.pool, &band).await?;
        views.push(band_view(&band, count, schedule, flux));
    }
    let mut ctx = context(&session);
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(views).unwrap_or_default(),
    );
    let nombre_bandes: i64 = sqlx::query_scalar(
        "SELECT CAST(valeur AS INTEGER) FROM parametre WHERE cle='nombre_bandes'",
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(3);
    let intervalle_bandes_j: i64 = sqlx::query_scalar(
        "SELECT CAST(valeur AS INTEGER) FROM parametre WHERE cle='intervalle_bandes_j'",
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(49);
    ctx.insert(
        "conduite_bandes".into(),
        json!({"nombre":nombre_bandes,"intervalle_j":intervalle_bandes_j}),
    );
    ctx.insert(
        "sites".into(),
        Value::Array(
            generic_rows(
                &state.pool,
                "SELECT code,nom,zone FROM site ORDER BY COALESCE(nom,code),zone",
            )
            .await?,
        ),
    );
    ctx.insert(
        "marquages".into(),
        Value::Array(
            generic_rows(
                &state.pool,
                "SELECT id,numero FROM numeromarquage WHERE actif=1 ORDER BY numero COLLATE NOCASE",
            )
            .await?,
        ),
    );
    render(&state, "bandes.html", Value::Object(ctx))
}

async fn selected_site(
    pool: &SqlitePool,
    form: &HashMap<String, String>,
) -> AppResult<Option<String>> {
    let Some(code) = form_text(form, "site") else {
        return Ok(None);
    };
    sqlx::query_scalar("SELECT code FROM site WHERE code=?")
        .bind(&code)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Invalid("Sélectionnez un site / une zone dans la liste".into()))
        .map(Some)
}

async fn selected_marking(
    pool: &SqlitePool,
    form: &HashMap<String, String>,
) -> AppResult<Option<String>> {
    let Some(number) = form_text(form, "num_officiel") else {
        return Ok(None);
    };
    sqlx::query_scalar("SELECT numero FROM numeromarquage WHERE actif=1 AND numero=?")
        .bind(number.trim().to_uppercase())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Invalid("Sélectionnez un numéro de marquage dans la liste".into()))
        .map(Some)
}

async fn marquage_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number = form_text(&form, "numero")
        .ok_or_else(|| AppError::Invalid("Numéro de marquage obligatoire".into()))?
        .trim()
        .to_uppercase();
    sqlx::query(
        "INSERT INTO numeromarquage(numero) VALUES(?) ON CONFLICT(numero) DO UPDATE SET actif=1",
    )
    .bind(number)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn bande_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let code =
        form_text(&form, "code").ok_or_else(|| AppError::Invalid("Code obligatoire".into()))?;
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE lower(code)=lower(?) AND active=1")
            .bind(&code)
            .fetch_one(&state.pool)
            .await?;
    if exists > 0 {
        return Err(AppError::Invalid("Cette bande existe déjà".into()));
    }
    let marking = selected_marking(&state.pool, &form).await?;
    let site = selected_site(&state.pool, &form).await?;
    sqlx::query("INSERT INTO bande(code,num_officiel,date_mb,site,active) VALUES(?,?,?,?,1)")
        .bind(&code)
        .bind(marking)
        .bind(form_date(&form, "date_mb")?)
        .bind(site)
        .execute(&state.pool)
        .await?;
    db::journal(
        &state.pool,
        &session.nom,
        "créer",
        "bande",
        &code,
        "/bandes/ajouter",
    )
    .await;
    Ok(Redirect::to("/bandes").into_response())
}

/// Édition « à la volée » depuis la liste `/bandes` (§2 des demandes en
/// attente) : chaque ligne du tableau est son propre petit formulaire (voir
/// `form="b{{ id }}"` dans `bandes.html`), pour changer mise-bas/site/n°
/// marquage sans ouvrir la fiche bande complète. Les champs calculés
/// (stade, effectif) ne sont pas éditables ici : ce sont des valeurs
/// dérivées (truies actives, planning), pas des colonnes de `bande` — les
/// modifier directement casserait la cohérence avec la fiche bande, qui les
/// recalcule toujours à partir des mêmes données.
async fn bande_modifier_rapide(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let marking = selected_marking(&state.pool, &form).await?;
    let site = selected_site(&state.pool, &form).await?;
    sqlx::query("UPDATE bande SET num_officiel=?,date_mb=?,site=?,updated_at=CURRENT_TIMESTAMP WHERE id=? AND active=1")
        .bind(marking)
        .bind(form_date(&form, "date_mb")?)
        .bind(site)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn bande_detail(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let sql = format!("SELECT {BAND_FIELDS} FROM bande WHERE id=?");
    let band = sqlx::query_as::<_, Bande>(&sql)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let sows = sqlx::query_as::<_, Truie>(TRUIE_SELECT_BY_BAND)
        .bind(&band.code)
        .fetch_all(&state.pool)
        .await?;
    let events = sqlx::query_as::<_, Evenement>(EVENT_SELECT_BY_BAND)
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
    let litters = load_gttt_litters(&state.pool, Some(&band.code)).await?;
    let technical_summary = if litters.is_empty() {
        gttt_band_fallback(&band, &events)
    } else {
        gttt_summary(&litters)
    };
    let schedule = load_band_schedule(&state.pool).await?;
    let dates = key_dates(band.date_mb.as_deref(), schedule);
    let porcs_presents = total_band_pigs(&state.pool, band.id, &band.code).await?;
    let flux = band_flow_summary(&state.pool, &band).await?;
    // Emplacement actuel (stade + effectif réel), déduit des cases où la
    // bande a été affectée : contrairement au journal ci-dessus (historique
    // brut des mouvements), chaque case n'apparaît qu'une fois, avec son
    // stade actuel et son effectif réellement présent aujourd'hui. Limite
    // connue : l'effectif compte tous les porcs de la case, pas seulement
    // ceux de cette bande, si plusieurs bandes y ont été mêlées.
    let emplacement_cases = generic_rows(
        &state.pool,
        &format!("SELECT c.id,COALESCE(si.nom,si.code) AS site,s.nom AS salle,c.nom AS unite,MAX(t.date) AS derniere_arrivee FROM transfert t JOIN casesalle c ON c.id=t.case_dest_id JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id WHERE t.espece='porc' AND t.bande_id={} GROUP BY c.id ORDER BY derniere_arrivee DESC", band.id),
    ).await?;
    let mut emplacement_actuel = Vec::new();
    for case in &emplacement_cases {
        let Some(case_id) = case.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let effectif = case_pig_count(&state.pool, case_id).await?;
        let stade = stade_from_case(&state.pool, case_id).await?;
        emplacement_actuel.push(json!({
            "case_id": case_id, "site": case.get("site"), "salle": case.get("salle"), "unite": case.get("unite"),
            "stade": stade, "effectif": effectif,
        }));
    }
    let vente_reelle: Option<String> = sqlx::query_scalar(
        "SELECT MAX(date) FROM venteapport v WHERE v.bande_id=? OR (json_type(CASE WHEN json_valid(v.lots_json) THEN v.lots_json ELSE 'null' END)='array' AND EXISTS(SELECT 1 FROM json_each(v.lots_json) j WHERE CAST(json_extract(j.value,'$.bande_id') AS INTEGER)=?))",
    ).bind(band.id).bind(band.id).fetch_one(&state.pool).await?;
    let depart_prevu = band
        .date_mb
        .as_deref()
        .and_then(parse_stored_date)
        .map(|date| date + Duration::days(schedule.departure));
    let reference = vente_reelle
        .as_deref()
        .and_then(parse_stored_date)
        .unwrap_or_else(|| Local::now().date_naive());
    let ecart_vente = depart_prevu.map(|date| (reference - date).num_days());
    let statut_vente = match (vente_reelle.is_some(), ecart_vente) {
        (true, Some(value)) if value < 0 => format!("Vendu avec {} jour(s) d’avance", -value),
        (true, Some(value)) if value > 0 => format!("Vendu avec {value} jour(s) de retard"),
        (true, Some(_)) => "Vendu à la date prévue".to_string(),
        (false, Some(value)) if value > 0 && porcs_presents > 0 => {
            format!("En retard de {value} jour(s)")
        }
        (false, Some(value)) if value <= 0 => format!("Départ prévu dans {} jour(s)", -value),
        (false, Some(_)) => "Date de départ dépassée, aucun porc présent".to_string(),
        _ => "Date de mise-bas à renseigner".to_string(),
    };
    // Indicateurs de présence : 1er départ, durée moyenne de présence, dernier départ.
    // — "réel" à partir des lots effectivement vendus (venteapport, y compris ceux
    //   répartis entre plusieurs bandes via lots_json) ;
    // — "prévision" pour les porcs encore présents, à partir de la date de mise-bas
    //   et de l'échéance de départ du planning d'élevage.
    let depart_rows = generic_rows(
        &state.pool,
        &format!(
            "SELECT date,nb_porcs FROM venteapport WHERE bande_id={0} AND date IS NOT NULL AND nb_porcs IS NOT NULL \
             UNION ALL \
             SELECT v.date,CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER) FROM venteapport v, json_each(v.lots_json) j \
             WHERE v.bande_id IS NULL AND json_valid(v.lots_json) AND json_type(v.lots_json)='array' \
             AND CAST(json_extract(j.value,'$.bande_id') AS INTEGER)={0} AND v.date IS NOT NULL",
            band.id
        ),
    ).await?;
    let date_mb = band.date_mb.as_deref().and_then(parse_stored_date);
    let mut premier_depart_reel: Option<NaiveDate> = None;
    let mut dernier_depart_reel: Option<NaiveDate> = None;
    let mut total_partis: i64 = 0;
    let mut jours_ponderes: i64 = 0;
    for row in &depart_rows {
        let Some(date) = row
            .get("date")
            .and_then(Value::as_str)
            .and_then(parse_stored_date)
        else {
            continue;
        };
        let nb = row.get("nb_porcs").and_then(Value::as_i64).unwrap_or(0);
        if nb <= 0 {
            continue;
        }
        premier_depart_reel = Some(premier_depart_reel.map_or(date, |d| d.min(date)));
        dernier_depart_reel = Some(dernier_depart_reel.map_or(date, |d| d.max(date)));
        total_partis += nb;
        if let Some(mb) = date_mb {
            jours_ponderes += (date - mb).num_days() * nb;
        }
    }
    let moyenne_jours_presence_reel = (total_partis > 0 && date_mb.is_some())
        .then(|| jours_ponderes as f64 / total_partis as f64);
    let prevision_presence = (porcs_presents > 0 && depart_prevu.is_some()).then(|| {
        json!({
            "premier_depart": depart_prevu.map(|d| d.format("%Y-%m-%d").to_string()),
            "dernier_depart": depart_prevu.map(|d| d.format("%Y-%m-%d").to_string()),
            "moyenne_jours_presence": schedule.departure,
        })
    });
    let mut ctx = context(&session);
    ctx.insert(
        "bande".into(),
        serde_json::to_value(&band).unwrap_or_default(),
    );
    ctx.insert(
        "truies".into(),
        serde_json::to_value(&sows).unwrap_or_default(),
    );
    ctx.insert(
        "evenements".into(),
        serde_json::to_value(&events).unwrap_or_default(),
    );
    ctx.insert("dates".into(), Value::Array(dates));
    ctx.insert("flux".into(), flux);
    ctx.insert(
        "emplacement_actuel".into(),
        Value::Array(emplacement_actuel),
    );
    ctx.insert(
        "suivi_porcs".into(),
        json!({
            "presents":porcs_presents,
            "depart_prevu":depart_prevu.map(|d|d.format("%Y-%m-%d").to_string()),
            "vente_reelle":vente_reelle,
            "statut":statut_vente,
            "ecart_jours":ecart_vente,
            "premier_depart_reel":premier_depart_reel.map(|d|d.format("%Y-%m-%d").to_string()),
            "dernier_depart_reel":dernier_depart_reel.map(|d|d.format("%Y-%m-%d").to_string()),
            "moyenne_jours_presence_reel":moyenne_jours_presence_reel,
            "prevision":prevision_presence,
        }),
    );
    ctx.insert(
        "resume".into(),
        json!({
            "truies": sows.len(),
            "portees": technical_summary.portees,
            "nv": technical_summary.total_nes_vifs,
            "sevres": technical_summary.total_sevres,
            "pertes": technical_summary.mortalite_allaitement,
            "nv_portee": technical_summary.nes_vifs_moy,
            "sevres_portee": technical_summary.sevres_moy,
            "mortnes": technical_summary.taux_mortnes,
        }),
    );
    ctx.insert(
        "marquages".into(),
        Value::Array(
            generic_rows(
                &state.pool,
                "SELECT numero FROM numeromarquage WHERE actif=1 ORDER BY numero COLLATE NOCASE",
            )
            .await?,
        ),
    );
    ctx.insert("today".into(), json!(today_iso()));
    render(&state, "bande.html", Value::Object(ctx))
}

async fn bande_marquage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number = form_text(&form, "num_marquage")
        .ok_or_else(|| AppError::Invalid("Numéro de marquage obligatoire".into()))?;
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM numeromarquage WHERE actif=1 AND numero=?")
            .bind(number.trim().to_uppercase())
            .fetch_one(&state.pool)
            .await?;
    if exists == 0 {
        return Err(AppError::Invalid(
            "Sélectionnez un numéro de marquage dans la liste".into(),
        ));
    }
    sqlx::query("UPDATE bande SET num_officiel=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(number.trim().to_uppercase())
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/bande/{id}")).into_response())
}

fn key_dates(date_mb: Option<&str>, schedule: BandSchedule) -> Vec<Value> {
    let Some(date) = date_mb.and_then(parse_stored_date) else {
        return vec![];
    };
    let today = Local::now().date_naive();
    let stages = schedule.stages();
    stages
        .iter()
        .enumerate()
        .map(|(index, (name, days))| {
            let stage_date = date + Duration::days(*days);
            let next = stages.get(index + 1).map(|(_, d)| date + Duration::days(*d));
            let current = stage_date <= today && next.map(|value| today < value).unwrap_or(true);
            let state = if current { "En cours" } else if stage_date < today { "Fait" } else { "À venir" };
            json!({"nom": name, "date": stage_date.format("%Y-%m-%d").to_string(), "actuelle": current, "etat": state})
        })
        .collect()
}

async fn bande_archiver(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE bande SET active=0,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn bande_desarchiver(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE bande SET active=1,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/bande/{id}")).into_response())
}

async fn bande_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM evenement WHERE bande_id=?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if linked > 0 {
        return Err(AppError::Invalid(
            "Impossible de supprimer une bande qui contient des événements; archive-la.".into(),
        ));
    }
    sqlx::query("DELETE FROM bande WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn archives(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    list_page(
        &state,
        &session,
        "Bandes archivées",
        "Historique conservé",
        "SELECT id,code,date_mb,site,note FROM bande WHERE active=0 ORDER BY date_mb DESC,id DESC",
        &["id", "code", "date_mb", "site", "note"],
    )
    .await
}

const TRUIE_FIELDS: &str = "id,num_travail,num_national,rfid,race,date_entree,statut,note,rang,date_naissance,reformee,date_reforme,motif_sortie,mere_cochette,bande_code,salle_id,case_id,lignee_id,perf_nt,perf_nv,perf_mn,perf_sevres,perf_tx_perte";
const TRUIE_SELECT_BY_BAND: &str = "SELECT id,num_travail,num_national,rfid,race,date_entree,statut,note,rang,date_naissance,reformee,date_reforme,motif_sortie,mere_cochette,bande_code,salle_id,case_id,lignee_id,perf_nt,perf_nv,perf_mn,perf_sevres,perf_tx_perte FROM truie WHERE bande_code=? AND reformee=0 ORDER BY num_travail";

async fn truies(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let q = query.get("q").cloned().unwrap_or_default();
    let sql = if q.is_empty() {
        format!("SELECT {TRUIE_FIELDS} FROM truie WHERE reformee=0 ORDER BY num_travail")
    } else {
        format!("SELECT {TRUIE_FIELDS} FROM truie WHERE reformee=0 AND (num_travail LIKE ? OR num_national LIKE ? OR rfid LIKE ? OR case_id IN (SELECT id FROM casesalle WHERE COALESCE(num_vanne,'') LIKE ?)) ORDER BY num_travail")
    };
    let sows = if q.is_empty() {
        sqlx::query_as::<_, Truie>(&sql)
            .fetch_all(&state.pool)
            .await?
    } else {
        let pattern = format!("%{q}%");
        sqlx::query_as::<_, Truie>(&sql)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&state.pool)
            .await?
    };
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let mut ctx = context(&session);
    ctx.insert(
        "truies".into(),
        serde_json::to_value(sows).unwrap_or_default(),
    );
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(bands).unwrap_or_default(),
    );
    ctx.insert("q".into(), json!(q));
    if session.module_genetique {
        ctx.insert(
            "lignees".into(),
            Value::Array(
                generic_rows(
                    &state.pool,
                    "SELECT id,nom FROM lignee_genetique ORDER BY nom",
                )
                .await?,
            ),
        );
    }
    render(&state, "truies.html", Value::Object(ctx))
}

async fn truie_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number = form_text(&form, "num_travail")
        .ok_or_else(|| AppError::Invalid("N° travail obligatoire".into()))?;
    let duplicate: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE num_travail=? AND reformee=0")
            .bind(&number)
            .fetch_one(&state.pool)
            .await?;
    if duplicate > 0 {
        return Err(AppError::Invalid("Ce numéro de travail existe déjà".into()));
    }
    // La lignée n'est proposée que si le module Génétique avancée est actif :
    // une valeur envoyée sans le module est ignorée plutôt que refusée.
    let lignee_id = session
        .module_genetique
        .then(|| form_i64(&form, "lignee_id"))
        .flatten();
    let result = sqlx::query("INSERT INTO truie(num_travail,num_national,rfid,race,date_entree,bande_code,lignee_id,statut,reformee,rang,mere_cochette) VALUES(?,?,?,?,?,?,?,'active',0,0,0)")
        .bind(&number).bind(form_text(&form,"num_national")).bind(form_text(&form,"rfid"))
        .bind(form_text(&form,"race")).bind(form_date(&form,"date_entree")?).bind(form_text(&form,"bande_code"))
        .bind(lignee_id)
        .execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/truie/{}", result.last_insert_rowid())).into_response())
}

async fn truies_affecter_bande(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let ids = form_selected_ids(&form, "truie_");
    if ids.is_empty() {
        return Err(AppError::Invalid("Sélectionne au moins une truie".into()));
    }
    let bande_code = form_text(&form, "bande_code");
    if let Some(code) = bande_code.as_deref() {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE code=? AND active=1")
                .bind(code)
                .fetch_one(&state.pool)
                .await?;
        if exists == 0 {
            return Err(AppError::Invalid("Bande active introuvable".into()));
        }
    }
    let mut tx = state.pool.begin().await?;
    for id in ids {
        sqlx::query(
            "UPDATE truie SET bande_code=?,updated_at=CURRENT_TIMESTAMP WHERE id=? AND reformee=0",
        )
        .bind(&bande_code)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let retour = if form.get("retour").is_some_and(|value| value == "/attente") {
        "/attente"
    } else {
        "/truies"
    };
    Ok(Redirect::to(retour).into_response())
}

async fn truie_detail(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let sql = format!("SELECT {TRUIE_FIELDS} FROM truie WHERE id=?");
    let sow = sqlx::query_as::<_, Truie>(&sql)
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let events = sqlx::query_as::<_, Evenement>(EVENT_SELECT_BY_SOW)
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
    let mesures = generic_rows(
        &state.pool,
        &format!("SELECT id,date,eld,poids,nec,periode,note FROM mesuretruie WHERE truie_id={id} ORDER BY date DESC,id DESC"),
    )
    .await?;
    let pertes = generic_rows(
        &state.pool,
        &format!("SELECT id,date,age_j,nb,cause FROM perteporcelet WHERE truie_id={id} ORDER BY date DESC,id DESC"),
    )
    .await?;
    let causes = generic_rows(
        &state.pool,
        "SELECT id,libelle FROM causeperte ORDER BY libelle COLLATE NOCASE",
    )
    .await?;
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let cases = generic_rows(
        &state.pool,
        "SELECT c.id,s.nom||' · '||c.nom||CASE WHEN COALESCE(c.num_vanne,'')<>'' THEN ' · vanne '||c.num_vanne ELSE '' END AS label,c.num_vanne FROM casesalle c JOIN salle s ON s.id=c.salle_id ORDER BY s.ordre,s.nom,c.nom",
    )
    .await?;
    let emplacement = sqlx::query("SELECT c.id,c.nom AS case_nom,c.num_vanne,s.nom AS salle,COALESCE(si.nom,si.code) AS site FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id LEFT JOIN salle s ON s.id=c.salle_id LEFT JOIN site si ON si.id=s.site_id WHERE t.id=?")
        .bind(id).fetch_all(&state.pool).await?;
    let emplacement = rows_to_json(emplacement)?
        .into_iter()
        .next()
        .unwrap_or_else(|| json!({}));
    let performances = historique_truie::portees(&state.pool, &sow).await?;
    let portee_actuelle = maternite_suivi::actuelle(&state.pool, id).await?;
    let probable_ia = sqlx::query("WITH mb AS (SELECT date FROM evenement WHERE truie_id=? AND type='mise_bas' ORDER BY date DESC,id DESC LIMIT 1) SELECT ia.date,ia.produit,ia.nb_doses,ia.creneaux_ia,CAST(julianday((SELECT date FROM mb))-julianday(ia.date) AS INTEGER) AS gestation_j,CASE WHEN ABS((julianday((SELECT date FROM mb))-julianday(ia.date))-115)<=2 THEN 'Correspondance forte' ELSE 'Correspondance plausible' END AS confiance FROM evenement ia WHERE ia.truie_id=? AND ia.type='ia' AND (SELECT date FROM mb) IS NOT NULL AND julianday((SELECT date FROM mb))-julianday(ia.date) BETWEEN 105 AND 125 ORDER BY ABS((julianday((SELECT date FROM mb))-julianday(ia.date))-115),ia.id DESC LIMIT 1")
        .bind(id).bind(id).fetch_all(&state.pool).await?;
    let probable_ia = rows_to_json(probable_ia)?.into_iter().next();
    let soins_portee = sqlx::query("SELECT sp.id,sp.date_prevue,sp.date_realisee,sp.note,a.libelle,a.produit,a.dose,a.unite,a.voie,c.num_vanne FROM soinportee sp JOIN acteprotocole a ON a.id=sp.protocole_id JOIN evenement e ON e.id=sp.evenement_id LEFT JOIN casesalle c ON c.id=e.case_id WHERE sp.truie_id=? ORDER BY sp.date_realisee IS NOT NULL,sp.date_prevue,sp.id")
        .bind(id).fetch_all(&state.pool).await?;
    let soins_portee = rows_to_json(soins_portee)?;
    let date_mb: Option<String> = if let Some(code) = sow.bande_code.as_deref() {
        sqlx::query_scalar(
            "SELECT date_mb FROM bande WHERE code=? ORDER BY active DESC,id DESC LIMIT 1",
        )
        .bind(code)
        .fetch_optional(&state.pool)
        .await?
        .flatten()
    } else {
        None
    };
    let mut ctx = context(&session);
    ctx.insert(
        "truie".into(),
        serde_json::to_value(&sow).unwrap_or_default(),
    );
    ctx.insert(
        "evenements".into(),
        serde_json::to_value(events).unwrap_or_default(),
    );
    ctx.insert("mesures".into(), Value::Array(mesures));
    ctx.insert("pertes".into(), Value::Array(pertes));
    ctx.insert("causes".into(), Value::Array(causes));
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(bands).unwrap_or_default(),
    );
    ctx.insert("cases".into(), Value::Array(cases));
    ctx.insert("emplacement".into(), emplacement);
    ctx.insert("portee_actuelle".into(), portee_actuelle);
    ctx.insert("performances_rang".into(), Value::Array(performances));
    ctx.insert("ia_probable".into(), probable_ia.unwrap_or(Value::Null));
    ctx.insert("soins_portee".into(), Value::Array(soins_portee));
    ctx.insert("motifs_sortie".into(), json!(SOW_EXIT_REASONS));
    let schedule = load_band_schedule(&state.pool).await?;
    ctx.insert(
        "dates".into(),
        Value::Array(key_dates(date_mb.as_deref(), schedule)),
    );
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    ctx.insert(
        "selection".into(),
        generic_rows(
            &state.pool,
            &format!("SELECT nb_tetines,splayleg FROM truie WHERE id={id}"),
        )
        .await?
        .into_iter()
        .next()
        .unwrap_or(Value::Null),
    );
    ctx.insert("mises_bas".into(), Value::Array(generic_rows(&state.pool,&format!("SELECT * FROM evenement WHERE truie_id={id} AND type='mise_bas' ORDER BY date DESC,id DESC")).await?));
    // Rattachement au catalogue de lignées (§2) : uniquement quand le module
    // Génétique avancée est actif, sinon la fiche garde `race` en texte libre.
    if session.module_genetique {
        ctx.insert(
            "lignees".into(),
            Value::Array(
                generic_rows(
                    &state.pool,
                    "SELECT id,nom,fournisseur FROM lignee_genetique ORDER BY nom",
                )
                .await?,
            ),
        );
        ctx.insert(
            "lignee".into(),
            generic_rows(
                &state.pool,
                &format!("SELECT l.nom,l.fournisseur,l.index_prolificite,l.index_croissance,l.index_ic FROM lignee_genetique l JOIN truie t ON t.lignee_id=l.id WHERE t.id={id}"),
            )
            .await?
            .into_iter()
            .next()
            .unwrap_or(Value::Null),
        );
    }
    render(&state, "truie.html", Value::Object(ctx))
}

/// Rattache la truie à une lignée du catalogue (§2). `race`, texte libre
/// historique, n'est pas modifié : les élevages qui n'activent pas le module
/// Génétique avancée continuent de fonctionner comme avant.
async fn truie_lignee(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    if !session.module_genetique {
        return Err(AppError::Invalid(
            "Le module Génétique avancée n'est pas activé (Paramètres > Type d'élevage et modules).".into(),
        ));
    }
    let lignee_id = form_i64(&form, "lignee_id");
    if let Some(lignee) = lignee_id {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lignee_genetique WHERE id=?")
            .bind(lignee)
            .fetch_one(&state.pool)
            .await?;
        if exists == 0 {
            return Err(AppError::Invalid("Lignée introuvable".into()));
        }
    }
    let result =
        sqlx::query("UPDATE truie SET lignee_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(lignee_id)
            .bind(id)
            .execute(&state.pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to(&format!("/truie/{id}#identification")).into_response())
}

async fn truie_bande(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE truie SET bande_code=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(form_text(&form, "bande_code"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

async fn truie_emplacement(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let case_id = form_i64(&form, "case_id");
    let salle_id: Option<i64> = if let Some(case_id) = case_id {
        Some(
            sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
                .bind(case_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or(AppError::NotFound)?,
        )
    } else {
        None
    };
    sqlx::query("UPDATE truie SET case_id=?,salle_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(case_id)
        .bind(salle_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{id}#identification")).into_response())
}

const SOW_EXIT_REASONS: &[&str] = &[
    "Réforme planifiée / âge ou rang",
    "Performances insuffisantes",
    "Infertilité / retours en chaleur",
    "Avortement / problème de gestation",
    "Prolapsus",
    "Complication de mise-bas / hémorragie",
    "Boiterie / appareil locomoteur",
    "Mamelle / allaitement",
    "État corporel insuffisant",
    "Agressivité / comportement maternel",
    "Maladie / cause sanitaire",
    "Trouble digestif constaté",
    "Trouble cardio-respiratoire constaté",
    "Trouble urinaire constaté",
    "Accident / traumatisme",
    "Morte subitement",
    "Mortalité de cause indéterminée",
    "Euthanasie",
    "Vente / transfert",
    "Autre motif validé",
];

async fn truie_reformer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "date")?;
    let reason =
        form_text(&form, "motif").ok_or_else(|| AppError::Invalid("Motif obligatoire".into()))?;
    if !SOW_EXIT_REASONS.contains(&reason.as_str()) {
        return Err(AppError::Invalid(
            "Sélectionnez un motif de sortie proposé".into(),
        ));
    }
    sqlx::query("UPDATE truie SET reformee=1,statut='reformee',date_reforme=?,motif_sortie=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(date).bind(reason).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/truies").into_response())
}

async fn truie_annuler_sortie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE truie SET reformee=0,statut='active',date_reforme=NULL,motif_sortie=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

async fn truie_mesure(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "date")?;
    sqlx::query(
        "INSERT INTO mesuretruie(truie_id,date,eld,poids,nec,note,periode) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(id)
    .bind(date)
    .bind(form_f64(&form, "eld"))
    .bind(form_f64(&form, "poids"))
    .bind(form_f64(&form, "nec"))
    .bind(form_text(&form, "note"))
    .bind(form_text(&form, "periode"))
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to(&format!("/truie/{id}#mesures")).into_response())
}

async fn mesure_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let sow_id: i64 = sqlx::query_scalar("SELECT truie_id FROM mesuretruie WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let eld = form_f64(&form, "eld").filter(|value| *value >= 0.0);
    let poids = form_f64(&form, "poids").filter(|value| *value >= 0.0);
    let nec = form_f64(&form, "nec").filter(|value| (1.0..=5.0).contains(value));
    if eld.is_none() && poids.is_none() && nec.is_none() {
        return Err(AppError::Invalid("Saisissez au moins une mesure".into()));
    }
    sqlx::query("UPDATE mesuretruie SET date=?,periode=?,eld=?,poids=?,nec=?,note=? WHERE id=?")
        .bind(form_date_or_today(&form, "date")?)
        .bind(form_text(&form, "periode"))
        .bind(eld)
        .bind(poids)
        .bind(nec)
        .bind(form_text(&form, "note"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{sow_id}#mesures")).into_response())
}

async fn mesure_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let sow: Option<i64> = sqlx::query_scalar("SELECT truie_id FROM mesuretruie WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    sqlx::query("DELETE FROM mesuretruie WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(
        &sow.map(|value| format!("/truie/{value}#mesures"))
            .unwrap_or_else(|| "/truies".into()),
    )
    .into_response())
}

async fn truie_perte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    maternite_suivi::enregistrer_perte(&state.pool, id, None, &form, None).await?;
    Ok(Redirect::to(&format!("/truie/{id}#pertes")).into_response())
}

/// Tableau opérationnel de maternité : la bande dont la mise-bas est la plus
/// proche est proposée, tout en permettant de revenir sur n'importe quelle
/// bande. Une truie reste dans ce tableau même si son affectation courante a
/// changé, dès lors qu'une mise-bas a été enregistrée pour le cycle choisi.
async fn maternite(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let requested_band = query
        .get("bande_id")
        .and_then(|value| value.parse::<i64>().ok());
    let bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site,active FROM bande WHERE date_mb IS NOT NULL AND trim(date_mb)<>'' ORDER BY date(date_mb) DESC,id DESC LIMIT 40",
    )
    .await?;
    let selected_id = if let Some(id) = requested_band {
        sqlx::query_scalar::<_, i64>("SELECT id FROM bande WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
    } else {
        sqlx::query_scalar::<_, i64>(
            "SELECT id FROM bande WHERE date_mb IS NOT NULL AND trim(date_mb)<>'' ORDER BY CASE WHEN date('now') BETWEEN date(date_mb,'-10 day') AND date(date_mb,'+28 day') THEN 0 ELSE 1 END,ABS(julianday('now')-julianday(date_mb)),id DESC LIMIT 1",
        )
        .fetch_optional(&state.pool)
        .await?
    };
    let mut ctx = context(&session);
    let vue = query
        .get("vue")
        .or_else(|| query.get("onglet"))
        .map(String::as_str)
        .unwrap_or("mises-bas");
    let vue = match vue {
        "adoptions" | "nourrices" | "sevrage" | "bilan" => vue,
        _ => "mises-bas",
    };
    ctx.insert("vue".into(), json!(vue));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("today".into(), json!(today_iso()));
    let Some(band_id) = selected_id else {
        ctx.insert("truies".into(), Value::Array(Vec::new()));
        return render(&state, "maternite.html", Value::Object(ctx));
    };
    let band_row = sqlx::query(
        "SELECT id,code,date_mb,site,active,CAST(julianday('now')-julianday(date_mb) AS INTEGER) AS jour_cycle,date(date_mb,'+28 day') AS fin_suivi FROM bande WHERE id=?",
    )
    .bind(band_id)
    .fetch_all(&state.pool)
    .await?;
    let band = rows_to_json(band_row)?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound)?;
    let sow_rows = sqlx::query(
        "WITH sow_ids AS (SELECT id FROM truie WHERE bande_code=(SELECT code FROM bande WHERE id=?) UNION SELECT DISTINCT truie_id FROM evenement WHERE bande_id=? AND truie_id IS NOT NULL) SELECT t.id,t.num_travail,t.rfid,t.rang,t.race,e.id AS mise_bas_id,e.date AS date_mise_bas,COALESCE(e.nes_totaux,0) AS nes_totaux,COALESCE(e.nes_vifs,0) AS nes_vifs,COALESCE(e.mort_nes,0) AS mort_nes,COALESCE(e.momifies,0) AS momifies,COALESCE(e.chetifs,0) AS chetifs,COALESCE(e.ecrases,0) AS ecrases,COALESCE(e.tues_truie,0) AS tues_truie,e.heure_debut,e.heure_fin,COALESCE(e.suivi_actif,0) AS suivi_actif,e.delivrance_ok,e.note,c.num_vanne,c.nom AS case_nom,COALESCE((SELECT COUNT(*) FROM soinportee sp WHERE sp.evenement_id=e.id AND sp.date_realisee IS NULL),0) AS soins_attendus,(SELECT MIN(sp.date_prevue) FROM soinportee sp WHERE sp.evenement_id=e.id AND sp.date_realisee IS NULL) AS prochain_soin,pe.date_sevrage,pe.cloturee,pe.adoptes,pe.retires,CASE WHEN e.date IS NOT NULL THEN CAST(julianday('now')-julianday(e.date) AS INTEGER) END AS jour_lactation,CASE WHEN e.date IS NOT NULL THEN date(e.date,'+28 day') END AS fin_suivi,CASE WHEN date('now')<date(e.date) THEN date(e.date) WHEN date('now')>date(e.date,'+28 day') THEN date(e.date,'+28 day') ELSE date('now') END AS date_perte_defaut,COALESCE(pe.pertes,0) AS pertes,COALESCE(pe.presents,0) AS porcelets_presents FROM sow_ids s JOIN truie t ON t.id=s.id LEFT JOIN evenement e ON e.id=(SELECT e2.id FROM evenement e2 WHERE e2.truie_id=t.id AND e2.bande_id=? AND e2.type='mise_bas' AND e2.date<=date('now') ORDER BY e2.date DESC,e2.id DESC LIMIT 1) LEFT JOIN portee_effectif pe ON pe.id=e.id LEFT JOIN casesalle c ON c.id=COALESCE(e.case_id,t.case_id) ORDER BY t.num_travail COLLATE NOCASE",
    )
    .bind(band_id)
    .bind(band_id)
    .bind(band_id)
    .fetch_all(&state.pool)
    .await?;
    let mut sows = rows_to_json(sow_rows)?;
    let loss_rows = sqlx::query(
        "SELECT p.id,p.truie_id,p.date,p.age_j,p.nb,p.cause,p.evenement_id FROM perteporcelet p JOIN portee_effectif pe ON pe.truie_id=p.truie_id WHERE pe.bande_id=? AND pe.id=(SELECT e.id FROM evenement e WHERE e.truie_id=pe.truie_id AND e.bande_id=pe.bande_id AND e.type='mise_bas' AND e.date<=date('now') ORDER BY e.date DESC,e.id DESC LIMIT 1) AND (p.evenement_id=pe.id OR (p.evenement_id IS NULL AND (p.bande_id IS pe.bande_id OR p.bande_id IS NULL OR pe.bande_id IS NULL) AND p.date>=pe.date AND (pe.prochaine_mb IS NULL OR p.date<pe.prochaine_mb))) AND p.date<=COALESCE(pe.date_sevrage,date('now')) ORDER BY p.date DESC,p.id DESC",
    )
    .bind(band_id)
    .fetch_all(&state.pool)
    .await?;
    let mut losses_by_sow: HashMap<i64, Vec<Value>> = HashMap::new();
    for loss in rows_to_json(loss_rows)? {
        if let Some(sow_id) = loss.get("truie_id").and_then(Value::as_i64) {
            losses_by_sow.entry(sow_id).or_default().push(loss);
        }
    }
    let mut totals = json!({"truies":sows.len(),"mises_bas":0,"restantes":0,"en_cours":0,"surveillance":0,"terminees":0,"sevrees":0,"nes_vifs":0,"mort_nes":0,"momifies":0,"pertes":0,"presents":0});
    for sow in &mut sows {
        let has_birth = sow["mise_bas_id"].as_i64().is_some();
        maternite_suivi::annoter(sow, has_birth);
        let object = json_object_mut(sow, "tableau de maternité")?;
        let sow_id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        object.insert(
            "pertes_detail".into(),
            Value::Array(losses_by_sow.remove(&sow_id).unwrap_or_default()),
        );
        let status = object
            .get("statut_code")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let totals_object = totals.as_object_mut().expect("objet de totaux");
        let increment = |map: &mut Map<String, Value>, key: &str, amount: i64| {
            let old = map.get(key).and_then(Value::as_i64).unwrap_or_default();
            map.insert(key.into(), json!(old + amount));
        };
        if status == "a_mettre_bas" {
            increment(totals_object, "restantes", 1);
        } else {
            increment(totals_object, "mises_bas", 1);
        }
        if status == "en_cours" {
            increment(totals_object, "en_cours", 1);
        }
        if status == "terminee" {
            increment(totals_object, "terminees", 1);
        }
        if status == "sevree" {
            increment(totals_object, "sevrees", 1);
        }
        if object.get("surveillance").and_then(Value::as_bool) == Some(true) {
            increment(totals_object, "surveillance", 1);
        }
        for key in ["mort_nes", "momifies"] {
            increment(
                totals_object,
                key,
                object.get(key).and_then(Value::as_i64).unwrap_or_default(),
            );
        }
        increment(
            totals_object,
            "nes_vifs",
            object
                .get("nes_vifs")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        );
        increment(
            totals_object,
            "pertes",
            object
                .get("pertes")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        );
        increment(
            totals_object,
            "presents",
            object
                .get("porcelets_presents")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        );
    }
    let causes = generic_rows(
        &state.pool,
        "SELECT libelle FROM causeperte ORDER BY libelle COLLATE NOCASE",
    )
    .await?;
    let weaning_sows = generic_rows(&state.pool, &format!(
        "SELECT t.id,t.num_travail,e.date AS date_mise_bas,e.case_id AS case_source_id,c.nom AS case_source,c.num_vanne AS vanne_source,COALESCE((SELECT presents FROM portee_effectif pe WHERE pe.id=e.id),0) AS porcelets_presents FROM evenement e JOIN truie t ON t.id=e.truie_id LEFT JOIN casesalle c ON c.id=COALESCE(e.case_id,t.case_id) WHERE e.bande_id={band_id} AND e.type='mise_bas' AND e.id=(SELECT e2.id FROM evenement e2 WHERE e2.bande_id={band_id} AND e2.truie_id=t.id AND e2.type='mise_bas' ORDER BY e2.date DESC,e2.id DESC LIMIT 1) AND EXISTS(SELECT 1 FROM portee_effectif pe WHERE pe.id=e.id AND pe.cloturee=0 AND pe.date<=date('now')) ORDER BY e.date,t.num_travail COLLATE NOCASE"
    )).await?;
    let mut destinations = generic_rows(&state.pool, "SELECT c.id,c.salle_id,c.nom,c.num_vanne,c.nb_max_porcs,s.nom AS salle,COALESCE(si.nom,si.code) AS site FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id WHERE lower(COALESCE(s.type,'')) LIKE '%sevr%' OR lower(s.nom) LIKE '%sevr%' ORDER BY COALESCE(si.nom,si.code),s.ordre,c.nom").await?;
    for destination in &mut destinations {
        let object = json_object_mut(destination, "les destinations de sevrage")?;
        let case_id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        let present = case_pig_count(&state.pool, case_id).await?;
        let capacity = object.get("nb_max_porcs").and_then(Value::as_i64);
        object.insert("effectif".into(), json!(present));
        object.insert(
            "places_disponibles".into(),
            capacity
                .map(|value| json!((value - present).max(0)))
                .unwrap_or(Value::Null),
        );
    }
    let receveuses = generic_rows(&state.pool, "SELECT e.id,t.num_travail,b.code AS bande,p.presents FROM portee_effectif p JOIN evenement e ON e.id=p.id JOIN truie t ON t.id=e.truie_id JOIN bande b ON b.id=e.bande_id WHERE t.reformee=0 AND p.cloturee=0 AND p.date<=date('now') AND e.id=(SELECT e2.id FROM evenement e2 WHERE e2.truie_id=t.id AND e2.type='mise_bas' ORDER BY e2.date DESC,e2.id DESC LIMIT 1) ORDER BY t.num_travail").await?;
    let adoptions = generic_rows(&state.pool, &format!("SELECT a.date,a.nombre,a.case_nourrice_id,a.note,ts.num_travail AS donneuse,td.num_travail AS receveuse,c.nom AS case_nourrice,s.nom AS salle_nourrice FROM adoptionporcelet a JOIN evenement es ON es.id=a.source_id LEFT JOIN evenement ed ON ed.id=a.destination_id JOIN truie ts ON ts.id=es.truie_id LEFT JOIN truie td ON td.id=ed.truie_id LEFT JOIN casesalle c ON c.id=a.case_nourrice_id LEFT JOIN salle s ON s.id=c.salle_id WHERE es.bande_id={band_id} OR ed.bande_id={band_id} ORDER BY a.date DESC,a.id DESC")).await?;
    let cases_nourrices = generic_rows(&state.pool, "SELECT c.id,c.nom,s.nom AS salle FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE lower(COALESCE(s.type,'')) LIKE '%nourri%' OR lower(s.nom) LIKE '%nourri%' OR lower(COALESCE(s.type,'')) LIKE '%matern%' OR lower(s.nom) LIKE '%matern%' ORDER BY s.nom,c.nom").await?;
    let nourrices = generic_rows(&state.pool, &format!("SELECT n.id,n.date,n.presents,c.nom AS case_nom,s.nom AS salle,t.num_travail AS donneuse FROM nourrice_effectif n JOIN casesalle c ON c.id=n.case_nourrice_id JOIN salle s ON s.id=c.salle_id JOIN truie t ON t.id=n.truie_id WHERE n.bande_id={band_id} ORDER BY s.nom,c.nom,n.date,n.id")).await?;
    let sorties_nourrices = generic_rows(&state.pool, &format!("SELECT sn.date,sn.type,sn.nombre,sn.cause,c.nom AS case_nom,t.num_travail AS donneuse FROM sortienourrice sn JOIN nourrice_effectif n ON n.id=sn.adoption_id JOIN casesalle c ON c.id=n.case_nourrice_id JOIN truie t ON t.id=n.truie_id WHERE n.bande_id={band_id} ORDER BY sn.date DESC,sn.id DESC")).await?;
    let presents_nourrice: i64 = nourrices
        .iter()
        .map(|n| n.get("presents").and_then(Value::as_i64).unwrap_or(0))
        .sum();
    let pertes_nourrice: i64 = sorties_nourrices
        .iter()
        .filter(|s| s.get("type").and_then(Value::as_str) == Some("perte"))
        .map(|s| s.get("nombre").and_then(Value::as_i64).unwrap_or(0))
        .sum();
    totals["sous_meres"] = totals["presents"].clone();
    totals["presents"] = json!(totals["presents"].as_i64().unwrap_or(0) + presents_nourrice);
    totals["pertes"] = json!(totals["pertes"].as_i64().unwrap_or(0) + pertes_nourrice);
    totals["nourrice"] = json!(presents_nourrice);
    ctx.insert("cases_nourrices".into(), Value::Array(cases_nourrices));
    ctx.insert("nourrices".into(), Value::Array(nourrices));
    ctx.insert("sorties_nourrices".into(), Value::Array(sorties_nourrices));
    ctx.insert("receveuses".into(), Value::Array(receveuses));
    ctx.insert("adoptions".into(), Value::Array(adoptions));
    ctx.insert("bande".into(), band);
    ctx.insert("truies".into(), Value::Array(sows));
    ctx.insert("truies_sevrage".into(), Value::Array(weaning_sows));
    ctx.insert("destinations_sevrage".into(), Value::Array(destinations));
    ctx.insert("causes".into(), Value::Array(causes));
    ctx.insert("totaux".into(), totals);
    render(&state, "maternite.html", Value::Object(ctx))
}

async fn maternite_sevrage(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(band_id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "date")?;
    let band_code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(band_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut selections = Vec::<(i64, i64, i64, Option<i64>, i64)>::new();
    let mut assigned: HashMap<i64, i64> = HashMap::new();
    for key in form.keys().filter(|key| key.starts_with("selection_")) {
        let sow_id = key
            .trim_start_matches("selection_")
            .parse::<i64>()
            .map_err(|_| AppError::Invalid("Sélection de truie invalide".into()))?;
        let number = form
            .get(&format!("nb_{sow_id}"))
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v >= 0)
            .ok_or_else(|| AppError::Invalid("Nombre de porcelets invalide".into()))?;
        let destination = form
            .get(&format!("case_{sow_id}"))
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or_else(|| {
                AppError::Invalid(
                    "Choisissez une case de post-sevrage pour chaque truie cochée".into(),
                )
            })?;
        let source: Option<i64> = sqlx::query_scalar("SELECT COALESCE(e.case_id,t.case_id) FROM evenement e JOIN truie t ON t.id=e.truie_id WHERE e.bande_id=? AND e.truie_id=? AND e.type='mise_bas' AND EXISTS(SELECT 1 FROM portee_effectif pe WHERE pe.id=e.id AND pe.cloturee=0 AND pe.date<=date('now')) ORDER BY e.date DESC,e.id DESC LIMIT 1")
            .bind(band_id).bind(sow_id).fetch_optional(&state.pool).await?.flatten();
        let born: Option<i64> = sqlx::query_scalar("SELECT (SELECT presents FROM portee_effectif pe WHERE pe.id=e.id) FROM evenement e WHERE e.bande_id=? AND e.truie_id=? AND e.type='mise_bas' AND EXISTS(SELECT 1 FROM portee_effectif pe WHERE pe.id=e.id AND pe.cloturee=0 AND pe.date<=date('now')) ORDER BY e.date DESC,e.id DESC LIMIT 1")
            .bind(band_id).bind(sow_id).fetch_optional(&state.pool).await?.flatten();
        let present = born.ok_or_else(|| {
            AppError::Invalid(
                "Cette truie est déjà sevrée ou n’a pas de mise-bas dans la bande".into(),
            )
        })?;
        if number != present {
            return Err(AppError::Invalid(format!(
                "Sevrez les {present} porcelet(s) présents, ou enregistrez d’abord les adoptions et les décès"
            )));
        }
        let destination_room: Option<i64> = sqlx::query_scalar("SELECT c.salle_id FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE c.id=? AND (lower(COALESCE(s.type,'')) LIKE '%sevr%' OR lower(s.nom) LIKE '%sevr%')").bind(destination).fetch_optional(&state.pool).await?;
        let room_id = destination_room.ok_or_else(|| {
            AppError::Invalid("La destination doit être une case de post-sevrage".into())
        })?;
        *assigned.entry(destination).or_default() += number;
        selections.push((sow_id, number, destination, source, room_id));
    }
    if selections.is_empty() {
        return Err(AppError::Invalid(
            "Cochez au moins une truie à sevrer".into(),
        ));
    }
    for (case_id, number) in &assigned {
        let (capacity,): (Option<i64>,) =
            sqlx::query_as("SELECT nb_max_porcs FROM casesalle WHERE id=?")
                .bind(case_id)
                .fetch_one(&state.pool)
                .await?;
        let present = case_pig_count(&state.pool, *case_id).await?;
        if capacity.is_some_and(|max| present + number > max) {
            return Err(AppError::Invalid(format!("Capacité dépassée dans la case {case_id} : {present} présent(s), {number} ajouté(s), maximum {}", capacity.unwrap_or_default())));
        }
    }
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    for (sow_id, number, destination, source, room_id) in selections {
        let present: Option<i64> = sqlx::query_scalar("SELECT presents FROM portee_effectif WHERE truie_id=? AND bande_id=? ORDER BY date DESC,id DESC LIMIT 1")
            .bind(sow_id).bind(band_id).fetch_optional(&mut *tx).await?;
        if present != Some(number) {
            return Err(AppError::Invalid(
                "L’effectif a changé : rechargez la maternité avant de sevrer".into(),
            ));
        }
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,note) VALUES('sevrage',?,?,?,?,?)")
            .bind(&date).bind(sow_id).bind(band_id).bind(number).bind("Sevrage depuis le tableau maternité").execute(&mut *tx).await?;
        let source_room: Option<i64> = if let Some(source_id) = source {
            sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
                .bind(source_id)
                .fetch_optional(&mut *tx)
                .await?
        } else {
            None
        };
        sqlx::query("INSERT INTO transfert(date,espece,bande_id,salle_source_id,salle_dest_id,case_source_id,case_dest_id,nombre,truie_id,note) VALUES(?,'porc',?,?,?,?,?,?,?,?)")
            .bind(&date).bind(band_id).bind(source_room).bind(room_id).bind(source).bind(destination).bind(number).bind(sow_id).bind("Sevrage : mouvement de la portée vers le post-sevrage").execute(&mut *tx).await?;
        sqlx::query("UPDATE truie SET bande_code=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=? AND bande_code=?").bind(sow_id).bind(&band_code).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE bande SET cs_total_sevres=(SELECT CAST(COALESCE(SUM(nb_sevres),0) AS INTEGER) FROM evenement WHERE bande_id=? AND type='sevrage'),updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(band_id).bind(band_id).execute(&mut *tx).await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "sevrer",
        "bande",
        &band_code,
        "/maternite",
    )
    .await;
    Ok(Redirect::to(&format!("/maternite?bande_id={band_id}&vue=sevrage")).into_response())
}

async fn maternite_perte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((band_id, sow_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    maternite_suivi::enregistrer_perte(&state.pool, sow_id, Some(band_id), &form, Some(28)).await?;
    Ok(Redirect::to(&format!("/maternite?bande_id={band_id}#truie-{sow_id}")).into_response())
}

async fn maternite_perte_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((band_id, loss_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let sow_id: Option<i64> = sqlx::query_scalar(
        "SELECT truie_id FROM perteporcelet WHERE id=? AND bande_id=? AND evenement_id IS NULL",
    )
    .bind(loss_id)
    .bind(band_id)
    .fetch_optional(&state.pool)
    .await?
    .flatten();
    let Some(sow_id) = sow_id else {
        return Err(AppError::NotFound);
    };
    sqlx::query("DELETE FROM perteporcelet WHERE id=? AND bande_id=? AND evenement_id IS NULL")
        .bind(loss_id)
        .bind(band_id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/maternite?bande_id={band_id}#truie-{sow_id}")).into_response())
}

async fn perte_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let sow: Option<i64> = sqlx::query_scalar("SELECT truie_id FROM perteporcelet WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .flatten();
    sqlx::query("DELETE FROM perteporcelet WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(
        &sow.map(|value| format!("/truie/{value}#pertes"))
            .unwrap_or_else(|| "/truies".into()),
    )
    .into_response())
}

async fn truie_cochette(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE truie SET mere_cochette=CASE mere_cochette WHEN 1 THEN 0 ELSE 1 END,updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(
        &if form.get("retour").map(String::as_str) == Some("cochettes") {
            "/cochettes".into()
        } else {
            format!("/truie/{id}")
        },
    )
    .into_response())
}

const EVENT_SELECT_BY_SOW: &str = "SELECT id,type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,nb_sevres,poids_moyen,adoptes,retires,produit,motif,resultat,nb_doses,creneaux_ia,case_id,suivi_actif,delivrance_ok,note FROM evenement WHERE truie_id=? ORDER BY date DESC,id DESC";
const EVENT_SELECT_BY_BAND: &str = "SELECT id,type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,nb_sevres,poids_moyen,adoptes,retires,produit,motif,resultat,nb_doses,creneaux_ia,case_id,suivi_actif,delivrance_ok,note FROM evenement WHERE bande_id=? ORDER BY date DESC,id DESC";

async fn evenement_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let kind =
        form_text(&form, "type").ok_or_else(|| AppError::Invalid("Type obligatoire".into()))?;
    let date =
        form_date(&form, "date")?.ok_or_else(|| AppError::Invalid("Date obligatoire".into()))?;
    let sow_id = form_i64(&form, "truie_id");
    let mut band_id = form_i64(&form, "bande_id");
    if band_id.is_none() {
        if let Some(sow_id) = sow_id {
            band_id = sqlx::query_scalar("SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=? ORDER BY b.active DESC,b.id DESC LIMIT 1")
                .bind(sow_id).fetch_optional(&state.pool).await?;
        }
    }
    let note = if kind == "chaleur" {
        let mut observations = Vec::new();
        if form.contains_key("chaleur_vulve") {
            observations.push("aspect de la vulve");
        }
        if form.contains_key("chaleur_comportement") {
            observations.push("comportement");
        }
        if form.contains_key("chaleur_immobilite") {
            observations.push("réflexe d’immobilité");
        }
        let libre = form_text(&form, "note");
        match (observations.is_empty(), libre) {
            (false, Some(libre)) => Some(format!("{} — {libre}", observations.join(", "))),
            (false, None) => Some(observations.join(", ")),
            (true, libre) => libre,
        }
    } else {
        form_text(&form, "note")
    };
    let slots = ia_slots(&form);
    if kind == "ia" && slots.is_empty() {
        return Err(AppError::Invalid(
            "Cochez matin, midi ou soir pour l'insémination".into(),
        ));
    }
    let case_id: Option<i64> = if let Some(sow_id) = sow_id {
        sqlx::query_scalar("SELECT case_id FROM truie WHERE id=?")
            .bind(sow_id)
            .fetch_optional(&state.pool)
            .await?
            .flatten()
    } else {
        None
    };
    let result = sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,chetifs,ecrases,tues_truie,nb_sevres,poids_moyen,adoptes,retires,produit,motif,delai_attente,resultat,nb_doses,creneaux_ia,case_id,heure_debut,heure_fin,note,suivi_actif,delivrance_ok) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&kind)
        .bind(&date)
        .bind(sow_id)
        .bind(band_id)
        .bind(form_i64(&form,"nes_totaux"))
        .bind(form_i64(&form,"nes_vifs"))
        .bind(form_i64(&form,"mort_nes"))
        .bind(form_i64(&form,"momifies"))
        .bind(form_i64(&form,"chetifs"))
        .bind(form_i64(&form,"ecrases"))
        .bind(form_i64(&form,"tues_truie"))
        .bind(form_i64(&form,"nb_sevres"))
        .bind(form_f64(&form,"poids_moyen"))
        .bind(form_i64(&form,"adoptes"))
        .bind(form_i64(&form,"retires"))
        .bind(form_text(&form,"produit"))
        .bind(form_text(&form,"motif"))
        .bind(form_i64(&form,"delai_attente"))
        .bind(form_text(&form,"resultat"))
        .bind((kind=="ia").then_some(slots.len() as i64).or_else(||form_i64(&form,"nb_doses")))
        .bind((!slots.is_empty()).then(||slots.join(",")))
        .bind(case_id)
        .bind(form_text(&form,"heure_debut"))
        .bind(form_text(&form,"heure_fin"))
        .bind(note)
        .bind(form.contains_key("suivi_actif") as i64)
        .bind(form_i64(&form,"delivrance_ok"))
        .execute(&state.pool).await?;
    if kind == "mise_bas" {
        if let Some(sow_id) = sow_id {
            let completed =
                parse_stored_date(&date).is_some_and(|value| value <= Local::now().date_naive());
            sqlx::query("UPDATE truie SET bande_code=COALESCE((SELECT code FROM bande WHERE id=?),bande_code),rang=rang+?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(band_id).bind(if completed { 1 } else { 0 }).bind(sow_id).execute(&state.pool).await?;
            synchroniser_pertes_mise_bas(
                &state.pool,
                result.last_insert_rowid(),
                sow_id,
                band_id,
                &date,
                (
                    form_i64(&form, "chetifs").unwrap_or(0).max(0),
                    form_i64(&form, "ecrases").unwrap_or(0).max(0),
                    form_i64(&form, "tues_truie").unwrap_or(0).max(0),
                ),
            )
            .await?;
            synchroniser_soins_portee(
                &state.pool,
                result.last_insert_rowid(),
                sow_id,
                band_id,
                &date,
            )
            .await?;
        }
    }
    let target = sow_id
        .map(|id| format!("/truie/{id}"))
        .unwrap_or_else(|| "/".into());
    Ok(Redirect::to(&target).into_response())
}

async fn evenement_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let event: Option<(Option<i64>, String, String)> =
        sqlx::query_as("SELECT truie_id,type,date FROM evenement WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM perteporcelet WHERE evenement_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM evenement WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some((Some(sow_id), kind, date)) = &event {
        if kind == "mise_bas"
            && parse_stored_date(date).is_some_and(|value| value <= Local::now().date_naive())
        {
            sqlx::query(
                "UPDATE truie SET rang=MAX(rang-1,0),updated_at=CURRENT_TIMESTAMP WHERE id=?",
            )
            .bind(sow_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    let sow = event.and_then(|value| value.0);
    Ok(Redirect::to(
        &sow.map(|x| format!("/truie/{x}"))
            .unwrap_or_else(|| "/".into()),
    )
    .into_response())
}

async fn evenement_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let event: (Option<i64>, String) =
        sqlx::query_as("SELECT truie_id,type FROM evenement WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let slots = ia_slots(&form);
    if event.1 == "ia" && slots.is_empty() {
        return Err(AppError::Invalid(
            "Cochez au moins un créneau d'insémination".into(),
        ));
    }
    sqlx::query("UPDATE evenement SET date=?,resultat=?,produit=?,motif=?,delai_attente=CASE WHEN ? THEN ? ELSE delai_attente END,note=?,nb_doses=CASE WHEN type='ia' THEN ? ELSE nb_doses END,creneaux_ia=CASE WHEN type='ia' THEN ? ELSE creneaux_ia END WHERE id=?")
        .bind(form_date_or_today(&form,"date")?).bind(form_text(&form,"resultat"))
        .bind(form_text(&form,"produit")).bind(form_text(&form,"motif"))
        .bind(form.contains_key("delai_attente")).bind(form_i64(&form,"delai_attente")).bind(form_text(&form,"note"))
        .bind(slots.len() as i64).bind((!slots.is_empty()).then(||slots.join(","))).bind(id)
        .execute(&state.pool).await?;
    Ok(Redirect::to(
        &event
            .0
            .map(|sow_id| format!("/truie/{sow_id}#historique"))
            .unwrap_or_else(|| "/".into()),
    )
    .into_response())
}

async fn inseminations(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let candidates = generic_rows(
        &state.pool,
        "SELECT t.id,t.num_travail,t.bande_code,(SELECT MAX(e.date) FROM evenement e WHERE e.truie_id=t.id AND e.type='chaleur') AS date_chaleur,date((SELECT MAX(e.date) FROM evenement e WHERE e.truie_id=t.id AND e.type='chaleur'),'+1 day') AS date_conseillee,(SELECT e.note FROM evenement e WHERE e.truie_id=t.id AND e.type='chaleur' ORDER BY e.date DESC,e.id DESC LIMIT 1) AS observation,CASE WHEN EXISTS(SELECT 1 FROM evenement ch WHERE ch.truie_id=t.id AND ch.type='chaleur' AND NOT EXISTS(SELECT 1 FROM evenement ia WHERE ia.truie_id=t.id AND ia.type='ia' AND ia.date>=ch.date)) THEN 1 ELSE 0 END AS a_inseminer,CASE WHEN lower(COALESCE((SELECT e.resultat FROM evenement e WHERE e.truie_id=t.id AND e.type IN ('echo','echographie') ORDER BY e.date DESC,e.id DESC LIMIT 1),'')) IN ('vide','négative','negative','negatif','négatif') THEN 'Truie vide' WHEN t.bande_code IS NULL OR trim(t.bande_code)='' THEN CASE WHEN t.rang=0 THEN 'Cochette à préparer' ELSE 'Prochaine IA' END ELSE 'Chaleur détectée' END AS categorie FROM truie t WHERE t.reformee=0 AND (EXISTS(SELECT 1 FROM evenement ch WHERE ch.truie_id=t.id AND ch.type='chaleur' AND NOT EXISTS(SELECT 1 FROM evenement ia WHERE ia.truie_id=t.id AND ia.type='ia' AND ia.date>=ch.date)) OR lower(COALESCE((SELECT e.resultat FROM evenement e WHERE e.truie_id=t.id AND e.type IN ('echo','echographie') ORDER BY e.date DESC,e.id DESC LIMIT 1),'')) IN ('vide','négative','negative','negatif','négatif') OR t.bande_code IS NULL OR trim(t.bande_code)='') ORDER BY a_inseminer DESC,categorie,t.num_travail",
    )
    .await?;
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let mut ctx = context(&session);
    ctx.insert("candidates".into(), Value::Array(candidates));
    ctx.insert(
        "bandes".into(),
        serde_json::to_value(bands).unwrap_or_default(),
    );
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    ctx.insert("semences".into(),Value::Array(generic_rows(&state.pool,"SELECT id,date,designation,num_facture,fournisseur,nb_doses FROM achatsemence WHERE trim(COALESCE(designation,''))<>'' ORDER BY date DESC,id DESC").await?));
    render(&state, "inseminations.html", Value::Object(ctx))
}

async fn inseminations_enregistrer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let ids = form_selected_ids(&form, "truie_");
    if ids.is_empty() {
        return Err(AppError::Invalid("Sélectionne au moins une truie".into()));
    }
    let date = form_date_or_today(&form, "date")?;
    let slots = ia_slots(&form);
    if slots.is_empty() {
        return Err(AppError::Invalid(
            "Cochez au moins un créneau d'insémination : matin, midi ou soir".into(),
        ));
    }
    let slot_text = slots.join(",");
    let mut choices = HashMap::new();
    for id in &ids {
        let band: Option<i64> = sqlx::query_scalar(
            "SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=?",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
        choices.insert(
            *id,
            ameliorations::semence_produit(&state.pool, &form, &date, band).await?,
        );
    }
    let mut tx = state.pool.begin().await?;
    for id in ids {
        let band_id: Option<i64> = sqlx::query_scalar(
            "SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=? ORDER BY b.active DESC,b.id DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,produit,nb_doses,creneaux_ia,note,suivi_actif) VALUES('ia',?,?,?,?,?,?,?,0)")
            .bind(&date)
            .bind(id)
            .bind(band_id)
            .bind(choices.get(&id).cloned().flatten())
            .bind(slots.len() as i64)
            .bind(&slot_text)
            .bind(form_text(&form, "note"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/inseminations").into_response())
}

fn csv_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => {
            if *value {
                "1".into()
            } else {
                "0".into()
            }
        }
        _ => String::new(),
    }
}

async fn export_mise_bas(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let band: (String, Option<String>) =
        sqlx::query_as("SELECT code,date_mb FROM bande WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let rows = generic_rows(&state.pool,&format!("SELECT t.num_travail,t.rang,t.race,e.nes_totaux,e.nes_vifs,e.mort_nes,e.momifies,e.nb_sevres FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={} AND e.type='mise_bas' ORDER BY t.num_travail,e.date",id)).await?;
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b';')
        .from_writer(Vec::new());
    let band_header = format!("Bande : {}", band.0);
    let date_header = format!("MB théorique : {}", band.1.unwrap_or_default());
    writer
        .write_record([
            "Liste des truies à la mise-bas",
            band_header.as_str(),
            date_header.as_str(),
        ])
        .map_err(|error| AppError::Internal(error.into()))?;
    writer
        .write_record([
            "N° travail",
            "Rang",
            "Race",
            "NT",
            "NV",
            "Mort-nés",
            "Momifiés",
            "Sevrés",
        ])
        .map_err(|error| AppError::Internal(error.into()))?;
    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("ligne CSV inattendue")))?;
        writer
            .write_record([
                csv_value(object.get("num_travail")),
                csv_value(object.get("rang")),
                csv_value(object.get("race")),
                csv_value(object.get("nes_totaux")),
                csv_value(object.get("nes_vifs")),
                csv_value(object.get("mort_nes")),
                csv_value(object.get("momifies")),
                csv_value(object.get("nb_sevres")),
            ])
            .map_err(|error| AppError::Internal(error.into()))?;
    }
    writer
        .flush()
        .map_err(|error| AppError::Internal(error.into()))?;
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(
        writer
            .into_inner()
            .map_err(|error| AppError::Internal(error.into_error().into()))?,
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=liste_mise_bas_{}.csv",
            band.0.replace(' ', "_")
        ))
        .map_err(|error| AppError::Internal(error.into()))?,
    );
    Ok((headers, bytes).into_response())
}

/// Fiche de mise-bas au format A4 définitif (§9 de la spécification) :
/// export CSV pour l'usage tableur, cette page pour l'impression/archivage
/// papier du registre d'élevage — même requête de détail que le CSV, mais
/// mise en page dédiée avec en-tête d'élevage et totaux de bande.
async fn fiche_mise_bas(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let band: (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT code,date_mb,site FROM bande WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let lignes = generic_rows(
        &state.pool,
        &format!(
            "SELECT t.num_travail,t.rang,t.race,e.nes_totaux,e.nes_vifs,e.mort_nes,e.momifies,e.nb_sevres FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={id} AND e.type='mise_bas' ORDER BY t.num_travail,e.date"
        ),
    )
    .await?;
    let totaux = sqlx::query_as::<_, (i64, f64, f64, f64, f64, f64)>(&format!(
        "SELECT COUNT(*),CAST(COALESCE(SUM(e.nes_totaux),0) AS REAL),CAST(COALESCE(SUM(e.nes_vifs),0) AS REAL),CAST(COALESCE(SUM(e.mort_nes),0) AS REAL),CAST(COALESCE(SUM(e.momifies),0) AS REAL),CAST(COALESCE(SUM(e.nb_sevres),0) AS REAL) FROM evenement e WHERE e.bande_id={id} AND e.type='mise_bas'"
    ))
    .fetch_one(&state.pool)
    .await?;
    let nom_elevage: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='nom_elevage'")
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    let mut ctx = context(&session);
    ctx.insert("nom_elevage".into(), json!(nom_elevage));
    ctx.insert(
        "bande".into(),
        json!({"id": id, "code": band.0, "date_mb": band.1, "site": band.2}),
    );
    ctx.insert("lignes".into(), Value::Array(lignes));
    ctx.insert(
        "totaux".into(),
        json!({
            "truies": totaux.0, "nes_totaux": totaux.1, "nes_vifs": totaux.2,
            "mort_nes": totaux.3, "momifies": totaux.4, "nb_sevres": totaux.5,
        }),
    );
    ctx.insert("today".into(), json!(today_iso()));
    render(&state, "fiche_mise_bas.html", Value::Object(ctx))
}

async fn truies_modele_csv() -> Response {
    let body="\u{feff}num_travail;num_national;rfid;race;date_entree;date_naissance;bande_code;note\r\nT001;FR000000001;250000000001;Large White;2026-01-01;2025-01-01;B1.26;Exemple à supprimer\r\n";
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=modele_import_truies.csv"),
    );
    (headers, body).into_response()
}

async fn truies_import(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let mut data = None;
    let mut filename = "import-truies.csv".to_string();
    let mut csrf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        let field_name = field.name().map(str::to_string);
        match field_name.as_deref() {
            Some("csrf_token") => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            Some("fichier") => {
                filename = field
                    .file_name()
                    .unwrap_or("import-truies.csv")
                    .chars()
                    .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
                    .take(180)
                    .collect();
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            _ => {}
        }
    }
    if csrf.as_deref() != Some(session.csrf.as_str()) {
        return Err(AppError::Forbidden);
    }
    let bytes = data.ok_or_else(|| AppError::Invalid("Fichier CSV manquant".into()))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(AppError::Invalid("Fichier trop volumineux".into()));
    }
    let digest = contenu_sha256(&bytes);
    let delimiter = if bytes
        .iter()
        .take(1024)
        .filter(|&&byte| byte == b';')
        .count()
        >= bytes
            .iter()
            .take(1024)
            .filter(|&&byte| byte == b',')
            .count()
    {
        b';'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(bytes.as_ref());
    let headers = reader
        .headers()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .clone();
    let normalized: Vec<String> = headers
        .iter()
        .map(|value| value.trim().trim_start_matches('\u{feff}').to_lowercase())
        .collect();
    if !normalized.iter().any(|value| value == "num_travail") {
        let is_historique = String::from_utf8_lossy(&bytes)
            .lines()
            .take(10)
            .any(|line| {
                let normalized_line = line.trim().trim_start_matches('\u{feff}').to_lowercase();
                normalized_line.contains("n° travail") && normalized_line.contains("date 1ère ia")
            });
        if is_historique {
            return Err(AppError::Invalid(
                "Ce fichier est un historique complet de truies. Utilisez l’import « Historique truie (export riche) » sur cette page.".into(),
            ));
        }
        return Err(AppError::Invalid("Colonne num_travail manquante".into()));
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut seen = std::collections::HashSet::new();
    let mut preview_rows = Vec::new();
    let mut additions = 0_i64;
    let ignored = 0_i64;
    let mut errors = 0_i64;
    let mut tx = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut tx, &digest).await?;
    sqlx::query(
        "UPDATE importjournal SET statut='expire' WHERE statut='apercu' AND cree_le<datetime('now','-1 day')",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,'truies',?,'apercu',?,?)")
        .bind(&token)
        .bind(&filename)
        .bind(session.uid)
        .bind(&digest)
        .execute(&mut *tx)
        .await?;

    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| AppError::Invalid(error.to_string()))?;
        let row: HashMap<&str, &str> = normalized
            .iter()
            .map(String::as_str)
            .zip(record.iter())
            .collect();
        let field = |key: &str| {
            row.get(key)
                .copied()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let number = field("num_travail").unwrap_or("");
        let date_entree_raw = field("date_entree");
        let date_naissance_raw = field("date_naissance");
        let date_entree = date_entree_raw.and_then(parse_iso_date);
        let date_naissance = date_naissance_raw.and_then(parse_iso_date);
        let invalid_date = date_entree_raw.is_some() && date_entree.is_none()
            || date_naissance_raw.is_some() && date_naissance.is_none();
        let mut action = "ajouter";
        let mut anomaly = None;
        if number.is_empty() {
            action = "erreur";
            anomaly = Some("Numéro de travail manquant".to_string());
            errors += 1;
        } else if invalid_date {
            action = "erreur";
            anomaly = Some("Date invalide : format attendu AAAA-MM-JJ".to_string());
            errors += 1;
        } else if !seen.insert(number.to_lowercase()) {
            action = "erreur";
            anomaly = Some("Doublon dans le fichier".to_string());
            errors += 1;
        } else {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM truie WHERE lower(trim(num_travail))=lower(trim(?)) AND reformee=0",
            )
            .bind(number)
            .fetch_one(&mut *tx)
            .await?;
            if exists > 0 {
                action = "erreur";
                anomaly = Some("Truie active déjà présente".to_string());
                errors += 1;
            } else {
                additions += 1;
            }
        }
        let payload = json!({
            "num_travail": number,
            "num_national": field("num_national"),
            "rfid": field("rfid"),
            "race": field("race"),
            "date_entree": date_entree,
            "date_naissance": date_naissance,
            "bande_code": field("bande_code"),
            "note": field("note"),
        });
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
            .bind(&token)
            .bind(index as i64 + 2)
            .bind(action)
            .bind(&anomaly)
            .bind(payload.to_string())
            .execute(&mut *tx)
            .await?;
        preview_rows.push(json!({
            "ligne": index + 2,
            "action": action,
            "anomalie": anomaly,
            "num_travail": number,
            "num_national": field("num_national"),
            "rfid": field("rfid"),
            "race": field("race"),
            "bande_code": field("bande_code"),
        }));
    }
    let summary = json!({"ajouter": additions, "ignorer": ignored, "erreur": errors});
    sqlx::query("UPDATE importjournal SET resume=? WHERE token=?")
        .bind(summary.to_string())
        .bind(&token)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let mut ctx = context(&session);
    ctx.insert("token".into(), json!(token));
    ctx.insert("nom_fichier".into(), json!(filename));
    ctx.insert("resume".into(), summary);
    ctx.insert("lignes".into(), Value::Array(preview_rows));
    Ok(render(&state, "import_apercu.html", Value::Object(ctx))?.into_response())
}

async fn truies_import_confirmer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    let mut tx = state.pool.begin().await?;
    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import='truies'",
    )
    .bind(&token)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT numero_ligne,donnees_json FROM importligne WHERE token=? AND action='ajouter' ORDER BY numero_ligne",
    )
    .bind(&token)
    .fetch_all(&mut *tx)
    .await?;
    let mut added = 0_i64;
    for (line, raw) in rows {
        let data: Value = serde_json::from_str(&raw)
            .map_err(|_| AppError::Invalid(format!("Données invalides à la ligne {line}")))?;
        let value = |key: &str| {
            data.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        };
        let number = value("num_travail")
            .ok_or_else(|| AppError::Invalid(format!("Numéro absent à la ligne {line}")))?;
        let checked_date = |key: &str| -> AppResult<Option<String>> {
            value(key)
                .map(|raw| {
                    parse_iso_date(raw).ok_or_else(|| {
                        AppError::Invalid(format!(
                            "Date {key} invalide à la ligne {line} : format AAAA-MM-JJ attendu"
                        ))
                    })
                })
                .transpose()
        };
        let date_entree = checked_date("date_entree")?;
        let date_naissance = checked_date("date_naissance")?;
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM truie WHERE lower(trim(num_travail))=lower(trim(?)) AND reformee=0",
        )
        .bind(number)
        .fetch_one(&mut *tx)
        .await?;
        if exists > 0 {
            return Err(AppError::Invalid(format!(
                "La truie {number} a été ajoutée depuis l'aperçu ; import entièrement annulé"
            )));
        }
        sqlx::query("INSERT INTO truie(num_travail,num_national,rfid,race,date_entree,date_naissance,bande_code,note,statut,rang,reformee,mere_cochette,source_import_id) VALUES(?,?,?,?,?,?,?,?,'active',0,0,0,?)")
            .bind(number)
            .bind(value("num_national"))
            .bind(value("rfid"))
            .bind(value("race"))
            .bind(date_entree)
            .bind(date_naissance)
            .bind(value("bande_code"))
            .bind(value("note"))
            .bind(&token)
            .execute(&mut *tx)
            .await?;
        added += 1;
    }
    sqlx::query(
        "UPDATE importjournal SET statut='applique',applique_le=CURRENT_TIMESTAMP WHERE token=?",
    )
    .bind(&token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "import",
        "truies",
        &format!("{added} truie(s), import {token}"),
        "/truies/import/confirmer",
    )
    .await;
    Ok(Redirect::to(&format!("/truies?import_ok={added}")).into_response())
}

async fn truies_import_annuler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    sqlx::query(
        "DELETE FROM importjournal WHERE token=? AND statut='apercu' AND (cree_par=? OR ?='admin')",
    )
    .bind(token)
    .bind(session.uid)
    .bind(&session.role)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/truies").into_response())
}

async fn truie_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let sow=generic_rows(&state.pool,&format!("SELECT num_travail,num_national,rfid,race,date_naissance,rang,bande_code,statut,note FROM truie WHERE id={id}")).await?.into_iter().next().ok_or(AppError::NotFound)?;
    let lines=generic_rows(&state.pool,&format!("SELECT date,type,COALESCE(resultat,produit,note,'') AS detail,nes_totaux,nes_vifs,nb_sevres FROM evenement WHERE truie_id={id} ORDER BY date DESC,id DESC")).await?;
    let mut ctx = context(&session);
    ctx.insert(
        "title".into(),
        json!(format!(
            "Fiche truie {}",
            sow.get("num_travail").and_then(Value::as_str).unwrap_or("")
        )),
    );
    ctx.insert("infos".into(), sow);
    ctx.insert("lignes".into(), Value::Array(lines));
    render(&state, "impression.html", Value::Object(ctx))
}

async fn truies_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    verify_csrf(&session, &form)?;
    let ids = form_selected_ids(&form, "truie_");
    let rows = if ids.is_empty() {
        let code = form_text(&form, "bande_code").ok_or_else(|| {
            AppError::Invalid("Cochez des truies ou sélectionnez une bande".into())
        })?;
        let result = sqlx::query("SELECT num_travail,num_national,rfid,race,rang,bande_code,statut FROM truie WHERE reformee=0 AND bande_code=? ORDER BY num_travail")
            .bind(&code).fetch_all(&state.pool).await?;
        rows_to_json(result)?
    } else {
        let safe_ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        generic_rows(&state.pool,&format!("SELECT num_travail,num_national,rfid,race,rang,bande_code,statut FROM truie WHERE id IN ({safe_ids}) ORDER BY bande_code,num_travail")).await?
    };
    let mut ctx = context(&session);
    ctx.insert("title".into(), json!("Liste des truies"));
    ctx.insert(
        "infos".into(),
        json!({"nombre":rows.len(),"selection":if ids.is_empty(){"bande"}else{"truies cochées"}}),
    );
    ctx.insert("lignes".into(), Value::Array(rows));
    render(&state, "impression.html", Value::Object(ctx))
}

async fn soin_portee_realiser(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let sow_id: i64 = sqlx::query_scalar("SELECT truie_id FROM soinportee WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    sqlx::query("UPDATE soinportee SET date_realisee=?,note=? WHERE id=?")
        .bind(form_date_or_today(&form, "date_realisee")?)
        .bind(form_text(&form, "note"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{sow_id}#soins-portee")).into_response())
}

async fn bande_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let band = generic_rows(
        &state.pool,
        &format!("SELECT code,num_officiel,date_mb,site,note,active FROM bande WHERE id={id}"),
    )
    .await?
    .into_iter()
    .next()
    .ok_or(AppError::NotFound)?;
    let lines=generic_rows(&state.pool,&format!("SELECT 'Truie' AS type,t.num_travail AS reference,'Rang '||t.rang AS detail,NULL AS date FROM truie t JOIN bande b ON b.code=t.bande_code WHERE b.id={id} AND t.reformee=0 UNION ALL SELECT e.type,COALESCE(t.num_travail,''),COALESCE(e.note,e.produit,e.resultat,''),e.date FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={id} ORDER BY date DESC,reference")).await?;
    let mut ctx = context(&session);
    ctx.insert(
        "title".into(),
        json!(format!(
            "Fiche bande {}",
            band.get("code").and_then(Value::as_str).unwrap_or("")
        )),
    );
    ctx.insert("infos".into(), band);
    ctx.insert("lignes".into(), Value::Array(lines));
    render(&state, "impression.html", Value::Object(ctx))
}

fn ics_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace(['\r', '\n'], " ")
}

async fn calendrier_ics(State(state): State<AppState>) -> AppResult<Response> {
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let stages = load_band_schedule(&state.pool).await?.stages();
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//EO-Suivi Elevage Rust//FR".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
    ];
    for band in bands {
        let Some(base) = band.date_mb.as_deref().and_then(parse_stored_date) else {
            continue;
        };
        for (name, days) in stages {
            let day = base + Duration::days(days);
            lines.push("BEGIN:VEVENT".into());
            lines.push(format!(
                "UID:bande-{}-{}@eo-suivi-rust",
                band.id,
                days + 400
            ));
            lines.push(format!("DTSTAMP:{}T000000Z", Local::now().format("%Y%m%d")));
            lines.push(format!("DTSTART;VALUE=DATE:{}", day.format("%Y%m%d")));
            lines.push(format!(
                "SUMMARY:{}",
                ics_escape(&format!("{} — {}", band.code, name))
            ));
            lines.push("END:VEVENT".into());
        }
    }
    lines.push("END:VCALENDAR".into());
    let body = format!("{}\r\n", lines.join("\r\n"));
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=elevage.ics"),
    );
    Ok((headers, body).into_response())
}

async fn imports_page(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let economic_imports = if matches!(session.role.as_str(), "admin" | "eleveur") {
        generic_rows(&state.pool,"SELECT token,replace(type_import,'economique:','') AS type_import,nom_fichier,statut,cree_le,applique_le,resume FROM importjournal WHERE type_import LIKE 'economique:%' ORDER BY cree_le DESC LIMIT 100").await?
    } else {
        Vec::new()
    };
    let mut ctx = context(&session);
    ctx.insert("imports_economiques".into(), Value::Array(economic_imports));
    render(&state, "imports.html", Value::Object(ctx))
}

async fn api_bandes_actives(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site FROM bande WHERE active=1 ORDER BY date_mb DESC,id DESC",
    )
    .await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_truies(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT t.id,t.num_travail,t.bande_code,t.rfid,c.num_vanne,b.date_mb AS mise_bas_prevue,CASE WHEN b.date_mb IS NOT NULL AND date(b.date_mb) BETWEEN date('now','-10 day') AND date('now','+10 day') AND NOT EXISTS(SELECT 1 FROM evenement e WHERE e.truie_id=t.id AND e.type='mise_bas' AND e.bande_id=b.id) THEN 1 ELSE 0 END AS dans_periode_mise_bas FROM truie t LEFT JOIN bande b ON b.code=t.bande_code AND b.active=1 LEFT JOIN casesalle c ON c.id=t.case_id WHERE t.reformee=0 ORDER BY t.num_travail",
    )
    .await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_bandes(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site,active FROM bande ORDER BY active DESC,date_mb DESC,id DESC",
    )
    .await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_cases(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT c.id,c.salle_id,c.nom,s.nom AS salle,si.code AS site,c.nb_max_porcs,c.num_vanne FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.code,s.ordre,c.nom",
    )
    .await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn recherche(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let q = query.get("q").map(|value| value.trim()).unwrap_or("");
    if q.is_empty() {
        return render_list_page(
            &state,
            &session,
            "Recherche",
            "Saisis un numéro de truie, une RFID, une bande, un apport ou un client.",
            vec![],
            &["type", "reference", "detail"],
        );
    }
    let pattern = format!("%{q}%");
    let rows = sqlx::query(
        "SELECT 'Truie' AS type,CAST(t.id AS TEXT) AS reference,COALESCE(t.num_travail,'')||CASE WHEN t.bande_code IS NOT NULL THEN ' · bande '||t.bande_code ELSE '' END||CASE WHEN c.num_vanne IS NOT NULL THEN ' · vanne '||c.num_vanne ELSE '' END AS detail,'/truie/'||t.id AS lien FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id WHERE t.num_travail LIKE ? OR COALESCE(t.num_national,'') LIKE ? OR COALESCE(t.rfid,'') LIKE ? OR COALESCE(c.num_vanne,'') LIKE ? UNION ALL SELECT 'Bande',CAST(id AS TEXT),code||COALESCE(' · '||site,''),'/bande/'||id FROM bande WHERE code LIKE ? OR COALESCE(num_officiel,'') LIKE ? UNION ALL SELECT 'Apport',CAST(id AS TEXT),COALESCE(num_apport,'')||' · '||COALESCE(CAST(nb_porcs AS TEXT),'0')||' porcs','/economique' FROM venteapport WHERE COALESCE(num_apport,'') LIKE ? UNION ALL SELECT 'Commande',CAST(id AS TEXT),nom_client||' · '||COALESCE(telephone,''),'/vente-directe/commande/'||id FROM commandeventedirecte WHERE nom_client LIKE ? OR COALESCE(telephone,'') LIKE ? OR COALESCE(email,'') LIKE ? LIMIT 200",
    )
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(&state.pool)
    .await?;
    render_list_page(
        &state,
        &session,
        "Résultats de recherche",
        &format!("Résultats pour « {q} »"),
        rows_to_json(rows)?,
        &["type", "reference", "detail"],
    )
}

async fn gttt(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let months = query
        .get("mois")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| matches!(value, 6 | 12 | 24 | 36 | 60 | 120))
        .unwrap_or(24);
    let selected_band = query
        .get("bande")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let all_litters = load_gttt_litters(&state.pool, None).await?;
    let undated = all_litters
        .iter()
        .filter(|litter| litter.farrowing_date.is_none())
        .count();
    let period_start =
        Local::now().date_naive() - Duration::days((months as f64 * 30.44).round() as i64);
    let period_litters = all_litters
        .into_iter()
        .filter(|litter| {
            litter
                .farrowing_date
                .is_some_and(|date| date >= period_start)
        })
        .collect::<Vec<_>>();
    let mut band_codes = period_litters
        .iter()
        .filter_map(|litter| litter.band.clone())
        .collect::<Vec<_>>();
    band_codes.sort();
    band_codes.dedup();
    let litters = period_litters
        .iter()
        .filter(|litter| {
            selected_band
                .as_deref()
                .is_none_or(|band| litter.band.as_deref() == Some(band))
        })
        .cloned()
        .collect::<Vec<_>>();
    let summary = gttt_summary(&litters);
    let rank_rows = gttt_rank_rows(&litters)?;
    let mut band_rows = Vec::with_capacity(band_codes.len());
    for code in &band_codes {
        let rows = period_litters
            .iter()
            .filter(|litter| litter.band.as_deref() == Some(code.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut value = serde_json::to_value(gttt_summary(&rows)).unwrap_or_default();
        json_object_mut(&mut value, "la synthèse GTTT")?.insert("bande".into(), json!(code));
        band_rows.push(value)
    }
    let mut ctx = context(&session);
    ctx.insert(
        "synthese".into(),
        serde_json::to_value(summary).unwrap_or_default(),
    );
    ctx.insert("rangs".into(), Value::Array(rank_rows));
    ctx.insert("bandes".into(), Value::Array(band_rows));
    ctx.insert("codes_bandes".into(), json!(band_codes));
    ctx.insert("bande_selectionnee".into(), json!(selected_band));
    ctx.insert("mois".into(), json!(months));
    ctx.insert("portees_sans_date".into(), json!(undated));
    ctx.insert(
        "periode_debut".into(),
        json!(period_start.format("%Y-%m-%d").to_string()),
    );
    render(&state, "gttt.html", Value::Object(ctx))
}

#[derive(Clone, Debug)]
struct GtttLitter {
    sow_number: String,
    band: Option<String>,
    farrowing_date: Option<NaiveDate>,
    rank: i64,
    gestation: Option<f64>,
    live_born: Option<f64>,
    stillborn: Option<f64>,
    stillborn_rate: Option<f64>,
    weaned: Option<f64>,
    adopted: Option<f64>,
    removed: Option<f64>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
struct GtttSummary {
    portees: usize,
    nes_totaux_moy: Option<f64>,
    nes_vifs_moy: Option<f64>,
    sevres_moy: Option<f64>,
    taux_mortnes: Option<f64>,
    mortalite_allaitement: Option<f64>,
    total_nes_vifs: i64,
    total_sevres: i64,
    gestation_moy: Option<f64>,
    truies_productives: usize,
    periode_jours: Option<i64>,
    portees_truie_an: Option<f64>,
    sevres_truie_an: Option<f64>,
}

async fn load_gttt_litters(pool: &SqlitePool, band: Option<&str>) -> AppResult<Vec<GtttLitter>> {
    let sql = if band.is_some() {
        "SELECT p.num_travail,p.bande,(SELECT b.date_mb FROM bande b WHERE b.code=p.bande ORDER BY b.active DESC,b.id DESC LIMIT 1),p.rang,p.duree_gest,p.nv,p.mn,p.tx_mn_nt,p.sev,p.ad,p.re FROM porteerang p WHERE p.bande=? ORDER BY p.rang,p.id"
    } else {
        "SELECT p.num_travail,p.bande,(SELECT b.date_mb FROM bande b WHERE b.code=p.bande ORDER BY b.active DESC,b.id DESC LIMIT 1),p.rang,p.duree_gest,p.nv,p.mn,p.tx_mn_nt,p.sev,p.ad,p.re FROM porteerang p ORDER BY p.rang,p.id"
    };
    let mut query = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            Option<String>,
            i64,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
        ),
    >(sql);
    if let Some(band) = band {
        query = query.bind(band);
    }
    Ok(query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| GtttLitter {
            sow_number: row.0,
            band: row.1,
            farrowing_date: row.2.as_deref().and_then(parse_stored_date),
            rank: row.3,
            gestation: row.4,
            live_born: row.5,
            stillborn: row.6,
            stillborn_rate: row.7,
            weaned: row.8,
            adopted: row.9,
            removed: row.10,
        })
        .collect())
}

fn gttt_real_total(litter: &GtttLitter) -> f64 {
    let base = litter.live_born.unwrap_or(0.0) + litter.stillborn.unwrap_or(0.0);
    match (litter.stillborn, litter.stillborn_rate) {
        (Some(stillborn), Some(rate)) if stillborn > 0.0 && rate > 0.0 => {
            (stillborn / (rate / 100.0)).max(base)
        }
        _ => base,
    }
}

fn gttt_summary(litters: &[GtttLitter]) -> GtttSummary {
    let valid: Vec<&GtttLitter> = litters
        .iter()
        .filter(|litter| litter.live_born.is_some())
        .collect();
    let mean = |values: Vec<f64>| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let total_real = valid
        .iter()
        .map(|litter| gttt_real_total(litter))
        .sum::<f64>();
    let total_stillborn = valid
        .iter()
        .map(|litter| litter.stillborn.unwrap_or(0.0))
        .sum::<f64>();
    let with_weaning: Vec<&GtttLitter> = valid
        .iter()
        .copied()
        .filter(|litter| litter.weaned.is_some())
        .collect();
    let available = with_weaning
        .iter()
        .map(|litter| {
            litter.live_born.unwrap_or(0.0) + litter.adopted.unwrap_or(0.0)
                - litter.removed.unwrap_or(0.0)
        })
        .sum::<f64>();
    let losses = with_weaning
        .iter()
        .map(|litter| {
            litter.live_born.unwrap_or(0.0) + litter.adopted.unwrap_or(0.0)
                - litter.removed.unwrap_or(0.0)
                - litter.weaned.unwrap_or(0.0)
        })
        .sum::<f64>();
    let total_weaned = litters
        .iter()
        .map(|litter| litter.weaned.unwrap_or(0.0))
        .sum::<f64>();
    let productive_sows = valid
        .iter()
        .map(|litter| litter.sow_number.as_str())
        .filter(|number| !number.is_empty())
        .collect::<HashSet<_>>();
    let mut dates = valid
        .iter()
        .filter_map(|litter| litter.farrowing_date)
        .collect::<Vec<_>>();
    dates.sort_unstable();
    dates.dedup();
    let mut intervals = dates
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_days())
        .filter(|days| *days > 0)
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    let typical_interval = intervals.get(intervals.len() / 2).copied();
    let observation_days = match (dates.first(), dates.last(), typical_interval) {
        (Some(first), Some(last), Some(interval)) => Some((*last - *first).num_days() + interval),
        _ => None,
    };
    let annual_factor = observation_days
        .filter(|days| *days > 0)
        .map(|days| 365.0 / days as f64);
    let sow_count = productive_sows.len();
    GtttSummary {
        portees: valid.len(),
        nes_totaux_moy: mean(
            valid
                .iter()
                .map(|litter| gttt_real_total(litter))
                .collect::<Vec<_>>(),
        )
        .map(|value| (value * 100.0).round() / 100.0),
        nes_vifs_moy: mean(
            valid
                .iter()
                .filter_map(|litter| litter.live_born)
                .collect::<Vec<_>>(),
        )
        .map(|value| (value * 100.0).round() / 100.0),
        sevres_moy: mean(
            litters
                .iter()
                .filter_map(|litter| litter.weaned)
                .collect::<Vec<_>>(),
        )
        .map(|value| (value * 100.0).round() / 100.0),
        taux_mortnes: (total_real > 0.0)
            .then(|| (total_stillborn / total_real * 1000.0).round() / 10.0),
        mortalite_allaitement: (available > 0.0)
            .then(|| ((losses.max(0.0) / available) * 1000.0).round() / 10.0),
        total_nes_vifs: valid
            .iter()
            .map(|litter| litter.live_born.unwrap_or(0.0))
            .sum::<f64>()
            .round() as i64,
        total_sevres: total_weaned.round() as i64,
        gestation_moy: mean(
            litters
                .iter()
                .filter_map(|litter| litter.gestation)
                .collect::<Vec<_>>(),
        )
        .map(|value| (value * 10.0).round() / 10.0),
        truies_productives: sow_count,
        periode_jours: observation_days,
        portees_truie_an: if sow_count > 0 {
            annual_factor
                .map(|factor| valid.len() as f64 / sow_count as f64 * factor)
                .map(|value| (value * 100.0).round() / 100.0)
        } else {
            None
        },
        sevres_truie_an: if sow_count > 0 {
            annual_factor
                .map(|factor| total_weaned / sow_count as f64 * factor)
                .map(|value| (value * 10.0).round() / 10.0)
        } else {
            None
        },
    }
}

fn gttt_band_fallback(band: &Bande, events: &[Evenement]) -> GtttSummary {
    if band.cs_nv_portee.is_some() || band.cs_sevres_portee.is_some() || band.cs_truies_mb.is_some()
    {
        let litters = band.cs_truies_mb.unwrap_or_default().max(0) as usize;
        let total_weaned = band.cs_total_sevres.unwrap_or_else(|| {
            (band.cs_sevres_portee.unwrap_or(0.0) * litters as f64).round() as i64
        });
        return GtttSummary {
            portees: litters,
            nes_totaux_moy: band.cs_nt_portee,
            nes_vifs_moy: band.cs_nv_portee,
            sevres_moy: band.cs_sevres_portee,
            taux_mortnes: match (band.cs_mn_portee, band.cs_nt_portee) {
                (Some(stillborn), Some(total)) if total > 0.0 => {
                    Some((stillborn / total * 1000.0).round() / 10.0)
                }
                _ => None,
            },
            mortalite_allaitement: band.cs_tx_pertes_nv,
            total_nes_vifs: (band.cs_nv_portee.unwrap_or_default() * litters as f64).round() as i64,
            total_sevres: total_weaned,
            ..GtttSummary::default()
        };
    }
    let births = events
        .iter()
        .filter(|event| event.r#type == "mise_bas")
        .collect::<Vec<_>>();
    let total_live = births
        .iter()
        .map(|event| event.nes_vifs.unwrap_or_default())
        .sum::<i64>();
    let total_real = births
        .iter()
        .map(|event| event.nes_totaux.unwrap_or_default())
        .sum::<i64>();
    let total_stillborn = births
        .iter()
        .map(|event| event.mort_nes.unwrap_or_default())
        .sum::<i64>();
    let total_weaned = events
        .iter()
        .map(|event| event.nb_sevres.unwrap_or_default())
        .sum::<i64>();
    let adopted = events
        .iter()
        .map(|event| event.adoptes.unwrap_or_default())
        .sum::<i64>();
    let removed = events
        .iter()
        .map(|event| event.retires.unwrap_or_default())
        .sum::<i64>();
    let available = total_live + adopted - removed;
    let mean_i64 = |values: Vec<i64>| {
        (!values.is_empty())
            .then(|| values.iter().sum::<i64>() as f64 / values.len() as f64)
            .map(|value| (value * 100.0).round() / 100.0)
    };
    GtttSummary {
        portees: births.len(),
        nes_totaux_moy: mean_i64(births.iter().filter_map(|event| event.nes_totaux).collect()),
        nes_vifs_moy: mean_i64(births.iter().filter_map(|event| event.nes_vifs).collect()),
        sevres_moy: mean_i64(events.iter().filter_map(|event| event.nb_sevres).collect()),
        taux_mortnes: (total_real > 0)
            .then(|| (total_stillborn as f64 / total_real as f64 * 1000.0).round() / 10.0),
        mortalite_allaitement: (available > 0).then(|| {
            ((available - total_weaned).max(0) as f64 / available as f64 * 1000.0).round() / 10.0
        }),
        total_nes_vifs: total_live,
        total_sevres: total_weaned,
        ..GtttSummary::default()
    }
}

fn gttt_rank_rows(litters: &[GtttLitter]) -> AppResult<Vec<Value>> {
    let mut ranks: Vec<i64> = litters.iter().map(|litter| litter.rank).collect();
    ranks.sort_unstable();
    ranks.dedup();
    ranks
        .into_iter()
        .map(|rank| -> AppResult<Value> {
            let selected = litters
                .iter()
                .filter(|litter| litter.rank == rank)
                .cloned()
                .collect::<Vec<_>>();
            let mut value = serde_json::to_value(gttt_summary(&selected)).unwrap_or_default();
            json_object_mut(&mut value, "la synthèse GTTT par rang")?
                .insert("rang".into(), json!(rank));
            Ok(value)
        })
        .collect()
}

const OBJECTIVE_INDICATORS: [(&str, &str, i64); 17] = [
    ("cs_truies_saillies", "Truies saillies", 0),
    ("cs_pleines", "Pleines à l'écho", 0),
    ("cs_truies_mb", "Truies mises-bas", 0),
    ("cs_nt_portee", "NT / portée", 1),
    ("cs_nv_portee", "NV / portée", 2),
    ("cs_mn_portee", "Mort-nés / portée", 2),
    ("cs_sevres_portee", "Sevrés / portée", 2),
    ("cs_tx_pertes_nv", "Taux pertes / nés vifs (%)", 1),
    ("cs_total_sevres", "Total sevrés", 0),
    ("cs_poids_sevrage", "Poids au sevrage (kg)", 1),
    ("cs_gmq_ps", "GMQ post-sevrage (g/j)", 0),
    ("cs_gmq_engr", "GMQ engraissement (g/j)", 0),
    ("cs_gmq_nv", "GMQ naissance-vente (g/j)", 0),
    ("cs_adoptes", "Adoptés (total bandes)", 0),
    ("cs_retires", "Retirés (total bandes)", 0),
    ("sevres_truie_an", "Sevrés / truie productive / an", 1),
    ("portees_truie_an", "Portées / truie productive / an", 2),
];

fn objective_definition(key: &str) -> Option<(&'static str, i64)> {
    OBJECTIVE_INDICATORS
        .iter()
        .find(|(candidate, _, _)| *candidate == key)
        .map(|(_, label, decimals)| (*label, *decimals))
}

fn objective_sql_expression(key: &str) -> Option<&'static str> {
    match key {
        "cs_truies_saillies" => Some("SUM(cs_truies_saillies)"),
        "cs_pleines" => Some("SUM(cs_pleines)"),
        "cs_truies_mb" => Some("SUM(cs_truies_mb)"),
        "cs_total_sevres" => Some("SUM(cs_total_sevres)"),
        "cs_adoptes" => Some("SUM(cs_adoptes)"),
        "cs_retires" => Some("SUM(cs_retires)"),
        "cs_nt_portee" => Some("SUM(cs_nt_portee*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_nt_portee IS NOT NULL THEN CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END ELSE 0 END),0)"),
        "cs_nv_portee" => Some("SUM(cs_nv_portee*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_nv_portee IS NOT NULL THEN CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END ELSE 0 END),0)"),
        "cs_mn_portee" => Some("SUM(cs_mn_portee*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_mn_portee IS NOT NULL THEN CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END ELSE 0 END),0)"),
        "cs_sevres_portee" => Some("SUM(cs_sevres_portee*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_sevres_portee IS NOT NULL THEN CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END ELSE 0 END),0)"),
        "cs_tx_pertes_nv" => Some("SUM(cs_tx_pertes_nv*COALESCE(cs_nv_portee,1)*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_tx_pertes_nv IS NOT NULL THEN COALESCE(cs_nv_portee,1)*CASE WHEN cs_truies_mb>0 THEN cs_truies_mb ELSE 1 END ELSE 0 END),0)"),
        "cs_poids_sevrage" => Some("SUM(cs_poids_sevrage*CASE WHEN cs_total_sevres>0 THEN cs_total_sevres ELSE 1 END)/NULLIF(SUM(CASE WHEN cs_poids_sevrage IS NOT NULL THEN CASE WHEN cs_total_sevres>0 THEN cs_total_sevres ELSE 1 END ELSE 0 END),0)"),
        "cs_gmq_ps" => Some("AVG(cs_gmq_ps)"),
        "cs_gmq_engr" => Some("AVG(cs_gmq_engr)"),
        "cs_gmq_nv" => Some("AVG(cs_gmq_nv)"),
        _ => None,
    }
}

async fn productivite_objectives(
    pool: &SqlitePool,
    cutoff: &str,
    gttt: &GtttSummary,
) -> AppResult<(Vec<Value>, Vec<Value>)> {
    let mut objectives = generic_rows(
        pool,
        "SELECT id,cle,libelle,valeur,sens,decimales,ordre FROM objectif WHERE actif=1 ORDER BY ordre,id",
    )
    .await?;
    let mut used = HashSet::new();
    for objective in &mut objectives {
        let object = json_object_mut(objective, "les objectifs")?;
        let key = object
            .get("cle")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        used.insert(key.clone());
        let actual = match key.as_str() {
            "sevres_truie_an" => gttt.sevres_truie_an,
            "portees_truie_an" => gttt.portees_truie_an,
            _ => {
                if let Some(expression) = objective_sql_expression(&key) {
                    let sql = format!(
                        "SELECT CAST(({expression}) AS REAL) FROM bande WHERE date_mb>=date('now',?)"
                    );
                    sqlx::query_scalar::<_, Option<f64>>(&sql)
                        .bind(cutoff)
                        .fetch_one(pool)
                        .await?
                } else {
                    None
                }
            }
        };
        let target = object.get("valeur").and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_i64().map(|number| number as f64))
                .or_else(|| value.as_u64().map(|number| number as f64))
        });
        let sense = object.get("sens").and_then(Value::as_str).unwrap_or("haut");
        let reached = match (actual, target) {
            (Some(actual), Some(target)) if sense == "bas" => Some(actual <= target),
            (Some(actual), Some(target)) => Some(actual >= target),
            _ => None,
        };
        object.insert("valeur_elevage".into(), json!(actual));
        object.insert("atteint".into(), json!(reached));
        object.insert(
            "ecart".into(),
            json!(actual
                .zip(target)
                .map(|(actual, target)| { ((actual - target) * 100.0).round() / 100.0 })),
        );
    }
    let available = OBJECTIVE_INDICATORS
        .iter()
        .filter(|(key, _, _)| !used.contains(*key))
        .map(|(key, label, decimals)| json!({"cle":key,"libelle":label,"decimales":decimals}))
        .collect::<Vec<_>>();
    Ok((objectives, available))
}

async fn objectifs_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let ids = sqlx::query_scalar::<_, i64>("SELECT id FROM objectif")
        .fetch_all(&state.pool)
        .await?;
    let mut tx = state.pool.begin().await?;
    for id in ids {
        let key = format!("obj_{id}");
        if let Some(raw) = form.get(&key) {
            let value = if raw.trim().is_empty() {
                None
            } else {
                parse_french_number(raw).filter(|value| value.is_finite())
            };
            sqlx::query("UPDATE objectif SET valeur=? WHERE id=?")
                .bind(value)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "modifier",
        "objectifs",
        "Objectifs de productivité",
        "/objectif/maj",
    )
    .await;
    Ok(Redirect::to("/productivite").into_response())
}

async fn objectif_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let key = form_text(&form, "cle")
        .ok_or_else(|| AppError::Invalid("Indicateur obligatoire".into()))?;
    let Some((default_label, default_decimals)) = objective_definition(&key) else {
        return Err(AppError::Invalid("Indicateur inconnu".into()));
    };
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM objectif WHERE cle=? AND actif=1")
        .bind(&key)
        .fetch_one(&state.pool)
        .await?;
    if exists > 0 {
        return Err(AppError::Invalid("Cet objectif existe déjà".into()));
    }
    let value = form.get("valeur").and_then(|raw| {
        if raw.trim().is_empty() {
            None
        } else {
            parse_french_number(raw).filter(|value| value.is_finite())
        }
    });
    let sense = if form.get("sens").map(String::as_str) == Some("bas") {
        "bas"
    } else {
        "haut"
    };
    let decimals = form_i64(&form, "decimales")
        .unwrap_or(default_decimals)
        .clamp(0, 3);
    sqlx::query("INSERT INTO objectif(cle,libelle,valeur,sens,decimales,ordre,actif) VALUES(?,?,?,?,?,(SELECT COALESCE(MAX(ordre),0)+1 FROM objectif),1)")
        .bind(&key)
        .bind(form_text(&form,"libelle").unwrap_or_else(||default_label.to_string()))
        .bind(value)
        .bind(sense)
        .bind(decimals)
        .execute(&state.pool)
        .await?;
    db::journal(
        &state.pool,
        &session.nom,
        "créer",
        "objectif",
        default_label,
        "/objectif/ajouter",
    )
    .await;
    Ok(Redirect::to("/productivite").into_response())
}

async fn objectif_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM objectif WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    db::journal(
        &state.pool,
        &session.nom,
        "supprimer",
        "objectif",
        &id.to_string(),
        "/objectif/supprimer",
    )
    .await;
    Ok(Redirect::to("/productivite").into_response())
}

async fn productivite(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let months = query
        .get("mois")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| matches!(value, 6 | 12 | 24 | 36 | 60 | 120))
        .unwrap_or(24);
    let view = query
        .get("vue")
        .map(String::as_str)
        .filter(|value| matches!(*value, "bandes" | "cheptel" | "rangs"))
        .unwrap_or("bandes");
    let cutoff = format!("-{months} months");
    let period_start =
        Local::now().date_naive() - Duration::days((months as f64 * 30.44).round() as i64);
    let gttt_litters = load_gttt_litters(&state.pool, None)
        .await?
        .into_iter()
        .filter(|litter| {
            litter
                .farrowing_date
                .is_some_and(|date| date >= period_start)
        })
        .collect::<Vec<_>>();
    let gttt_period = gttt_summary(&gttt_litters);
    let (objectives, available_objectives) =
        productivite_objectives(&state.pool, &cutoff, &gttt_period).await?;
    let rows = sqlx::query("SELECT b.id,b.code,b.date_mb,b.site,b.cs_truies_saillies,b.cs_pleines,b.cs_truies_mb,b.cs_nv_portee,b.cs_sevres_portee,b.cs_total_sevres,b.cs_tx_pertes_nv,b.cs_poids_sevrage,b.cs_gmq_ps,b.cs_gmq_engr,(SELECT MIN(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='ia') AS premiere_ia_reelle,(SELECT MIN(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='mise_bas' AND e.date<=date('now')) AS premiere_mb_reelle,(SELECT MAX(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='sevrage') AS dernier_sevrage_reel,(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type IN('echo','echographie')) AS echos,(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type IN('echo','echographie') AND lower(COALESCE(e.resultat,'')) IN('positive','positif','pleine','oui')) AS echos_positives,ROUND(100.0*(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type IN('echo','echographie') AND lower(COALESCE(e.resultat,'')) IN('positive','positif','pleine','oui'))/NULLIF((SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type IN('echo','echographie')),0),1) AS taux_pleines_echo,ROUND(100.0*COALESCE(b.cs_truies_mb,0)/NULLIF(b.cs_truies_saillies,0),1) AS taux_mb_saillies FROM bande b WHERE b.date_mb>=date('now',?) ORDER BY b.date_mb DESC,b.id DESC")
        .bind(&cutoff)
        .fetch_all(&state.pool)
        .await?;
    let technical = sqlx::query("SELECT b.id,b.code,b.date_mb,CAST(SUM(COALESCE(v.nb_porcs,0)) AS INTEGER) AS porcs,ROUND(SUM(COALESCE(v.poids_total,0))/NULLIF(SUM(COALESCE(v.nb_porcs,0)),0),1) AS poids_moyen,ROUND(SUM(CASE WHEN v.tmp IS NOT NULL THEN v.tmp*COALESCE(v.nb_porcs,0) ELSE 0 END)/NULLIF(SUM(CASE WHEN v.tmp IS NOT NULL THEN COALESCE(v.nb_porcs,0) ELSE 0 END),0),2) AS tmp,ROUND(SUM(CASE WHEN v.muscle_lot IS NOT NULL THEN v.muscle_lot*COALESCE(v.nb_porcs,0) ELSE 0 END)/NULLIF(SUM(CASE WHEN v.muscle_lot IS NOT NULL THEN COALESCE(v.nb_porcs,0) ELSE 0 END),0),2) AS muscle_lot,ROUND(SUM(COALESCE(v.montant_ht,0))/NULLIF(SUM(COALESCE(v.poids_total,0)),0),3) AS prix_ht_kg,ROUND(SUM(CASE WHEN v.plus_value IS NOT NULL THEN v.plus_value*COALESCE(v.nb_porcs,0) ELSE 0 END)/NULLIF(SUM(CASE WHEN v.plus_value IS NOT NULL THEN COALESCE(v.nb_porcs,0) ELSE 0 END),0),2) AS plus_value,CAST(ROUND(SUM(COALESCE(v.montant_ht,0)),2) AS REAL) AS montant_ht FROM venteapport v JOIN bande b ON b.id=v.bande_id WHERE v.date IS NULL OR v.date>=date('now',?) GROUP BY b.id,b.code,b.date_mb ORDER BY b.date_mb DESC,b.id DESC")
        .bind(&cutoff)
        .fetch_all(&state.pool)
        .await?;
    let funnel = generic_rows(
        &state.pool,
        &format!("SELECT CAST(COALESCE(SUM(cs_truies_saillies),0) AS INTEGER) AS saillies,CAST(COALESCE(SUM(cs_pleines),0) AS INTEGER) AS pleines,CAST(COALESCE(SUM(cs_truies_mb),0) AS INTEGER) AS mises_bas,CAST(COALESCE(SUM(cs_total_sevres),0) AS INTEGER) AS sevres,CAST(COALESCE(SUM(cs_adoptes),0) AS INTEGER) AS adoptes,CAST(COALESCE(SUM(cs_retires),0) AS INTEGER) AS retires FROM bande WHERE date_mb>=date('now','-{months} months')"),
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({}));
    let active_sows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE reformee=0")
        .fetch_one(&state.pool)
        .await?;
    let eld_summary = generic_rows(
        &state.pool,
        "WITH latest AS (SELECT m.truie_id,m.eld FROM mesuretruie m JOIN truie t ON t.id=m.truie_id AND t.reformee=0 WHERE m.eld IS NOT NULL AND NOT EXISTS(SELECT 1 FROM mesuretruie n WHERE n.truie_id=m.truie_id AND n.eld IS NOT NULL AND (n.date>m.date OR (n.date=m.date AND n.id>m.id)))) SELECT COUNT(*) AS mesures,CAST(ROUND(AVG(eld),2) AS REAL) AS moyenne FROM latest",
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({}));
    let ranks = gttt_rank_rows(&gttt_litters)?;
    let schedule = load_band_schedule(&state.pool).await?;
    let today = Local::now().date_naive();
    let stage_source = generic_rows(
        &state.pool,
        "SELECT b.date_mb,COUNT(t.id) AS truies FROM bande b LEFT JOIN truie t ON t.bande_code=b.code AND t.reformee=0 WHERE b.active=1 GROUP BY b.id,b.date_mb ORDER BY b.date_mb,b.id",
    )
    .await?;
    let mut stage_counts: HashMap<String, i64> = HashMap::new();
    for row in stage_source {
        let Some(date) = row
            .get("date_mb")
            .and_then(Value::as_str)
            .and_then(parse_stored_date)
        else {
            continue;
        };
        let age = (today - date).num_days();
        let count = row
            .get("truies")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        *stage_counts
            .entry(schedule.stage(age).0.to_string())
            .or_default() += count;
    }
    let unassigned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM truie WHERE reformee=0 AND (bande_code IS NULL OR trim(bande_code)='' OR NOT EXISTS(SELECT 1 FROM bande b WHERE b.code=truie.bande_code AND b.active=1))",
    )
    .fetch_one(&state.pool)
    .await?;
    if unassigned > 0 {
        stage_counts.insert("Sans bande active".into(), unassigned);
    }
    let stage_order = [
        "Planifiée",
        "Verraterie",
        "Gestante",
        "Maternité (préparation)",
        "Maternité",
        "Post-sevrage",
        "Engraissement",
        "Départ / terminé",
        "Sans bande active",
    ];
    let stages = stage_order
        .into_iter()
        .filter_map(|name| {
            stage_counts
                .get(name)
                .copied()
                .filter(|count| *count > 0)
                .map(|count| json!({"nom":name,"truies":count}))
        })
        .collect::<Vec<_>>();
    let mut ctx = context(&session);
    ctx.insert("bandes".into(), Value::Array(rows_to_json(rows)?));
    ctx.insert("technique".into(), Value::Array(rows_to_json(technical)?));
    ctx.insert("entonnoir".into(), funnel);
    ctx.insert("truies_actives".into(), json!(active_sows));
    ctx.insert("eld_resume".into(), eld_summary);
    ctx.insert("stades".into(), Value::Array(stages));
    ctx.insert("rangs".into(), Value::Array(ranks));
    ctx.insert("objectifs".into(), Value::Array(objectives));
    ctx.insert(
        "objectifs_disponibles".into(),
        Value::Array(available_objectives),
    );
    ctx.insert(
        "gttt_periode".into(),
        serde_json::to_value(gttt_period).unwrap_or_default(),
    );
    ctx.insert("mois".into(), json!(months));
    ctx.insert("vue".into(), json!(view));
    render(&state, "productivite.html", Value::Object(ctx))
}

async fn parameter_f64(pool: &SqlitePool, key: &str, default: f64) -> AppResult<f64> {
    let value: Option<String> = sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle=?")
        .bind(key)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(value
        .and_then(|value| parse_french_number(&value))
        .unwrap_or(default))
}
async fn parameter_list(pool: &SqlitePool, key: &str, defaults: &[&str]) -> AppResult<Vec<String>> {
    let value: Option<String> = sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle=?")
        .bind(key)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(value
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| defaults.iter().map(|value| (*value).to_string()).collect()))
}

/// Type d'élevage actif (`parametre.type_elevage`), retombe sur le cycle complet
/// historique si non renseigné ou si la valeur enregistrée n'est plus reconnue.
async fn type_elevage_actif(pool: &SqlitePool) -> AppResult<String> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='type_elevage'")
            .fetch_optional(pool)
            .await?
            .flatten();
    Ok(value
        .filter(|value| auth::TYPES_ELEVAGE.iter().any(|(code, _)| *code == value))
        .unwrap_or_else(|| auth::TYPE_ELEVAGE_DEFAUT.to_string()))
}

/// Places encore disponibles pour une étape (verraterie, maternité,
/// post-sevrage, engraissement, §0/§3 de la spécification) : jamais négatif,
/// une capacité dépassée affiche 0 place plutôt qu'un nombre négatif trompeur.
fn places_disponibles(capacite: i64, occupation: i64) -> i64 {
    (capacite - occupation).max(0)
}

/// Un rappel sanitaire (§8) est en retard dès que son échéance est atteinte
/// ou dépassée, échéance incluse.
fn rappel_en_retard(echeance: NaiveDate, today: NaiveDate) -> bool {
    echeance <= today
}

/// Consommation quotidienne moyenne (tonnes/jour) entre deux relevés de
/// silo, par bilan de matière : ce qu'il y avait + ce qui a été livré - ce
/// qu'il reste, réparti sur le nombre de jours écoulés. `None` si l'écart
/// entre les deux relevés n'est pas positif (relevés non ordonnés).
fn consommation_quotidienne_tonnes(
    niveau_precedent: f64,
    livraisons_recues: f64,
    niveau_actuel: f64,
    jours_ecoules: i64,
) -> Option<f64> {
    (jours_ecoules > 0).then(|| {
        ((niveau_precedent + livraisons_recues - niveau_actuel) / jours_ecoules as f64).max(0.0)
    })
}

/// Nombre de jours avant rupture de stock au rythme de consommation actuel.
/// `None` si la consommation quotidienne est nulle (pas de rupture prévisible).
fn jours_avant_rupture(
    niveau_actuel_tonnes: f64,
    consommation_quotidienne_tonnes: f64,
) -> Option<f64> {
    (consommation_quotidienne_tonnes > 0.0)
        .then(|| niveau_actuel_tonnes / consommation_quotidienne_tonnes)
}

/// Tonnage à commander pour ramener le silo à sa capacité déclarée (§3
/// « prévision de commande d'aliment avant rechargement »). `None` si la
/// capacité n'est pas renseignée (rien à recommander sans référence).
fn quantite_a_commander(niveau_actuel_tonnes: f64, capacite_tonnes: Option<f64>) -> Option<f64> {
    capacite_tonnes.map(|capacite| (capacite - niveau_actuel_tonnes).max(0.0))
}

/// Vrai si une commande doit être passée maintenant compte tenu du délai de
/// livraison habituel : le stock serait épuisé avant que la commande
/// n'arrive. `jours_avant_rupture=None` (consommation nulle ou inconnue) ne
/// déclenche jamais d'alerte — on ne prévient pas d'une rupture qu'on ne
/// sait pas dater.
fn commande_urgente(jours_avant_rupture: Option<f64>, delai_livraison_jours: i64) -> bool {
    jours_avant_rupture.is_some_and(|jours| jours <= delai_livraison_jours as f64)
}

async fn reglage_i64(pool: &SqlitePool, cle: &str, default: i64) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT valeur FROM reglage WHERE cle=?")
        .bind(cle)
        .fetch_optional(pool)
        .await?
        .unwrap_or(default))
}

/// Effectif de truies actives dans les salles dont le type correspond au
/// motif LIKE donné (ex. "%verrater%", "%matern%").
async fn occupation_truies(pool: &SqlitePool, type_like: &str) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id LEFT JOIN salle s ON s.id=COALESCE(t.salle_id,c.salle_id) WHERE t.reformee=0 AND lower(COALESCE(s.type,'')) LIKE ?",
    )
    .bind(type_like)
    .fetch_one(pool)
    .await?)
}

/// Stade déduit du type de salle (mêmes motifs que `occupation_porcs`/
/// `occupation_truies`), fonction pure testable indépendamment de la base.
/// `None` si le type de salle n'est pas reconnu (ex. salle non renseignée).
fn stade_pour_type_salle(type_salle: &str) -> Option<String> {
    let t = type_salle.to_lowercase();
    if t.contains("verrater") {
        Some("Verraterie".to_string())
    } else if t.contains("matern") {
        Some("Maternité".to_string())
    } else if t.contains("sevr") {
        Some("Post-sevrage".to_string())
    } else if t.contains("engrais") || t.contains("finition") {
        Some("Engraissement".to_string())
    } else {
        None
    }
}

/// Stade déduit du type de la salle contenant la case donnée, pour ne plus
/// dépendre d'une sélection manuelle qui peut ne pas correspondre à la case
/// réellement choisie lors d'une déclaration de mortalité. `None` si la case
/// est inconnue ou si le type de sa salle n'est pas reconnu : dans ce cas
/// l'appelant retombe sur la valeur du formulaire.
async fn stade_from_case(pool: &SqlitePool, case_id: i64) -> AppResult<Option<String>> {
    let salle_type: Option<Option<String>> = sqlx::query_scalar(
        "SELECT s.type FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE c.id=?",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    Ok(salle_type.flatten().and_then(|t| stade_pour_type_salle(&t)))
}

/// Effectif de porcs présents dans les cases dont la salle correspond à l'un
/// des motifs LIKE donnés, recalculé à partir des mouvements réels (voir
/// `case_pig_count`) plutôt qu'une valeur figée.
async fn occupation_porcs(pool: &SqlitePool, types_like: &[&str]) -> AppResult<i64> {
    let mut cases = std::collections::HashSet::new();
    for type_like in types_like {
        let matched: Vec<i64> = sqlx::query_scalar(
            "SELECT c.id FROM casesalle c JOIN salle s ON s.id=c.salle_id WHERE lower(COALESCE(s.type,'')) LIKE ?",
        )
        .bind(type_like)
        .fetch_all(pool)
        .await?;
        cases.extend(matched);
    }
    let mut total = 0;
    for case_id in cases {
        total += case_pig_count(pool, case_id).await?;
    }
    Ok(total)
}

/// Capacités par étape (§0/§3 de la spécification), limitées aux phases
/// réellement présentes pour le type d'élevage actif.
async fn capacites_par_etape(pool: &SqlitePool, session: &SessionData) -> AppResult<Vec<Value>> {
    let mut etapes = Vec::new();
    if session.a_truies() {
        for (cle, libelle, type_like) in [
            ("capacite_verraterie", "Verraterie", "%verrater%"),
            ("capacite_maternite", "Maternité", "%matern%"),
        ] {
            let capacite = reglage_i64(pool, cle, 0).await?;
            let occ = occupation_truies(pool, type_like).await?;
            etapes.push(json!({
                "libelle": libelle, "capacite": capacite, "occupation": occ,
                "places_disponibles": places_disponibles(capacite, occ),
            }));
        }
    }
    etapes.push({
        let capacite = reglage_i64(pool, "capacite_postsevrage", 0).await?;
        let occ = occupation_porcs(pool, &["%sevr%"]).await?;
        json!({
            "libelle": "Post-sevrage", "capacite": capacite, "occupation": occ,
            "places_disponibles": places_disponibles(capacite, occ),
        })
    });
    if session.engraisse() {
        let capacite = reglage_i64(pool, "capacite_engraissement", 0).await?;
        let occ = occupation_porcs(pool, &["%engrais%", "%finition%"]).await?;
        etapes.push(json!({
            "libelle": "Engraissement", "capacite": capacite, "occupation": occ,
            "places_disponibles": places_disponibles(capacite, occ),
        }));
    }
    Ok(etapes)
}

/// Lit un module optionnel booléen (`parametre.cle` = "1"/"0"). `default` est
/// utilisé pour les bases existantes qui n'ont jamais enregistré le réglage :
/// `true` préserve le comportement actuel (module déjà utilisé de fait),
/// `false` respecte le principe « la complexité s'active, elle ne s'impose
/// pas » pour un module réellement nouveau.
async fn module_actif(pool: &SqlitePool, cle: &str, default: bool) -> AppResult<bool> {
    let value: Option<String> = sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle=?")
        .bind(cle)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(value.map(|value| value == "1").unwrap_or(default))
}

async fn reformes_seuils(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let keys = [
        "seuil_nv_min",
        "seuil_sevres_min",
        "seuil_retours_max",
        "seuil_ecrases_max",
        "seuil_rang_max",
        "seuil_chetifs_max",
    ];
    let mut tx = state.pool.begin().await?;
    for key in keys {
        if let Some(value) = form_f64(&form, key).filter(|value| value.is_finite() && *value >= 0.0)
        {
            sqlx::query("INSERT INTO parametre(cle,valeur) VALUES(?,?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(key).bind(value.to_string()).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(Redirect::to("/reformes").into_response())
}
async fn reformes_criteres(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let allowed = ["nv", "sevres", "retours", "ecrases", "rang", "chetifs"];
    let selected = allowed
        .iter()
        .filter(|code| form.contains_key(&format!("crit_{code}")))
        .copied()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(AppError::Invalid("Sélectionne au moins un critère".into()));
    }
    sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('reforme_criteres',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(selected.join(",")).execute(&state.pool).await?;
    Ok(Redirect::to("/reformes").into_response())
}
async fn cochettes_criteres(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let allowed = [
        "nv", "sevres", "ecrases", "retours", "rang", "chetifs", "issf", "mort_nes", "momifies",
        "splayleg", "tetines",
    ];
    let selected = allowed
        .iter()
        .filter(|code| form.contains_key(&format!("crit_{code}")))
        .copied()
        .collect::<Vec<_>>();
    if selected.is_empty() || selected.len() > 6 {
        return Err(AppError::Invalid(
            "Sélectionnez entre un et six critères".into(),
        ));
    }
    sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('cochette_criteres',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(selected.join(",")).execute(&state.pool).await?;
    Ok(Redirect::to("/cochettes").into_response())
}

/// Nombre de retours en chaleur depuis la dernière mise-bas (ou depuis
/// l'entrée en verraterie pour une cochette qui n'a jamais mis bas) : une
/// chaleur observée après une IA au cours du même cycle signale l'échec de
/// cette IA. Vrai garde-fou trouvé cassé en vérifiant les prérequis de
/// gestion d'élevage : la colonne historique `truie.nb_retours` (comme
/// `truie.issf` ci-dessous), utilisée par le seuil de réforme « retours
/// élevés » et par la sélection des mères à cochettes, n'était plus jamais
/// mise à jour par l'application Rust — figée à sa valeur héritée de la
/// base Python 1.65 au jour de la bascule, elle ne détectait donc plus
/// aucune dérive survenue depuis. Recalculée en direct depuis l'historique
/// des événements, comme le reste des effectifs de ce fichier, plutôt que
/// maintenue comme un compteur à mettre à jour à chaque saisie (risque
/// d'oubli à un des nombreux points de saisie de chaleur/IA/mise-bas).
const NB_RETOURS_EXPR: &str = "(SELECT COUNT(*) FROM evenement c WHERE c.truie_id=t.id AND c.type='chaleur' AND c.date>COALESCE((SELECT MAX(mb.date) FROM evenement mb WHERE mb.truie_id=t.id AND mb.type='mise_bas'),'0000-01-01') AND EXISTS(SELECT 1 FROM evenement i WHERE i.truie_id=t.id AND i.type='ia' AND i.date<c.date AND i.date>COALESCE((SELECT MAX(mb2.date) FROM evenement mb2 WHERE mb2.truie_id=t.id AND mb2.type='mise_bas'),'0000-01-01')))";

/// Intervalle sevrage → saillie fécondante (ISSF, en jours) : écart entre le
/// sevrage précédent et l'IA la plus proche de 115 jours avant la dernière
/// mise-bas (même repère de correspondance que `probable_ia` sur la fiche
/// truie). `NULL` tant qu'aucune mise-bas n'est enregistrée : avant cela,
/// aucune IA ne peut être identifiée comme celle qui a fécondé.
// Note d'implémentation : le choix de l'IA la plus proche de 115 jours
// avant la mise-bas passe volontairement par une égalité avec le minimum
// (sous-requête MIN) plutôt que par un `ORDER BY ... LIMIT 1` corrélé à la
// table externe — un vrai bug SQLite trouvé en écrivant les tests : un
// `ORDER BY` référençant la truie externe (`t.id`) au sein d'une
// sous-requête scalaire renvoie « no such column: t.id », alors que la même
// référence fonctionne dans une clause WHERE de la même sous-requête.
const ISSF_EXPR: &str = "(CASE WHEN (SELECT MAX(mb.date) FROM evenement mb WHERE mb.truie_id=t.id AND mb.type='mise_bas') IS NOT NULL THEN (SELECT CAST(julianday(x.date)-julianday((SELECT MAX(sev.date) FROM evenement sev WHERE sev.truie_id=t.id AND sev.type='sevrage' AND sev.date<x.date)) AS INTEGER) FROM evenement x WHERE x.truie_id=t.id AND x.type='ia' AND x.date IS NOT NULL AND (SELECT MAX(sev.date) FROM evenement sev WHERE sev.truie_id=t.id AND sev.type='sevrage' AND sev.date<x.date) IS NOT NULL AND ABS(julianday((SELECT MAX(mb.date) FROM evenement mb WHERE mb.truie_id=t.id AND mb.type='mise_bas'))-julianday(x.date)-115)=(SELECT MIN(ABS(julianday((SELECT MAX(mb2.date) FROM evenement mb2 WHERE mb2.truie_id=t.id AND mb2.type='mise_bas'))-julianday(x2.date)-115)) FROM evenement x2 WHERE x2.truie_id=t.id AND x2.type='ia' AND x2.date IS NOT NULL) LIMIT 1) ELSE NULL END)";

#[cfg(test)]
mod reforme_indicateurs_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_sow() -> anyhow::Result<(SqlitePool, i64)> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        let sow =
            sqlx::query("INSERT INTO truie(num_travail,statut,reformee) VALUES('T1','active',0)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        Ok((pool, sow))
    }

    async fn scalar(pool: &SqlitePool, expr: &str, sow: i64) -> anyhow::Result<Option<i64>> {
        let sql = format!("SELECT {expr} FROM truie t WHERE t.id=?");
        Ok(sqlx::query_scalar(&sql).bind(sow).fetch_one(pool).await?)
    }

    /// Vrai garde-fou trouvé cassé en vérifiant les prérequis de gestion
    /// d'élevage : `truie.nb_retours` (échecs d'IA répétés, critère de
    /// réforme) n'était plus jamais recalculé par l'application Rust.
    /// Ici : deux échecs d'IA doivent être comptés (deux chaleurs observées
    /// après une IA), la toute première chaleur (avant toute IA) ne compte
    /// pas — ce n'est pas un retour, c'est la chaleur normale de reprise.
    #[tokio::test]
    async fn nb_retours_compte_les_chaleurs_qui_suivent_une_ia() -> anyhow::Result<()> {
        let (pool, sow) = pool_with_sow().await?;
        for (date, kind) in [
            ("2026-01-01", "chaleur"), // première chaleur, avant toute IA : pas un retour
            ("2026-01-02", "ia"),
            ("2026-01-23", "chaleur"), // retour n°1 (après l'IA du 02/01)
            ("2026-01-24", "ia"),
            ("2026-02-14", "chaleur"), // retour n°2 (après l'IA du 24/01)
        ] {
            sqlx::query("INSERT INTO evenement(type,date,truie_id) VALUES(?,?,?)")
                .bind(kind)
                .bind(date)
                .bind(sow)
                .execute(&pool)
                .await?;
        }
        assert_eq!(scalar(&pool, NB_RETOURS_EXPR, sow).await?, Some(2));
        Ok(())
    }

    /// Un cycle achevé par une mise-bas repart de zéro : les retours
    /// d'avant cette mise-bas ne doivent plus compter pour le cycle suivant.
    #[tokio::test]
    async fn nb_retours_se_reinitialise_apres_une_mise_bas() -> anyhow::Result<()> {
        let (pool, sow) = pool_with_sow().await?;
        for (date, kind) in [
            ("2026-01-01", "ia"),
            ("2026-01-22", "chaleur"), // retour n°1, avant la mise-bas
            ("2026-01-23", "ia"),
            ("2026-05-18", "mise_bas"), // clôture le cycle
            ("2026-06-01", "chaleur"),  // nouvelle chaleur, nouveau cycle : pas un retour
        ] {
            sqlx::query("INSERT INTO evenement(type,date,truie_id) VALUES(?,?,?)")
                .bind(kind)
                .bind(date)
                .bind(sow)
                .execute(&pool)
                .await?;
        }
        assert_eq!(scalar(&pool, NB_RETOURS_EXPR, sow).await?, Some(0));
        Ok(())
    }

    /// Vrai garde-fou trouvé cassé, même défaut que `nb_retours` :
    /// `truie.issf` n'était plus recalculé. L'ISSF est l'écart entre le
    /// sevrage précédent et l'IA la plus proche de 115 jours avant la
    /// mise-bas qui a suivi.
    #[tokio::test]
    async fn issf_mesure_lecart_entre_sevrage_et_ia_fecondante() -> anyhow::Result<()> {
        let (pool, sow) = pool_with_sow().await?;
        for (date, kind) in [
            ("2026-01-01", "sevrage"),
            ("2026-01-06", "ia"), // IA fécondante : 5 jours après le sevrage
            ("2026-05-01", "mise_bas"), // ~115 jours après l'IA du 06/01
        ] {
            sqlx::query("INSERT INTO evenement(type,date,truie_id) VALUES(?,?,?)")
                .bind(kind)
                .bind(date)
                .bind(sow)
                .execute(&pool)
                .await?;
        }
        assert_eq!(scalar(&pool, ISSF_EXPR, sow).await?, Some(5));
        Ok(())
    }

    /// Sans mise-bas enregistrée, aucune IA ne peut être identifiée comme
    /// fécondante : l'ISSF reste `NULL` plutôt que d'inventer une valeur.
    #[tokio::test]
    async fn issf_est_nul_sans_mise_bas_enregistree() -> anyhow::Result<()> {
        let (pool, sow) = pool_with_sow().await?;
        sqlx::query("INSERT INTO evenement(type,date,truie_id) VALUES('ia','2026-01-06',?)")
            .bind(sow)
            .execute(&pool)
            .await?;
        assert_eq!(scalar(&pool, ISSF_EXPR, sow).await?, None);
        Ok(())
    }
}

async fn reformes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let seuils = json!({
        "seuil_nv_min":parameter_f64(&state.pool,"seuil_nv_min",13.0).await?,
        "seuil_sevres_min":parameter_f64(&state.pool,"seuil_sevres_min",11.0).await?,
        "seuil_retours_max":parameter_f64(&state.pool,"seuil_retours_max",2.0).await?,
        "seuil_ecrases_max":parameter_f64(&state.pool,"seuil_ecrases_max",4.0).await?,
        "seuil_rang_max":parameter_f64(&state.pool,"seuil_rang_max",7.0).await?,
        "seuil_chetifs_max":parameter_f64(&state.pool,"seuil_chetifs_max",20.0).await?,
    });
    let criteria = parameter_list(
        &state.pool,
        "reforme_criteres",
        &["nv", "sevres", "retours", "ecrases", "rang", "chetifs"],
    )
    .await?;
    let raw=generic_rows(&state.pool,&format!("SELECT id,num_travail,bande_code,rang,perf_nv,perf_sevres,{NB_RETOURS_EXPR} AS nb_retours,tx_chetifs,CAST(COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.truie_id=t.id AND lower(COALESCE(p.cause,'')) LIKE '%cras%'),0) AS INTEGER) AS ecrases FROM truie t WHERE reformee=0 ORDER BY num_travail")).await?;
    let mut rows = Vec::new();
    for mut row in raw {
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let mut reasons = Vec::new();
        let f = |key: &str| object.get(key).and_then(Value::as_f64);
        let i = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_i64)
                .map(|value| value as f64)
        };
        if criteria.iter().any(|c| c == "nv")
            && f("perf_nv").is_some_and(|v| v < seuils["seuil_nv_min"].as_f64().unwrap_or(13.0))
        {
            reasons.push("nés vifs bas")
        };
        if criteria.iter().any(|c| c == "sevres")
            && f("perf_sevres")
                .is_some_and(|v| v < seuils["seuil_sevres_min"].as_f64().unwrap_or(11.0))
        {
            reasons.push("sevrés bas")
        };
        if criteria.iter().any(|c| c == "retours")
            && i("nb_retours")
                .is_some_and(|v| v > seuils["seuil_retours_max"].as_f64().unwrap_or(2.0))
        {
            reasons.push("retours élevés")
        };
        if criteria.iter().any(|c| c == "ecrases")
            && i("ecrases").is_some_and(|v| v > seuils["seuil_ecrases_max"].as_f64().unwrap_or(4.0))
        {
            reasons.push("écrasés élevés")
        };
        if criteria.iter().any(|c| c == "rang")
            && i("rang").is_some_and(|v| v > seuils["seuil_rang_max"].as_f64().unwrap_or(7.0))
        {
            reasons.push("rang élevé")
        };
        if criteria.iter().any(|c| c == "chetifs")
            && f("tx_chetifs")
                .is_some_and(|v| v > seuils["seuil_chetifs_max"].as_f64().unwrap_or(20.0))
        {
            reasons.push("chétifs élevés")
        };
        if !reasons.is_empty() {
            object.insert("raisons".into(), json!(reasons.join(", ")));
            object.insert("score".into(), json!(reasons.len()));
            object.insert("priorite".into(), json!(reasons.len().min(4)));
            rows.push(row)
        }
    }
    rows.sort_by_key(|row| {
        std::cmp::Reverse(row.get("score").and_then(Value::as_u64).unwrap_or_default())
    });
    let exits=generic_rows(&state.pool,"SELECT id,num_travail,date_reforme,motif_sortie,rang FROM truie WHERE reformee=1 ORDER BY date_reforme DESC,id DESC LIMIT 200").await?;
    let mut ctx = context(&session);
    ctx.insert("seuils".into(), seuils);
    ctx.insert("criteres".into(), json!(criteria));
    ctx.insert("candidates".into(), Value::Array(rows));
    ctx.insert("sorties".into(), Value::Array(exits));
    render(&state, "reformes.html", Value::Object(ctx))
}

async fn cochettes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let criteria = parameter_list(
        &state.pool,
        "cochette_criteres",
        &["nv", "sevres", "ecrases", "retours"],
    )
    .await?;
    let averages=generic_rows(&state.pool,&format!("SELECT ROUND(AVG(perf_nv),2) AS nv,ROUND(AVG(perf_sevres),2) AS sevres,ROUND(AVG({ISSF_EXPR}),2) AS issf,ROUND(AVG(tx_chetifs),2) AS chetifs,ROUND(AVG(perf_mn),2) AS mort_nes,ROUND(AVG(perf_mo),2) AS momifies,ROUND(AVG(nb_tetines),2) AS tetines FROM truie t WHERE reformee=0")).await?.into_iter().next().unwrap_or_else(||json!({}));
    let rows=generic_rows(&state.pool,&format!("SELECT id,num_travail,bande_code,mere_cochette,rang,perf_nv,perf_sevres,perf_mn,perf_mo,splayleg,nb_tetines,perf_tx_perte,{NB_RETOURS_EXPR} AS nb_retours,{ISSF_EXPR} AS issf,tx_chetifs,CAST(COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.truie_id=t.id AND lower(COALESCE(p.cause,'')) LIKE '%cras%'),0) AS INTEGER) AS ecrases FROM truie t WHERE reformee=0 ORDER BY num_travail")).await?;
    let threshold = criteria.len().saturating_sub(1).max(1);
    let mut candidates = Vec::new();
    let mut designated = Vec::new();
    for mut row in rows {
        let is_designated = row.get("mere_cochette").and_then(Value::as_i64) == Some(1);
        if is_designated {
            designated.push(row.clone())
        }
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let mut details = Vec::new();
        let f = |key: &str| object.get(key).and_then(Value::as_f64);
        let i = |key: &str| object.get(key).and_then(Value::as_i64);
        if criteria.iter().any(|c| c == "nv")
            && f("perf_nv").is_some_and(|v| v >= averages["nv"].as_f64().unwrap_or(0.0))
        {
            details.push("nés vifs")
        };
        if criteria.iter().any(|c| c == "sevres")
            && f("perf_sevres").is_some_and(|v| v >= averages["sevres"].as_f64().unwrap_or(0.0))
        {
            details.push("capacité maternelle")
        };
        if criteria.iter().any(|c| c == "ecrases") && i("ecrases").unwrap_or_default() <= 1 {
            details.push("peu d’écrasés")
        };
        if criteria.iter().any(|c| c == "retours") && i("nb_retours").unwrap_or_default() <= 1 {
            details.push("fertilité")
        };
        if criteria.iter().any(|c| c == "rang") && i("rang").unwrap_or_default() >= 3 {
            details.push("longévité")
        };
        if criteria.iter().any(|c| c == "chetifs")
            && f("tx_chetifs").is_some_and(|v| v <= averages["chetifs"].as_f64().unwrap_or(v))
        {
            details.push("homogénéité")
        };
        if criteria.iter().any(|c| c == "issf")
            && f("issf").is_some_and(|v| v <= averages["issf"].as_f64().unwrap_or(v))
        {
            details.push("ISSF")
        };
        for (criterion, field) in [("mort_nes", "perf_mn"), ("momifies", "perf_mo")] {
            if criteria.iter().any(|c| c == criterion)
                && f(field)
                    .zip(averages[criterion].as_f64())
                    .is_some_and(|(v, avg)| v <= avg)
            {
                details.push(if criterion == "mort_nes" {
                    "peu de mort-nés"
                } else {
                    "peu de momifiés"
                });
            }
        }
        if criteria.iter().any(|c| c == "splayleg") && i("splayleg") == Some(0) {
            details.push("sans splayleg");
        }
        if criteria.iter().any(|c| c == "tetines")
            && i("nb_tetines")
                .zip(averages["tetines"].as_f64())
                .is_some_and(|(v, avg)| v as f64 >= avg)
        {
            details.push("tétines fonctionnelles");
        }
        if details.len() >= threshold || is_designated {
            object.insert("score".into(), json!(details.len()));
            object.insert("details".into(), json!(details.join(", ")));
            candidates.push(row)
        }
    }
    candidates.sort_by_key(|row| {
        std::cmp::Reverse(row.get("score").and_then(Value::as_u64).unwrap_or_default())
    });
    let mut ctx = context(&session);
    ctx.insert("criteres".into(), json!(criteria));
    ctx.insert("moyennes".into(), averages);
    ctx.insert("candidates".into(), Value::Array(candidates));
    ctx.insert("designees".into(), Value::Array(designated));
    render(&state, "cochettes.html", Value::Object(ctx))
}

async fn ifip(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let mut references=generic_rows(&state.pool,"SELECT id,libelle,cle,annee,moyenne,tiers_sup,sens,decimales,ordre,CASE cle WHEN 'poids_sevrage' THEN (SELECT ROUND(AVG(cs_poids_sevrage),2) FROM bande WHERE cs_poids_sevrage IS NOT NULL) WHEN 'gmq_ps' THEN (SELECT ROUND(AVG(cs_gmq_ps),2) FROM bande WHERE cs_gmq_ps IS NOT NULL) WHEN 'gmq_engr' THEN (SELECT ROUND(AVG(cs_gmq_engr),2) FROM bande WHERE cs_gmq_engr IS NOT NULL) ELSE NULL END AS valeur_elevage FROM referenceifip ORDER BY ordre,id").await?;
    let gttt = gttt_summary(&load_gttt_litters(&state.pool, None).await?);
    for reference in &mut references {
        let object = json_object_mut(reference, "les références IFIP")?;
        let value = match object.get("cle").and_then(Value::as_str) {
            Some("sevres_truie_an") => gttt.sevres_truie_an,
            Some("nes_vifs") => gttt.nes_vifs_moy,
            Some("sevres_portee") => gttt.sevres_moy,
            Some("tx_pertes_allait") => gttt.mortalite_allaitement,
            _ => continue,
        };
        object.insert("valeur_elevage".into(), json!(value));
    }
    let mut ctx = context(&session);
    ctx.insert("references".into(), Value::Array(references));
    render(&state, "ifip.html", Value::Object(ctx))
}

async fn ifip_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let id =
        form_i64(&form, "id").ok_or_else(|| AppError::Invalid("Référence manquante".into()))?;
    let sense = match form.get("sens").map(String::as_str) {
        Some("bas") => "bas",
        _ => "haut",
    };
    sqlx::query("UPDATE referenceifip SET moyenne=?,tiers_sup=?,annee=?,sens=? WHERE id=?")
        .bind(form_f64(&form, "moyenne"))
        .bind(form_f64(&form, "tiers_sup"))
        .bind(form_text(&form, "annee"))
        .bind(sense)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/ifip").into_response())
}

async fn charcutiers(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    if !session.module_charcutiers_rfid {
        return Err(AppError::Forbidden);
    }
    let q = query.get("q").map(|value| value.trim()).unwrap_or("");
    let rows = if q.is_empty() {
        generic_rows(
            &state.pool,
            "SELECT id,rfid,bande_code,date_naissance,sexe,mere_courante,structure,poids1,poids2,poids3,date_mort,cause_mort,destination FROM porccharcutier ORDER BY id DESC LIMIT 1000",
        )
        .await?
    } else {
        let pattern = format!("%{q}%");
        let rows = sqlx::query("SELECT id,rfid,bande_code,date_naissance,sexe,mere_courante,structure,poids1,poids2,poids3,date_mort,cause_mort,destination FROM porccharcutier WHERE COALESCE(rfid,'') LIKE ? OR COALESCE(bande_code,'') LIKE ? OR COALESCE(mere_courante,'') LIKE ? ORDER BY id DESC LIMIT 1000")
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&state.pool)
            .await?;
        rows_to_json(rows)?
    };
    let mut ctx = context(&session);
    ctx.insert("porcs".into(), Value::Array(rows));
    ctx.insert("q".into(), json!(q));
    render(&state, "charcutiers.html", Value::Object(ctx))
}

async fn charcutier_detail(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    if !session.module_charcutiers_rfid {
        return Err(AppError::Forbidden);
    }
    let animal = generic_rows(
        &state.pool,
        &format!("SELECT id,rfid,date_naissance,bande_code,cahier_charges,sexe,mere_bio,mere_courante,structure,poids1,poids2,poids3,date_mort,cause_mort,type_perte,destination,note FROM porccharcutier WHERE id={id}"),
    )
    .await?
    .into_iter()
    .next()
    .ok_or(AppError::NotFound)?;
    let treatments = generic_rows(
        &state.pool,
        &format!("SELECT id,date,produit,dose,motif,delai_attente,note FROM traitementcharcutier WHERE charcutier_id={id} ORDER BY date DESC,id DESC"),
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("porc".into(), animal);
    ctx.insert("traitements".into(), Value::Array(treatments));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "charcutier.html", Value::Object(ctx))
}

async fn charcutier_traitement(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let product = form_text(&form, "produit")
        .ok_or_else(|| AppError::Invalid("Produit obligatoire".into()))?;
    let band: Option<String> =
        sqlx::query_scalar("SELECT bande_code FROM porccharcutier WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    if band.is_none() {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM porccharcutier WHERE id=?")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
        if exists == 0 {
            return Err(AppError::NotFound);
        }
    }
    sqlx::query("INSERT INTO traitementcharcutier(charcutier_id,bande_code,date,produit,dose,motif,delai_attente,note) VALUES(?,?,?,?,?,?,?,?)")
        .bind(id)
        .bind(band)
        .bind(form_date_or_today(&form, "date")?)
        .bind(product)
        .bind(form_text(&form, "dose"))
        .bind(form_text(&form, "motif"))
        .bind(form_i64(&form, "delai_attente").filter(|value| *value >= 0))
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/charcutier/{id}")).into_response())
}

async fn charcutier_traitement_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let animal: Option<i64> =
        sqlx::query_scalar("SELECT charcutier_id FROM traitementcharcutier WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    sqlx::query("DELETE FROM traitementcharcutier WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(
        &animal
            .map(|animal| format!("/charcutier/{animal}"))
            .unwrap_or_else(|| "/charcutiers".into()),
    )
    .into_response())
}

async fn transferts(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let cases = generic_rows(
        &state.pool,
        "SELECT si.code AS site,s.id AS salle_id,s.nom AS salle,c.id AS case_id,c.nom AS case_nom,c.nb_max_porcs,CAST(COALESCE((SELECT SUM(CASE WHEN t.case_dest_id=c.id THEN COALESCE(t.nombre,0) WHEN t.id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(t.nombre,0) END) FROM transfert t WHERE t.espece='porc' AND (t.case_dest_id=c.id OR t.case_source_id=c.id)),0)-COALESCE((SELECT SUM(d.nombre) FROM declarationmort d WHERE d.case_id=c.id),0) AS INTEGER) AS porcs,(SELECT COUNT(*) FROM truie tr WHERE tr.reformee=0 AND tr.case_id=c.id) AS nb_truies,(SELECT GROUP_CONCAT(tr.num_travail,', ') FROM truie tr WHERE tr.reformee=0 AND tr.case_id=c.id) AS truies FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.code,COALESCE(s.ordre,0),s.nom,c.nom",
    )
    .await?;
    let mut bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site FROM bande WHERE active=1 ORDER BY date_mb DESC,code",
    )
    .await?;
    for band in &mut bands {
        let object = json_object_mut(band, "les bandes de transfert")?;
        let id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        let code = object
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let remaining = remaining_band_pigs(&state.pool, id, &code).await?;
        object.insert("restant".into(), json!(remaining));
    }
    let sows = generic_rows(
        &state.pool,
        "SELECT t.id,t.num_travail,t.bande_code,si.code AS site,s.nom AS salle,c.nom AS case_nom FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id LEFT JOIN salle s ON s.id=t.salle_id LEFT JOIN site si ON si.id=s.site_id WHERE t.reformee=0 ORDER BY t.num_travail",
    )
    .await?;
    let history = generic_rows(
        &state.pool,
        "SELECT t.id,t.date,t.espece,b.code AS bande,tr.num_travail AS truie,ss.nom AS salle_source,cs.nom AS case_source,sd.nom AS salle_destination,cd.nom AS case_destination,t.nombre,t.note FROM transfert t LEFT JOIN bande b ON b.id=t.bande_id LEFT JOIN truie tr ON tr.id=t.truie_id LEFT JOIN salle ss ON ss.id=t.salle_source_id LEFT JOIN casesalle cs ON cs.id=t.case_source_id LEFT JOIN salle sd ON sd.id=t.salle_dest_id LEFT JOIN casesalle cd ON cd.id=t.case_dest_id ORDER BY t.date DESC,t.id DESC LIMIT 100",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("cases".into(), Value::Array(cases));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("truies".into(), Value::Array(sows));
    ctx.insert("historique".into(), Value::Array(history));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "transferts.html", Value::Object(ctx))
}

/// Effectif d'une case, sans plancher à zéro (`case_pig_count` applique ce
/// plancher pour l'affichage normal). Une valeur négative signale un
/// inventaire ou des mortalités incohérents ; voir aussi la même logique
/// répliquée en SQL pur dans `etat_donnees` (« Cases avec effectif calculé
/// négatif ») pour le rapport de contrôles structurels.
async fn case_pig_count_raw(pool: &SqlitePool, case_id: i64) -> AppResult<i64> {
    let inventory: Option<(String, i64)> = sqlx::query_as(
        "SELECT date,nombre FROM inventairecase WHERE case_id=? ORDER BY date DESC,id DESC LIMIT 1",
    )
    .bind(case_id)
    .fetch_optional(pool)
    .await?;
    let (date, base) = inventory
        .map(|(date, number)| (Some(date), number))
        .unwrap_or((None, 0));
    let movements: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(CASE WHEN case_dest_id=? THEN COALESCE(nombre,0) WHEN id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(nombre,0) END),0) AS INTEGER) FROM transfert WHERE espece='porc' AND (case_dest_id=? OR case_source_id=?) AND (? IS NULL OR date>?)",
    )
    .bind(case_id)
    .bind(case_id)
    .bind(case_id)
    .bind(&date)
    .bind(&date)
    .fetch_one(pool)
    .await?;
    let deaths: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM declarationmort WHERE case_id=? AND (? IS NULL OR date>?)",
    )
    .bind(case_id)
    .bind(&date)
    .bind(&date)
    .fetch_one(pool)
    .await?;
    Ok(base + movements - deaths)
}

async fn case_pig_count(pool: &SqlitePool, case_id: i64) -> AppResult<i64> {
    Ok(case_pig_count_raw(pool, case_id).await?.max(0))
}

/// Vivants des portées : naissances et adoptions reçues, moins les sorties et
/// les décès déclarés. Les portées sevrées ne sont plus présentes. La vue commune
/// évite de diverger entre Structure, Maternité et les contrôles de mouvements.
async fn case_litter_count(pool: &SqlitePool, case_id: i64) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT CAST(COALESCE((SELECT SUM(p.presents) FROM portee_effectif p JOIN evenement e ON e.id=p.id JOIN truie t ON t.id=p.truie_id WHERE COALESCE(e.case_id,t.case_id)=?1),0)+COALESCE((SELECT SUM(n.presents) FROM nourrice_effectif n WHERE n.case_nourrice_id=?1),0) AS INTEGER)",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

/// Effectif de la case à utiliser pour vérifier une sortie : l'effectif réel
/// de la portée pour une case de maternité (voir `case_litter_count`), sinon
/// `case_pig_count`. Centralise le choix pour ne pas le dupliquer à chaque
/// point de saisie qui vérifie un effectif de case source.
async fn case_departure_count(pool: &SqlitePool, case_id: i64) -> AppResult<i64> {
    if stade_from_case(pool, case_id).await?.as_deref() == Some("Maternité") {
        case_litter_count(pool, case_id).await
    } else {
        case_pig_count(pool, case_id).await
    }
}

#[cfg(test)]
mod transfert_maternite_tests {
    use super::*;
    use crate::config::Config;
    use minijinja::Environment;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_state() -> AppResult<(AppState, i64, i64)> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        let site = sqlx::query("INSERT INTO site(code,nom) VALUES('S1','Site test')")
            .execute(&pool)
            .await?
            .last_insert_rowid();
        let maternity_room = sqlx::query(
            "INSERT INTO salle(site_id,nom,type,nb_cases,ordre) VALUES(?,'Maternité 1','Maternité',1,1)",
        )
        .bind(site)
        .execute(&pool)
        .await?
        .last_insert_rowid();
        let maternity_pen = sqlx::query("INSERT INTO casesalle(salle_id,nom) VALUES(?,'Case 1')")
            .bind(maternity_room)
            .execute(&pool)
            .await?
            .last_insert_rowid();
        let dest_room = sqlx::query(
            "INSERT INTO salle(site_id,nom,type,nb_cases,ordre) VALUES(?,'Post-sevrage 1','Post-sevrage',1,2)",
        )
        .bind(site)
        .execute(&pool)
        .await?
        .last_insert_rowid();
        let dest_pen =
            sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'PS 1',100)")
                .bind(dest_room)
                .execute(&pool)
                .await?
                .last_insert_rowid();
        let band =
            sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('BTEST','2026-08-01',1)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        let sow = sqlx::query(
            "INSERT INTO truie(num_travail,bande_code,statut,reformee) VALUES('T1','BTEST','active',0)",
        )
        .execute(&pool)
        .await?
        .last_insert_rowid();
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,case_id,nes_vifs) VALUES('mise_bas','2026-08-01',?,?,?,10)")
            .bind(sow).bind(band).bind(maternity_pen).execute(&pool).await?;

        let config = Config {
            bind: "0.0.0.0:8080".parse().unwrap(),
            db_path: std::path::PathBuf::from("data/test.db"),
            secure_cookies: false,
        };
        let env = Environment::new();
        let state = AppState::new(config, pool, env);
        Ok((state, maternity_pen, dest_pen))
    }

    fn session() -> SessionData {
        SessionData {
            uid: 1,
            identifiant: "test".into(),
            nom: "Test".into(),
            role: "admin".into(),
            sections: vec![],
            csrf: "csrf-test".into(),
            doit_changer_mdp: false,
            type_elevage: "naisseur_engraisseur".into(),
            module_genetique: false,
            module_prestataires: true,
            module_charcutiers_rfid: false,
            module_vente_directe: true,
        }
    }

    fn movement_form(source_case: i64, dest_case: i64, nombre: &str) -> HashMap<String, String> {
        [
            ("csrf_token", "csrf-test"),
            ("source", &format!("case:{source_case}")),
            ("case_dest_id", &dest_case.to_string()),
            ("nombre", nombre),
            ("date", "2026-08-10"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
    }

    /// Vrai bug corrigé : déplacer des porcelets sous la mère depuis une
    /// case de maternité via l'écran générique « Mouvement »/`/transferts`
    /// (hors du flux dédié « Sevrage ») était systématiquement refusé —
    /// `case_pig_count` ne lit que `inventairecase`/`transfert`, jamais
    /// renseignés pour des porcelets sous la mère.
    #[tokio::test]
    async fn deplacer_des_porcelets_sous_la_mere_est_accepte() -> anyhow::Result<()> {
        let (state, maternity_pen, dest_pen) = test_state().await?;
        transferts_porcs(
            State(state.clone()),
            Extension(session()),
            Form(movement_form(maternity_pen, dest_pen, "4")),
        )
        .await?;
        let recorded: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(nombre),0) FROM transfert WHERE espece='porc'")
                .fetch_one(&state.pool)
                .await?;
        assert_eq!(recorded, 4);
        Ok(())
    }

    /// Toujours refuser un déplacement au-delà de la portée réellement
    /// présente, avec le vrai effectif plutôt qu'un compte de case figé à 0.
    #[tokio::test]
    async fn deplacer_plus_de_porcelets_que_la_portee_est_refuse() -> anyhow::Result<()> {
        let (state, maternity_pen, dest_pen) = test_state().await?;
        let err = transferts_porcs(
            State(state),
            Extension(session()),
            Form(movement_form(maternity_pen, dest_pen, "50")),
        )
        .await
        .expect_err("doit être refusé");
        match err {
            AppError::Invalid(message) => {
                assert!(message.contains("10 porc"), "message: {message}");
            }
            other => panic!("erreur inattendue: {other:?}"),
        }
        Ok(())
    }
}

async fn remaining_band_pigs(pool: &SqlitePool, band_id: i64, code: &str) -> AppResult<i64> {
    let stock_date: Option<String> = sqlx::query_scalar(
        "SELECT MAX(date) FROM mouvementstock WHERE est_stock=1 AND bande_code=?",
    )
    .bind(code)
    .fetch_one(pool)
    .await?;
    let Some(stock_date) = stock_date else {
        return Ok(0);
    };
    let base = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM mouvementstock WHERE est_stock=1 AND bande_code=? AND date=? AND lower(COALESCE(libelle,'')) NOT LIKE '%truie%' AND lower(COALESCE(libelle,'')) NOT LIKE '%pleine%' AND lower(COALESCE(libelle,'')) NOT LIKE '%lactation%'",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let deaths = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM declarationmort WHERE bande_code=? AND date>?",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let slaughter_deaths = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM porccharcutier WHERE bande_code=? AND date_mort IS NOT NULL AND date_mort>?",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let sold = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(CASE WHEN v.bande_id=? THEN COALESCE(v.nb_porcs,0) WHEN v.bande_id IS NULL AND json_type(CASE WHEN json_valid(v.lots_json) THEN v.lots_json ELSE 'null' END)='array' THEN (SELECT COALESCE(SUM(CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER)),0) FROM json_each(v.lots_json) j WHERE CAST(json_extract(j.value,'$.bande_id') AS INTEGER)=?) ELSE 0 END),0) AS INTEGER) FROM venteapport v WHERE date>?",
    )
    .bind(band_id)
    .bind(band_id)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let transferred = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM transfert WHERE espece='porc' AND bande_id=? AND date>?",
    )
    .bind(band_id)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    Ok((base - deaths - slaughter_deaths - sold - transferred).max(0))
}

async fn total_band_pigs(pool: &SqlitePool, band_id: i64, code: &str) -> AppResult<i64> {
    let unassigned = remaining_band_pigs(pool, band_id, code).await?;
    let stock_date: Option<String> = sqlx::query_scalar(
        "SELECT MAX(date) FROM mouvementstock WHERE est_stock=1 AND bande_code=?",
    )
    .bind(code)
    .fetch_one(pool)
    .await?;
    let Some(stock_date) = stock_date else {
        return Ok(unassigned);
    };
    let placed: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM transfert WHERE espece='porc' AND bande_id=? AND date>?",
    ).bind(band_id).bind(stock_date).fetch_one(pool).await?;
    Ok(unassigned + placed)
}

async fn transferts_porcs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let source =
        form_text(&form, "source").ok_or_else(|| AppError::Invalid("Source obligatoire".into()))?;
    let destination = form_i64(&form, "case_dest_id")
        .ok_or_else(|| AppError::Invalid("Case de destination obligatoire".into()))?;
    let number = form_i64(&form, "nombre")
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::Invalid("Nombre invalide".into()))?;
    let date = form_date_or_today(&form, "date")?;
    let destination_row = sqlx::query_as::<_, (i64, i64, Option<i64>)>(
        "SELECT id,salle_id,nb_max_porcs FROM casesalle WHERE id=?",
    )
    .bind(destination)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Invalid("Case de destination introuvable".into()))?;
    let present = case_pig_count(&state.pool, destination).await?;
    if let Some(capacity) = destination_row.2.filter(|value| *value > 0) {
        if present + number > capacity {
            return Err(AppError::Invalid(format!(
                "Capacité dépassée : {} place(s) disponible(s)",
                (capacity - present).max(0)
            )));
        }
    }
    let (kind, raw_id) = source
        .split_once(':')
        .ok_or_else(|| AppError::Invalid("Source invalide".into()))?;
    let source_id = raw_id
        .parse::<i64>()
        .map_err(|_| AppError::Invalid("Source invalide".into()))?;
    let mut band_id = None;
    let mut source_case = None;
    let mut source_room = None;
    match kind {
        "bande" => {
            let code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=? AND active=1")
                .bind(source_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| AppError::Invalid("Bande source introuvable".into()))?;
            let available = remaining_band_pigs(&state.pool, source_id, &code).await?;
            if number > available {
                return Err(AppError::Invalid(format!(
                    "Effectif insuffisant : {available} porc(s) disponible(s)"
                )));
            }
            band_id = Some(source_id);
        }
        "case" => {
            if source_id == destination {
                return Err(AppError::Invalid(
                    "La source et la destination sont identiques".into(),
                ));
            }
            let room: i64 = sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
                .bind(source_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| AppError::Invalid("Case source introuvable".into()))?;
            // Vrai bug corrigé ici : une case de maternité choisie comme
            // source d'un mouvement générique (écran « Mouvement » ou
            // /transferts) renvoyait toujours 0 via `case_pig_count`
            // (inventaire/transferts jamais renseignés pour des porcelets
            // sous la mère), bloquant tout déplacement hors du flux dédié
            // « Sevrage » — même défaut que celui corrigé en 2.2.38 pour la
            // saisie rapide « Perte ». `case_departure_count` bascule sur
            // l'effectif réel de la portée pour les cases de maternité.
            let available = case_departure_count(&state.pool, source_id).await?;
            if number > available {
                return Err(AppError::Invalid(format!(
                    "Effectif insuffisant : {available} porc(s) disponible(s)"
                )));
            }
            source_case = Some(source_id);
            source_room = Some(room);
        }
        _ => return Err(AppError::Invalid("Type de source invalide".into())),
    }
    sqlx::query("INSERT INTO transfert(date,espece,bande_id,salle_source_id,salle_dest_id,case_source_id,case_dest_id,nombre,note) VALUES(?,'porc',?,?,?,?,?,?,?)")
        .bind(&date)
        .bind(band_id)
        .bind(source_room)
        .bind(destination_row.1)
        .bind(source_case)
        .bind(destination)
        .bind(number)
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/transferts").into_response())
}

async fn transferts_truies(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let ids = form_selected_ids(&form, "truie_");
    if ids.is_empty() {
        return Err(AppError::Invalid("Sélectionne au moins une truie".into()));
    }
    let destination = form_i64(&form, "case_dest_id")
        .ok_or_else(|| AppError::Invalid("Case de destination obligatoire".into()))?;
    let destination_room: i64 = sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
        .bind(destination)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::Invalid("Case de destination introuvable".into()))?;
    let date = form_date_or_today(&form, "date")?;
    let mut tx = state.pool.begin().await?;
    for id in ids {
        let current = sqlx::query_as::<_, (Option<i64>, Option<i64>)>("SELECT t.case_id,COALESCE(t.salle_id,c.salle_id) FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id WHERE t.id=? AND t.reformee=0")
            .bind(id).fetch_optional(&mut *tx).await?;
        let Some((source_case, source_room)) = current else {
            continue;
        };
        if source_case == Some(destination) {
            continue;
        }
        sqlx::query(
            "UPDATE truie SET case_id=?,salle_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?",
        )
        .bind(destination)
        .bind(destination_room)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO transfert(date,espece,salle_source_id,salle_dest_id,case_source_id,case_dest_id,nombre,truie_id,note) VALUES(?,'truie',?,?,?,?,1,?,?)")
            .bind(&date).bind(source_room).bind(destination_room).bind(source_case).bind(destination).bind(id).bind(form_text(&form, "note")).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/transferts").into_response())
}

async fn transfert_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let movement = sqlx::query_as::<_, (String, Option<i64>, Option<i64>, Option<i64>)>(
        "SELECT espece,truie_id,case_source_id,salle_source_id FROM transfert WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    if let Some((species, sow_id, source_case, source_room)) = movement {
        let mut tx = state.pool.begin().await?;
        if species == "truie" {
            if let Some(sow_id) = sow_id {
                let latest: Option<i64> = sqlx::query_scalar(
                    "SELECT MAX(id) FROM transfert WHERE espece='truie' AND truie_id=?",
                )
                .bind(sow_id)
                .fetch_one(&mut *tx)
                .await?;
                if latest == Some(id) {
                    sqlx::query("UPDATE truie SET case_id=?,salle_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                        .bind(source_case).bind(source_room).bind(sow_id).execute(&mut *tx).await?;
                }
            }
        }
        sqlx::query("DELETE FROM transfert WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(Redirect::to("/transferts").into_response())
}

async fn effectifs() -> Redirect {
    Redirect::to("/structure")
}

async fn effectifs_inventaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let band_id =
        form_i64(&form, "bande_id").ok_or_else(|| AppError::Invalid("Bande obligatoire".into()))?;
    let code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(band_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::Invalid("Bande introuvable".into()))?;
    let number = form_i64(&form, "nombre")
        .filter(|value| *value >= 0)
        .ok_or_else(|| AppError::Invalid("Effectif invalide".into()))?;
    let date = form_date_or_today(&form, "date")?;
    let label = form_text(&form, "libelle").unwrap_or_else(|| "stock porcs".into());
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM mouvementstock WHERE est_stock=1 AND bande_code=? AND date=? AND lower(trim(COALESCE(libelle,'')))=lower(trim(?))")
        .bind(&code).bind(&date).bind(&label).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mouvementstock(code_ifip,date,bande_code,nombre,poids,montant,libelle,destination,type_saisie,est_stock) VALUES(NULL,?,?,?,?,?,?,NULL,'inventaire',1)")
        .bind(&date).bind(&code).bind(number).bind(form_f64(&form,"poids")).bind(form_f64(&form,"montant")).bind(&label).execute(&mut *tx).await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "inventorier",
        "bande",
        &format!("{code} · {date} · {label}"),
        "/effectifs/inventaire",
    )
    .await;
    Ok(Redirect::to("/structure").into_response())
}

async fn effectifs_inventaire_case(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let case_id =
        form_i64(&form, "case_id").ok_or_else(|| AppError::Invalid("Case obligatoire".into()))?;
    let number = form_i64(&form, "nombre")
        .filter(|value| *value >= 0)
        .ok_or_else(|| AppError::Invalid("Effectif invalide".into()))?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM casesalle WHERE id=?")
        .bind(case_id)
        .fetch_one(&state.pool)
        .await?;
    if exists == 0 {
        return Err(AppError::Invalid("Case introuvable".into()));
    }
    let date = form_date_or_today(&form, "date")?;
    let note = form_text(&form, "note");
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM inventairecase WHERE case_id=? AND date=?")
        .bind(case_id)
        .bind(&date)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,?,?,?,?)")
        .bind(case_id)
        .bind(&date)
        .bind(number)
        .bind(note)
        .bind(&session.identifiant)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "inventorier",
        "case",
        &format!("{case_id} · {date}"),
        "/effectifs/inventaire-case",
    )
    .await;
    let target = form_text(&form, "retour")
        .filter(|value| {
            value
                .strip_prefix("/bande/")
                .is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        })
        .unwrap_or_else(|| "/structure".into());
    Ok(Redirect::to(&target).into_response())
}

async fn etat_donnees(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if session.role == "salarie" {
        return Err(AppError::Forbidden);
    }
    list_page(
        &state,
        &session,
        "État des données",
        "Contrôles structurels en lecture seule. Une valeur à zéro signifie que le contrôle est conforme.",
        "WITH effectif_case AS (SELECT c.id,c.nb_max_porcs,(SELECT date FROM inventairecase WHERE case_id=c.id ORDER BY date DESC,id DESC LIMIT 1) AS inv_date,COALESCE((SELECT nombre FROM inventairecase WHERE case_id=c.id ORDER BY date DESC,id DESC LIMIT 1),0) AS base FROM casesalle c),effectif_case2 AS (SELECT e.id,e.nb_max_porcs,e.base+COALESCE((SELECT SUM(CASE WHEN t.case_dest_id=e.id THEN COALESCE(t.nombre,0) WHEN t.id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(t.nombre,0) END) FROM transfert t WHERE t.espece='porc' AND (t.case_dest_id=e.id OR t.case_source_id=e.id) AND (e.inv_date IS NULL OR t.date>e.inv_date)),0)-COALESCE((SELECT SUM(d.nombre) FROM declarationmort d WHERE d.case_id=e.id AND (e.inv_date IS NULL OR d.date>e.inv_date)),0) AS effectif FROM effectif_case e) SELECT 'Doublons de numéros de truies actives' AS controle,COUNT(*) AS anomalies FROM (SELECT num_travail FROM truie WHERE reformee=0 GROUP BY lower(trim(num_travail)) HAVING COUNT(*)>1) UNION ALL SELECT 'Événements sans truie',COUNT(*) FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.truie_id IS NOT NULL AND t.id IS NULL UNION ALL SELECT 'Événements sans bande',COUNT(*) FROM evenement e LEFT JOIN bande b ON b.id=e.bande_id WHERE e.bande_id IS NOT NULL AND b.id IS NULL UNION ALL SELECT 'Transferts vers une case absente',COUNT(*) FROM transfert t LEFT JOIN casesalle c ON c.id=t.case_dest_id WHERE t.case_dest_id IS NOT NULL AND c.id IS NULL UNION ALL SELECT 'Bandes actives sans date de mise-bas',COUNT(*) FROM bande WHERE active=1 AND (date_mb IS NULL OR trim(date_mb)='') UNION ALL SELECT 'Truies actives sans bande',COUNT(*) FROM truie WHERE reformee=0 AND (bande_code IS NULL OR trim(bande_code)='') UNION ALL SELECT 'Cases avec effectif calculé négatif',COUNT(*) FROM effectif_case2 WHERE effectif<0 UNION ALL SELECT 'Cases dépassant leur capacité déclarée',COUNT(*) FROM effectif_case2 WHERE nb_max_porcs IS NOT NULL AND nb_max_porcs>0 AND effectif>nb_max_porcs UNION ALL SELECT 'Déclarations de mortalité sans stade renseigné',COUNT(*) FROM declarationmort WHERE stade IS NULL OR trim(stade)='' UNION ALL SELECT 'Porcs charcutiers sans bande d''origine',COUNT(*) FROM porccharcutier WHERE bande_code IS NULL OR trim(bande_code)=''",
        &["controle", "anomalies"],
    )
    .await
}

async fn energie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let meters = sqlx::query_as::<_,CompteurEnergie>("SELECT id,nom,type,site_id,unite,rappel_jours,actif,note FROM compteur_energie WHERE actif=1 ORDER BY type,nom")
        .fetch_all(&state.pool).await?;
    let sites = generic_rows(
        &state.pool,
        "SELECT id,code,nom,zone FROM site ORDER BY COALESCE(nom,code),zone",
    )
    .await?;
    let mut data = Vec::new();
    // Coût redistribué aux bandes selon leur présence (§ « aliment et
    // stock » des demandes en attente) : clé (bande, type de compteur) →
    // total. Une seule table cumulée pour eau et électricité, distinguées
    // par colonne à l'affichage.
    let mut cout_bandes: HashMap<(String, String), f64> = HashMap::new();
    for meter in &meters {
        let mut readings = sqlx::query_as::<_,ReleveCompteur>("SELECT id,compteur_id,date_releve,valeur_index,bandes,note,remplacement_compteur,prix_unitaire FROM releve_compteur WHERE compteur_id=? ORDER BY date_releve,id")
            .bind(meter.id).fetch_all(&state.pool).await?;
        let mut previous: Option<f64> = None;
        let mut enriched = Vec::new();
        for reading in &readings {
            let consumption = if reading.remplacement_compteur {
                None
            } else {
                previous.map(|value| reading.valeur_index - value)
            };
            previous = Some(reading.valeur_index);
            let cost = cout_consommation(consumption, reading.prix_unitaire);
            if let Some(cost) = cost {
                let bandes: Vec<String> = reading
                    .bandes
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect();
                for (bande, part) in repartir_cout_par_bande(cost, &bandes) {
                    *cout_bandes
                        .entry((bande, meter.r#type.clone()))
                        .or_insert(0.0) += part;
                }
            }
            let mut value = serde_json::to_value(reading).unwrap_or_default();
            let object = json_object_mut(&mut value, "les relevés d’énergie")?;
            object.insert("conso".into(), json!(consumption));
            object.insert(
                "cout".into(),
                json!(cost.map(|value| (value * 100.0).round() / 100.0)),
            );
            enriched.push(value);
        }
        enriched.reverse();
        readings.reverse();
        let last_date = readings
            .first()
            .and_then(|reading| parse_stored_date(&reading.date_releve));
        let due = if meter.r#type == "eau" {
            meter.rappel_jours.map(|days| {
                last_date.unwrap_or(Local::now().date_naive() - Duration::days(days))
                    + Duration::days(days)
            })
        } else {
            None
        };
        let overdue = due.map(|date| (Local::now().date_naive() - date).num_days().max(0));
        let site: Option<String> = if let Some(id) = meter.site_id {
            sqlx::query_scalar("SELECT COALESCE(nom,code) FROM site WHERE id=?")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
        } else {
            None
        };
        let alert = due.is_some_and(|date| date <= Local::now().date_naive());
        data.push(json!({"compteur":meter,"releves":enriched,"site":site,"alerte":alert,"jours_retard":overdue}));
    }
    let mut cout_bandes_vec: Vec<Value> = cout_bandes
        .into_iter()
        .map(|((bande, type_compteur), montant)| json!({"bande": bande, "type": type_compteur, "montant": (montant*100.0).round()/100.0}))
        .collect();
    cout_bandes_vec.sort_by(|a, b| {
        a.get("bande")
            .and_then(Value::as_str)
            .cmp(&b.get("bande").and_then(Value::as_str))
            .then(
                a.get("type")
                    .and_then(Value::as_str)
                    .cmp(&b.get("type").and_then(Value::as_str)),
            )
    });
    let mut ctx = context(&session);
    ctx.insert(
        "compteurs".into(),
        serde_json::to_value(meters).unwrap_or_default(),
    );
    ctx.insert("sites".into(), Value::Array(sites));
    ctx.insert("data".into(), Value::Array(data));
    ctx.insert("cout_bandes".into(), Value::Array(cout_bandes_vec));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "energie.html", Value::Object(ctx))
}

async fn energie_compteur(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name =
        form_text(&form, "nom").ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let kind = if form.get("type").map(String::as_str) == Some("electricite") {
        "electricite"
    } else {
        "eau"
    };
    let unit = if kind == "electricite" { "kWh" } else { "m³" };
    sqlx::query("INSERT INTO compteur_energie(nom,type,site_id,unite,rappel_jours,actif,note) SELECT ?,?,?,?,?,1,? WHERE NOT EXISTS(SELECT 1 FROM compteur_energie WHERE actif=1 AND type=? AND lower(trim(nom))=lower(trim(?)) AND COALESCE(site_id,-1)=COALESCE(?,-1))").bind(&name).bind(kind).bind(form_i64(&form,"site_id")).bind(unit).bind(if kind=="eau"{form_i64(&form,"rappel_jours")}else{None}).bind(form_text(&form,"note")).bind(kind).bind(&name).bind(form_i64(&form,"site_id")).execute(&state.pool).await?;
    Ok(Redirect::to("/energie").into_response())
}

async fn energie_releve(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let meter_id = form_i64(&form, "compteur_id")
        .ok_or_else(|| AppError::Invalid("Compteur obligatoire".into()))?;
    let date = form_date_or_today(&form, "date_releve")?;
    let index =
        form_f64(&form, "index").ok_or_else(|| AppError::Invalid("Index invalide".into()))?;
    let replacement = form.contains_key("remplacement_compteur");
    let prix_unitaire = form_f64(&form, "prix_unitaire").filter(|value| *value >= 0.0);
    let bands = ameliorations::bandes_releve(&state.pool, meter_id, &date, &form).await?;
    sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index,bandes,note,remplacement_compteur,prix_unitaire) VALUES(?,?,?,?,?,?,?)").bind(meter_id).bind(&date).bind(index).bind(bands).bind(form_text(&form,"note").or_else(||replacement.then(||"Compteur remplacé – nouvel index de départ".into()))).bind(replacement).bind(prix_unitaire).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/energie#compteur-{meter_id}")).into_response())
}

async fn present_bands(pool: &SqlitePool, site: Option<&str>, day: &str) -> AppResult<Vec<String>> {
    let Some(day) =
        parse_iso_date(day).and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok())
    else {
        return Ok(vec![]);
    };
    let rows = generic_rows(
        pool,
        "SELECT b.code,b.date_mb,COALESCE((SELECT s.code FROM site s WHERE lower(trim(b.site)) IN (lower(trim(s.code)),lower(trim(COALESCE(s.nom,'')))) LIMIT 1),b.site) AS site FROM bande b WHERE date_mb IS NOT NULL",
    )
    .await?;
    let mut out = Vec::new();
    for row in rows {
        let Some(obj) = row.as_object() else {
            continue;
        };
        let code = obj.get("code").and_then(Value::as_str).unwrap_or("");
        let mb = obj
            .get("date_mb")
            .and_then(Value::as_str)
            .and_then(parse_iso_date)
            .and_then(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d").ok());
        let row_site = obj.get("site").and_then(Value::as_str);
        if let Some(mb) = mb {
            if site.is_some() && site != row_site {
                continue;
            }
            if day >= mb - Duration::days(115)
                && day <= mb + Duration::days(225)
                && !code.is_empty()
            {
                out.push(code.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Coût d'un relevé (§ « redistribuer aux bandes selon leur présence » des
/// demandes en attente) : la conso ne peut se calculer qu'entre deux
/// relevés (voir `energie()`), le tarif est saisi sur le relevé le plus
/// récent des deux (celui qui ferme la période) — c'est lui qui reflète le
/// prix payé pour cette consommation.
fn cout_consommation(consommation: Option<f64>, prix_unitaire: Option<f64>) -> Option<f64> {
    match (consommation, prix_unitaire) {
        (Some(conso), Some(prix)) if conso > 0.0 => Some(conso * prix),
        _ => None,
    }
}

/// Répartit un coût à parts égales entre les bandes présentes sur la
/// période du relevé (`present_bands`). Choix assumé : à parts égales, pas
/// au prorata d'un effectif — l'eau/l'électricité d'un site se consomme
/// pour l'essentiel indépendamment du nombre de têtes par bande (lavage,
/// ventilation, chauffage communs), contrairement à l'aliment qui, lui, se
/// répartit déjà naturellement par les livraisons rattachées à une bande.
fn repartir_cout_par_bande(cout: f64, bandes: &[String]) -> Vec<(String, f64)> {
    if bandes.is_empty() || cout == 0.0 {
        return Vec::new();
    }
    let part = (cout / bandes.len() as f64 * 100.0).round() / 100.0;
    bandes.iter().map(|bande| (bande.clone(), part)).collect()
}

async fn energie_rappel(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE compteur_energie SET rappel_jours=? WHERE id=? AND type='eau'")
        .bind(form_i64(&form, "rappel_jours"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/energie#compteur-{id}")).into_response())
}
async fn energie_releve_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let meter: Option<i64> =
        sqlx::query_scalar("SELECT compteur_id FROM releve_compteur WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    sqlx::query("DELETE FROM releve_compteur WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(
        &meter
            .map(|x| format!("/energie#compteur-{x}"))
            .unwrap_or_else(|| "/energie".into()),
    )
    .into_response())
}

async fn energie_modele_csv() -> Response {
    let body="\u{feff}type_compteur;nom_compteur;site;date_releve;index;unite;rappel_jours;remplacement_compteur;prix_unitaire;note\r\neau;Compteur général;Site principal;2026-08-16;12345,6;m³;7;;1,45;Relevé hebdomadaire\r\n";
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=modele_import_eau_electricite.csv"),
    );
    (headers, body).into_response()
}

async fn energie_import(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let mut data = None;
    let mut filename = "import-energie.csv".to_string();
    let mut csrf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        match field.name() {
            Some("fichier") => {
                filename = field
                    .file_name()
                    .unwrap_or("import-energie.csv")
                    .chars()
                    .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
                    .take(180)
                    .collect();
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            Some("csrf_token") => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            _ => {}
        }
    }
    let mut csrf_form = HashMap::new();
    csrf_form.insert("csrf_token".to_string(), csrf.unwrap_or_default());
    verify_csrf(&session, &csrf_form)?;

    let bytes = data.ok_or_else(|| AppError::Invalid("Fichier manquant".into()))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(AppError::Invalid("Fichier trop volumineux".into()));
    }
    let digest = contenu_sha256(&bytes);
    let delimiter = if bytes.iter().take(512).filter(|&&byte| byte == b';').count()
        > bytes.iter().take(512).filter(|&&byte| byte == b',').count()
    {
        b';'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_reader(bytes.as_ref());
    let headers = reader
        .headers()
        .map_err(|error| AppError::Invalid(error.to_string()))?
        .clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| AppError::Invalid(error.to_string()))?;
        rows.push(
            headers
                .iter()
                .zip(record.iter())
                .map(|(key, value)| (key.trim().to_lowercase(), value.trim().to_string()))
                .collect::<HashMap<_, _>>(),
        );
    }

    let mut transaction = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut transaction, &digest).await?;
    let import_token = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256,applique_le) VALUES(?,'energie',?,'applique',?,?,CURRENT_TIMESTAMP)")
        .bind(&import_token)
        .bind(&filename)
        .bind(session.uid)
        .bind(&digest)
        .execute(&mut *transaction)
        .await?;
    let mut added = 0;
    for row in rows {
        let kind = match row.get("type_compteur").map(|value| value.to_lowercase()) {
            Some(value) if value.contains("elect") || value.contains("élect") => "electricite",
            Some(value) if value == "eau" => "eau",
            None => "eau",
            _ => return Err(AppError::Invalid("type_compteur invalide".into())),
        };
        let name = row
            .get("nom_compteur")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Invalid("nom_compteur manquant".into()))?;
        let date_raw = row
            .get("date_releve")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Invalid("date_releve manquante".into()))?;
        let date = parse_iso_date(date_raw)
            .ok_or_else(|| AppError::Invalid(format!("date_releve invalide : {date_raw}")))?;
        let index = row
            .get("index")
            .and_then(|value| value.replace(',', ".").parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| AppError::Invalid("index invalide".into()))?;

        let site_id = if let Some(site) = row.get("site").filter(|value| !value.is_empty()) {
            let mut site_key = site.to_lowercase().replace("berrue", "berue");
            site_key.retain(|character| !character.is_whitespace() && character != '-');
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM site WHERE replace(replace(replace(lower(COALESCE(code,'')),'berrue','berue'),' ',''),'-','')=? OR replace(replace(replace(lower(COALESCE(nom,'')),'berrue','berue'),' ',''),'-','')=? LIMIT 1",
            )
            .bind(&site_key)
            .bind(&site_key)
            .fetch_optional(&mut *transaction)
            .await?;
            match existing {
                Some(id) => Some(id),
                None => Some(
                    sqlx::query("INSERT INTO site(code,nom) VALUES(?,?)")
                        .bind(site)
                        .bind(site)
                        .execute(&mut *transaction)
                        .await?
                        .last_insert_rowid(),
                ),
            }
        } else {
            None
        };
        let meter: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM compteur_energie WHERE type=? AND lower(trim(nom))=lower(trim(?)) AND COALESCE(site_id,-1)=COALESCE(?,-1) LIMIT 1",
        )
        .bind(kind)
        .bind(name)
        .bind(site_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let meter_id = match meter {
            Some(id) => id,
            None => sqlx::query("INSERT INTO compteur_energie(nom,type,site_id,unite,rappel_jours,actif) VALUES(?,?,?,?,?,1)")
                .bind(name)
                .bind(kind)
                .bind(site_id)
                .bind(row.get("unite").filter(|value| !value.is_empty()).cloned().unwrap_or_else(|| if kind == "electricite" { "kWh".into() } else { "m³".into() }))
                .bind(row.get("rappel_jours").and_then(|value| value.parse::<i64>().ok()))
                .execute(&mut *transaction)
                .await?
                .last_insert_rowid(),
        };
        let duplicate: Option<f64> = sqlx::query_scalar(
            "SELECT valeur_index FROM releve_compteur WHERE compteur_id=? AND date_releve=? LIMIT 1",
        )
        .bind(meter_id)
        .bind(&date)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(existing) = duplicate {
            return Err(AppError::Invalid(format!(
                "Relevé déjà présent pour {name} le {date} (index existant {existing}, fichier {index})"
            )));
        } else {
            // Même auto-tagging des bandes présentes que la saisie manuelle
            // (`energie_releve`) — sinon un relevé importé en masse ne
            // contribuerait jamais à la répartition du coût par bande.
            let site_code: Option<String> = if let Some(id) = site_id {
                sqlx::query_scalar("SELECT COALESCE(code,nom) FROM site WHERE id=?")
                    .bind(id)
                    .fetch_optional(&mut *transaction)
                    .await?
            } else {
                None
            };
            let bands = present_bands(&state.pool, site_code.as_deref(), &date).await?;
            let prix_unitaire = row
                .get("prix_unitaire")
                .and_then(|value| value.replace(',', ".").parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0);
            sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index,bandes,note,remplacement_compteur,prix_unitaire) VALUES(?,?,?,?,?,?,?)")
                .bind(meter_id)
                .bind(&date)
                .bind(index)
                .bind(if bands.is_empty() { None } else { Some(bands.join(",")) })
                .bind(row.get("note"))
                .bind(matches!(row.get("remplacement_compteur").map(|value| value.to_lowercase()).as_deref(), Some("oui" | "1" | "true" | "x")))
                .bind(prix_unitaire)
                .execute(&mut *transaction)
                .await?;
            added += 1;
        }
    }
    transaction.commit().await?;
    Ok(Redirect::to(&format!("/energie?import_ok={added}")).into_response())
}

async fn economique(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let ventes_total: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
    let aliment: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM livraisonaliment",
    )
    .fetch_one(&state.pool)
    .await?;
    let veto: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatveto")
            .fetch_one(&state.pool)
            .await?;
    let semence: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatsemence")
            .fetch_one(&state.pool)
            .await?;
    let genetique: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatgenetique")
            .fetch_one(&state.pool)
            .await?;
    let bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site FROM bande ORDER BY active DESC,date_mb IS NULL,date_mb,id",
    )
    .await?;
    let band_results = generic_rows(
        &state.pool,
        "WITH ventes AS (SELECT bande_id,SUM(COALESCE(nb_porcs,0)) AS porcs,SUM(COALESCE(poids_total,0)) AS poids,SUM(COALESCE(montant_ht,0)) AS recettes FROM ventelot GROUP BY bande_id),nb AS (SELECT categorie,facture_id,COUNT(*) AS n FROM affectationfacturebande GROUP BY categorie,facture_id),aliment AS (SELECT af.bande_id,SUM(COALESCE(x.montant_ht,0)/nb.n) AS cout FROM livraisonaliment x JOIN affectationfacturebande af ON af.categorie='aliment' AND af.facture_id=x.id JOIN nb ON nb.categorie=af.categorie AND nb.facture_id=af.facture_id GROUP BY af.bande_id),veto AS (SELECT af.bande_id,SUM(COALESCE(x.montant_ht,0)/nb.n) AS cout FROM achatveto x JOIN affectationfacturebande af ON af.categorie='veto' AND af.facture_id=x.id JOIN nb ON nb.categorie=af.categorie AND nb.facture_id=af.facture_id GROUP BY af.bande_id),semence AS (SELECT af.bande_id,SUM(COALESCE(x.montant_ht,0)/nb.n) AS cout FROM achatsemence x JOIN affectationfacturebande af ON af.categorie='semence' AND af.facture_id=x.id JOIN nb ON nb.categorie=af.categorie AND nb.facture_id=af.facture_id GROUP BY af.bande_id),genetique AS (SELECT af.bande_id,SUM(COALESCE(x.montant_ht,0)/nb.n) AS cout FROM achatgenetique x JOIN affectationfacturebande af ON af.categorie='genetique' AND af.facture_id=x.id JOIN nb ON nb.categorie=af.categorie AND nb.facture_id=af.facture_id GROUP BY af.bande_id) SELECT b.id,b.code,b.site,CAST(COALESCE(v.porcs,0) AS INTEGER) AS porcs,ROUND(COALESCE(v.poids,0),1) AS poids,ROUND(COALESCE(v.recettes,0),2) AS recettes,ROUND(COALESCE(a.cout,0),2) AS aliment,ROUND(COALESCE(vt.cout,0),2) AS veto,ROUND(COALESCE(se.cout,0),2) AS semence,ROUND(COALESCE(g.cout,0),2) AS genetique,ROUND(COALESCE(v.recettes,0)-COALESCE(a.cout,0)-COALESCE(vt.cout,0)-COALESCE(se.cout,0)-COALESCE(g.cout,0),2) AS marge,ROUND((COALESCE(a.cout,0)+COALESCE(vt.cout,0)+COALESCE(se.cout,0)+COALESCE(g.cout,0))/NULLIF(v.porcs,0),2) AS cout_par_porc,ROUND(COALESCE(v.recettes,0)/NULLIF(v.poids,0),3) AS prix_ht_kg FROM bande b LEFT JOIN ventes v ON v.bande_id=b.id LEFT JOIN aliment a ON a.bande_id=b.id LEFT JOIN veto vt ON vt.bande_id=b.id LEFT JOIN semence se ON se.bande_id=b.id LEFT JOIN genetique g ON g.bande_id=b.id WHERE v.porcs IS NOT NULL OR a.cout IS NOT NULL OR vt.cout IS NOT NULL OR se.cout IS NOT NULL OR g.cout IS NOT NULL ORDER BY b.date_mb IS NULL,b.date_mb,b.id",
    )
    .await?;
    let ventes = ventes::rows(&state.pool).await?;
    let achats = generic_rows(&state.pool,"SELECT id,date,'aliment' AS categorie,produit AS libelle,tonnage AS quantite,montant_ht FROM livraisonaliment UNION ALL SELECT id,date,'vétérinaire',produit,quantite,montant_ht FROM achatveto UNION ALL SELECT id,date,'semence',designation,nb_doses,montant_ht FROM achatsemence UNION ALL SELECT id,date,'génétique',designation,nb_animaux,montant_ht FROM achatgenetique ORDER BY date DESC,id DESC LIMIT 50").await?;
    let genetiques = generic_rows(&state.pool,"SELECT a.id,a.date,a.toutes_bandes,a.num_facture,a.fournisseur,a.designation,a.nb_animaux,a.poids_total,a.montant_ht,CASE WHEN a.montant_ht IS NULL THEN 1 ELSE 0 END AS ht_manquant,COALESCE((SELECT GROUP_CONCAT(b.code,', ') FROM affectationfacturebande af JOIN bande b ON b.id=af.bande_id WHERE af.categorie='genetique' AND af.facture_id=a.id),'Non affecté') AS bandes_affectees,COALESCE((SELECT GROUP_CONCAT(af.bande_id) FROM affectationfacturebande af WHERE af.categorie='genetique' AND af.facture_id=a.id),'') AS bandes_ids,EXISTS(SELECT 1 FROM affectationfacturebande af WHERE af.categorie='genetique' AND af.facture_id=a.id AND af.automatique=1) AS affectation_auto FROM achatgenetique a ORDER BY a.date DESC,a.id DESC LIMIT 250").await?;
    let mut aliments = generic_rows(&state.pool,"SELECT a.id,a.date,a.stade_aliment,COALESCE(NULLIF(trim(a.date_reference),''),trim(a.date)) AS date_reference,a.site,a.sites_json,replace(replace(a.sites_json,'[',''),']','') AS sites_ids,a.num_facture,a.fournisseur,a.produit,a.tonnage,a.montant_ht,COALESCE((SELECT GROUP_CONCAT(b.code,', ') FROM affectationfacturebande af JOIN bande b ON b.id=af.bande_id WHERE af.categorie='aliment' AND af.facture_id=a.id),'Non affecté') AS bandes_affectees,COALESCE((SELECT GROUP_CONCAT(af.bande_id) FROM affectationfacturebande af WHERE af.categorie='aliment' AND af.facture_id=a.id),'') AS bandes_ids,EXISTS(SELECT 1 FROM affectationfacturebande af WHERE af.categorie='aliment' AND af.facture_id=a.id AND af.automatique=1) AS affectation_auto FROM livraisonaliment a ORDER BY a.date DESC,a.id DESC LIMIT 250").await?;
    let mut veterinaires = generic_rows(&state.pool,"SELECT a.id,a.date,COALESCE(NULLIF(trim(a.date_reference),''),trim(a.date)) AS date_reference,a.site,a.sites_json,replace(replace(a.sites_json,'[',''),']','') AS sites_ids,a.num_facture,a.fournisseur,a.produit,a.quantite,a.montant_ht,COALESCE((SELECT GROUP_CONCAT(b.code,', ') FROM affectationfacturebande af JOIN bande b ON b.id=af.bande_id WHERE af.categorie='veto' AND af.facture_id=a.id),'Non affecté') AS bandes_affectees,COALESCE((SELECT GROUP_CONCAT(af.bande_id) FROM affectationfacturebande af WHERE af.categorie='veto' AND af.facture_id=a.id),'') AS bandes_ids,EXISTS(SELECT 1 FROM affectationfacturebande af WHERE af.categorie='veto' AND af.facture_id=a.id AND af.automatique=1) AS affectation_auto FROM achatveto a ORDER BY a.date DESC,a.id DESC LIMIT 250").await?;
    crate::affectation::explain_unassigned(&state.pool, "aliment", &mut aliments).await?;
    crate::affectation::explain_unassigned(&state.pool, "veto", &mut veterinaires).await?;
    let semences = generic_rows(&state.pool,"SELECT a.id,a.date,a.num_facture,a.fournisseur,a.designation,a.nb_doses,a.montant_ht,COALESCE((SELECT GROUP_CONCAT(b.code,', ') FROM affectationfacturebande af JOIN bande b ON b.id=af.bande_id WHERE af.categorie='semence' AND af.facture_id=a.id),'Non affecté') AS bandes_affectees,COALESCE((SELECT GROUP_CONCAT(af.bande_id) FROM affectationfacturebande af WHERE af.categorie='semence' AND af.facture_id=a.id),'') AS bandes_ids,EXISTS(SELECT 1 FROM affectationfacturebande af WHERE af.categorie='semence' AND af.facture_id=a.id AND af.automatique=1) AS affectation_auto FROM achatsemence a ORDER BY a.date DESC,a.id DESC LIMIT 250").await?;
    let valuations = generic_rows(&state.pool,"SELECT id,num_apport,date,libelle,montant,categorie,CASE WHEN lower(COALESCE(categorie,''))='retenue' THEN 1 ELSE 0 END AS est_retenue FROM valorisationapport ORDER BY date DESC,id DESC LIMIT 200").await?;
    let monthly = generic_rows(&state.pool,"WITH RECURSIVE mois(m) AS (SELECT date('now','start of month',CASE WHEN EXISTS(SELECT 1 FROM parametre WHERE cle='demo_portal' AND valeur='1') THEN '-60 months' ELSE '-11 months' END) UNION ALL SELECT date(m,'+1 month') FROM mois WHERE m<date('now','start of month')),depenses AS (SELECT substr(date,1,7) AS m,SUM(COALESCE(montant_ht,0)) AS montant FROM livraisonaliment GROUP BY m UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_ht,0)) FROM achatveto GROUP BY substr(date,1,7) UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_ht,0)) FROM achatsemence GROUP BY substr(date,1,7) UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_ht,0)) FROM achatgenetique GROUP BY substr(date,1,7)),revenus AS (SELECT substr(date,1,7) AS m,SUM(COALESCE(montant_ht,0)) AS montant,SUM(COALESCE(poids_total,0)) AS poids FROM venteapport GROUP BY m) SELECT substr(m.m,1,7) AS mois,ROUND(COALESCE((SELECT SUM(d.montant) FROM depenses d WHERE d.m=substr(m.m,1,7)),0),2) AS depenses,ROUND(COALESCE(r.montant,0),2) AS revenus,ROUND(r.montant/NULLIF(r.poids,0),3) AS prix_ht_kg FROM mois m LEFT JOIN revenus r ON r.m=substr(m.m,1,7) ORDER BY m.m").await?;
    let unallocated = generic_rows(&state.pool,"SELECT 'Aliment' AS categorie,ROUND(COALESCE(SUM(montant_ht),0),2) AS montant FROM livraisonaliment x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie='aliment' AND a.facture_id=x.id) UNION ALL SELECT 'Vétérinaire',ROUND(COALESCE(SUM(montant_ht),0),2) FROM achatveto x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie='veto' AND a.facture_id=x.id) UNION ALL SELECT 'Semence',ROUND(COALESCE(SUM(montant_ht),0),2) FROM achatsemence x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie='semence' AND a.facture_id=x.id) UNION ALL SELECT 'Génétique',ROUND(COALESCE(SUM(COALESCE(montant_ht,0)),0),2) FROM achatgenetique x WHERE NOT EXISTS(SELECT 1 FROM affectationfacturebande a WHERE a.categorie='genetique' AND a.facture_id=x.id)").await?;
    let imports = generic_rows(&state.pool,"SELECT token,replace(type_import,'economique:','') AS type_import,nom_fichier,statut,cree_le,applique_le FROM importjournal WHERE type_import LIKE 'economique:%' ORDER BY cree_le DESC LIMIT 15").await?;
    let total_weight: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(poids_total),0) AS REAL) FROM venteapport WHERE montant_ht IS NOT NULL",
    )
            .fetch_one(&state.pool)
            .await?;
    let total_pigs: i64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
    // Cahiers des charges (§3) : intégrés à Économie plutôt que sur une page
    // séparée (/cahiers, orpheline de toute navigation) — voir cahiers().
    let cahiers = generic_rows(
        &state.pool,
        "SELECT id,nom,valeur_par_porc,actif,note FROM cahiercharges ORDER BY actif DESC,nom",
    )
    .await?;
    let cahiers_reels = generic_rows(
        &state.pool,
        "SELECT libelle,ROUND(SUM(montant),2) AS montant,COUNT(DISTINCT num_apport) AS apports FROM valorisationapport WHERE COALESCE(categorie,'valorisation')<>'retenue' GROUP BY libelle ORDER BY montant DESC",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert(
        "sites".into(),
        json!(generic_rows(&state.pool, "SELECT id,code,nom FROM site ORDER BY code").await?),
    );
    ctx.insert("cahiers".into(), Value::Array(cahiers));
    ctx.insert("cahiers_reels".into(), Value::Array(cahiers_reels));
    ctx.insert("totaux".into(),json!({"ventes":ventes_total,"aliment":aliment,"veto":veto,"semence":semence,"genetique":genetique,"marge":ventes_total-aliment-veto-semence-genetique,"porcs":total_pigs,"prix_ht_kg":if total_weight>0.0{Some(ventes_total/total_weight)}else{None}}));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("resultats_bandes".into(), Value::Array(band_results));
    ctx.insert("ventes".into(), Value::Array(ventes));
    ctx.insert("achats".into(), Value::Array(achats));
    ctx.insert("genetiques".into(), Value::Array(genetiques));
    ctx.insert("aliments".into(), Value::Array(aliments));
    ctx.insert("veterinaires".into(), Value::Array(veterinaires));
    ctx.insert("semences".into(), Value::Array(semences));
    ctx.insert(
        "secteur".into(),
        json!(query
            .get("secteur")
            .map(String::as_str)
            .unwrap_or("synthese")),
    );
    ctx.insert("valorisations".into(), Value::Array(valuations));
    ctx.insert("mensuel".into(), Value::Array(monthly));
    ctx.insert("non_affectes".into(), Value::Array(unallocated));
    ctx.insert("imports_pdf".into(), Value::Array(imports));
    ctx.insert("import_ok".into(), json!(query.get("import_ok")));
    ctx.insert("liaisons".into(), json!(query.get("liaisons")));
    ctx.insert("imports_prets".into(), json!(query.get("imports_prets")));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "economique.html", Value::Object(ctx))
}

// --- GTE (Gestion Technico-Économique, §7 de la spécification) ---
//
// Fonctions pures, testées indépendamment de la base : le calcul dépend des
// phases réellement présentes pour le type d'élevage actif (voir
// SessionData::a_truies/engraisse), pas d'un unique parcours naisseur-
// engraisseur.

/// Indice de consommation : kg d'aliment consommés pour produire 1 kg de
/// porc. `None` si aucun poids produit n'est connu (pas de division par 0).
fn indice_consommation(aliment_kg: f64, poids_produit_kg: f64) -> Option<f64> {
    (poids_produit_kg > 0.0).then(|| aliment_kg / poids_produit_kg)
}

/// Coût alimentaire par porc produit.
fn cout_alimentaire_par_porc(cout_aliment: f64, effectif_produit: i64) -> Option<f64> {
    (effectif_produit > 0).then(|| cout_aliment / effectif_produit as f64)
}

/// Marge sur coût alimentaire (MSA) : recettes moins le seul coût aliment
/// (à distinguer de la marge nette, qui inclut aussi vétérinaire/génétique).
fn marge_sur_cout_alimentaire(recettes: f64, cout_aliment: f64) -> f64 {
    recettes - cout_aliment
}

/// Coût d'achat des animaux entrés dans le lot, rapporté à un animal entré
/// (§1bis). `None` si aucune réception n'est enregistrée pour ce lot.
fn cout_achat_par_animal_entre(cout_achat: f64, effectif_entre: i64) -> Option<f64> {
    (effectif_entre > 0).then(|| cout_achat / effectif_entre as f64)
}

/// Marge après imputation du coût d'achat des animaux entrants : la MSA ne
/// retient que l'aliment, or un post-sevreur ou un engraisseur achète ses
/// animaux — cette charge d'entrée doit être déduite pour que la marge du lot
/// soit comparable à celle d'un naisseur-engraisseur, qui produit les siens.
/// Pour un lot sans réception d'achat, `cout_achat` vaut 0 et la valeur est
/// identique à la MSA (pas de régression pour les profils naisseurs).
fn marge_apres_cout_achat(msa: f64, cout_achat: f64) -> f64 {
    msa - cout_achat
}

/// Marge brute répartie par truie active du lot (non applicable si le lot n'a
/// pas de truies, ex. profil post-sevreur/engraisseur seul).
fn marge_brute_par_truie(marge_totale: f64, nb_truies: i64) -> Option<f64> {
    (nb_truies > 0).then(|| marge_totale / nb_truies as f64)
}

/// Taux de renouvellement du cheptel (%) : réformes rapportées à l'effectif
/// de truies actives sur la même période.
fn taux_renouvellement_pct(reformees_periode: i64, effectif_actif: i64) -> Option<f64> {
    (effectif_actif > 0).then(|| 100.0 * reformees_periode as f64 / effectif_actif as f64)
}

/// Requête des indicateurs GTE par lot. Isolée en constante pour être
/// rejouée telle quelle par les tests sur une base en mémoire : le calcul
/// des charges (aliment, achat d'animaux) est en SQL, pas seulement dans
/// les fonctions pures ci-dessus.
const GTE_LOTS_SQL: &str = "SELECT b.id,b.code,b.site,\
         CAST(COALESCE(v.porcs,0) AS INTEGER),CAST(COALESCE(v.poids,0) AS REAL),CAST(COALESCE(v.recettes,0) AS REAL),\
         CAST(COALESCE(a.tonnes,0) AS REAL),CAST(COALESCE(a.cout,0) AS REAL),\
         CAST(COALESCE(t.truies,0) AS INTEGER),\
         CAST(COALESCE(r.achat,0) AS REAL),CAST(COALESCE(r.entres,0) AS INTEGER) \
         FROM bande b \
         LEFT JOIN (SELECT bande_id,SUM(COALESCE(nb_porcs,0)) AS porcs,SUM(COALESCE(poids_total,0)) AS poids,SUM(COALESCE(montant_ht,0)) AS recettes FROM ventelot GROUP BY bande_id) v ON v.bande_id=b.id \
         LEFT JOIN (SELECT af.bande_id,SUM(COALESCE(l.tonnage,0)/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=l.id)) AS tonnes,SUM(COALESCE(l.montant_ht,0)/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=l.id)) AS cout FROM livraisonaliment l JOIN affectationfacturebande af ON af.categorie='aliment' AND af.facture_id=l.id GROUP BY af.bande_id) a ON a.bande_id=b.id \
         LEFT JOIN (SELECT bande_code,COUNT(*) AS truies FROM truie WHERE reformee=0 GROUP BY bande_code) t ON t.bande_code=b.code \
         LEFT JOIN (SELECT bande_code,SUM(COALESCE(prix_total,0)) AS achat,SUM(COALESCE(effectif,0)) AS entres FROM receptionachat WHERE COALESCE(trim(bande_code),'')<>'' GROUP BY bande_code) r ON r.bande_code=b.code \
         WHERE b.active=1 AND (v.porcs IS NOT NULL OR a.cout IS NOT NULL OR t.truies IS NOT NULL OR r.entres IS NOT NULL) \
         ORDER BY b.date_mb IS NULL,b.date_mb,b.id";

async fn gte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        i64,
        String,
        Option<String>,
        i64,
        f64,
        f64,
        f64,
        f64,
        i64,
        f64,
        i64,
    )> = sqlx::query_as(GTE_LOTS_SQL).fetch_all(&state.pool).await?;

    let bandes: Vec<Value> = rows
        .into_iter()
        .map(
            |(
                id,
                code,
                site,
                porcs,
                poids,
                recettes,
                tonnes,
                cout_aliment,
                truies,
                cout_achat,
                entres,
            )| {
                let aliment_kg = tonnes * 1000.0;
                let ic = indice_consommation(aliment_kg, poids);
                let cout_par_porc = cout_alimentaire_par_porc(cout_aliment, porcs);
                let msa = marge_sur_cout_alimentaire(recettes, cout_aliment);
                let achat_par_animal = cout_achat_par_animal_entre(cout_achat, entres);
                // La marge brute par truie se calcule sur la marge réellement
                // dégagée, donc après la charge d'entrée : sans réception
                // d'achat le coût vaut 0 et le résultat est inchangé.
                let marge_apres_achat = marge_apres_cout_achat(msa, cout_achat);
                let marge_brute_truie = marge_brute_par_truie(marge_apres_achat, truies);
                json!({
                    "id": id, "code": code, "site": site,
                    "porcs": porcs, "poids": (poids * 10.0).round() / 10.0,
                    "aliment_kg": (aliment_kg * 10.0).round() / 10.0,
                    "cout_aliment": (cout_aliment * 100.0).round() / 100.0,
                    "ic": ic.map(|v| (v * 100.0).round() / 100.0),
                    "cout_par_porc": cout_par_porc.map(|v| (v * 100.0).round() / 100.0),
                    "msa": (msa * 100.0).round() / 100.0,
                    "entres": entres,
                    "cout_achat": (cout_achat * 100.0).round() / 100.0,
                    "achat_par_animal": achat_par_animal.map(|v| (v * 100.0).round() / 100.0),
                    "marge_apres_achat": (marge_apres_achat * 100.0).round() / 100.0,
                    "truies": truies,
                    "marge_brute_truie": marge_brute_truie.map(|v| (v * 100.0).round() / 100.0),
                })
            },
        )
        .collect();

    // Taux de renouvellement du cheptel sur 12 mois glissants (§7) : n'a de
    // sens que pour les profils qui conduisent des truies.
    let renouvellement = if session.a_truies() {
        let reformees_12m: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM truie WHERE reformee=1 AND date_reforme>=date('now','-12 months')",
        )
        .fetch_one(&state.pool)
        .await?;
        let effectif_actif: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE reformee=0")
            .fetch_one(&state.pool)
            .await?;
        taux_renouvellement_pct(reformees_12m, effectif_actif).map(|v| (v * 10.0).round() / 10.0)
    } else {
        None
    };

    let mut ctx = context(&session);
    ctx.insert("bandes".into(), Value::Array(bandes));
    ctx.insert("renouvellement".into(), json!(renouvellement));
    render(&state, "gte.html", Value::Object(ctx))
}

fn require_economic_import(session: &SessionData) -> AppResult<()> {
    if matches!(session.role.as_str(), "admin" | "eleveur") {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn import_detail_str<'a>(line: &'a ImportLine, key: &str) -> Option<&'a str> {
    line.details
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn import_detail_f64(line: &ImportLine, key: &str) -> Option<f64> {
    line.details.get(key).and_then(Value::as_f64)
}

fn import_detail_i64(line: &ImportLine, key: &str) -> Option<i64> {
    line.details.get(key).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_f64().map(|number| number.round() as i64))
    })
}

fn import_key(line: &ImportLine) -> String {
    let detail = match line.kind.as_str() {
        "aliment" => format!(
            "{}|{}",
            import_detail_str(line, "produit").unwrap_or(""),
            import_detail_str(line, "silo").unwrap_or("")
        ),
        "veto" => import_detail_str(line, "produit").unwrap_or("").to_string(),
        "vente" | "synthese" => import_detail_str(line, "frappe").unwrap_or("").to_string(),
        "valorisation" | "retenue" => format!("{}|{}", line.kind, line.label),
        _ => String::new(),
    };
    format!(
        "{}|{}|{}|{}",
        line.details
            .get("source_ligne")
            .map(Value::to_string)
            .unwrap_or_default(),
        line.kind,
        line.reference.as_deref().unwrap_or(""),
        detail.to_lowercase()
    )
}

async fn economic_preview_action(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    line: &ImportLine,
    seen: &mut HashSet<String>,
) -> AppResult<(String, Option<String>)> {
    if line
        .reference
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Ok((
            "erreur".into(),
            Some("Référence de facture, apport ou frappe manquante".into()),
        ));
    }
    if line.amount.is_none() && line.kind != "synthese" {
        return Ok((
            "erreur".into(),
            Some("Montant non détecté de façon fiable".into()),
        ));
    }
    let key = import_key(line);
    if !seen.insert(key) {
        return Ok(("ignorer".into(), Some("Doublon dans le document".into())));
    }
    let reference = line.reference.as_deref().unwrap_or_default();
    let exists: i64 = match line.kind.as_str() {
        "aliment" => sqlx::query_scalar("SELECT COUNT(*) FROM livraisonaliment WHERE COALESCE(num_facture,'')=? AND lower(COALESCE(produit,''))=lower(?) AND COALESCE(silo,'')=COALESCE(?, '')")
            .bind(reference).bind(import_detail_str(line,"produit")).bind(import_detail_str(line,"silo")).fetch_one(&mut **transaction).await?,
        "veto" => sqlx::query_scalar("SELECT COUNT(*) FROM achatveto WHERE COALESCE(num_facture,'')=? AND lower(COALESCE(produit,''))=lower(?)")
            .bind(reference).bind(import_detail_str(line,"produit")).fetch_one(&mut **transaction).await?,
        "semence" => sqlx::query_scalar("SELECT COUNT(*) FROM achatsemence WHERE COALESCE(num_facture,'')=?")
            .bind(reference).fetch_one(&mut **transaction).await?,
        "genetique" => sqlx::query_scalar("SELECT COUNT(*) FROM achatgenetique WHERE COALESCE(num_facture,'')=?")
            .bind(reference).fetch_one(&mut **transaction).await?,
        "vente" => sqlx::query_scalar("SELECT COUNT(*) FROM venteapport WHERE COALESCE(num_apport,'')=? AND COALESCE(frappe,'')=COALESCE(?, '')")
            .bind(reference).bind(import_detail_str(line,"frappe")).fetch_one(&mut **transaction).await?,
        "synthese" => sqlx::query_scalar("SELECT COUNT(*) FROM venteapport WHERE COALESCE(frappe,'')=?")
            .bind(reference).fetch_one(&mut **transaction).await?,
        "valorisation" | "retenue" => sqlx::query_scalar("SELECT COUNT(*) FROM venteapport WHERE COALESCE(num_apport,'')=?")
            .bind(reference).fetch_one(&mut **transaction).await?,
        _ => return Ok(("erreur".into(),Some("Type de ligne non pris en charge".into()))),
    };
    let action = if exists > 0 {
        return Ok((
            "erreur".into(),
            Some("Référence déjà importée : modification interdite".into()),
        ));
    } else {
        "ajouter"
    };
    Ok((action.into(), None))
}

async fn economique_import_pdf(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    let mut csrf = None;
    let mut files = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        match field.name() {
            Some("csrf_token") => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                )
            }
            Some("fichier") => {
                let filename: String = field
                    .file_name()
                    .unwrap_or("document.pdf")
                    .chars()
                    .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
                    .take(180)
                    .collect();
                let data = field
                    .bytes()
                    .await
                    .map_err(|error| AppError::Invalid(error.to_string()))?;
                files.push((filename, data));
            }
            _ => {}
        }
    }
    if csrf.as_deref() != Some(session.csrf.as_str()) {
        return Err(AppError::Forbidden);
    }
    if files.is_empty() {
        return Err(AppError::Invalid("Fichier PDF manquant".into()));
    }
    if files.len() > 5 {
        return Err(AppError::Invalid(
            "Sélectionne au maximum 5 PDF à la fois".into(),
        ));
    }
    let total_size: usize = files.iter().map(|(_, bytes)| bytes.len()).sum();
    if total_size > 40 * 1024 * 1024 {
        return Err(AppError::Invalid(
            "Lot de PDF trop volumineux (maximum 40 Mo)".into(),
        ));
    }
    let mut documents = Vec::new();
    for (filename, bytes) in files {
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(AppError::Invalid(format!("{filename} dépasse 8 Mo")));
        }
        if !filename.to_lowercase().ends_with(".pdf") {
            return Err(AppError::Invalid(format!(
                "{filename} n'est pas un fichier PDF"
            )));
        }
        let text = economic_import::extract_pdf_text(&bytes)
            .map_err(|error| AppError::Invalid(format!("{filename} : {error}")))?;
        let parsed = economic_import::parse_document(&text)
            .map_err(|error| AppError::Invalid(format!("{filename} : {error}")))?;
        documents.push((filename, contenu_sha256(&bytes), parsed));
    }
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        "UPDATE importjournal SET statut='expire' WHERE statut='apercu' AND cree_le<datetime('now','-1 day')",
    )
    .execute(&mut *transaction)
    .await?;
    let mut tokens = Vec::new();
    for (filename, digest, parsed) in documents {
        let token = uuid::Uuid::new_v4().simple().to_string();
        refuser_fichier_deja_importe(&mut transaction, &digest).await?;
        sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,?,?,'apercu',?,?)")
            .bind(&token).bind(format!("economique:{}",parsed.document_type)).bind(&filename).bind(session.uid).bind(&digest)
            .execute(&mut *transaction).await?;
        let mut seen = HashSet::new();
        let mut counts = HashMap::<String, i64>::new();
        for (index, line) in parsed.lines.iter().enumerate() {
            let (action, anomaly) =
                economic_preview_action(&mut transaction, line, &mut seen).await?;
            *counts.entry(action.clone()).or_default() += 1;
            sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
                .bind(&token).bind(index as i64 + 1).bind(&action).bind(&anomaly)
                .bind(serde_json::to_string(line).map_err(|error|AppError::Internal(error.into()))?)
                .execute(&mut *transaction).await?;
        }
        let summary = json!({
            "ajouter": counts.get("ajouter").copied().unwrap_or_default(),
            "mettre_a_jour": counts.get("mettre_a_jour").copied().unwrap_or_default(),
            "remplacer": counts.get("remplacer").copied().unwrap_or_default(),
            "ignorer": counts.get("ignorer").copied().unwrap_or_default(),
            "erreur": counts.get("erreur").copied().unwrap_or_default(),
            "avertissements": parsed.warnings,
        });
        sqlx::query("UPDATE importjournal SET resume=? WHERE token=?")
            .bind(summary.to_string())
            .bind(&token)
            .execute(&mut *transaction)
            .await?;
        tokens.push(token);
    }
    transaction.commit().await?;
    if tokens.len() == 1 {
        Ok(Redirect::to(&format!("/economique/import/{}", tokens[0])).into_response())
    } else {
        Ok(Redirect::to(&format!("/economique?imports_prets={}", tokens.len())).into_response())
    }
}

async fn economique_import_apercu(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    require_economic_import(&session)?;
    let journal = sqlx::query_as::<_,(String,Option<String>,Option<String>,Option<i64>)>("SELECT type_import,nom_fichier,resume,cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import LIKE 'economique:%'")
        .bind(&token).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    if journal.3 != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let stored = sqlx::query_as::<_,(i64,String,Option<String>,String)>("SELECT numero_ligne,action,anomalie,donnees_json FROM importligne WHERE token=? ORDER BY numero_ligne")
        .bind(&token).fetch_all(&state.pool).await?;
    let bands = ventes::bands(&state.pool).await?;
    let sites = generic_rows(&state.pool, "SELECT id,code,nom FROM site ORDER BY code").await?;
    let mut rows = Vec::new();
    for (number, action, anomaly, raw) in stored {
        let line: ImportLine =
            serde_json::from_str(&raw).map_err(|error| AppError::Internal(error.into()))?;
        let (suggested_band, suggestion_note) = if line.kind == "vente" {
            ventes::suggestion(import_detail_str(&line, "frappe"), &bands)
        } else {
            (None, "")
        };
        rows.push(json!({"sites_suggeres":factures::suggest_sites(import_detail_str(&line,"destination"),&sites),"details":line.details.clone(),"suggested_band":suggested_band,"suggestion_note":suggestion_note,"lot_ref":import_detail_str(&line,"frappe"),"ligne":number,"action":action,"anomalie":anomaly,"type":line.kind,"date":line.date,"reference":line.reference,"libelle":line.label,"quantite":line.quantity,"prix_unitaire":line.unit_price,"montant":line.amount}));
    }
    let summary = journal
        .2
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    let mut ctx = context(&session);
    ctx.insert("sites".into(), json!(sites));
    ctx.insert("token".into(), json!(token));
    ctx.insert(
        "type_document".into(),
        json!(journal.0.trim_start_matches("economique:")),
    );
    ctx.insert("nom_fichier".into(), json!(journal.1));
    ctx.insert("resume".into(), summary);
    ctx.insert("lignes".into(), Value::Array(rows));
    ctx.insert("bandes".into(), Value::Array(bands));
    render(&state, "economique_import_apercu.html", Value::Object(ctx))
}

type SaleMovementRow = (
    i64,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

/// Rejouable : chaque apport possède ses propres mouvements. Les porcs sont
/// retirés en priorité des cases où des entrées de la même bande sont tracées.
/// Si l'origine détaillée manque, un mouvement « origine non renseignée »
/// conserve malgré tout la sortie abattoir dans le registre.
async fn synchronise_sortie_abattoir(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sale_id: i64,
    date: Option<&str>,
    reference: &str,
    band_id: Option<i64>,
    number: Option<i64>,
    lots_json: Option<&str>,
) -> AppResult<()> {
    sqlx::query("DELETE FROM transfert WHERE vente_apport_id=?")
        .bind(sale_id)
        .execute(&mut **tx)
        .await?;
    let mut allocations = Vec::<(i64, i64)>::new();
    if let Some(raw) = lots_json {
        if let Ok(Value::Array(lots)) = serde_json::from_str::<Value>(raw) {
            let is_aggregate = lots.len() > 1
                && number
                    == lots
                        .iter()
                        .map(|lot| lot["nb_porcs"].as_i64())
                        .collect::<Option<Vec<_>>>()
                        .map(|counts| counts.iter().sum());
            for lot in lots.into_iter().filter(|_| is_aggregate) {
                if let (Some(band), Some(count)) = (
                    lot.get("bande_id").and_then(Value::as_i64).or(band_id),
                    lot.get("nb_porcs").and_then(Value::as_i64),
                ) {
                    if count > 0 {
                        allocations.push((band, count));
                    }
                }
            }
        }
    }
    if allocations.is_empty() {
        if let (Some(band), Some(count)) = (band_id, number.filter(|n| *n > 0)) {
            allocations.push((band, count));
        }
    }
    let movement_date = date
        .map(str::to_owned)
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    for (band, count) in allocations {
        let cases: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT c.id,CAST(COALESCE(SUM(CASE WHEN t.case_dest_id=c.id THEN COALESCE(t.nombre,0) WHEN t.id IN (SELECT transfert_id FROM sortienourrice WHERE transfert_id IS NOT NULL) THEN 0 ELSE -COALESCE(t.nombre,0) END),0)-COALESCE((SELECT SUM(d.nombre) FROM declarationmort d JOIN bande b ON b.code=d.bande_code WHERE d.case_id=c.id AND b.id=? AND date(d.date)<=date(?)),0) AS INTEGER) AS disponible FROM casesalle c JOIN transfert t ON (t.case_dest_id=c.id OR t.case_source_id=c.id) WHERE t.espece='porc' AND t.bande_id=? AND date(t.date)<=date(?) GROUP BY c.id HAVING disponible>0 ORDER BY MAX(t.date),c.id",
        )
        .bind(band)
        .bind(&movement_date)
        .bind(band)
        .bind(&movement_date)
        .fetch_all(&mut **tx)
        .await?;
        let mut remaining = count;
        for (case_id, available) in cases {
            if remaining <= 0 {
                break;
            }
            let moved = remaining.min(available);
            sqlx::query("INSERT INTO transfert(date,espece,bande_id,case_source_id,nombre,vente_apport_id,note) VALUES(?,'porc',?,?,?,?,?)")
                .bind(&movement_date).bind(band).bind(case_id).bind(moved).bind(sale_id)
                .bind(format!("Sortie abattoir — apport {reference}"))
                .execute(&mut **tx).await?;
            remaining -= moved;
        }
        if remaining > 0 {
            sqlx::query("INSERT INTO transfert(date,espece,bande_id,nombre,vente_apport_id,note) VALUES(?,'porc',?,?,?,?)")
                .bind(&movement_date).bind(band).bind(remaining).bind(sale_id)
                .bind(format!("Sortie abattoir — apport {reference} (case d’origine non renseignée)"))
                .execute(&mut **tx).await?;
        }
    }
    Ok(())
}

async fn economique_import_confirmer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(token): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let sites_available: Vec<i64> = sqlx::query_scalar("SELECT id FROM site")
        .fetch_all(&state.pool)
        .await?;
    let mut transaction = state.pool.begin().await?;
    let owner: Option<i64> = sqlx::query_scalar("SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import LIKE 'economique:%'")
        .bind(&token).fetch_optional(&mut *transaction).await?.flatten();
    if owner.is_none() {
        return Err(AppError::NotFound);
    }
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let errors: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM importligne WHERE token=? AND action='erreur'")
            .bind(&token)
            .fetch_one(&mut *transaction)
            .await?;
    if errors > 0 {
        return Err(AppError::Invalid(
            "L'import contient une erreur bloquante".into(),
        ));
    }
    let band_id = form_i64(&form, "bande_id");
    let stored = sqlx::query_as::<_,(i64,String)>("SELECT numero_ligne,donnees_json FROM importligne WHERE token=? AND action NOT IN ('erreur','ignorer') ORDER BY numero_ligne")
        .bind(&token).fetch_all(&mut *transaction).await?;
    let mut lines = Vec::new();
    for (number, raw) in stored {
        let line: ImportLine = serde_json::from_str(&raw)
            .map_err(|_| AppError::Invalid(format!("Données invalides à la ligne {number}")))?;
        lines.push((number, line));
    }
    // Revérification atomique au moment de confirmer : un autre import peut
    // avoir été appliqué depuis l'affichage de l'aperçu.
    let mut confirmation_seen = HashSet::new();
    for (_, line) in &lines {
        let (action, anomaly) =
            economic_preview_action(&mut transaction, line, &mut confirmation_seen).await?;
        if action != "ajouter" {
            return Err(AppError::Invalid(anomaly.unwrap_or_else(|| {
                "Import en conflit avec des données déjà présentes".into()
            })));
        }
    }
    let mut affected_apports = HashSet::new();
    let mut applied = 0_i64;
    for (number, line) in &lines {
        // La bande globale reste un raccourci, mais chaque ligne (et donc
        // chaque lot d'un apport) peut être ventilée indépendamment.
        let band_id =
            match form_text(&form, &format!("bande_ligne_{number}")).as_deref() {
                Some("none") => None,
                None | Some("default") => band_id,
                Some(value) => Some(value.parse::<i64>().ok().filter(|id| *id > 0).ok_or_else(
                    || AppError::Invalid(format!("Bande invalide à la ligne {number}")),
                )?),
            };
        let band_code: Option<String> = if let Some(id) = band_id {
            Some(
                sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
                    .bind(id)
                    .fetch_optional(&mut *transaction)
                    .await?
                    .ok_or_else(|| {
                        AppError::Invalid(format!("Bande inconnue à la ligne {number}"))
                    })?,
            )
        } else {
            None
        };
        let reference = line
            .reference
            .as_deref()
            .ok_or_else(|| AppError::Invalid("Référence manquante".into()))?;
        if matches!(line.kind.as_str(), "vente" | "valorisation" | "retenue") {
            affected_apports.insert(reference.to_string());
        }
        match line.kind.as_str() {
            "aliment" | "veto" => {
                let sites: Vec<i64> = sites_available
                    .iter()
                    .copied()
                    .filter(|id| form.contains_key(&format!("site_ligne_{number}_{id}")))
                    .collect();
                let reference_date = form_date(&form, &format!("date_reference_{number}"))?
                    .or_else(|| {
                        (line.kind == "veto")
                            .then(|| import_detail_str(line, "date_commande").map(str::to_string))
                            .flatten()
                    })
                    .or_else(|| import_detail_str(line, "date_livraison").map(str::to_string))
                    .or_else(|| line.date.clone());
                let inserted = if line.kind == "aliment" {
                    sqlx::query("INSERT INTO livraisonaliment(date,fournisseur,produit,silo,tonnage,pu_ht,montant_ht,num_facture,bande_id,date_reference,sites_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(import_detail_str(line,"fournisseur")).bind(import_detail_str(line,"produit")).bind(import_detail_str(line,"silo")).bind(import_detail_f64(line,"tonnage")).bind(import_detail_f64(line,"pu_ht")).bind(line.amount).bind(reference).bind(band_id).bind(reference_date).bind(json!(sites).to_string()).execute(&mut *transaction).await?.last_insert_rowid()
                } else {
                    sqlx::query("INSERT INTO achatveto(date,produit,quantite,pu_ht,montant_ht,num_facture,fournisseur,bande_id,date_reference,sites_json) VALUES(?,?,?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(import_detail_str(line,"produit")).bind(import_detail_f64(line,"quantite")).bind(import_detail_f64(line,"pu_ht")).bind(line.amount).bind(reference).bind(import_detail_str(line,"fournisseur")).bind(band_id).bind(reference_date).bind(json!(sites).to_string()).execute(&mut *transaction).await?.last_insert_rowid()
                };
                if line.kind == "aliment" {
                    let stage = form
                        .get(&format!("stade_aliment_{number}"))
                        .map(String::as_str)
                        .unwrap_or("auto");
                    if !crate::affectation::valid_stage(stage) {
                        return Err(AppError::Invalid("Stade d'aliment invalide.".into()));
                    }
                    sqlx::query("UPDATE livraisonaliment SET stade_aliment=? WHERE id=?")
                        .bind(stage)
                        .bind(inserted)
                        .execute(&mut *transaction)
                        .await?;
                }
                if form
                    .get(&format!("bande_ligne_{number}"))
                    .map(String::as_str)
                    == Some("none")
                {
                    sqlx::query("INSERT INTO affectationfacturecontrole(categorie,facture_id,verrou_manuel) VALUES(?,?,1)").bind(&line.kind).bind(inserted).execute(&mut *transaction).await?;
                }
            }
            "semence" => {
                let id: Option<i64> = sqlx::query_scalar("SELECT id FROM achatsemence WHERE COALESCE(num_facture,'')=? ORDER BY id LIMIT 1").bind(reference).fetch_optional(&mut *transaction).await?;
                if let Some(id) = id {
                    sqlx::query("UPDATE achatsemence SET date=?,fournisseur=?,designation=?,nb_doses=?,montant_ht=?,montant_ttc=?,bande_id=COALESCE(?,bande_id) WHERE id=?")
                        .bind(line.date.as_deref()).bind(import_detail_str(line,"fournisseur")).bind(import_detail_str(line,"designation")).bind(import_detail_i64(line,"nb_doses")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_ttc")).bind(band_id).bind(id).execute(&mut *transaction).await?;
                } else {
                    sqlx::query("INSERT INTO achatsemence(date,num_facture,fournisseur,designation,nb_doses,montant_ht,montant_ttc,bande_id) VALUES(?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(reference).bind(import_detail_str(line,"fournisseur")).bind(import_detail_str(line,"designation")).bind(import_detail_i64(line,"nb_doses")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_ttc")).bind(band_id).execute(&mut *transaction).await?;
                }
            }
            "genetique" => {
                let id: Option<i64> = sqlx::query_scalar("SELECT id FROM achatgenetique WHERE COALESCE(num_facture,'')=? ORDER BY id LIMIT 1").bind(reference).fetch_optional(&mut *transaction).await?;
                if let Some(id) = id {
                    sqlx::query("UPDATE achatgenetique SET date=?,fournisseur=?,designation=?,nb_animaux=?,poids_total=?,prix_moyen=?,montant_ht=?,montant_net=?,bande_code=COALESCE(?,bande_code) WHERE id=?")
                        .bind(line.date.as_deref()).bind(import_detail_str(line,"fournisseur")).bind(import_detail_str(line,"designation")).bind(import_detail_i64(line,"nb_animaux")).bind(import_detail_f64(line,"poids_total")).bind(import_detail_f64(line,"prix_moyen")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_net")).bind(band_code.as_deref()).bind(id).execute(&mut *transaction).await?;
                } else {
                    sqlx::query("INSERT INTO achatgenetique(date,num_facture,fournisseur,designation,nb_animaux,poids_total,prix_moyen,montant_ht,montant_net,bande_code) VALUES(?,?,?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(reference).bind(import_detail_str(line,"fournisseur")).bind(import_detail_str(line,"designation")).bind(import_detail_i64(line,"nb_animaux")).bind(import_detail_f64(line,"poids_total")).bind(import_detail_f64(line,"prix_moyen")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_net")).bind(band_code.as_deref()).execute(&mut *transaction).await?;
                }
                if line.details["toutes_bandes"].as_bool() == Some(true) {
                    sqlx::query("UPDATE achatgenetique SET toutes_bandes=1 WHERE num_facture=?")
                        .bind(reference)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
            "vente" => {
                let frappe = import_detail_str(line, "frappe");
                let id: Option<i64> = sqlx::query_scalar("SELECT id FROM venteapport WHERE COALESCE(num_apport,'')=? AND COALESCE(frappe,'')=COALESCE(?, '') ORDER BY id LIMIT 1").bind(reference).bind(frappe).fetch_optional(&mut *transaction).await?;
                let lots_json = line.details.get("lots_json").map(Value::to_string);
                if let Some(id) = id {
                    sqlx::query("UPDATE venteapport SET date=?,bande_id=COALESCE(?,bande_id),frappe=?,nb_porcs=?,poids_total=?,poids_moyen=?,prix_moyen=?,plus_value=?,montant_ht=?,montant_net=?,tmp=?,muscle_gamme=?,muscle_lot=?,total_retenues=?,semaine=?,lots_json=? WHERE id=?")
                        .bind(line.date.as_deref()).bind(band_id).bind(frappe).bind(import_detail_i64(line,"nb_porcs")).bind(import_detail_f64(line,"poids_total")).bind(import_detail_f64(line,"poids_moyen")).bind(import_detail_f64(line,"prix_moyen")).bind(import_detail_f64(line,"plus_value")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_net")).bind(import_detail_f64(line,"tmp")).bind(import_detail_f64(line,"muscle_gamme")).bind(import_detail_f64(line,"muscle_lot")).bind(import_detail_f64(line,"total_retenues")).bind(import_detail_str(line,"semaine")).bind(lots_json).bind(id).execute(&mut *transaction).await?;
                } else {
                    sqlx::query("INSERT INTO venteapport(date,num_apport,bande_id,frappe,nb_porcs,poids_total,poids_moyen,prix_moyen,plus_value,montant_ht,montant_net,tmp,muscle_gamme,muscle_lot,total_retenues,semaine,lots_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(reference).bind(band_id).bind(frappe).bind(import_detail_i64(line,"nb_porcs")).bind(import_detail_f64(line,"poids_total")).bind(import_detail_f64(line,"poids_moyen")).bind(import_detail_f64(line,"prix_moyen")).bind(import_detail_f64(line,"plus_value")).bind(import_detail_f64(line,"montant_ht")).bind(import_detail_f64(line,"montant_net")).bind(import_detail_f64(line,"tmp")).bind(import_detail_f64(line,"muscle_gamme")).bind(import_detail_f64(line,"muscle_lot")).bind(import_detail_f64(line,"total_retenues")).bind(import_detail_str(line,"semaine")).bind(lots_json).execute(&mut *transaction).await?;
                }
            }
            "valorisation" | "retenue" => {
                sqlx::query("INSERT INTO valorisationapport(num_apport,date,libelle,montant,categorie) VALUES(?,?,?,?,?)")
                    .bind(reference).bind(line.date.as_deref()).bind(&line.label).bind(line.amount).bind(&line.kind).execute(&mut *transaction).await?;
            }
            "synthese" => {
                let id: Option<i64> = sqlx::query_scalar("SELECT id FROM venteapport WHERE COALESCE(frappe,'')=? ORDER BY date DESC,id DESC LIMIT 1").bind(reference).fetch_optional(&mut *transaction).await?;
                if let Some(id) = id {
                    sqlx::query("UPDATE venteapport SET poids_moyen=COALESCE(?,poids_moyen),tmp=COALESCE(?,tmp),tx_qualification=COALESCE(?,tx_qualification),nb_porcs=COALESCE(nb_porcs,?),plus_value=COALESCE(?,plus_value) WHERE id=?")
                        .bind(import_detail_f64(line,"poids_moyen")).bind(import_detail_f64(line,"tmp")).bind(import_detail_f64(line,"tx_qualification")).bind(import_detail_i64(line,"nb_porcs")).bind(import_detail_f64(line,"plus_value")).bind(id).execute(&mut *transaction).await?;
                } else {
                    sqlx::query("INSERT INTO venteapport(date,bande_id,frappe,nb_porcs,poids_moyen,tmp,tx_qualification,plus_value) VALUES(?,?,?,?,?,?,?,?)")
                        .bind(line.date.as_deref()).bind(band_id).bind(reference).bind(import_detail_i64(line,"nb_porcs")).bind(import_detail_f64(line,"poids_moyen")).bind(import_detail_f64(line,"tmp")).bind(import_detail_f64(line,"tx_qualification")).bind(import_detail_f64(line,"plus_value")).execute(&mut *transaction).await?;
                }
            }
            _ => return Err(AppError::Invalid("Type de ligne non pris en charge".into())),
        }
        applied += 1;
    }
    for reference in affected_apports {
        let sales: Vec<SaleMovementRow> = sqlx::query_as(
            "SELECT id,date,bande_id,nb_porcs,lots_json FROM venteapport WHERE num_apport=?",
        )
        .bind(&reference)
        .fetch_all(&mut *transaction)
        .await?;
        for (sale_id, date, sale_band_id, number, lots_json) in sales {
            synchronise_sortie_abattoir(
                &mut transaction,
                sale_id,
                date.as_deref(),
                &reference,
                sale_band_id,
                number,
                lots_json.as_deref(),
            )
            .await?;
        }
        sqlx::query("UPDATE venteapport SET total_retenues=(SELECT ROUND(COALESCE(SUM(ABS(montant)),0),2) FROM valorisationapport WHERE num_apport=? AND lower(COALESCE(categorie,''))='retenue') WHERE num_apport=?")
            .bind(&reference).bind(&reference).execute(&mut *transaction).await?;
    }
    sqlx::query(
        "UPDATE importjournal SET statut='applique',applique_le=CURRENT_TIMESTAMP WHERE token=?",
    )
    .bind(&token)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let auto_linked = db::auto_assign_economic_invoices(&state.pool).await?;
    db::journal(
        &state.pool,
        &session.identifiant,
        "import",
        "economique",
        &format!("{applied} ligne(s), import {token}"),
        "/economique/import/confirmer",
    )
    .await;
    Ok(Redirect::to(&format!(
        "/economique?import_ok={applied}&liaisons={auto_linked}"
    ))
    .into_response())
}

async fn economique_import_annuler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(token): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let mut transaction = state.pool.begin().await?;
    let owner:Option<i64>=sqlx::query_scalar("SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import LIKE 'economique:%'").bind(&token).fetch_optional(&mut *transaction).await?.flatten();
    if owner.is_none() {
        return Err(AppError::NotFound);
    }
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM importligne WHERE token=?")
        .bind(&token)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM importjournal WHERE token=? AND statut='apercu'")
        .bind(&token)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Redirect::to("/economique").into_response())
}

async fn economique_aliment(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let amount = economic_amount(&form, "montant_ht")
        .ok_or_else(|| AppError::Invalid("Montant HT obligatoire".into()))?;
    sqlx::query("INSERT INTO livraisonaliment(date,fournisseur,produit,silo,tonnage,pu_ht,montant_ht,num_facture,site,bande_id) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(form_date(&form,"date")?).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"produit")).bind(form_text(&form,"silo")).bind(form_f64(&form,"tonnage")).bind(form_f64(&form,"pu_ht")).bind(amount).bind(form_text(&form,"num_facture")).bind(form_text(&form,"site")).bind(form_i64(&form,"bande_id")).execute(&state.pool).await?;
    db::auto_assign_economic_invoices(&state.pool).await?;
    Ok(Redirect::to("/economique").into_response())
}
async fn economique_veto(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let amount = economic_amount(&form, "montant_ht")
        .ok_or_else(|| AppError::Invalid("Montant HT obligatoire".into()))?;
    sqlx::query("INSERT INTO achatveto(date,produit,quantite,pu_ht,montant_ht,num_facture,delai_attente,fournisseur,site,bande_id) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(form_date(&form,"date")?).bind(form_text(&form,"produit")).bind(form_f64(&form,"quantite")).bind(form_f64(&form,"pu_ht")).bind(amount).bind(form_text(&form,"num_facture")).bind(form_i64(&form,"delai_attente")).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"site")).bind(form_i64(&form,"bande_id")).execute(&state.pool).await?;
    db::auto_assign_economic_invoices(&state.pool).await?;
    Ok(Redirect::to("/economique").into_response())
}
async fn economique_vente(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number = form_i64(&form, "nb_porcs").filter(|value| *value >= 0);
    let weight = form_f64(&form, "poids_total").filter(|value| *value >= 0.0);
    let average = match (number, weight) {
        (Some(n), Some(w)) if n > 0 => Some(w / n as f64),
        _ => form_f64(&form, "poids_moyen"),
    };
    let amount = economic_amount(&form, "montant_ht")
        .ok_or_else(|| AppError::Invalid("Montant HT obligatoire".into()))?;
    let date = form_date(&form, "date")?;
    let apport = form_text(&form, "num_apport");
    let band_id = form_i64(&form, "bande_id");
    let mut tx = state.pool.begin().await?;
    let result=sqlx::query("INSERT INTO venteapport(date,num_apport,bande_id,nb_porcs,poids_total,poids_moyen,prix_moyen,plus_value,montant_ht,tmp,tx_qualification,nb_hors_poids,nb_tmp_bas,nb_g2,nb_tatouage,nb_qualifies,nb_livres,muscle_gamme,muscle_lot,total_retenues) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(&date).bind(&apport).bind(band_id).bind(number).bind(weight).bind(average).bind(form_f64(&form,"prix_moyen")).bind(form_f64(&form,"plus_value")).bind(amount).bind(form_f64(&form,"tmp")).bind(form_f64(&form,"tx_qualification")).bind(form_i64(&form,"nb_hors_poids")).bind(form_i64(&form,"nb_tmp_bas")).bind(form_i64(&form,"nb_g2")).bind(form_i64(&form,"nb_tatouage")).bind(form_i64(&form,"nb_qualifies")).bind(form_i64(&form,"nb_livres")).bind(form_f64(&form,"muscle_gamme")).bind(form_f64(&form,"muscle_lot")).bind(form_f64(&form,"total_retenues")).execute(&mut *tx).await?;
    synchronise_sortie_abattoir(
        &mut tx,
        result.last_insert_rowid(),
        date.as_deref(),
        apport.as_deref().unwrap_or("sans numéro"),
        band_id,
        number,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(Redirect::to("/economique").into_response())
}
async fn economique_semence(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let amount = economic_amount(&form, "montant_ht")
        .ok_or_else(|| AppError::Invalid("Montant HT obligatoire".into()))?;
    let ttc = economic_amount(&form, "montant_ttc");
    sqlx::query("INSERT INTO achatsemence(date,num_facture,fournisseur,designation,nb_doses,montant_ht,montant_ttc,bande_id,note) VALUES(?,?,?,?,?,?,?,?,?)").bind(form_date(&form,"date")?).bind(form_text(&form,"num_facture")).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"designation")).bind(form_i64(&form,"nb_doses")).bind(amount).bind(ttc).bind(form_i64(&form,"bande_id")).bind(form_text(&form,"note")).execute(&state.pool).await?;
    db::auto_assign_economic_invoices(&state.pool).await?;
    Ok(Redirect::to("/economique").into_response())
}
async fn economique_genetique(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let amount = economic_amount(&form, "montant_ht")
        .ok_or_else(|| AppError::Invalid("Montant HT obligatoire".into()))?;
    sqlx::query("INSERT INTO achatgenetique(date,num_facture,fournisseur,designation,nb_animaux,poids_total,prix_moyen,montant_ht,montant_net,bande_code,note) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(form_date(&form,"date")?).bind(form_text(&form,"num_facture")).bind(form_text(&form,"fournisseur").unwrap_or_else(||"Cooperl".into())).bind(form_text(&form,"designation")).bind(form_i64(&form,"nb_animaux")).bind(form_f64(&form,"poids_total")).bind(form_f64(&form,"prix_moyen")).bind(amount).bind(None::<f64>).bind(form_text(&form,"bande_code")).bind(form_text(&form,"note")).execute(&state.pool).await?;
    db::auto_assign_economic_invoices(&state.pool).await?;
    Ok(Redirect::to("/economique").into_response())
}

async fn economique_valorisation(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let label = form_text(&form, "libelle")
        .ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    let lower = label.to_lowercase();
    let forced_retention = [
        "équarrissage",
        "equarrissage",
        "groupement",
        "cvee",
        "contribution sanitaire",
        "cotisation",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let category =
        if forced_retention || form.get("categorie").map(String::as_str) == Some("retenue") {
            "retenue"
        } else {
            "valorisation"
        };
    let amount = form_f64(&form, "montant")
        .ok_or_else(|| AppError::Invalid("Montant obligatoire".into()))?;
    let stored = if category == "retenue" {
        -amount.abs()
    } else {
        amount
    };
    let number = form_text(&form, "num_apport");
    let date = form_date(&form, "date")?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO valorisationapport(num_apport,date,libelle,montant,categorie) VALUES(?,?,?,?,?)").bind(&number).bind(date).bind(label).bind(stored).bind(category).execute(&mut *tx).await?;
    if let Some(number) = number {
        sqlx::query("UPDATE venteapport SET total_retenues=(SELECT ROUND(COALESCE(SUM(ABS(montant)),0),2) FROM valorisationapport WHERE num_apport=? AND lower(COALESCE(categorie,''))='retenue') WHERE num_apport=?").bind(&number).bind(&number).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/economique").into_response())
}

macro_rules! delete_handler {
    ($name:ident,$category:literal) => {
        async fn $name(
            state: State<AppState>,
            session: Extension<SessionData>,
            Path(id): Path<i64>,
            form: Form<HashMap<String, String>>,
        ) -> AppResult<Response> {
            factures::remove(state, session, Path(($category.into(), id)), form).await
        }
    };
}
delete_handler!(economique_aliment_supprimer, "aliment");
delete_handler!(economique_veto_supprimer, "veto");
delete_handler!(economique_semence_supprimer, "semence");
delete_handler!(economique_genetique_supprimer, "genetique");

async fn economique_valorisation_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let number: Option<String> =
        sqlx::query_scalar("SELECT num_apport FROM valorisationapport WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM valorisationapport WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some(number) = number {
        sqlx::query("UPDATE venteapport SET total_retenues=(SELECT ROUND(COALESCE(SUM(ABS(montant)),0),2) FROM valorisationapport WHERE num_apport=? AND lower(COALESCE(categorie,''))='retenue') WHERE num_apport=?").bind(&number).bind(&number).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Redirect::to("/economique").into_response())
}

async fn vente_directe(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let products=generic_rows(&state.pool,"SELECT id,nom,prix,unite,actif,ordre,quantite_disponible,image_mime FROM produitventedirecte ORDER BY ordre,nom").await?;
    let sessions = generic_rows(
        &state.pool,
        "SELECT id,nom,date_livraison,active FROM sessionventedirecte ORDER BY active DESC,id DESC",
    )
    .await?;
    let settings = generic_rows(
        &state.pool,
        "SELECT date_livraison,texte_livraison,commandes_ouvertes,message_fermeture,logo_data IS NOT NULL AS logo FROM reglageventedirecte WHERE id=1",
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({"date_livraison":null,"texte_livraison":null,"commandes_ouvertes":1,"message_fermeture":null,"logo":false}));
    let nb_commandes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commandeventedirecte")
        .fetch_one(&state.pool)
        .await?;

    let debut = query
        .get("debut")
        .filter(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok())
        .cloned();
    let fin = query
        .get("fin")
        .filter(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok())
        .cloned();
    let session_id = query
        .get("session_id")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0);
    let tri = match query.get("tri").map(String::as_str) {
        Some("chiffre_affaires") => "chiffre_affaires",
        Some("prix_moyen") => "prix_moyen",
        Some("commandes") => "commandes",
        Some("kg") => "kg_vendus",
        _ => "quantite_vendue",
    };
    let sales_sql = format!(
        "SELECT l.nom_produit AS produit,l.unite,ROUND(SUM(l.quantite),2) AS quantite_vendue,ROUND(SUM(CASE WHEN lower(l.unite)='kg' THEN l.quantite ELSE 0 END),2) AS kg_vendus,ROUND(SUM(CASE WHEN lower(l.unite)<>'kg' THEN l.quantite ELSE 0 END),2) AS pieces_vendues,ROUND(SUM(l.total_ligne),2) AS chiffre_affaires,ROUND(SUM(l.total_ligne)/NULLIF(SUM(l.quantite),0),2) AS prix_moyen,COUNT(DISTINCT c.id) AS commandes FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.statut<>'annulee' AND (? IS NULL OR date(c.cree_le)>=date(?)) AND (? IS NULL OR date(c.cree_le)<=date(?)) AND (? IS NULL OR c.session_vente_id=?) GROUP BY l.nom_produit,l.unite ORDER BY {tri} DESC,produit COLLATE NOCASE"
    );
    let sales = sqlx::query(&sales_sql)
        .bind(debut.as_deref())
        .bind(debut.as_deref())
        .bind(fin.as_deref())
        .bind(fin.as_deref())
        .bind(session_id)
        .bind(session_id)
        .fetch_all(&state.pool)
        .await?;
    let sales = rows_to_json(sales)?;
    let totals: (i64, f64, f64, f64) = sqlx::query_as(
        "SELECT COUNT(DISTINCT c.id),CAST(COALESCE(SUM(l.total_ligne),0) AS REAL),CAST(COALESCE(SUM(CASE WHEN lower(l.unite)='kg' THEN l.quantite ELSE 0 END),0) AS REAL),CAST(COALESCE(SUM(CASE WHEN lower(l.unite)<>'kg' THEN l.quantite ELSE 0 END),0) AS REAL) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.statut<>'annulee' AND (? IS NULL OR date(c.cree_le)>=date(?)) AND (? IS NULL OR date(c.cree_le)<=date(?)) AND (? IS NULL OR c.session_vente_id=?)",
    )
    .bind(debut.as_deref())
    .bind(debut.as_deref())
    .bind(fin.as_deref())
    .bind(fin.as_deref())
    .bind(session_id)
    .bind(session_id)
    .fetch_one(&state.pool)
    .await?;
    let mut ctx = context(&session);
    ctx.insert("produits".into(), Value::Array(products));
    ctx.insert("sessions_vente".into(), Value::Array(sessions));
    ctx.insert("reglage".into(), settings);
    ctx.insert("nb_commandes".into(), json!(nb_commandes));
    ctx.insert("produits_vendus".into(), Value::Array(sales));
    ctx.insert(
        "totaux_ventes".into(),
        json!({"commandes":totals.0,"chiffre_affaires":totals.1,"kg":totals.2,"pieces":totals.3}),
    );
    ctx.insert(
        "filtres_ventes".into(),
        json!({"debut":debut,"fin":fin,"session_id":session_id,"tri":tri}),
    );
    render(&state, "vente_directe.html", Value::Object(ctx))
}
async fn vente_directe_commandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let orders = match generic_rows(&state.pool,"SELECT c.id,c.cree_le,c.nom_client,c.telephone,c.email,c.notes,c.statut,c.total,c.session_vente_id,c.recap_envoye_le,c.code_modification,s.nom AS session_nom,(SELECT GROUP_CONCAT(l.nom_produit||' × '||l.quantite||' '||l.unite,', ') FROM lignecommandeventedirecte l WHERE l.commande_id=c.id) AS lignes,ROUND(COALESCE((SELECT SUM(l.quantite) FROM lignecommandeventedirecte l WHERE l.commande_id=c.id AND lower(trim(l.unite)) IN ('kg','kilogramme','kilogrammes','kilo','kilos')),0),2) AS kg_commandes FROM commandeventedirecte c LEFT JOIN sessionventedirecte s ON s.id=c.session_vente_id ORDER BY c.cree_le DESC,c.id DESC LIMIT 500").await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "lecture complète des commandes impossible, affichage de secours");
            generic_rows(&state.pool,"SELECT id,cree_le,nom_client,telephone,email,notes,statut,total,NULL AS session_vente_id,recap_envoye_le,code_modification,NULL AS session_nom,'Détail disponible dans Modifier' AS lignes,0 AS kg_commandes FROM commandeventedirecte ORDER BY id DESC LIMIT 500").await?
        }
    };
    let sessions = generic_rows(
        &state.pool,
        "SELECT id,nom,date_livraison,active FROM sessionventedirecte ORDER BY active DESC,id DESC",
    )
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "liste des sessions de vente indisponible");
        Vec::new()
    });
    let mut ctx = context(&session);
    ctx.insert("commandes".into(), Value::Array(orders));
    ctx.insert("sessions_vente".into(), Value::Array(sessions));
    render(&state, "vente_directe_commandes.html", Value::Object(ctx))
}

async fn vente_directe_bilan(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let rows = generic_rows(
        &state.pool,
        "SELECT s.id,s.nom,s.date_creation,s.date_livraison,s.date_cloture,s.active,s.nb_porcs,
        (SELECT COUNT(*) FROM commandeventedirecte c WHERE c.session_vente_id=s.id AND c.statut<>'annulee') AS commandes,
        ROUND(COALESCE((SELECT SUM(c.total) FROM commandeventedirecte c WHERE c.session_vente_id=s.id AND c.statut<>'annulee'),0),2) AS chiffre_affaires,
        ROUND(COALESCE((SELECT SUM(l.quantite) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id=s.id AND c.statut<>'annulee'),0),2) AS quantite_totale,
        ROUND(COALESCE((SELECT SUM(l.quantite) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id=s.id AND c.statut<>'annulee' AND lower(l.unite)='kg'),0),2) AS kg_vendus,
        ROUND(COALESCE((SELECT SUM(l.quantite) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id=s.id AND c.statut<>'annulee' AND lower(l.unite)<>'kg'),0),2) AS pieces_vendues,
        ROUND(COALESCE((SELECT semence+gestation+maternite+post_sevrage+engraissement+veto_autres FROM coutelevageventedirecte WHERE session_vente_id=s.id),0),2) AS cout_elevage,
        ROUND(COALESCE((SELECT SUM(montant) FROM chargeventedirecte WHERE session_vente_id=s.id),0),2) AS autres_charges,
        ROUND(COALESCE((SELECT semence+gestation+maternite+post_sevrage+engraissement+veto_autres FROM coutelevageventedirecte WHERE session_vente_id=s.id),0)+COALESCE((SELECT SUM(montant) FROM chargeventedirecte WHERE session_vente_id=s.id),0),2) AS prix_revient_total,
        ROUND((COALESCE((SELECT semence+gestation+maternite+post_sevrage+engraissement+veto_autres FROM coutelevageventedirecte WHERE session_vente_id=s.id),0)+COALESCE((SELECT SUM(montant) FROM chargeventedirecte WHERE session_vente_id=s.id),0))/NULLIF((SELECT SUM(l.quantite) FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id=s.id AND c.statut<>'annulee' AND lower(l.unite)='kg'),0),2) AS prix_revient_kg,
        ROUND(COALESCE((SELECT SUM(c.total) FROM commandeventedirecte c WHERE c.session_vente_id=s.id AND c.statut<>'annulee'),0)-COALESCE((SELECT semence+gestation+maternite+post_sevrage+engraissement+veto_autres FROM coutelevageventedirecte WHERE session_vente_id=s.id),0)-COALESCE((SELECT SUM(montant) FROM chargeventedirecte WHERE session_vente_id=s.id),0),2) AS marge
        FROM sessionventedirecte s ORDER BY s.active DESC,COALESCE(s.date_livraison,s.date_creation) DESC,s.id DESC",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("bilans".into(), Value::Array(rows));
    render(&state, "vente_directe_bilan.html", Value::Object(ctx))
}
async fn produit_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name =
        form_text(&form, "nom").ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let price = form_f64(&form, "prix").ok_or_else(|| AppError::Invalid("Prix invalide".into()))?;
    let order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(ordre),0)+1 FROM produitventedirecte")
        .fetch_one(&state.pool)
        .await?;
    sqlx::query("INSERT INTO produitventedirecte(nom,prix,unite,actif,ordre,quantite_disponible) VALUES(?,?,?,1,?,?)").bind(name).bind(price).bind(form_text(&form,"unite").unwrap_or_else(||"kg".into())).bind(order).bind(form_f64(&form,"quantite_disponible")).execute(&state.pool).await?;
    Ok(Redirect::to("/vente-directe").into_response())
}
async fn produit_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name =
        form_text(&form, "nom").ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let price = form_f64(&form, "prix")
        .filter(|value| *value >= 0.0)
        .ok_or_else(|| AppError::Invalid("Prix invalide".into()))?;
    let unit = if form.get("unite").map(String::as_str) == Some("pièce") {
        "pièce"
    } else {
        "kg"
    };
    sqlx::query("UPDATE produitventedirecte SET nom=?,prix=?,unite=?,actif=? WHERE id=?")
        .bind(name)
        .bind(price)
        .bind(unit)
        .bind(form.contains_key("actif"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe#produits").into_response())
}

async fn produit_image(State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<Response> {
    if !module_actif(&state.pool, "module_vente_directe", true).await? {
        return Err(AppError::NotFound);
    }
    let image: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT image_data,image_mime FROM produitventedirecte WHERE id=? AND image_data IS NOT NULL AND image_mime IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((bytes, mime)) = image else {
        return Err(AppError::NotFound);
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).map_err(|_| AppError::Invalid("Image invalide".into()))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok((headers, bytes).into_response())
}

async fn produit_image_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let (form, file, _) = parity::multipart_fields(multipart, "image").await?;
    verify_csrf(&session, &form)?;
    if form.contains_key("supprimer_image") {
        sqlx::query("UPDATE produitventedirecte SET image_data=NULL,image_mime=NULL WHERE id=?")
            .bind(id)
            .execute(&state.pool)
            .await?;
        return Ok(Redirect::to("/vente-directe#produits").into_response());
    }
    let bytes = file.ok_or_else(|| AppError::Invalid("Image obligatoire".into()))?;
    let mime = detect_image_mime(&bytes)?;
    sqlx::query("UPDATE produitventedirecte SET image_data=?,image_mime=? WHERE id=?")
        .bind(bytes)
        .bind(mime)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe#produits").into_response())
}

/// Détecte le format d'une image envoyée (JPEG/PNG/WebP) par sa signature
/// binaire, indépendamment du nom de fichier ou de l'en-tête HTTP envoyés
/// par le navigateur (non fiables). Partagé entre l'image produit et le
/// logo d'enseigne de la vente directe.
fn detect_image_mime(bytes: &[u8]) -> AppResult<&'static str> {
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(AppError::Invalid("Image limitée à 5 Mo".into()));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Ok("image/webp")
    } else {
        Err(AppError::Invalid(
            "Format refusé : utilisez une image JPEG, PNG ou WebP".into(),
        ))
    }
}

/// Logo d'enseigne affiché en en-tête de la page publique de vente directe
/// (`/vente-directe/commande/{token}` ou équivalent) — un logo par
/// installation, comme le reste des réglages de vente directe
/// (`reglageventedirecte`, une seule ligne `id=1`). Même mécanisme que
/// l'image par produit (colonnes BLOB dédiées, jamais de fichier écrit sur
/// disque).
async fn vente_enseigne_logo(State(state): State<AppState>) -> AppResult<Response> {
    let image: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT logo_data,logo_mime FROM reglageventedirecte WHERE id=1 AND logo_data IS NOT NULL AND logo_mime IS NOT NULL",
    )
    .fetch_optional(&state.pool)
    .await?;
    let Some((bytes, mime)) = image else {
        return Err(AppError::NotFound);
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).map_err(|_| AppError::Invalid("Image invalide".into()))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok((headers, bytes).into_response())
}

async fn vente_enseigne_logo_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let (form, file, _) = parity::multipart_fields(multipart, "logo").await?;
    verify_csrf(&session, &form)?;
    sqlx::query("INSERT OR IGNORE INTO reglageventedirecte(id) VALUES(1)")
        .execute(&state.pool)
        .await?;
    if form.contains_key("supprimer_logo") {
        sqlx::query("UPDATE reglageventedirecte SET logo_data=NULL,logo_mime=NULL WHERE id=1")
            .execute(&state.pool)
            .await?;
        return Ok(Redirect::to("/vente-directe#parametres").into_response());
    }
    let bytes = file.ok_or_else(|| AppError::Invalid("Logo obligatoire".into()))?;
    let mime = detect_image_mime(&bytes)?;
    sqlx::query("UPDATE reglageventedirecte SET logo_data=?,logo_mime=? WHERE id=1")
        .bind(bytes)
        .bind(mime)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe#parametres").into_response())
}

async fn produit_inventaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let stock = if form.contains_key("stock_illimite") {
        None
    } else {
        Some(
            form_f64(&form, "quantite_disponible")
                .filter(|value| *value >= 0.0)
                .ok_or_else(|| AppError::Invalid("Quantité d’inventaire invalide".into()))?,
        )
    };
    let product: Option<String> =
        sqlx::query_scalar("SELECT nom FROM produitventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let Some(product) = product else {
        return Err(AppError::NotFound);
    };
    sqlx::query("UPDATE produitventedirecte SET quantite_disponible=? WHERE id=?")
        .bind(stock)
        .bind(id)
        .execute(&state.pool)
        .await?;
    db::journal(
        &state.pool,
        &session.nom,
        "inventaire",
        "vente_directe",
        &format!(
            "{product}: {}",
            stock
                .map(|value| value.to_string())
                .unwrap_or_else(|| "illimité".into())
        ),
        "/vente-directe",
    )
    .await;
    Ok(Redirect::to("/vente-directe#produits").into_response())
}

async fn produit_deplacer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT ordre FROM produitventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    if let Some(current) = current {
        let direction = form.get("direction").map(String::as_str).unwrap_or("");
        let other:Option<(i64,i64)>=match direction{"haut"=>sqlx::query_as("SELECT id,ordre FROM produitventedirecte WHERE ordre<? ORDER BY ordre DESC LIMIT 1").bind(current).fetch_optional(&state.pool).await?,"bas"=>sqlx::query_as("SELECT id,ordre FROM produitventedirecte WHERE ordre>? ORDER BY ordre LIMIT 1").bind(current).fetch_optional(&state.pool).await?,_=>None};
        if let Some((other_id, other_order)) = other {
            let mut tx = state.pool.begin().await?;
            sqlx::query("UPDATE produitventedirecte SET ordre=? WHERE id=?")
                .bind(other_order)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE produitventedirecte SET ordre=? WHERE id=?")
                .bind(current)
                .bind(other_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }
    Ok(Redirect::to("/vente-directe#produits").into_response())
}

async fn vente_reglage_livraison(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison,texte_livraison) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison,texte_livraison=excluded.texte_livraison").bind(form_date(&form,"date_livraison")?).bind(form_text(&form,"texte_livraison")).execute(&state.pool).await?;
    Ok(Redirect::to("/vente-directe").into_response())
}

async fn vente_commandes_ouverture(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let open = form.get("ouvert").map(String::as_str) == Some("1");
    let message = form_text(&form, "message_fermeture")
        .filter(|value| value.len() <= 500)
        .unwrap_or_else(|| "Cette vente est terminée. Les commandes sont fermées.".into());
    sqlx::query("INSERT INTO reglageventedirecte(id,commandes_ouvertes,message_fermeture) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET commandes_ouvertes=excluded.commandes_ouvertes,message_fermeture=excluded.message_fermeture")
        .bind(open)
        .bind(&message)
        .execute(&state.pool)
        .await?;
    db::journal(
        &state.pool,
        &session.nom,
        if open { "ouvrir" } else { "fermer" },
        "commandes_vente_directe",
        &message,
        "/vente-directe",
    )
    .await;
    Ok(Redirect::to("/vente-directe#commandes-client").into_response())
}

async fn commande_statut(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let new_status = form_text(&form, "statut").unwrap_or_else(|| "nouvelle".into());
    if !matches!(
        new_status.as_str(),
        "nouvelle" | "validee" | "preparation" | "prete" | "livree" | "annulee"
    ) {
        return Err(AppError::Invalid("Statut invalide".into()));
    }
    let mut tx = state.pool.begin().await?;
    let old: Option<String> =
        sqlx::query_scalar("SELECT statut FROM commandeventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(old) = old else {
        return Err(AppError::NotFound);
    };
    let lines = sqlx::query_as::<_, (Option<i64>, f64)>(
        "SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    if new_status == "annulee" && old != "annulee" {
        for (product_id, quantity) in &lines {
            if let Some(product_id) = product_id {
                sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;
            }
        }
    } else if old == "annulee" && new_status != "annulee" {
        for (product_id, quantity) in &lines {
            if let Some(product_id) = product_id {
                let stock: Option<f64> = sqlx::query_scalar(
                    "SELECT quantite_disponible FROM produitventedirecte WHERE id=?",
                )
                .bind(product_id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                if stock.is_some_and(|value| value < *quantity) {
                    return Err(AppError::Invalid(
                        "Stock insuffisant pour réactiver la commande".into(),
                    ));
                }
            }
        }
        for (product_id, quantity) in &lines {
            if let Some(product_id) = product_id {
                sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;
            }
        }
    }
    sqlx::query("UPDATE commandeventedirecte SET statut=? WHERE id=?")
        .bind(new_status)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/vente-directe/commandes").into_response())
}

async fn commande_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let mut tx = state.pool.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT statut FROM commandeventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if status.as_deref().is_some_and(|value| value != "annulee") {
        let lines = sqlx::query_as::<_, (Option<i64>, f64)>(
            "SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (product_id, quantity) in lines {
            if let Some(product_id) = product_id {
                sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;
            }
        }
    }
    sqlx::query("DELETE FROM commandeventedirecte WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/vente-directe/commandes").into_response())
}

async fn vente_commande_modifier_page(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let order = generic_rows(
        &state.pool,
        &format!("SELECT id,nom_client,telephone,email,notes,statut,total,session_vente_id,cree_le FROM commandeventedirecte WHERE id={id}"),
    )
    .await?;
    let Some(order) = order.into_iter().next() else {
        return Err(AppError::NotFound);
    };
    let products = generic_rows(
        &state.pool,
        &format!("SELECT p.id,p.nom,p.prix,p.unite,p.actif,p.ordre,p.quantite_disponible,COALESCE((SELECT l.quantite FROM lignecommandeventedirecte l WHERE l.commande_id={id} AND l.produit_id=p.id LIMIT 1),0) AS quantite_commande FROM produitventedirecte p ORDER BY p.ordre,p.nom"),
    )
    .await?;
    let sessions = generic_rows(
        &state.pool,
        "SELECT id,nom,date_livraison,active FROM sessionventedirecte ORDER BY active DESC,date_livraison DESC,id DESC",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("commande".into(), order);
    ctx.insert("produits".into(), Value::Array(products));
    ctx.insert("sessions_vente".into(), Value::Array(sessions));
    render(&state, "vente_commande_modifier.html", Value::Object(ctx))
}

async fn vente_commande_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name = form_text(&form, "nom_client")
        .filter(|value| value.len() <= 160)
        .ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let phone = form_text(&form, "telephone")
        .filter(|value| value.len() <= 40)
        .ok_or_else(|| AppError::Invalid("Téléphone obligatoire".into()))?;
    let status = form_text(&form, "statut").unwrap_or_else(|| "nouvelle".into());
    if !matches!(
        status.as_str(),
        "nouvelle" | "validee" | "preparation" | "prete" | "livree" | "annulee"
    ) {
        return Err(AppError::Invalid("Statut invalide".into()));
    }
    let mut tx = state.pool.begin().await?;
    let old_status: Option<String> =
        sqlx::query_scalar("SELECT statut FROM commandeventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(old_status) = old_status else {
        return Err(AppError::NotFound);
    };
    if old_status != "annulee" {
        let old_lines = sqlx::query_as::<_, (Option<i64>, f64)>(
            "SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for (product_id, quantity) in old_lines {
            if let Some(product_id) = product_id {
                sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL")
                    .bind(quantity)
                    .bind(product_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    let products = sqlx::query_as::<_, ProduitVenteDirecte>(
        "SELECT id,nom,prix,unite,actif,ordre,quantite_disponible,image_mime FROM produitventedirecte ORDER BY ordre,nom",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut lines = Vec::new();
    let mut total = 0.0;
    for product in products {
        let quantity = form_f64(&form, &format!("q_{}", product.id)).unwrap_or(0.0);
        if quantity <= 0.0 {
            continue;
        }
        if quantity > 10_000.0 {
            return Err(AppError::Invalid("Quantité invalide".into()));
        }
        if status != "annulee"
            && product
                .quantite_disponible
                .is_some_and(|stock| quantity > stock)
        {
            return Err(AppError::Invalid(format!(
                "Stock insuffisant pour {}",
                product.nom
            )));
        }
        let line_total = (quantity * product.prix * 100.0).round() / 100.0;
        total += line_total;
        lines.push((product, quantity, line_total));
    }
    if lines.is_empty() {
        return Err(AppError::Invalid(
            "La commande doit contenir au moins un produit".into(),
        ));
    }
    let session_id = form_i64(&form, "session_vente_id");
    if let Some(session_id) = session_id {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessionventedirecte WHERE id=?")
            .bind(session_id)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            return Err(AppError::Invalid("Session introuvable".into()));
        }
    }
    sqlx::query("UPDATE commandeventedirecte SET nom_client=?,telephone=?,email=?,notes=?,statut=?,session_vente_id=?,total=? WHERE id=?")
        .bind(&name)
        .bind(&phone)
        .bind(form_text(&form, "email"))
        .bind(form_text(&form, "notes"))
        .bind(&status)
        .bind(session_id)
        .bind(total)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM lignecommandeventedirecte WHERE commande_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (product, quantity, line_total) in lines {
        sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,?,?,?,?,?)")
            .bind(id)
            .bind(product.id)
            .bind(&product.nom)
            .bind(product.prix)
            .bind(&product.unite)
            .bind(quantity)
            .bind(line_total)
            .execute(&mut *tx)
            .await?;
        if status != "annulee" {
            sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible IS NOT NULL")
                .bind(quantity)
                .bind(product.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "modifier",
        "commande_vente_directe",
        &format!("commande {id}"),
        &format!("/vente-directe/commande/{id}"),
    )
    .await;
    Ok(Redirect::to("/vente-directe/commandes").into_response())
}

async fn vente_commande_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let order = generic_rows(&state.pool,&format!("SELECT c.id,c.cree_le,c.nom_client,c.telephone,c.email,c.notes,c.statut,c.total,s.nom AS session_nom,s.date_livraison FROM commandeventedirecte c LEFT JOIN sessionventedirecte s ON s.id=c.session_vente_id WHERE c.id={id}")).await?;
    let Some(order) = order.into_iter().next() else {
        return Err(AppError::NotFound);
    };
    let lines=generic_rows(&state.pool,&format!("SELECT nom_produit,prix_unitaire,unite,quantite,CASE WHEN lower(trim(unite)) IN ('kg','kilogramme','kilogrammes','kilo','kilos') THEN quantite ELSE NULL END AS poids_kg,total_ligne FROM lignecommandeventedirecte WHERE commande_id={id} ORDER BY id")).await?;
    let totals = generic_rows(
        &state.pool,
        &format!(
            "SELECT COUNT(*) AS lignes,ROUND(COALESCE(SUM(CASE WHEN lower(trim(unite)) IN ('kg','kilogramme','kilogrammes','kilo','kilos') THEN quantite ELSE 0 END),0),2) AS poids_kg,ROUND(COALESCE(SUM(CASE WHEN lower(trim(unite)) NOT IN ('kg','kilogramme','kilogrammes','kilo','kilos') THEN quantite ELSE 0 END),0),2) AS autres_unites,ROUND(COALESCE(SUM(total_ligne),0),2) AS montant FROM lignecommandeventedirecte WHERE commande_id={id}"
        ),
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({"lignes":0,"poids_kg":0,"autres_unites":0,"montant":0}));
    let farm = generic_rows(
        &state.pool,
        "SELECT MAX(CASE WHEN cle='nom_elevage' THEN valeur END) AS nom,MAX(CASE WHEN cle='adresse_elevage' THEN valeur END) AS adresse,MAX(CASE WHEN cle='telephone_elevage' THEN valeur END) AS telephone,MAX(CASE WHEN cle='email_elevage' THEN valeur END) AS email FROM parametre",
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({}));
    let mut ctx = context(&session);
    ctx.insert("commande".into(), order);
    ctx.insert("lignes".into(), Value::Array(lines));
    ctx.insert("totaux".into(), totals);
    ctx.insert("elevage".into(), farm);
    render(&state, "vente_commande_impression.html", Value::Object(ctx))
}

async fn vente_preparation_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let session_id = query
        .get("session_id")
        .and_then(|value| value.parse::<i64>().ok());
    let session_id = match session_id {
        Some(value) => Some(value),
        None => {
            sqlx::query_scalar(
                "SELECT id FROM sessionventedirecte WHERE active=1 ORDER BY id DESC LIMIT 1",
            )
            .fetch_optional(&state.pool)
            .await?
        }
    };
    let Some(session_id) = session_id else {
        return Err(AppError::Invalid("Aucune session de vente active".into()));
    };
    let sale_session=generic_rows(&state.pool,&format!("SELECT id,nom,date_livraison,nb_porcs,bande_reference FROM sessionventedirecte WHERE id={session_id}")).await?;
    let Some(sale_session) = sale_session.into_iter().next() else {
        return Err(AppError::NotFound);
    };
    let products=generic_rows(&state.pool,&format!("SELECT l.nom_produit,l.unite,ROUND(SUM(l.quantite),2) AS quantite,COUNT(DISTINCT c.id) AS commandes FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id={session_id} AND c.statut<>'annulee' GROUP BY l.nom_produit,l.unite ORDER BY l.nom_produit")).await?;
    let orders=generic_rows(&state.pool,&format!("SELECT c.id,c.nom_client,c.telephone,c.notes,c.statut,c.total,(SELECT GROUP_CONCAT(l.nom_produit||' × '||l.quantite,', ') FROM lignecommandeventedirecte l WHERE l.commande_id=c.id) AS lignes FROM commandeventedirecte c WHERE c.session_vente_id={session_id} AND c.statut<>'annulee' ORDER BY c.nom_client,c.id")).await?;
    let mut ctx = context(&session);
    ctx.insert("session_vente".into(), sale_session);
    ctx.insert("produits".into(), Value::Array(products));
    ctx.insert("commandes".into(), Value::Array(orders));
    render(&state, "vente_preparation.html", Value::Object(ctx))
}

async fn client_order_by_token(pool: &SqlitePool, token: &str) -> AppResult<Value> {
    let row = sqlx::query(
        "SELECT c.id,c.client_id,c.nom_client,c.telephone,c.email,c.notes,c.statut,c.total,c.token_modification,c.code_modification,c.recap_envoye_le,c.cree_le,s.nom AS session_nom,s.date_livraison,s.date_limite_commandes,CASE WHEN s.active=1 AND COALESCE((SELECT commandes_ouvertes FROM reglageventedirecte WHERE id=1),1)=1 AND (s.date_limite_commandes IS NULL OR date('now')<=date(s.date_limite_commandes)) AND c.statut NOT IN('livree','annulee') THEN 1 ELSE 0 END AS modification_ouverte FROM commandeventedirecte c LEFT JOIN sessionventedirecte s ON s.id=c.session_vente_id WHERE c.token_modification=?",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(rows_to_json(vec![row])?
        .into_iter()
        .next()
        .unwrap_or_else(|| json!({})))
}

async fn commande_confirmation(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    if token.len() < 32 {
        return Err(AppError::NotFound);
    }
    let order = client_order_by_token(&state.pool, &token).await?;
    let id = order
        .get("id")
        .and_then(Value::as_i64)
        .ok_or(AppError::NotFound)?;
    let lines = generic_rows(&state.pool,&format!("SELECT nom_produit,quantite,unite,prix_unitaire,total_ligne FROM lignecommandeventedirecte WHERE commande_id={id} ORDER BY id")).await?;
    render(
        &state,
        "commande_confirmation.html",
        json!({"commande":order,"lignes":lines,"token":token}),
    )
}

async fn render_client_order_edit(
    state: &AppState,
    token: &str,
    verified: bool,
    error: &str,
) -> AppResult<Html<String>> {
    let order = client_order_by_token(&state.pool, token).await?;
    let id = order
        .get("id")
        .and_then(Value::as_i64)
        .ok_or(AppError::NotFound)?;
    let products = if verified {
        generic_rows(&state.pool,&format!("SELECT p.id,p.nom,p.prix,p.unite,p.actif,p.quantite_disponible,COALESCE((SELECT l.quantite FROM lignecommandeventedirecte l WHERE l.commande_id={id} AND l.produit_id=p.id LIMIT 1),0) AS quantite_commande FROM produitventedirecte p WHERE p.actif=1 OR EXISTS(SELECT 1 FROM lignecommandeventedirecte l WHERE l.commande_id={id} AND l.produit_id=p.id) ORDER BY p.ordre,p.nom")).await?
    } else {
        Vec::new()
    };
    render(
        state,
        "commande_client_modifier.html",
        json!({"commande":order,"produits":products,"token":token,"verifie":verified,"erreur":error}),
    )
}

async fn commande_client_modifier_page(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppResult<Html<String>> {
    if token.len() < 32 {
        return Err(AppError::NotFound);
    }
    render_client_order_edit(&state, &token, false, "").await
}

async fn commande_client_modifier(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    let order = client_order_by_token(&state.pool, &token).await?;
    let expected = order
        .get("code_modification")
        .and_then(Value::as_str)
        .unwrap_or("");
    let code = form_text(&form, "code_modification").unwrap_or_default();
    if expected.is_empty() || !expected.eq_ignore_ascii_case(&code) {
        return Ok(
            render_client_order_edit(&state, &token, false, "Code incorrect")
                .await?
                .into_response(),
        );
    }
    if order.get("modification_ouverte").and_then(Value::as_i64) != Some(1) {
        return Ok(render_client_order_edit(
            &state,
            &token,
            true,
            "La date limite est dépassée ou la vente est terminée.",
        )
        .await?
        .into_response());
    }
    if form.get("action").map(String::as_str) != Some("enregistrer") {
        return Ok(render_client_order_edit(&state, &token, true, "")
            .await?
            .into_response());
    }
    let id = order
        .get("id")
        .and_then(Value::as_i64)
        .ok_or(AppError::NotFound)?;
    let name = form_text(&form, "nom_client")
        .filter(|value| value.len() <= 160)
        .ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let phone = form_text(&form, "telephone")
        .filter(|value| value.len() <= 40)
        .ok_or_else(|| AppError::Invalid("Téléphone obligatoire".into()))?;
    let products = sqlx::query_as::<_, ProduitVenteDirecte>(
        "SELECT p.id,p.nom,p.prix,p.unite,p.actif,p.ordre,p.quantite_disponible,p.image_mime FROM produitventedirecte p WHERE p.actif=1 OR EXISTS(SELECT 1 FROM lignecommandeventedirecte l WHERE l.commande_id=? AND l.produit_id=p.id) ORDER BY p.ordre,p.nom",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let old_lines = sqlx::query_as::<_, (Option<i64>, f64)>(
        "SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let old_by_product: HashMap<i64, f64> = old_lines
        .iter()
        .filter_map(|(product, quantity)| product.map(|product| (product, *quantity)))
        .collect();
    let mut selected = Vec::new();
    let mut total = 0.0;
    for product in products {
        let quantity = form_f64(&form, &format!("q_{}", product.id)).unwrap_or(0.0);
        if quantity <= 0.0 {
            continue;
        }
        if quantity > 10_000.0 {
            return Err(AppError::Invalid("Quantité invalide".into()));
        }
        let available = product
            .quantite_disponible
            .map(|stock| stock + old_by_product.get(&product.id).copied().unwrap_or(0.0));
        if available.is_some_and(|stock| quantity > stock) {
            return Ok(render_client_order_edit(
                &state,
                &token,
                true,
                &format!("Stock insuffisant pour {}", product.nom),
            )
            .await?
            .into_response());
        }
        let line_total = (quantity * product.prix * 100.0).round() / 100.0;
        total += line_total;
        selected.push((product, quantity, line_total));
    }
    if selected.is_empty() {
        return Ok(render_client_order_edit(
            &state,
            &token,
            true,
            "La commande doit contenir au moins un produit.",
        )
        .await?
        .into_response());
    }
    let mut tx = state.pool.begin().await?;
    let still_open: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commandeventedirecte c JOIN sessionventedirecte s ON s.id=c.session_vente_id WHERE c.id=? AND c.token_modification=? AND c.statut NOT IN('livree','annulee') AND s.active=1 AND (s.date_limite_commandes IS NULL OR date('now')<=date(s.date_limite_commandes)) AND COALESCE((SELECT commandes_ouvertes FROM reglageventedirecte WHERE id=1),1)=1")
        .bind(id).bind(&token).fetch_one(&mut *tx).await?;
    if still_open == 0 {
        return Err(AppError::Invalid(
            "La période de modification est terminée".into(),
        ));
    }
    for (product_id, quantity) in old_lines {
        if let Some(product_id) = product_id {
            sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL")
                .bind(quantity).bind(product_id).execute(&mut *tx).await?;
        }
    }
    sqlx::query("DELETE FROM lignecommandeventedirecte WHERE commande_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for (product, quantity, line_total) in &selected {
        sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,?,?,?,?,?)")
            .bind(id).bind(product.id).bind(&product.nom).bind(product.prix).bind(&product.unite).bind(quantity).bind(line_total).execute(&mut *tx).await?;
        if product.quantite_disponible.is_some() {
            let reserved = sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible>=?")
                .bind(quantity).bind(product.id).bind(quantity).execute(&mut *tx).await?;
            if reserved.rows_affected() == 0 {
                return Err(AppError::Invalid(format!(
                    "Stock insuffisant pour {}",
                    product.nom
                )));
            }
        }
    }
    let email = form_text(&form, "email");
    sqlx::query("UPDATE commandeventedirecte SET nom_client=?,telephone=?,email=?,notes=?,total=? WHERE id=?")
        .bind(&name).bind(&phone).bind(&email).bind(form_text(&form,"notes")).bind(total).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    let summary = selected
        .iter()
        .map(|(product, quantity, line_total)| {
            (
                product.nom.clone(),
                *quantity,
                product.unite.clone(),
                *line_total,
            )
        })
        .collect::<Vec<_>>();
    if let Some(to) = email {
        let recap = parity::RecapCommande {
            order_id: id,
            client_id: order.get("client_id").and_then(Value::as_i64),
            destinataire: &to,
            nom_client: &name,
            token: &token,
            code: &code,
            total,
            lignes: &summary,
        };
        if let Err(error) = parity::envoyer_recap_commande(&state.pool, recap).await {
            tracing::warn!(%error, commande_id=id, "récapitulatif de commande non envoyé");
        }
    }
    Ok(Redirect::to(&format!("/commande/confirmation/{token}")).into_response())
}

async fn commande_page(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    if !module_actif(&state.pool, "module_vente_directe", true).await? {
        return render(
            &state,
            "commande.html",
            json!({"produits":[],"reglage":{"commandes_ouvertes":0,"message_fermeture":"Le module de vente directe est désactivé."},"session_active":null,"ok":false,"error":""}),
        );
    }
    let products=sqlx::query_as::<_,ProduitVenteDirecte>("SELECT id,nom,prix,unite,actif,ordre,quantite_disponible,image_mime FROM produitventedirecte WHERE actif=1 AND (quantite_disponible IS NULL OR quantite_disponible>0) ORDER BY ordre,nom").fetch_all(&state.pool).await?;
    let settings = generic_rows(
        &state.pool,
        "SELECT date_livraison,texte_livraison,CASE WHEN commandes_ouvertes=1 AND EXISTS(SELECT 1 FROM sessionventedirecte s WHERE s.active=1 AND (s.date_limite_commandes IS NULL OR date('now')<=date(s.date_limite_commandes))) THEN 1 ELSE 0 END AS commandes_ouvertes,message_fermeture,logo_data IS NOT NULL AS logo FROM reglageventedirecte WHERE id=1",
    )
    .await?
    .into_iter()
    .next()
    .unwrap_or_else(|| json!({"date_livraison":null,"texte_livraison":null,"commandes_ouvertes":1,"message_fermeture":null,"logo":false}));
    let active=generic_rows(&state.pool,"SELECT id,nom,date_livraison,date_limite_commandes FROM sessionventedirecte WHERE active=1 AND (date_limite_commandes IS NULL OR date('now')<=date(date_limite_commandes)) ORDER BY id DESC LIMIT 1").await?.into_iter().next().unwrap_or(Value::Null);
    render(
        &state,
        "commande.html",
        json!({"produits":products,"reglage":settings,"session_active":active,"ok":query.contains_key("ok"),"error":query.get("err").cloned().unwrap_or_default()}),
    )
}

async fn commande_post(
    State(state): State<AppState>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !module_actif(&state.pool, "module_vente_directe", true).await? {
        return Ok(Redirect::to("/commande?err=fermee").into_response());
    }
    if form_text(&form, "website").is_some() {
        return Ok(Redirect::to("/commande?ok=1").into_response());
    }
    let name = form_text(&form, "nom_client")
        .filter(|value| value.len() <= 160)
        .ok_or_else(|| AppError::Invalid("Nom obligatoire".into()))?;
    let phone = form_text(&form, "telephone")
        .filter(|value| value.len() <= 40)
        .ok_or_else(|| AppError::Invalid("Téléphone obligatoire".into()))?;
    let mut tx = state.pool.begin().await?;
    let commandes_ouvertes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(commandes_ouvertes,1) FROM reglageventedirecte WHERE id=1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(1);
    if commandes_ouvertes == 0 {
        return Ok(Redirect::to("/commande?err=fermee").into_response());
    }
    let products=sqlx::query_as::<_,ProduitVenteDirecte>("SELECT id,nom,prix,unite,actif,ordre,quantite_disponible,image_mime FROM produitventedirecte WHERE actif=1 ORDER BY ordre,nom").fetch_all(&mut *tx).await?;
    let mut lines = Vec::new();
    let mut total = 0.0;
    for product in products {
        let quantity = form_f64(&form, &format!("q_{}", product.id)).unwrap_or(0.0);
        if quantity <= 0.0 {
            continue;
        }
        if quantity > 10_000.0 {
            return Err(AppError::Invalid("Quantité invalide".into()));
        }
        if product
            .quantite_disponible
            .is_some_and(|stock| quantity > stock)
        {
            return Ok(Redirect::to("/commande?err=stock-insuffisant").into_response());
        }
        let line_total = (quantity * product.prix * 100.0).round() / 100.0;
        total += line_total;
        lines.push((product, quantity, line_total));
    }
    if lines.is_empty() {
        return Ok(Redirect::to("/commande?err=commande-vide").into_response());
    }
    let session_id:Option<i64>=sqlx::query_scalar("SELECT id FROM sessionventedirecte WHERE active=1 AND (date_limite_commandes IS NULL OR date('now')<=date(date_limite_commandes)) ORDER BY date_creation DESC,id DESC LIMIT 1").fetch_optional(&mut *tx).await?;
    let Some(session_id) = session_id else {
        return Ok(Redirect::to("/commande?err=fermee").into_response());
    };
    let unsubscribe_token = uuid::Uuid::new_v4().simple().to_string();
    let modification_token = auth::new_secure_token();
    let modification_code = auth::new_secure_token()
        .chars()
        .take(8)
        .collect::<String>()
        .to_uppercase();
    let email = form_text(&form, "email");
    // Une commande publique ne doit jamais permettre de prendre le contrôle
    // d'une fiche client en connaissant seulement son numéro de téléphone.
    // Seule une adresse e-mail identique permet de réutiliser la fiche ; sans
    // e-mail, une nouvelle fiche indépendante est créée.
    let existing_client = if let Some(email) = email.as_deref() {
        sqlx::query_scalar::<_, i64>(
            "SELECT id FROM clientventedirecte WHERE lower(trim(email))=lower(trim(?)) LIMIT 1",
        )
        .bind(email)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        None
    };
    let client_id = if let Some(id) = existing_client {
        sqlx::query("UPDATE clientventedirecte SET nom=?,telephone=? WHERE id=?")
            .bind(&name)
            .bind(&phone)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        id
    } else {
        sqlx::query("INSERT INTO clientventedirecte(nom,email,telephone,newsletter_email,newsletter_sms,cree_le,token_desinscription) VALUES(?,?,?,0,0,CURRENT_TIMESTAMP,?)").bind(&name).bind(&email).bind(&phone).bind(unsubscribe_token).execute(&mut *tx).await?.last_insert_rowid()
    };
    let order_id=sqlx::query("INSERT INTO commandeventedirecte(client_id,session_vente_id,nom_client,telephone,email,notes,statut,total,cree_le,token_modification,code_modification) VALUES(?,?,?,?,?,?,'nouvelle',?,CURRENT_TIMESTAMP,?,?)").bind(client_id).bind(session_id).bind(&name).bind(&phone).bind(&email).bind(form_text(&form,"notes")).bind(total).bind(&modification_token).bind(&modification_code).execute(&mut *tx).await?.last_insert_rowid();
    let mut summary = Vec::new();
    for (product, quantity, line_total) in lines {
        sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,?,?,?,?,?)").bind(order_id).bind(product.id).bind(&product.nom).bind(product.prix).bind(&product.unite).bind(quantity).bind(line_total).execute(&mut *tx).await?;
        sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product.id).execute(&mut *tx).await?;
        summary.push((product.nom, quantity, product.unite, line_total));
    }
    tx.commit().await?;
    if let Some(to) = email {
        let recap = parity::RecapCommande {
            order_id,
            client_id: Some(client_id),
            destinataire: &to,
            nom_client: &name,
            token: &modification_token,
            code: &modification_code,
            total,
            lignes: &summary,
        };
        if let Err(error) = parity::envoyer_recap_commande(&state.pool, recap).await {
            tracing::warn!(%error, commande_id=order_id, "récapitulatif de commande non envoyé");
        }
    }
    Ok(Redirect::to(&format!("/commande/confirmation/{modification_token}")).into_response())
}

async fn utilisateurs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let users=sqlx::query_as::<_,Utilisateur>("SELECT id,identifiant,nom,prenom,hash_mdp,role,actif,sections,doit_changer_mdp,tentatives_echec,bloque_jusqu FROM utilisateur ORDER BY identifiant").fetch_all(&state.pool).await?;
    let mut ctx = context(&session);
    ctx.insert(
        "utilisateurs".into(),
        serde_json::to_value(users).unwrap_or_default(),
    );
    render(&state, "utilisateurs.html", Value::Object(ctx))
}
async fn utilisateur_creer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let id = form_text(&form, "identifiant")
        .ok_or_else(|| AppError::Invalid("Identifiant obligatoire".into()))?;
    let password = form.get("mdp").cloned().unwrap_or_default();
    if password.len() < 8 {
        return Err(AppError::Invalid(
            "Mot de passe: 8 caractères minimum".into(),
        ));
    }
    let role = form.get("role").map(String::as_str).unwrap_or("salarie");
    if !matches!(role, "admin" | "eleveur" | "salarie" | "engraisseur") {
        return Err(AppError::Invalid("Rôle invalide".into()));
    }
    let hash = auth::hash_password_async(password).await?;
    sqlx::query("INSERT INTO utilisateur(identifiant,nom,prenom,hash_mdp,role,actif,doit_changer_mdp) VALUES(?,?,?,?,?,1,1)").bind(id).bind(form_text(&form,"nom")).bind(form_text(&form,"prenom")).bind(hash).bind(role).execute(&state.pool).await?;
    Ok(Redirect::to("/utilisateurs").into_response())
}
async fn utilisateur_actif(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE utilisateur SET actif=CASE actif WHEN 1 THEN 0 ELSE 1 END WHERE id=? AND identifiant<>'admin'").bind(id).execute(&state.pool).await?;
    state.sessions.retain(|_, active| active.uid != id);
    Ok(Redirect::to("/utilisateurs").into_response())
}

async fn utilisateur_sections(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let allowed = [
        "planning",
        "bandes",
        "truies",
        "charcutiers",
        "productivite",
        "ifip",
        "reformes",
        "cochettes",
        "sanitaire",
        "stock",
        "economique",
        "structure",
        "effectifs",
        "archives",
        "entretien",
    ];
    let sections = allowed
        .iter()
        .filter(|section| form.contains_key(&format!("section_{section}")))
        .copied()
        .collect::<Vec<_>>()
        .join(",");
    sqlx::query("UPDATE utilisateur SET sections=? WHERE id=? AND role='salarie'")
        .bind(sections)
        .bind(id)
        .execute(&state.pool)
        .await?;
    // Les sections sont mises en cache dans SessionData : forcer une nouvelle
    // connexion évite de conserver les anciens droits.
    state.sessions.retain(|_, active| active.uid != id);
    Ok(Redirect::to("/utilisateurs").into_response())
}

async fn utilisateur_mdp(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let password = form.get("mdp").cloned().unwrap_or_default();
    if password.len() < 8 {
        return Err(AppError::Invalid(
            "Le mot de passe doit contenir au moins 8 caractères".into(),
        ));
    }
    let hash = auth::hash_password_async(password).await?;
    sqlx::query("UPDATE utilisateur SET hash_mdp=?,doit_changer_mdp=1,tentatives_echec=0,bloque_jusqu=NULL WHERE id=?")
        .bind(hash)
        .bind(id)
        .execute(&state.pool)
        .await?;
    state.sessions.retain(|_, active| active.uid != id);
    Ok(Redirect::to("/utilisateurs").into_response())
}

async fn sauvegarde(Extension(session): Extension<SessionData>) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    Ok(Redirect::to("/maj#sauvegardes").into_response())
}

async fn sauvegarde_telecharger(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&state.pool)
        .await?;
    let bytes = tokio::fs::read(&state.config.db_path)
        .await
        .map_err(anyhow::Error::from)?;
    let filename = format!("elevage_sauvegarde_{}.db", Local::now().date_naive());
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-sqlite3"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|e| AppError::Internal(e.into()))?,
    );
    Ok((headers, bytes).into_response())
}

async fn structure(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let sites = generic_rows(
        &state.pool,
        "SELECT id,code,nom FROM site ORDER BY COALESCE(nom,code)",
    )
    .await?;
    let rooms = generic_rows(&state.pool, "SELECT s.id,s.site_id,s.nom,s.type,s.rfid,s.nb_cases,s.ordre,COALESCE(si.nom,si.code) AS site FROM salle s JOIN site si ON si.id=s.site_id ORDER BY COALESCE(si.nom,si.code),s.ordre,s.nom").await?;
    let mut cases = generic_rows(&state.pool, "SELECT c.id,c.salle_id,c.nom,c.rfid,c.nb_max_porcs,c.nb_max_truies,c.nb_max_porcelets,c.num_vanne,c.surface_config,s.type AS type_salle,s.nom AS salle,COALESCE(si.nom,si.code) AS site,(SELECT COUNT(*) FROM truie t WHERE t.case_id=c.id AND t.reformee=0) AS truies_presentes,COALESCE((SELECT SUM(MAX(0,COALESCE(e.nes_vifs,0)-COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.evenement_id=e.id OR (p.evenement_id IS NULL AND p.truie_id=e.truie_id AND p.bande_id=e.bande_id)),0))) FROM evenement e WHERE e.type='mise_bas' AND e.case_id=c.id AND NOT EXISTS(SELECT 1 FROM evenement sv WHERE sv.type='sevrage' AND sv.truie_id=e.truie_id AND sv.bande_id=e.bande_id AND sv.date>=e.date)),0) AS porcelets_presents FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY COALESCE(si.nom,si.code),s.ordre,c.nom").await?;
    for case in &mut cases {
        let object = json_object_mut(case, "la capacité des cases")?;
        let sows = object
            .get("truies_presentes")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        let piglets = case_litter_count(&state.pool, id).await?;
        object.insert("porcelets_presents".into(), json!(piglets));
        let sow_places = object
            .get("nb_max_truies")
            .and_then(Value::as_i64)
            .map(|v| (v - sows).max(0));
        object.insert("places_truies_dispo".into(), json!(sow_places));
        object.insert("places_porcelets_dispo".into(), Value::Null);
        surfaces::enrich(&state.pool, object).await?;
    }
    let mut ctx = context(&session);
    ctx.insert("sites".into(), Value::Array(sites));
    ctx.insert("salles".into(), Value::Array(rooms));
    ctx.insert("cases".into(), Value::Array(cases));
    render(&state, "structure.html", Value::Object(ctx))
}
async fn structure_site(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let code =
        form_text(&form, "code").ok_or_else(|| AppError::Invalid("Code obligatoire".into()))?;
    sqlx::query("INSERT INTO site(code,nom,zone) VALUES(?,?,?)")
        .bind(code)
        .bind(form_text(&form, "nom"))
        .bind(form_text(&form, "zone"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}
async fn structure_site_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let code =
        form_text(&form, "code").ok_or_else(|| AppError::Invalid("Code obligatoire".into()))?;
    let old: String = sqlx::query_scalar("SELECT code FROM site WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE site SET code=?,nom=?,zone=? WHERE id=?")
        .bind(&code)
        .bind(form_text(&form, "nom"))
        .bind(form_text(&form, "zone"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE bande SET site=? WHERE site=?")
        .bind(&code)
        .bind(old)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/structure").into_response())
}
async fn structure_salle(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("INSERT INTO salle(site_id,nom,type,rfid,nb_cases,ordre) VALUES(?,?,?,?,0,COALESCE((SELECT MAX(ordre)+1 FROM salle WHERE site_id=?),0))").bind(form_i64(&form,"site_id")).bind(form_text(&form,"nom")).bind(form_text(&form,"type")).bind(form_text(&form,"rfid")).bind(form_i64(&form,"site_id")).execute(&state.pool).await?;
    Ok(Redirect::to("/structure").into_response())
}
async fn structure_case(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query(
        "INSERT INTO casesalle(salle_id,nom,rfid,nb_max_porcs,nb_max_truies,nb_max_porcelets,num_vanne) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(form_i64(&form, "salle_id"))
    .bind(form_text(&form, "nom"))
    .bind(form_text(&form, "rfid"))
    .bind(form_i64(&form, "nb_max_porcs"))
    .bind(form_i64(&form, "nb_max_truies"))
    .bind(None::<i64>)
    .bind(form_text(&form, "num_vanne"))
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_salle_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom de salle obligatoire".into()))?;
    sqlx::query("UPDATE salle SET nom=?,type=?,rfid=? WHERE id=?")
        .bind(name)
        .bind(form_text(&form, "type"))
        .bind(form_text(&form, "rfid"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_salle_ordre(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let current: Option<(i64, i64)> =
        sqlx::query_as("SELECT site_id,COALESCE(ordre,0) FROM salle WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((site_id, order)) = current else {
        return Err(AppError::NotFound);
    };
    let direction = form.get("direction").map(String::as_str).unwrap_or("");
    let other: Option<(i64, i64)> = if direction == "haut" {
        sqlx::query_as("SELECT id,COALESCE(ordre,0) FROM salle WHERE site_id=? AND COALESCE(ordre,0)<? ORDER BY COALESCE(ordre,0) DESC,id DESC LIMIT 1")
            .bind(site_id).bind(order).fetch_optional(&state.pool).await?
    } else if direction == "bas" {
        sqlx::query_as("SELECT id,COALESCE(ordre,0) FROM salle WHERE site_id=? AND COALESCE(ordre,0)>? ORDER BY COALESCE(ordre,0),id LIMIT 1")
            .bind(site_id).bind(order).fetch_optional(&state.pool).await?
    } else {
        None
    };
    if let Some((other_id, other_order)) = other {
        let mut tx = state.pool.begin().await?;
        sqlx::query("UPDATE salle SET ordre=? WHERE id=?")
            .bind(other_order)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE salle SET ordre=? WHERE id=?")
            .bind(order)
            .bind(other_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_case_rfid(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE casesalle SET rfid=?,num_vanne=?,nb_max_porcs=?,nb_max_truies=?,nb_max_porcelets=?,nom=COALESCE(?,nom) WHERE id=?")
        .bind(form_text(&form, "rfid"))
        .bind(form_text(&form, "num_vanne"))
        .bind(form_i64(&form, "nb_max_porcs"))
        .bind(form_i64(&form, "nb_max_truies"))
        .bind(None::<i64>)
        .bind(form_text(&form, "nom"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_case_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let nursery_history: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM adoptionporcelet WHERE case_nourrice_id=?")
            .bind(id)
            .fetch_one(&state.pool)
            .await?;
    if nursery_history > 0 {
        return Err(AppError::Invalid(
            "Cette case possède un historique de nourrice artificielle".into(),
        ));
    }
    let used: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM transfert WHERE case_source_id=? OR case_dest_id=?)+(SELECT COUNT(*) FROM declarationmort WHERE case_id=?)+(SELECT COUNT(*) FROM truie WHERE case_id=?)+(SELECT COUNT(*) FROM evenement WHERE case_id=?)+(SELECT COUNT(*) FROM inventairecase WHERE case_id=?)+(SELECT COUNT(*) FROM receptionachat WHERE case_id=?)")
        .bind(id).bind(id).bind(id).bind(id).bind(id).bind(id).bind(id).fetch_one(&state.pool).await?;
    if used > 0 {
        return Err(AppError::Invalid(
            "Cette case contient un historique ou des animaux et ne peut pas être supprimée".into(),
        ));
    }
    sqlx::query("DELETE FROM casesalle WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_salle_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let children: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM casesalle WHERE salle_id=?)+(SELECT COUNT(*) FROM truie WHERE salle_id=?)+(SELECT COUNT(*) FROM transfert WHERE salle_source_id=? OR salle_dest_id=?)+(SELECT COUNT(*) FROM controlequotidien WHERE salle_id=?)")
        .bind(id)
        .bind(id).bind(id).bind(id).bind(id)
        .fetch_one(&state.pool)
        .await?;
    if children > 0 {
        return Err(AppError::Invalid(
            "Cette salle contient des cases, animaux, contrôles ou mouvements. Conservez-la pour préserver l’historique, ou déplacez d’abord les éléments actifs.".into(),
        ));
    }
    sqlx::query("DELETE FROM salle WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn structure_site_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let children: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM salle WHERE site_id=?)+(SELECT COUNT(*) FROM compteur_energie WHERE site_id=?)+(SELECT COUNT(*) FROM silo_aliment WHERE site_id=?)+(SELECT COUNT(*) FROM bande WHERE site=(SELECT code FROM site WHERE id=?))")
        .bind(id)
        .bind(id).bind(id).bind(id)
        .fetch_one(&state.pool)
        .await?;
    if children > 0 {
        return Err(AppError::Invalid(
            "Ce site est encore utilisé par une bande, une salle, un compteur ou un silo. Il n’a pas été supprimé afin de conserver les données.".into(),
        ));
    }
    sqlx::query("DELETE FROM site WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn taches(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    entretien(State(state), Extension(session)).await
}
async fn tache_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("INSERT INTO tache(titre,type,bande_code,salle,echeance,note,fait,cree_le) VALUES(?,?,?,?,?,?,0,CURRENT_TIMESTAMP)").bind(form_text(&form,"titre").ok_or_else(||AppError::Invalid("Titre obligatoire".into()))?).bind(form_text(&form,"type")).bind(form_text(&form,"bande_code")).bind(form_text(&form,"salle")).bind(form_date(&form,"echeance")?).bind(form_text(&form,"note")).execute(&state.pool).await?;
    Ok(Redirect::to("/taches").into_response())
}
async fn tache_fait(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE tache SET fait=CASE fait WHEN 1 THEN 0 ELSE 1 END WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/taches").into_response())
}
async fn tache_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM tache WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/taches").into_response())
}

async fn sanitaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let protocols = generic_rows(&state.pool, "SELECT id,libelle,cible,reference,jour,produit,dose,unite,voie,duree_j,delai_attente,aiguille,preconisations,note FROM acteprotocole WHERE actif=1 ORDER BY cible,jour,id").await?;
    let bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb FROM bande WHERE active=1 ORDER BY date_mb DESC,code",
    )
    .await?;
    let verrats = generic_rows(
        &state.pool,
        "SELECT id,code FROM verrat WHERE actif=1 ORDER BY code",
    )
    .await?;
    // Réunit les deux tables d'historique (bande et verrat, voir
    // acterealiseverrat dans les migrations) pour un seul tableau « Réalisés ».
    let completed = generic_rows(
        &state.pool,
        // Alias explicites sur id/date_realise nécessaires : sans eux, SQLite
        // refuse l'ORDER BY sur un SELECT composé (UNION ALL) avec l'erreur
        // « ORDER BY term does not match any column in the result set »
        // (trouvé en écrivant le test associé, tests/schema.rs).
        "SELECT ar.id AS id,ar.date_realise AS date_realise,b.code AS cible_nom,a.libelle,a.produit,ar.note FROM acterealise ar JOIN bande b ON b.id=ar.bande_id JOIN acteprotocole a ON a.id=ar.acte_id \
         UNION ALL \
         SELECT arv.id AS id,arv.date_realise AS date_realise,v.code AS cible_nom,a.libelle,a.produit,arv.note FROM acterealiseverrat arv JOIN verrat v ON v.id=arv.verrat_id JOIN acteprotocole a ON a.id=arv.acte_id \
         ORDER BY date_realise DESC,id DESC LIMIT 250",
    )
    .await?;
    let treated_pigs = generic_rows(&state.pool, "SELECT tc.id,tc.date,COALESCE(NULLIF(p.rfid,''),'Porc #'||p.id) AS animal,COALESCE(tc.bande_code,p.bande_code) AS bande,tc.produit,tc.dose,tc.motif,tc.delai_attente,tc.note FROM traitementcharcutier tc JOIN porccharcutier p ON p.id=tc.charcutier_id WHERE lower(COALESCE(tc.motif,'')) NOT LIKE '%vaccin%' AND lower(COALESCE(tc.produit,'')) NOT LIKE '%vaccin%' ORDER BY tc.date DESC,tc.id DESC LIMIT 500").await?;
    let today = Local::now().date_naive();
    // Rappels sanitaires (§8) : un protocole marqué rappel=1 dont l'échéance
    // (reference + jour) est atteinte pour une bande active, et qui n'a pas
    // encore de réalisation enregistrée. Calculés par catégorie (cible) —
    // uniquement pour les protocoles rattachés à mise_bas, seule référence de
    // date disponible au niveau bande à ce stade (les autres références
    // laissent le protocole visible sans échéance calculée plutôt que de
    // planter ou d'inventer une date).
    let mut reminders = generic_rows(
        &state.pool,
        "SELECT ap.id AS protocole_id,ap.libelle,ap.cible,ap.produit,b.id AS bande_id,b.code AS bande_code,date(b.date_mb,printf('%+d day',ap.jour)) AS echeance \
         FROM acteprotocole ap JOIN bande b ON b.active=1 \
         WHERE ap.actif=1 AND ap.rappel=1 AND ap.reference='mise_bas' AND b.date_mb IS NOT NULL \
         AND NOT EXISTS(SELECT 1 FROM acterealise ar WHERE ar.acte_id=ap.id AND ar.bande_id=b.id) \
         ORDER BY ap.cible,echeance",
    )
    .await?;
    for reminder in &mut reminders {
        let en_retard = reminder
            .get("echeance")
            .and_then(Value::as_str)
            .and_then(parse_stored_date)
            .is_some_and(|echeance| rappel_en_retard(echeance, today));
        if let Some(object) = reminder.as_object_mut() {
            object.insert("en_retard".into(), json!(en_retard));
        }
    }
    let mut ctx = context(&session);
    ctx.insert("protocoles".into(), Value::Array(protocols));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("verrats".into(), Value::Array(verrats));
    ctx.insert("realises".into(), Value::Array(completed));
    ctx.insert("porcs_traites".into(), Value::Array(treated_pigs));
    ctx.insert("rappels".into(), Value::Array(reminders));
    ctx.insert("today".into(), json!(today.format("%Y-%m-%d").to_string()));
    render(&state, "sanitaire.html", Value::Object(ctx))
}

async fn sanitaire_acte_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let label = form_text(&form, "libelle")
        .ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    let target =
        form_text(&form, "cible").ok_or_else(|| AppError::Invalid("Cible obligatoire".into()))?;
    let reference = form_text(&form, "reference").unwrap_or_else(|| "mise_bas".into());
    let day = form_i64(&form, "jour").unwrap_or(0);
    let rappel = form.contains_key("rappel");
    let result=sqlx::query("INSERT INTO acteprotocole(libelle,cible,reference,jour,produit,dose,unite,voie,duree_j,delai_attente,aiguille,preconisations,note,actif,rappel) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,1,?)")
        .bind(label).bind(target).bind(reference).bind(day)
        .bind(form_text(&form,"produit")).bind(form_text(&form,"dose")).bind(form_text(&form,"unite"))
        .bind(form_text(&form,"voie")).bind(form_i64(&form,"duree_j")).bind(form_i64(&form,"delai_attente"))
        .bind(form_text(&form,"aiguille")).bind(form_text(&form,"preconisations")).bind(form_text(&form,"note"))
        .bind(rappel)
        .execute(&state.pool).await?;
    synchroniser_protocole_portees(&state.pool, result.last_insert_rowid()).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn sanitaire_acte_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let id = form_i64(&form, "id").ok_or_else(|| AppError::Invalid("Acte manquant".into()))?;
    let label = form_text(&form, "libelle")
        .ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    sqlx::query("UPDATE acteprotocole SET libelle=?,cible=?,reference=?,jour=?,produit=?,dose=?,unite=?,voie=?,duree_j=?,delai_attente=?,aiguille=?,preconisations=?,note=? WHERE id=?")
        .bind(label).bind(form_text(&form,"cible")).bind(form_text(&form,"reference")).bind(form_i64(&form,"jour").unwrap_or(0))
        .bind(form_text(&form,"produit")).bind(form_text(&form,"dose")).bind(form_text(&form,"unite")).bind(form_text(&form,"voie"))
        .bind(form_i64(&form,"duree_j")).bind(form_i64(&form,"delai_attente")).bind(form_text(&form,"aiguille"))
        .bind(form_text(&form,"preconisations")).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;
    synchroniser_protocole_portees(&state.pool, id).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn sanitaire_acte_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let id = form_i64(&form, "id").ok_or_else(|| AppError::Invalid("Acte manquant".into()))?;
    sqlx::query("UPDATE acteprotocole SET actif=0 WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    synchroniser_protocole_portees(&state.pool, id).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn sanitaire_fait(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session, &form)?;
    let act =
        form_i64(&form, "acte_id").ok_or_else(|| AppError::Invalid("Acte manquant".into()))?;
    let band =
        form_i64(&form, "bande_id").ok_or_else(|| AppError::Invalid("Bande manquante".into()))?;
    let date = form_date_or_today(&form, "date_realise")?;
    sqlx::query("INSERT INTO acterealise(acte_id,bande_id,date_realise,note) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM acteprotocole WHERE id=? AND actif=1) AND EXISTS(SELECT 1 FROM bande WHERE id=?)")
        .bind(act).bind(band).bind(date).bind(form_text(&form,"note")).bind(act).bind(band).execute(&state.pool).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

/// Équivalent de `sanitaire_fait` pour un verrat : un verrat n'appartenant à
/// aucune bande, `acterealise` (bande_id NOT NULL) ne peut pas l'accueillir
/// — voir `acterealiseverrat` dans les migrations. Sans cette route, un
/// protocole ciblant un verrat n'était réalisable qu'en le rattachant à une
/// bande sans rapport, ce qui aurait corrompu l'historique sanitaire.
async fn sanitaire_fait_verrat(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session, &form)?;
    let act =
        form_i64(&form, "acte_id").ok_or_else(|| AppError::Invalid("Acte manquant".into()))?;
    let verrat =
        form_i64(&form, "verrat_id").ok_or_else(|| AppError::Invalid("Verrat manquant".into()))?;
    let date = form_date_or_today(&form, "date_realise")?;
    sqlx::query("INSERT INTO acterealiseverrat(acte_id,verrat_id,date_realise,note) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM acteprotocole WHERE id=? AND actif=1) AND EXISTS(SELECT 1 FROM verrat WHERE id=?)")
        .bind(act).bind(verrat).bind(date).bind(form_text(&form,"note")).bind(act).bind(verrat).execute(&state.pool).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn pharmacie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !matches!(session.role.as_str(), "admin" | "eleveur") {
        return Err(AppError::Forbidden);
    }
    let products=generic_rows(&state.pool,"SELECT id,produit,stock_actuel,unite,seuil_alerte,maj,note,CASE WHEN seuil_alerte IS NOT NULL AND stock_actuel<=seuil_alerte THEN 1 ELSE 0 END AS alerte FROM produitpharmacie ORDER BY alerte DESC,produit").await?;
    let movements=generic_rows(&state.pool,"SELECT id,produit,date,type,quantite,bande_code,note FROM mouvementpharmacie ORDER BY date DESC,id DESC LIMIT 300").await?;
    let bands = generic_rows(
        &state.pool,
        "SELECT code FROM bande WHERE active=1 ORDER BY date_mb DESC,code",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("produits".into(), Value::Array(products));
    ctx.insert("mouvements".into(), Value::Array(movements));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "pharmacie.html", Value::Object(ctx))
}

async fn resolution_problemes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let products = generic_rows(
        &state.pool,
        "SELECT produit,stock_actuel,unite,note FROM produitpharmacie ORDER BY produit",
    )
    .await?;
    let protocols = generic_rows(
        &state.pool,
        "SELECT libelle,cible,categorie,produit,dose,unite,voie,duree_j,delai_attente,preconisations FROM acteprotocole WHERE actif=1 AND produit IS NOT NULL ORDER BY categorie,libelle",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("produits".into(), Value::Array(products));
    ctx.insert("protocoles".into(), Value::Array(protocols));
    render(&state, "resolution_problemes.html", Value::Object(ctx))
}

async fn pharmacie_mouvement(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !matches!(session.role.as_str(), "admin" | "eleveur") {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let product = form_text(&form, "produit")
        .ok_or_else(|| AppError::Invalid("Produit obligatoire".into()))?;
    let quantity = form_f64(&form, "quantite")
        .filter(|value| *value >= 0.0)
        .ok_or_else(|| AppError::Invalid("Quantité invalide".into()))?;
    let kind = form.get("type").map(String::as_str).unwrap_or("sortie");
    if !matches!(kind, "entree" | "sortie" | "inventaire") {
        return Err(AppError::Invalid("Type de mouvement invalide".into()));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO produitpharmacie(produit,stock_actuel,unite,maj) SELECT ?,0,?,CURRENT_TIMESTAMP WHERE NOT EXISTS(SELECT 1 FROM produitpharmacie WHERE lower(produit)=lower(?))")
        .bind(&product).bind(form_text(&form,"unite").unwrap_or_else(||"doses".into())).bind(&product).execute(&mut *tx).await?;
    let stock: f64=sqlx::query_scalar("SELECT CAST(COALESCE(stock_actuel,0) AS REAL) FROM produitpharmacie WHERE lower(produit)=lower(?) LIMIT 1").bind(&product).fetch_one(&mut *tx).await?;
    let new_stock = match kind {
        "entree" => stock + quantity,
        "inventaire" => quantity,
        _ if quantity <= stock => stock - quantity,
        _ => return Err(AppError::Invalid(format!("Stock insuffisant : {stock}"))),
    };
    sqlx::query("UPDATE produitpharmacie SET stock_actuel=?,maj=CURRENT_TIMESTAMP WHERE lower(produit)=lower(?)").bind(new_stock).bind(&product).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mouvementpharmacie(produit,date,type,quantite,note,bande_code) VALUES(?,?,?,?,?,?)").bind(&product).bind(form_date(&form,"date")?).bind(kind).bind(quantity).bind(form_text(&form,"note")).bind(form_text(&form,"bande_code")).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Redirect::to("/pharmacie").into_response())
}

async fn pharmacie_regler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !matches!(session.role.as_str(), "admin" | "eleveur") {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let id = form_i64(&form, "id").ok_or_else(|| AppError::Invalid("Produit manquant".into()))?;
    sqlx::query("UPDATE produitpharmacie SET unite=?,seuil_alerte=?,note=?,maj=CURRENT_TIMESTAMP WHERE id=?").bind(form_text(&form,"unite")).bind(form_f64(&form,"seuil_alerte")).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/pharmacie").into_response())
}
async fn planning(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let events=generic_rows(&state.pool,"SELECT e.id,e.date,e.type,t.num_travail,b.code AS bande,e.produit,e.note FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id LEFT JOIN bande b ON b.id=e.bande_id WHERE date(e.date)>=date('now','-30 days') ORDER BY e.date LIMIT 500").await?;
    let tasks=generic_rows(&state.pool,"SELECT id,titre,type,bande_code,echeance,fait,note FROM tache WHERE fait=0 ORDER BY echeance IS NULL,echeance,id").await?;
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let schedule = load_band_schedule(&state.pool).await?;
    let mut calculated = Vec::new();
    for band in bands {
        for date in key_dates(band.date_mb.as_deref(), schedule) {
            calculated.push(json!({"bande":band.code,"type":date["nom"],"date":date["date"],"etat":date["etat"]}));
        }
    }
    calculated.sort_by_key(|v| v["date"].as_str().unwrap_or_default().to_string());
    let mut ctx = context(&session);
    ctx.insert("evenements".into(), Value::Array(events));
    ctx.insert("taches".into(), Value::Array(tasks));
    ctx.insert("echeances".into(), Value::Array(calculated));
    render(&state, "planning.html", Value::Object(ctx))
}
async fn stock(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let movements=generic_rows(&state.pool,"SELECT id,date,bande_code,nombre,poids,montant,libelle,destination,type_saisie,est_stock FROM mouvementstock ORDER BY date DESC,id DESC LIMIT 500").await?;
    let pharmacy=generic_rows(&state.pool,"SELECT id,produit,stock_actuel,unite,seuil_alerte,CASE WHEN seuil_alerte IS NOT NULL AND stock_actuel<=seuil_alerte THEN 1 ELSE 0 END AS alerte FROM produitpharmacie ORDER BY alerte DESC,produit").await?;
    let purchases=generic_rows(&state.pool,"SELECT produit,CAST(COALESCE(SUM(quantite*COALESCE(doses_unite,1)),0) AS REAL) AS doses_achetees,MAX(doses_unite) AS doses_unite FROM achatveto GROUP BY produit ORDER BY produit").await?;
    let mut bands_view = Vec::new();
    for band in sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?
    {
        let remaining = remaining_band_pigs(&state.pool, band.id, &band.code).await?;
        bands_view.push(json!({"id":band.id,"code":band.code,"date_mb":band.date_mb,"effectif_estime":remaining}));
    }
    let mut ctx = context(&session);
    ctx.insert("mouvements".into(), Value::Array(movements));
    ctx.insert("pharmacie".into(), Value::Array(pharmacy));
    ctx.insert("achats_veto".into(), Value::Array(purchases));
    ctx.insert("bandes_stock".into(), Value::Array(bands_view));
    render(&state, "stock.html", Value::Object(ctx))
}
async fn journal(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let rows=generic_rows(&state.pool,"SELECT id,horodatage,utilisateur,action,objet,detail,chemin FROM journal ORDER BY horodatage DESC,id DESC LIMIT 1000").await?;
    let mut ctx = context(&session);
    ctx.insert("lignes".into(), Value::Array(rows));
    render(&state, "journal.html", Value::Object(ctx))
}

async fn entretien(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT id,nom,type,site,derniere_date,frequence_jours,date(derniere_date,'+'||frequence_jours||' days') AS prochaine_date,CASE WHEN derniere_date IS NOT NULL AND date(derniere_date,'+'||frequence_jours||' days')<=date('now') THEN 1 ELSE 0 END AS en_retard,note FROM entretien ORDER BY en_retard DESC,COALESCE(prochaine_date,'9999-12-31'),nom",
    )
    .await?;
    let salles = generic_rows(
        &state.pool,
        "SELECT sa.id,si.code AS site,sa.nom,sa.type,sa.dernier_lavage,CAST(julianday('now')-julianday(sa.dernier_lavage) AS INTEGER) AS jours_depuis_lavage FROM salle sa JOIN site si ON si.id=sa.site_id ORDER BY si.code,sa.ordre,sa.nom",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("entretiens".into(), Value::Array(rows));
    ctx.insert("salles".into(), Value::Array(salles));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    ctx.insert(
        "taches".into(),
        Value::Array(
            generic_rows(
                &state.pool,
                "SELECT * FROM tache ORDER BY fait,COALESCE(echeance,'9999-12-31'),id DESC",
            )
            .await?,
        ),
    );
    render(&state, "entretien.html", Value::Object(ctx))
}

async fn entretien_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let nom = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom d’entretien obligatoire".into()))?;
    sqlx::query("INSERT INTO entretien(nom,type,site,derniere_date,frequence_jours,note) VALUES(?,?,?,?,?,?)")
        .bind(nom)
        .bind(form_text(&form, "type"))
        .bind(form_text(&form, "site"))
        .bind(form_date(&form, "derniere_date")?)
        .bind(form_i64(&form, "frequence_jours").unwrap_or(365).max(1))
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/entretien").into_response())
}

async fn entretien_date(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let date = form_date_or_today(&form, "derniere_date")?;
    sqlx::query("UPDATE entretien SET derniere_date=? WHERE id=?")
        .bind(date)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/entretien").into_response())
}

async fn entretien_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM entretien WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/entretien").into_response())
}

async fn engraissement(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.module_prestataires {
        return Err(AppError::Forbidden);
    }
    let sql = if session.role == "engraisseur" {
        format!(
            "SELECT d.id,d.horodatage,d.bande_code,d.date,d.stade,d.cause,d.poids,d.nombre,d.declare_par,d.note FROM declarationmort d JOIN bande b ON b.code=d.bande_code WHERE b.engraisseur_id={} ORDER BY d.horodatage DESC LIMIT 250",
            session.uid
        )
    } else {
        "SELECT id,horodatage,bande_code,date,stade,cause,poids,nombre,declare_par,note FROM declarationmort ORDER BY horodatage DESC LIMIT 250".to_string()
    };
    let declarations = generic_rows(&state.pool, &sql).await?;
    let band_sql = if session.role == "engraisseur" {
        format!(
            "SELECT id,code,instructions,poids_cible FROM bande WHERE active=1 AND engraisseur_id={} ORDER BY date_mb DESC,code",
            session.uid
        )
    } else {
        "SELECT id,code,instructions,poids_cible FROM bande WHERE active=1 ORDER BY date_mb DESC,code"
            .to_string()
    };
    let bands = generic_rows(&state.pool, &band_sql).await?;
    // Effectif réel par bande (au lieu d'un total générique d'engraissement
    // toutes bandes confondues) : même calcul que la fiche bande
    // (`total_band_pigs`), pour que le prestataire/engraisseur sache combien
    // de porcs sont réellement présents pour chaque bande qui lui est
    // confiée.
    let mut effectifs_bandes = Vec::new();
    for band in &bands {
        let (Some(id), Some(code)) = (
            band.get("id").and_then(Value::as_i64),
            band.get("code").and_then(Value::as_str),
        ) else {
            continue;
        };
        let effectif = total_band_pigs(&state.pool, id, code).await?;
        effectifs_bandes.push(json!({"code": code, "effectif": effectif}));
    }
    let cases = generic_rows(
        &state.pool,
        "SELECT c.id,COALESCE(si.nom,si.code)||' · '||s.nom||' · '||c.nom AS nom FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.nom,s.ordre,c.nom",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("declarations".into(), Value::Array(declarations));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("effectifs_bandes".into(), Value::Array(effectifs_bandes));
    ctx.insert("cases".into(), Value::Array(cases));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "prestataire.html", Value::Object(ctx))
}

async fn declaration_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session, &form)?;
    let band = form_text(&form, "bande_code")
        .ok_or_else(|| AppError::Invalid("Bande obligatoire".into()))?;
    if session.role == "engraisseur" {
        let authorized: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM bande WHERE code=? AND engraisseur_id=? AND active=1",
        )
        .bind(&band)
        .bind(session.uid)
        .fetch_one(&state.pool)
        .await?;
        if authorized == 0 {
            return Err(AppError::Forbidden);
        }
    } else {
        require_writer(&session)?;
    }
    let number = form_i64(&form, "nombre")
        .filter(|value| *value > 0 && *value <= 10_000)
        .ok_or_else(|| AppError::Invalid("Nombre invalide".into()))?;
    let case_id = form_i64(&form, "case_id");
    let mut stade = form_text(&form, "stade");
    if let Some(case_id) = case_id {
        let present = case_pig_count(&state.pool, case_id).await?;
        if number > present {
            return Err(AppError::Invalid(format!(
                "Effectif insuffisant dans la case : {present} porc(s) présent(s)"
            )));
        }
        // La case choisie fait foi : évite un stade saisi manuellement en
        // décalage avec la salle réellement sélectionnée.
        if let Some(deduced) = stade_from_case(&state.pool, case_id).await? {
            stade = Some(deduced);
        }
    }
    sqlx::query("INSERT INTO declarationmort(bande_code,date,stade,case_id,cause,poids,nombre,declare_par,note) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(&band)
        .bind(form_date_or_today(&form, "date")?)
        .bind(stade)
        .bind(case_id)
        .bind(form_text(&form, "cause"))
        .bind(form_f64(&form, "poids"))
        .bind(number)
        .bind(&session.identifiant)
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/engraissement").into_response())
}

/// Vrai si la quarantaine associée à une réception est toujours active à la
/// date donnée (`quarantaine_jusqu` inclus). Fonction pure pour être testable
/// indépendamment de la base.
fn quarantaine_active(quarantaine_jusqu: Option<&str>, today: NaiveDate) -> bool {
    quarantaine_jusqu
        .and_then(parse_stored_date)
        .is_some_and(|jusqu| jusqu >= today)
}

async fn reception(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.recoit_achats() {
        return Err(AppError::Invalid(
            "Ce type d'élevage ne reçoit pas d'animaux achetés (changez-le dans Paramètres si besoin).".into(),
        ));
    }
    let receptions = generic_rows(
        &state.pool,
        "SELECT id,date,fournisseur,num_bon_livraison,lot_origine_fournisseur,bande_code,effectif,poids_moyen,poids_total,prix_total,quarantaine_jusqu,note FROM receptionachat ORDER BY date DESC,id DESC LIMIT 200",
    )
    .await?;
    let today = today_iso();
    let today_date = Local::now().date_naive();
    let mut quarantaines_actives = generic_rows(
        &state.pool,
        "SELECT id,date,bande_code,effectif,fournisseur,quarantaine_jusqu FROM receptionachat WHERE quarantaine_jusqu IS NOT NULL ORDER BY quarantaine_jusqu",
    )
    .await?;
    quarantaines_actives.retain(|row| {
        quarantaine_active(
            row.get("quarantaine_jusqu").and_then(Value::as_str),
            today_date,
        )
    });
    let bands = generic_rows(
        &state.pool,
        "SELECT code FROM bande WHERE active=1 ORDER BY code",
    )
    .await?;
    let cases = generic_rows(
        &state.pool,
        "SELECT c.id,COALESCE(si.nom,si.code)||' · '||s.nom||' · '||c.nom AS nom FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.nom,s.ordre,c.nom",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("receptions".into(), Value::Array(receptions));
    ctx.insert(
        "quarantaines_actives".into(),
        Value::Array(quarantaines_actives),
    );
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("cases".into(), Value::Array(cases));
    ctx.insert("today".into(), json!(today));
    render(&state, "reception.html", Value::Object(ctx))
}

async fn reception_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    if !session.recoit_achats() {
        return Err(AppError::Invalid(
            "Ce type d'élevage ne reçoit pas d'animaux achetés (changez-le dans Paramètres si besoin).".into(),
        ));
    }
    let bande_code = form_text(&form, "bande_code")
        .ok_or_else(|| AppError::Invalid("Lot (code de bande) obligatoire".into()))?;
    let effectif = form_i64(&form, "effectif")
        .filter(|value| *value > 0 && *value <= 10_000)
        .ok_or_else(|| AppError::Invalid("Effectif invalide".into()))?;
    let date = form_date_or_today(&form, "date")?;
    let case_id = form_i64(&form, "case_id");
    let quarantaine_jusqu = form_i64(&form, "quarantaine_jours")
        .filter(|jours| *jours > 0)
        .and_then(|jours| {
            parse_stored_date(&date).map(|start| {
                (start + Duration::days(jours))
                    .format("%Y-%m-%d")
                    .to_string()
            })
        });

    let mut tx = state.pool.begin().await?;
    // Le lot (bande) est créé au premier arrivage s'il n'existe pas déjà ; les
    // réceptions suivantes sur le même code s'y rattachent sans le dupliquer.
    sqlx::query("INSERT INTO bande(code,note,active) SELECT ?,?,1 WHERE NOT EXISTS(SELECT 1 FROM bande WHERE code=?)")
        .bind(&bande_code)
        .bind(form_text(&form, "note"))
        .bind(&bande_code)
        .execute(&mut *tx)
        .await?;
    let band_id: i64 = sqlx::query_scalar("SELECT id FROM bande WHERE code=? ORDER BY id LIMIT 1")
        .bind(&bande_code)
        .fetch_one(&mut *tx)
        .await?;
    // Alimente le même registre de mouvements que le reste de l'effectif (voir
    // remaining_band_pigs/total_band_pigs) : la réception devient le nouveau
    // point de départ du décompte pour ce lot, jamais une valeur figée.
    let mouvementstock_id = sqlx::query("INSERT INTO mouvementstock(date,bande_code,nombre,poids,montant,libelle,destination,type_saisie,est_stock) VALUES(?,?,?,?,?,'réception achat',NULL,'reception',1)")
        .bind(&date)
        .bind(&bande_code)
        .bind(effectif)
        .bind(form_f64(&form, "poids_total"))
        .bind(form_f64(&form, "prix_total"))
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    let transfert_id = if let Some(case_id) = case_id {
        Some(
            sqlx::query("INSERT INTO transfert(date,espece,bande_id,case_dest_id,nombre,note) VALUES(?,'porc',?,?,?,'Réception achat')")
                .bind(&date)
                .bind(band_id)
                .bind(case_id)
                .bind(effectif)
                .execute(&mut *tx)
                .await?
                .last_insert_rowid(),
        )
    } else {
        None
    };
    sqlx::query("INSERT INTO receptionachat(date,fournisseur,num_bon_livraison,lot_origine_fournisseur,bande_code,case_id,effectif,poids_moyen,poids_total,prix_total,quarantaine_jusqu,note,cree_par,mouvementstock_id,transfert_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&date)
        .bind(form_text(&form, "fournisseur"))
        .bind(form_text(&form, "num_bon_livraison"))
        .bind(form_text(&form, "lot_origine_fournisseur"))
        .bind(&bande_code)
        .bind(case_id)
        .bind(effectif)
        .bind(form_f64(&form, "poids_moyen"))
        .bind(form_f64(&form, "poids_total"))
        .bind(form_f64(&form, "prix_total"))
        .bind(&quarantaine_jusqu)
        .bind(form_text(&form, "note"))
        .bind(&session.identifiant)
        .bind(mouvementstock_id)
        .bind(transfert_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "recevoir",
        "réception",
        &format!("{bande_code} · {effectif} porc(s) · {date}"),
        "/reception",
    )
    .await;
    Ok(Redirect::to("/reception").into_response())
}

async fn genetique(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.module_genetique {
        return Err(AppError::Invalid(
            "Le module Génétique avancée n'est pas activé (Paramètres > Type d'élevage et modules).".into(),
        ));
    }
    let lignees = generic_rows(
        &state.pool,
        "SELECT l.id,l.nom,l.fournisseur,l.index_prolificite,l.index_croissance,l.index_ic,l.contrat_renouvellement,l.note,(SELECT COUNT(*) FROM truie t WHERE t.lignee_id=l.id) AS truies FROM lignee_genetique l ORDER BY l.nom",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("lignees".into(), Value::Array(lignees));
    ctx.insert(
        "catalogue".into(),
        serde_json::from_str(include_str!("../../resources/danbred-2026-08.json"))
            .map_err(|e| AppError::Internal(e.into()))?,
    );
    render(&state, "genetique.html", Value::Object(ctx))
}

async fn genetique_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    if !session.module_genetique {
        return Err(AppError::Invalid(
            "Le module Génétique avancée n'est pas activé (Paramètres > Type d'élevage et modules).".into(),
        ));
    }
    let nom = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom de la lignée obligatoire".into()))?;
    sqlx::query("INSERT INTO lignee_genetique(nom,fournisseur,index_prolificite,index_croissance,index_ic,contrat_renouvellement,note) VALUES(?,?,?,?,?,?,?)")
        .bind(&nom)
        .bind(form_text(&form, "fournisseur"))
        .bind(form_f64(&form, "index_prolificite"))
        .bind(form_f64(&form, "index_croissance"))
        .bind(form_f64(&form, "index_ic"))
        .bind(form_text(&form, "contrat_renouvellement"))
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/genetique").into_response())
}

async fn genetique_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    // `truie.lignee_id` référence cette table : supprimer une lignée encore
    // utilisée déclencherait une violation de clé étrangère (erreur technique).
    // On refuse explicitement, en indiquant combien de truies sont concernées.
    let rattachees: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE lignee_id=?")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if rattachees > 0 {
        return Err(AppError::Invalid(format!(
            "Lignée utilisée par {rattachees} truie(s) : détache-les depuis leur fiche avant de la supprimer."
        )));
    }
    sqlx::query("DELETE FROM lignee_genetique WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/genetique").into_response())
}

async fn aliment_previsions(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let aliment_delai_commande_jours =
        reglage_i64(&state.pool, "aliment_delai_commande_jours", 5).await?;
    let silos = generic_rows(
        &state.pool,
        "SELECT id,nom,capacite_tonnes FROM silo_aliment WHERE actif=1 ORDER BY nom",
    )
    .await?;
    let mut previsions = Vec::new();
    for silo in &silos {
        let Some(id) = silo.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let nom = silo
            .get("nom")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let readings: Vec<(String, f64)> = sqlx::query_as(
            "SELECT date,niveau_tonnes FROM releve_silo WHERE silo_id=? ORDER BY date DESC,id DESC LIMIT 2",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
        let history = generic_rows(
            &state.pool,
            "SELECT id,date,niveau_tonnes,note FROM releve_silo WHERE silo_id=? ORDER BY date DESC,id DESC LIMIT 20",
        )
        .await?;
        let capacite = silo.get("capacite_tonnes").and_then(Value::as_f64);
        // Consommation réellement mesurée par une machine à soupe (import
        // Histo_fab), quand l'éleveur en a relié une à ce silo — purement
        // informatif : ne remplace pas le bilan de matière ci-dessous (basé
        // sur les relevés manuels), qui reste la seule source utilisée pour
        // « jours avant rupture »/« quantité à commander ». Les unités
        // machine (souvent litres ou kg) ne sont pas forcément des tonnes ;
        // affiché tel quel plutôt que converti au hasard.
        let machine_90j: f64 = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(quantite_recue),0) AS REAL) FROM consommationsoupe WHERE silo_id=? AND date>=date('now','-90 days')",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        let mut entry = json!({
            "id": id, "nom": nom, "capacite_tonnes": silo.get("capacite_tonnes"),
            "historique": history, "niveau_actuel": Value::Null,
            "consommation_quotidienne": Value::Null, "jours_avant_rupture": Value::Null,
            "quantite_a_commander": Value::Null, "commande_urgente": false,
            "machine_soupe_90j": if machine_90j > 0.0 { json!((machine_90j * 100.0).round() / 100.0) } else { Value::Null },
        });
        if let [(date_actuel, niveau_actuel), (date_precedent, niveau_precedent)] =
            readings.as_slice()
        {
            if let (Some(actuel), Some(precedent)) = (
                parse_stored_date(date_actuel),
                parse_stored_date(date_precedent),
            ) {
                let jours = (actuel - precedent).num_days();
                let livraisons: f64 = sqlx::query_scalar(
                    "SELECT CAST(COALESCE(SUM(tonnage),0) AS REAL) FROM livraisonaliment WHERE lower(trim(COALESCE(silo,'')))=lower(trim(?)) AND date>? AND date<=?",
                )
                .bind(&nom)
                .bind(date_precedent)
                .bind(date_actuel)
                .fetch_one(&state.pool)
                .await?;
                let conso = consommation_quotidienne_tonnes(
                    *niveau_precedent,
                    livraisons,
                    *niveau_actuel,
                    jours,
                );
                let rupture = conso.and_then(|c| jours_avant_rupture(*niveau_actuel, c));
                if let Some(object) = entry.as_object_mut() {
                    object.insert("niveau_actuel".into(), json!(niveau_actuel));
                    object.insert(
                        "consommation_quotidienne".into(),
                        json!(conso.map(|v| (v * 100.0).round() / 100.0)),
                    );
                    object.insert(
                        "jours_avant_rupture".into(),
                        json!(rupture.map(|v| v.round())),
                    );
                    object.insert(
                        "quantite_a_commander".into(),
                        json!(quantite_a_commander(*niveau_actuel, capacite)),
                    );
                    object.insert(
                        "commande_urgente".into(),
                        json!(commande_urgente(rupture, aliment_delai_commande_jours)),
                    );
                }
            }
        } else if let [(_, niveau_actuel)] = readings.as_slice() {
            if let Some(object) = entry.as_object_mut() {
                object.insert("niveau_actuel".into(), json!(niveau_actuel));
                object.insert(
                    "quantite_a_commander".into(),
                    json!(quantite_a_commander(*niveau_actuel, capacite)),
                );
            }
        }
        previsions.push(entry);
    }
    // Consommation d'aliment par bande (§3 « prévision de consommation… par
    // bande ») : total livré aux 90 derniers jours, pour les bandes encore
    // actives — donne une visibilité par bande complémentaire à la vue par
    // silo ci-dessus, sans inventer une projection qu'aucune donnée ne
    // permet de dater avec confiance (poids cible/effectif restant varient
    // trop pour un chiffre fiable sans intervention de l'éleveur).
    //
    // Vrai bug corrigé ici : une livraison d'aliment est très souvent
    // affectée à plusieurs bandes à la fois (une même facture couvrant
    // plusieurs lots, `affectationfacturebande`, comme pour le reste de
    // l'économie — voir `auto_assign_economic_invoices`). Cette section
    // rejoignait directement `livraisonaliment.bande_id`, la seule bande
    // « principale » historique, et ignorait donc silencieusement les
    // autres bandes d'une même facture répartie : leur consommation
    // n'apparaissait nulle part. Rejoint maintenant sur
    // `affectationfacturebande` (comme les coûts économiques) et répartit
    // le tonnage à parts égales entre toutes les bandes affectées à une
    // même facture, plutôt que de le compter en entier sur chacune.
    let consommation_bandes = generic_rows(
        &state.pool,
        "SELECT b.id,b.code,CAST(COALESCE(SUM(l.tonnage/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=l.id)),0) AS REAL) AS tonnage_90j FROM bande b JOIN affectationfacturebande af ON af.categorie='aliment' AND af.bande_id=b.id JOIN livraisonaliment l ON l.id=af.facture_id WHERE b.active=1 AND l.date>=date('now','-90 days') GROUP BY b.id,b.code ORDER BY tonnage_90j DESC",
    )
    .await?;
    let sites = generic_rows(&state.pool, "SELECT id,code,nom FROM site ORDER BY code").await?;
    let mut ctx = context(&session);
    ctx.insert("previsions".into(), Value::Array(previsions));
    ctx.insert(
        "consommation_bandes".into(),
        Value::Array(consommation_bandes),
    );
    ctx.insert("sites".into(), Value::Array(sites));
    ctx.insert("today".into(), json!(today_iso()));
    render(&state, "aliment_previsions.html", Value::Object(ctx))
}

async fn silo_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let nom = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom du silo obligatoire".into()))?;
    sqlx::query("INSERT INTO silo_aliment(nom,site_id,capacite_tonnes,actif) VALUES(?,?,?,1)")
        .bind(&nom)
        .bind(form_i64(&form, "site_id"))
        .bind(form_f64(&form, "capacite_tonnes"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/aliment-previsions").into_response())
}

async fn silo_releve_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let niveau = form_f64(&form, "niveau_tonnes")
        .filter(|value| *value >= 0.0)
        .ok_or_else(|| AppError::Invalid("Niveau invalide".into()))?;
    sqlx::query("INSERT INTO releve_silo(silo_id,date,niveau_tonnes,note) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM silo_aliment WHERE id=?)")
        .bind(id)
        .bind(form_date_or_today(&form, "date")?)
        .bind(niveau)
        .bind(form_text(&form, "note"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/aliment-previsions").into_response())
}

async fn silo_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE silo_aliment SET actif=0 WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/aliment-previsions").into_response())
}

/// Importe un export « Histo_fab » de machine à soupe Asserva (§ « aliment
/// et stock » des demandes en attente). Étape 1/2 : dépose chaque gâchée
/// dans `importligne` (même mécanisme que l'import truies) et présente les
/// produits distincts trouvés, pour que l'éleveur les relie lui-même à un
/// silo existant — aucune correspondance de nom devinée automatiquement.
async fn machine_soupe_import(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let mut data = None;
    let mut filename = "histo_fab.csv".to_string();
    let mut csrf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        match field.name().map(str::to_string).as_deref() {
            Some("csrf_token") => {
                csrf = Some(
                    field
                        .text()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                )
            }
            Some("fichier") => {
                filename = field
                    .file_name()
                    .unwrap_or("histo_fab.csv")
                    .chars()
                    .filter(|character| character.is_alphanumeric() || ".-_ ".contains(*character))
                    .take(180)
                    .collect();
                data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|error| AppError::Invalid(error.to_string()))?,
                );
            }
            _ => {}
        }
    }
    if csrf.as_deref() != Some(session.csrf.as_str()) {
        return Err(AppError::Forbidden);
    }
    let bytes = data.ok_or_else(|| AppError::Invalid("Fichier CSV manquant".into()))?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err(AppError::Invalid("Fichier trop volumineux".into()));
    }
    let digest = contenu_sha256(&bytes);
    let lignes = machine_soupe::parse_fabrication_csv(&bytes).map_err(AppError::Invalid)?;
    if lignes.is_empty() {
        return Err(AppError::Invalid(
            "Aucune gâchée avec un produit réel et une quantité non nulle dans ce fichier".into(),
        ));
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut tx = state.pool.begin().await?;
    refuser_fichier_deja_importe(&mut tx, &digest).await?;
    sqlx::query(
        "UPDATE importjournal SET statut='expire' WHERE statut='apercu' AND cree_le<datetime('now','-1 day')",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par,contenu_sha256) VALUES(?,'machine_soupe',?,'apercu',?,?)")
        .bind(&token)
        .bind(&filename)
        .bind(session.uid)
        .bind(&digest)
        .execute(&mut *tx)
        .await?;
    let mut seen = HashSet::new();
    for (index, ligne) in lignes.iter().enumerate() {
        let deja_importe: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM consommationsoupe WHERE COALESCE(date,'')=COALESCE(?,'') AND COALESCE(heure_debut,'')=COALESCE(?,'') AND lower(trim(produit_machine))=lower(trim(?))",
        )
        .bind(&ligne.date)
        .bind(&ligne.heure_debut)
        .bind(&ligne.produit)
        .fetch_one(&mut *tx)
        .await?;
        let key = format!(
            "{}|{}|{}",
            ligne.date.as_deref().unwrap_or_default(),
            ligne.heure_debut.as_deref().unwrap_or_default(),
            ligne.produit.trim().to_lowercase()
        );
        let (action, anomalie) = if !seen.insert(key) {
            ("erreur", Some("Doublon dans le fichier".to_string()))
        } else if deja_importe > 0 {
            ("erreur", Some("Déjà importé".to_string()))
        } else {
            ("ajouter", None)
        };
        let payload = json!(ligne);
        sqlx::query("INSERT INTO importligne(token,numero_ligne,action,anomalie,donnees_json) VALUES(?,?,?,?,?)")
            .bind(&token)
            .bind(index as i64 + 1)
            .bind(action)
            .bind(&anomalie)
            .bind(payload.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    if lignes.len() as i64
        != sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM importligne WHERE token=? AND action='ajouter'",
        )
        .bind(&token)
        .fetch_one(&state.pool)
        .await?
    {
        return Err(AppError::Invalid(
            "Import refusé : le fichier contient une consommation déjà présente ou en double"
                .into(),
        ));
    }

    let a_importer: Vec<machine_soupe::LigneFabrication> = sqlx::query_scalar::<_, String>(
        "SELECT donnees_json FROM importligne WHERE token=? AND action='ajouter' ORDER BY numero_ligne",
    )
    .bind(&token)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .filter_map(|raw| serde_json::from_str(&raw).ok())
    .collect();
    let ignorees = lignes.len() as i64 - a_importer.len() as i64;
    let mut produits = Vec::new();
    for nom in machine_soupe::produits_distincts(&a_importer) {
        let sous_total: f64 = a_importer
            .iter()
            .filter(|l| l.produit == nom)
            .map(|l| l.quantite_recue)
            .sum();
        let nb_gachees = a_importer.iter().filter(|l| l.produit == nom).count();
        produits.push(json!({"nom": nom, "nb_gachees": nb_gachees, "total_recue": (sous_total * 100.0).round() / 100.0}));
    }
    let silos = generic_rows(
        &state.pool,
        "SELECT id,nom FROM silo_aliment WHERE actif=1 ORDER BY nom",
    )
    .await?;

    let mut ctx = context(&session);
    ctx.insert("token".into(), json!(token));
    ctx.insert("nom_fichier".into(), json!(filename));
    ctx.insert("nb_gachees".into(), json!(a_importer.len()));
    ctx.insert("nb_ignorees".into(), json!(ignorees));
    ctx.insert("produits".into(), Value::Array(produits));
    ctx.insert("silos".into(), Value::Array(silos));
    Ok(render(&state, "machine_soupe_apercu.html", Value::Object(ctx))?.into_response())
}

/// Étape 2/2 : applique la correspondance produit → silo choisie par
/// l'éleveur (un champ `silo_{index}` par produit distinct, valeur
/// « nouveau:Nom du silo » pour en créer un, ou l'id d'un silo existant) et
/// enregistre chaque gâchée dans `consommationsoupe`.
async fn machine_soupe_import_confirmer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    let mut tx = state.pool.begin().await?;
    let owner: Option<i64> = sqlx::query_scalar(
        "SELECT cree_par FROM importjournal WHERE token=? AND statut='apercu' AND type_import='machine_soupe'",
    )
    .bind(&token)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if owner != Some(session.uid) && !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let lignes: Vec<machine_soupe::LigneFabrication> = sqlx::query_scalar::<_, String>(
        "SELECT donnees_json FROM importligne WHERE token=? AND action='ajouter' ORDER BY numero_ligne",
    )
    .bind(&token)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .filter_map(|raw| serde_json::from_str(&raw).ok())
    .collect();
    if lignes.is_empty() {
        return Err(AppError::Invalid(
            "Aucune gâchée à importer (aperçu vide ou déjà appliqué)".into(),
        ));
    }

    let mut silo_par_produit: HashMap<String, i64> = HashMap::new();
    for (index, produit) in machine_soupe::produits_distincts(&lignes)
        .into_iter()
        .enumerate()
    {
        let choix = form
            .get(&format!("silo_{index}"))
            .map(String::as_str)
            .unwrap_or("");
        let silo_id = if let Some(nom_nouveau) = choix.strip_prefix("nouveau:") {
            let nom = if nom_nouveau.trim().is_empty() {
                produit.clone()
            } else {
                nom_nouveau.trim().to_string()
            };
            let existant: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM silo_aliment WHERE lower(trim(nom))=lower(trim(?))",
            )
            .bind(&nom)
            .fetch_optional(&mut *tx)
            .await?;
            match existant {
                Some(id) => id,
                None => sqlx::query("INSERT INTO silo_aliment(nom,actif) VALUES(?,1)")
                    .bind(&nom)
                    .execute(&mut *tx)
                    .await?
                    .last_insert_rowid(),
            }
        } else {
            choix
                .parse::<i64>()
                .map_err(|_| AppError::Invalid(format!("Silo non choisi pour « {produit} »")))?
        };
        silo_par_produit.insert(produit, silo_id);
    }

    for ligne in &lignes {
        let Some(silo_id) = silo_par_produit.get(&ligne.produit) else {
            continue;
        };
        let deja_importe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM consommationsoupe WHERE COALESCE(date,'')=COALESCE(?,'') AND COALESCE(heure_debut,'')=COALESCE(?,'') AND lower(trim(produit_machine))=lower(trim(?))")
            .bind(&ligne.date).bind(&ligne.heure_debut).bind(&ligne.produit)
            .fetch_one(&mut *tx).await?;
        if deja_importe > 0 {
            return Err(AppError::Invalid(format!(
                "Consommation déjà importée : {} {} {}",
                ligne.date.as_deref().unwrap_or_default(),
                ligne.heure_debut.as_deref().unwrap_or_default(),
                ligne.produit
            )));
        }
        sqlx::query("INSERT INTO consommationsoupe(date,heure_debut,no_formule,produit_machine,silo_id,quantite_consigne,quantite_recue,token_import) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&ligne.date)
            .bind(&ligne.heure_debut)
            .bind(ligne.no_formule)
            .bind(&ligne.produit)
            .bind(silo_id)
            .bind(ligne.quantite_consigne)
            .bind(ligne.quantite_recue)
            .bind(&token)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE importjournal SET statut='applique', applique_le=CURRENT_TIMESTAMP WHERE token=?",
    )
    .bind(&token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Redirect::to("/aliment-previsions").into_response())
}

async fn machine_soupe_import_annuler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let token = form_text(&form, "token")
        .ok_or_else(|| AppError::Invalid("Aperçu d'import manquant".into()))?;
    sqlx::query(
        "DELETE FROM importjournal WHERE token=? AND statut='apercu' AND (cree_par=? OR ?='admin')",
    )
    .bind(token)
    .bind(session.uid)
    .bind(&session.role)
    .execute(&state.pool)
    .await?;
    Ok(Redirect::to("/aliment-previsions").into_response())
}

async fn reception_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    // Annule aussi le mouvement d'effectif et le transfert générés à la
    // réception : supprimer la réception ne doit jamais laisser un effectif
    // fantôme dans le registre (principe §10 de la spécification).
    let refs: Option<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT mouvementstock_id,transfert_id FROM receptionachat WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM receptionachat WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let Some((mouvementstock_id, transfert_id)) = refs {
        if let Some(mouvementstock_id) = mouvementstock_id {
            sqlx::query("DELETE FROM mouvementstock WHERE id=?")
                .bind(mouvementstock_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(transfert_id) = transfert_id {
            sqlx::query("DELETE FROM transfert WHERE id=?")
                .bind(transfert_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(Redirect::to("/reception").into_response())
}

async fn declaration_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session, &form)?;
    if session.role == "engraisseur" {
        sqlx::query("DELETE FROM declarationmort WHERE id=? AND declare_par=?")
            .bind(id)
            .bind(&session.identifiant)
            .execute(&state.pool)
            .await?;
    } else {
        require_writer(&session)?;
        sqlx::query("DELETE FROM declarationmort WHERE id=?")
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Redirect::to("/engraissement").into_response())
}

async fn abattoir(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if session.role == "salarie" {
        return Err(AppError::Forbidden);
    }
    let ventes = generic_rows(
        &state.pool,
        "WITH legacy AS (SELECT v.id,v.date,v.num_apport,json_extract(j.value,'$.ref') AS lot_ref,b.code AS bande,CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER) AS nb_porcs,CAST(json_extract(j.value,'$.poids') AS REAL) AS poids_total,ROUND(CAST(json_extract(j.value,'$.poids') AS REAL)/NULLIF(CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER),0),2) AS poids_moyen,CAST(json_extract(j.value,'$.muscle_lot') AS REAL) AS tmp,v.tx_qualification,v.plus_value,CAST(json_extract(j.value,'$.montant_ht') AS REAL) AS montant_ht FROM venteapport v,json_each(v.lots_json) j LEFT JOIN bande b ON b.id=CAST(json_extract(j.value,'$.bande_id') AS INTEGER) WHERE json_valid(v.lots_json) AND json_type(v.lots_json)='array' AND json_array_length(v.lots_json)>1 AND v.nb_porcs=(SELECT SUM(CAST(json_extract(x.value,'$.nb_porcs') AS INTEGER)) FROM json_each(v.lots_json) x)), direct AS (SELECT v.id,v.date,v.num_apport,v.frappe AS lot_ref,b.code AS bande,v.nb_porcs,v.poids_total,v.poids_moyen,v.tmp,v.tx_qualification,v.plus_value,v.montant_ht FROM venteapport v LEFT JOIN bande b ON b.id=v.bande_id WHERE NOT (json_valid(v.lots_json) AND json_type(v.lots_json)='array' AND json_array_length(v.lots_json)>1 AND v.nb_porcs=(SELECT SUM(CAST(json_extract(x.value,'$.nb_porcs') AS INTEGER)) FROM json_each(v.lots_json) x))) SELECT * FROM legacy UNION ALL SELECT * FROM direct ORDER BY date DESC,id DESC LIMIT 250",
    )
    .await?;
    let saisies = generic_rows(
        &state.pool,
        "SELECT id,date,bande_code,num_apport,morceau,nombre,motif,note FROM saisieabattoir ORDER BY date DESC,id DESC LIMIT 250",
    )
    .await?;
    let bandes = generic_rows(
        &state.pool,
        "SELECT id,code FROM bande ORDER BY active DESC,date_mb DESC,id DESC",
    )
    .await?;
    let total_abattus: i64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
    let ht_total: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM venteapport")
            .fetch_one(&state.pool)
            .await?;
    let poids_moy: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(COALESCE(poids_moyen,0)*COALESCE(nb_porcs,0))/NULLIF(SUM(COALESCE(nb_porcs,0)),0) FROM venteapport WHERE poids_moyen IS NOT NULL",
    )
    .fetch_one(&state.pool)
    .await?;
    let tmp_moy: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(COALESCE(tmp,0)*COALESCE(nb_porcs,0))/NULLIF(SUM(COALESCE(nb_porcs,0)),0) FROM venteapport WHERE tmp IS NOT NULL",
    )
    .fetch_one(&state.pool)
    .await?;
    let prix_ht_kg: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(COALESCE(montant_ht,0))/NULLIF(SUM(COALESCE(poids_total,0)),0) FROM venteapport",
    )
    .fetch_one(&state.pool)
    .await?;
    let total_saisies: i64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM saisieabattoir")
            .fetch_one(&state.pool)
            .await?;
    let synthesis = generic_rows(
        &state.pool,
        "WITH v AS (SELECT bande_id,SUM(COALESCE(nb_porcs,0)) AS porcs,SUM(COALESCE(poids_total,0)) AS poids,SUM(COALESCE(montant_ht,0)) AS ht,SUM(COALESCE(tmp,0)*COALESCE(nb_porcs,0))/NULLIF(SUM(COALESCE(nb_porcs,0)),0) AS tmp FROM ventelot GROUP BY bande_id),s AS (SELECT bande_code,SUM(COALESCE(nombre,0)) AS saisies FROM saisieabattoir GROUP BY bande_code) SELECT b.id,b.code,CAST(COALESCE(v.porcs,0) AS INTEGER) AS porcs,ROUND(v.poids/NULLIF(v.porcs,0),1) AS poids_moyen,ROUND(v.tmp,2) AS tmp,ROUND(v.ht/NULLIF(v.poids,0),3) AS prix_ht_kg,ROUND(v.ht,2) AS ht,CAST(COALESCE(s.saisies,0) AS INTEGER) AS saisies FROM bande b LEFT JOIN v ON v.bande_id=b.id LEFT JOIN s ON s.bande_code=b.code WHERE v.porcs IS NOT NULL OR s.saisies IS NOT NULL ORDER BY b.date_mb DESC,b.id DESC",
    )
    .await?;
    let seizure_causes = generic_rows(
        &state.pool,
        "SELECT morceau,COALESCE(NULLIF(motif,''),'Non précisé') AS motif,CAST(SUM(nombre) AS INTEGER) AS nombre FROM saisieabattoir GROUP BY morceau,COALESCE(NULLIF(motif,''),'Non précisé') ORDER BY nombre DESC,morceau LIMIT 30",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("ventes".into(), Value::Array(ventes));
    ctx.insert("saisies".into(), Value::Array(saisies));
    ctx.insert("bandes".into(), Value::Array(bandes));
    ctx.insert("synthese".into(), Value::Array(synthesis));
    ctx.insert("causes_saisies".into(), Value::Array(seizure_causes));
    ctx.insert(
        "stats".into(),
        json!({"total_abattus":total_abattus,"ht_total":ht_total,"poids_moy":poids_moy,"tmp_moy":tmp_moy,"prix_ht_kg":prix_ht_kg,"total_saisies":total_saisies}),
    );
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "abattoir.html", Value::Object(ctx))
}

async fn abattoir_saisie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let morceau = form_text(&form, "morceau")
        .ok_or_else(|| AppError::Invalid("Morceau obligatoire".into()))?;
    sqlx::query("INSERT INTO saisieabattoir(date,bande_code,num_apport,morceau,nombre,motif,note) VALUES(?,?,?,?,?,?,?)")
        .bind(form_date_or_today(&form, "date")?)
        .bind(form_text(&form, "bande_code"))
        .bind(form_text(&form, "num_apport"))
        .bind(morceau)
        .bind(form_i64(&form, "nombre").unwrap_or(1).max(1))
        .bind(form_text(&form, "motif"))
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/abattoir#saisies").into_response())
}

async fn abattoir_saisie_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM saisieabattoir WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/abattoir#saisies").into_response())
}

/// Page « Cahiers des charges » retirée (§3) : son tableau est désormais
/// intégré à `/economique` (voir `economique()`). Redirection conservée
/// pour tout lien ou favori existant vers l'ancienne URL.
async fn cahiers() -> Response {
    Redirect::to("/economique#cahiers").into_response()
}

async fn cahier_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let nom = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom du cahier obligatoire".into()))?;
    sqlx::query("INSERT INTO cahiercharges(nom,valeur_par_porc,actif,note) VALUES(?,?,1,?)")
        .bind(nom)
        .bind(form_f64(&form, "valeur_par_porc"))
        .bind(form_text(&form, "note"))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique#cahiers").into_response())
}

async fn cahier_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE cahiercharges SET valeur_par_porc=?,actif=?,note=? WHERE id=?")
        .bind(form_f64(&form, "valeur_par_porc"))
        .bind(form.contains_key("actif"))
        .bind(form_text(&form, "note"))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique#cahiers").into_response())
}

async fn cahier_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM cahiercharges WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/economique#cahiers").into_response())
}

async fn quotidien(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let today = Local::now().date_naive();
    let jour = query
        .get("jour")
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .unwrap_or(today)
        .format("%Y-%m-%d")
        .to_string();
    let rows = sqlx::query("SELECT id,date,horodatage,categorie,salle_nom,element,statut,note,utilisateur FROM controlequotidien WHERE date=? ORDER BY horodatage DESC,id DESC LIMIT 200")
        .bind(&jour)
        .fetch_all(&state.pool)
        .await?;
    let mut ctx = context(&session);
    ctx.insert("jour".into(), json!(jour));
    ctx.insert("controles".into(), Value::Array(rows_to_json(rows)?));
    let page = query
        .get("page")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(1)
        .clamp(1, 1_000_000);
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM controlequotidien WHERE categorie='note_libre'")
            .fetch_one(&state.pool)
            .await?;
    let notes=sqlx::query("SELECT id,date,note,utilisateur FROM controlequotidien WHERE categorie='note_libre' ORDER BY date DESC,id DESC LIMIT 30 OFFSET ?").bind((page-1)*30).fetch_all(&state.pool).await?;
    ctx.insert(
        "historique_notes".into(),
        Value::Array(rows_to_json(notes)?),
    );
    ctx.insert("page".into(), json!(page));
    ctx.insert("plus_notes".into(), json!(page * 30 < total));
    ctx.insert("total_notes".into(), json!(total));
    render(&state, "quotidien.html", Value::Object(ctx))
}

async fn quotidien_note(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let jour = form_text(&form, "jour")
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    if let Some(note) = form_text(&form, "note") {
        sqlx::query("INSERT INTO controlequotidien(date,horodatage,categorie,statut,note,utilisateur) VALUES(?,CURRENT_TIMESTAMP,'note_libre','note',?,?)")
            .bind(&jour)
            .bind(note)
            .bind(&session.nom)
            .execute(&state.pool)
            .await?;
    }
    Ok(Redirect::to(&format!("/quotidien?jour={jour}")).into_response())
}

async fn quotidien_ras(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let jour = form_text(&form, "jour")
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    sqlx::query("INSERT INTO controlequotidien(date,horodatage,categorie,statut,note,utilisateur) VALUES(?,CURRENT_TIMESTAMP,'note_libre','ok','RAS',?)")
        .bind(&jour)
        .bind(&session.nom)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/quotidien?jour={jour}")).into_response())
}

async fn vente_sessions(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let sessions=generic_rows(&state.pool,"SELECT s.id,s.nom,s.date_creation,s.date_livraison,s.date_limite_commandes,s.nb_porcs,s.bande_reference,s.active,s.notes,COUNT(DISTINCT c.id) AS commandes,ROUND(COALESCE(SUM(CASE WHEN c.statut<>'annulee' THEN c.total ELSE 0 END),0),2) AS chiffre_affaires,ROUND(COALESCE((SELECT SUM(ch.montant) FROM chargeventedirecte ch WHERE ch.session_vente_id=s.id),0),2) AS charges,ROUND(COALESCE(ce.semence,0)+COALESCE(ce.gestation,0)+COALESCE(ce.maternite,0)+COALESCE(ce.post_sevrage,0)+COALESCE(ce.engraissement,0)+COALESCE(ce.veto_autres,0),2) AS cout_elevage,ROUND(COALESCE(SUM(CASE WHEN c.statut<>'annulee' THEN c.total ELSE 0 END),0)-COALESCE((SELECT SUM(ch.montant) FROM chargeventedirecte ch WHERE ch.session_vente_id=s.id),0)-COALESCE(ce.semence,0)-COALESCE(ce.gestation,0)-COALESCE(ce.maternite,0)-COALESCE(ce.post_sevrage,0)-COALESCE(ce.engraissement,0)-COALESCE(ce.veto_autres,0),2) AS marge,COALESCE(ce.semence,0) AS semence,COALESCE(ce.gestation,0) AS gestation,COALESCE(ce.maternite,0) AS maternite,COALESCE(ce.post_sevrage,0) AS post_sevrage,COALESCE(ce.engraissement,0) AS engraissement,COALESCE(ce.veto_autres,0) AS veto_autres,ce.bande_id,ce.nb_porcs_calcules,ce.poids_moyen_kg,ce.cout_par_porc,ce.cout_par_kg,ce.calcule_le FROM sessionventedirecte s LEFT JOIN commandeventedirecte c ON c.session_vente_id=s.id LEFT JOIN coutelevageventedirecte ce ON ce.session_vente_id=s.id GROUP BY s.id ORDER BY s.active DESC,s.date_livraison DESC,s.id DESC").await?;
    let charges=generic_rows(&state.pool,"SELECT id,session_vente_id,categorie,libelle,montant,note FROM chargeventedirecte ORDER BY id DESC").await?;
    let bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,ROUND((SELECT SUM(v.poids_total)/NULLIF(SUM(v.nb_porcs),0) FROM venteapport v WHERE v.bande_id=b.id AND v.poids_total>0 AND v.nb_porcs>0),2) AS poids_moyen FROM bande b ORDER BY active DESC,date_mb DESC,code",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("sessions_vente".into(), Value::Array(sessions));
    ctx.insert("charges".into(), Value::Array(charges));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert(
        "today".into(),
        json!(Local::now().date_naive().format("%Y-%m-%d").to_string()),
    );
    render(&state, "vente_sessions.html", Value::Object(ctx))
}

async fn vente_session_creer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom de session obligatoire".into()))?
        .chars()
        .take(160)
        .collect::<String>();
    let delivery_date = form_date(&form, "date_livraison")?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE sessionventedirecte SET active=0 WHERE active=1")
        .execute(&mut *tx)
        .await?;
    let deadline = form_date(&form, "date_limite_commandes")?.or_else(|| delivery_date.clone());
    let id=sqlx::query("INSERT INTO sessionventedirecte(nom,date_creation,date_livraison,date_limite_commandes,nb_porcs,bande_reference,active,notes) VALUES(?,date('now'),?,?,?,?,1,?)").bind(name).bind(&delivery_date).bind(deadline).bind(form_i64(&form,"nb_porcs").unwrap_or(0).max(0)).bind(form_text(&form,"bande_reference")).bind(form_text(&form,"notes")).execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT OR IGNORE INTO coutelevageventedirecte(session_vente_id) VALUES(?)")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison").bind(delivery_date).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO reglageventedirecte(id,commandes_ouvertes) VALUES(1,1) ON CONFLICT(id) DO UPDATE SET commandes_ouvertes=1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/vente-directe/sessions").into_response())
}

async fn vente_session_activer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT date_livraison FROM sessionventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let Some((date,)) = row else {
        return Err(AppError::NotFound);
    };
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE sessionventedirecte SET active=CASE WHEN id=? THEN 1 ELSE 0 END")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison").bind(date).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO reglageventedirecte(id,commandes_ouvertes) VALUES(1,1) ON CONFLICT(id) DO UPDATE SET commandes_ouvertes=1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to("/vente-directe/sessions").into_response())
}

async fn vente_session_cloturer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query(
        "UPDATE sessionventedirecte SET active=0,date_cloture=CURRENT_TIMESTAMP WHERE id=? AND active=1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::Invalid(
            "Cette session est déjà clôturée ou introuvable".into(),
        ));
    }
    sqlx::query("INSERT INTO reglageventedirecte(id,commandes_ouvertes,message_fermeture) VALUES(1,0,'Cette vente est terminée. Les commandes sont fermées.') ON CONFLICT(id) DO UPDATE SET commandes_ouvertes=0")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "clôturer",
        "session_vente_directe",
        &id.to_string(),
        "/vente-directe/session/cloturer",
    )
    .await;
    Ok(Redirect::to("/vente-directe/bilan").into_response())
}

async fn vente_session_modifier(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let name = form_text(&form, "nom")
        .ok_or_else(|| AppError::Invalid("Nom de session obligatoire".into()))?
        .chars()
        .take(160)
        .collect::<String>();
    let mut tx = state.pool.begin().await?;
    let active: Option<bool> =
        sqlx::query_scalar("SELECT active FROM sessionventedirecte WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(active) = active else {
        return Err(AppError::NotFound);
    };
    let delivery_date = form_date(&form, "date_livraison")?;
    let deadline = form_date(&form, "date_limite_commandes")?.or_else(|| delivery_date.clone());
    sqlx::query("UPDATE sessionventedirecte SET nom=?,date_livraison=?,date_limite_commandes=?,nb_porcs=?,bande_reference=?,notes=? WHERE id=?")
        .bind(name)
        .bind(&delivery_date)
        .bind(deadline)
        .bind(form_i64(&form, "nb_porcs").unwrap_or(0).max(0))
        .bind(form_text(&form, "bande_reference"))
        .bind(form_text(&form, "notes"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if active {
        sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison")
            .bind(delivery_date)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Redirect::to(&format!("/vente-directe/sessions#session-{id}")).into_response())
}

async fn vente_session_couts(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence,gestation,maternite,post_sevrage,engraissement,veto_autres) VALUES(?,?,?,?,?,?,?) ON CONFLICT(session_vente_id) DO UPDATE SET semence=excluded.semence,gestation=excluded.gestation,maternite=excluded.maternite,post_sevrage=excluded.post_sevrage,engraissement=excluded.engraissement,veto_autres=excluded.veto_autres").bind(id).bind(form_f64(&form,"semence").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"gestation").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"maternite").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"post_sevrage").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"engraissement").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"veto_autres").unwrap_or(0.0).max(0.0)).execute(&state.pool).await?;
    Ok(Redirect::to("/vente-directe/sessions").into_response())
}

async fn vente_session_cout_calculer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let band_id =
        form_i64(&form, "bande_id").ok_or_else(|| AppError::Invalid("Bande obligatoire".into()))?;
    let pigs = form_i64(&form, "nb_porcs")
        .filter(|value| *value > 0)
        .ok_or_else(|| AppError::Invalid("Nombre de porcs obligatoire".into()))?;
    let average_weight = form_f64(&form, "poids_moyen_kg")
        .filter(|value| *value > 0.0)
        .ok_or_else(|| AppError::Invalid("Poids produit moyen obligatoire".into()))?;
    let band_code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(band_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let costs = sqlx::query_as::<_, (f64, f64, f64, f64)>(
        "SELECT CAST(COALESCE((SELECT SUM(x.montant_ht/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='aliment' AND n.facture_id=x.id)) FROM livraisonaliment x JOIN affectationfacturebande a ON a.categorie='aliment' AND a.facture_id=x.id WHERE a.bande_id=?),0) AS REAL),CAST(COALESCE((SELECT SUM(x.montant_ht/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='veto' AND n.facture_id=x.id)) FROM achatveto x JOIN affectationfacturebande a ON a.categorie='veto' AND a.facture_id=x.id WHERE a.bande_id=?),0) AS REAL),CAST(COALESCE((SELECT SUM(x.montant_ht/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='semence' AND n.facture_id=x.id)) FROM achatsemence x JOIN affectationfacturebande a ON a.categorie='semence' AND a.facture_id=x.id WHERE a.bande_id=?),0) AS REAL),CAST(COALESCE((SELECT SUM(COALESCE(x.montant_ht,0)/(SELECT COUNT(*) FROM affectationfacturebande n WHERE n.categorie='genetique' AND n.facture_id=x.id)) FROM achatgenetique x JOIN affectationfacturebande a ON a.categorie='genetique' AND a.facture_id=x.id WHERE a.bande_id=?),0) AS REAL)",
    )
    .bind(band_id)
    .bind(band_id)
    .bind(band_id)
    .bind(band_id)
    .fetch_one(&state.pool)
    .await?;
    let total = (costs.0 + costs.1 + costs.2 + costs.3).max(0.0);
    let cost_per_pig = total / pigs as f64;
    let cost_per_kg = cost_per_pig / average_weight;
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence,gestation,maternite,post_sevrage,engraissement,veto_autres,bande_id,nb_porcs_calcules,poids_moyen_kg,cout_par_porc,cout_par_kg,calcule_le) VALUES(?,?,?,0,0,?,?,?,?,?,?,?,CURRENT_TIMESTAMP) ON CONFLICT(session_vente_id) DO UPDATE SET semence=excluded.semence,gestation=excluded.gestation,maternite=0,post_sevrage=0,engraissement=excluded.engraissement,veto_autres=excluded.veto_autres,bande_id=excluded.bande_id,nb_porcs_calcules=excluded.nb_porcs_calcules,poids_moyen_kg=excluded.poids_moyen_kg,cout_par_porc=excluded.cout_par_porc,cout_par_kg=excluded.cout_par_kg,calcule_le=CURRENT_TIMESTAMP")
        .bind(id)
        .bind(costs.2)
        .bind(costs.3)
        .bind(costs.0)
        .bind(costs.1)
        .bind(band_id)
        .bind(pigs)
        .bind(average_weight)
        .bind(cost_per_pig)
        .bind(cost_per_kg)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessionventedirecte SET bande_reference=?,nb_porcs=? WHERE id=?")
        .bind(band_code)
        .bind(pigs)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Redirect::to(&format!("/vente-directe/sessions#session-{id}")).into_response())
}

async fn vente_session_charge_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let label = form_text(&form, "libelle")
        .ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    let amount = form_f64(&form, "montant")
        .filter(|value| *value >= 0.0)
        .ok_or_else(|| AppError::Invalid("Montant invalide".into()))?;
    sqlx::query("INSERT INTO chargeventedirecte(session_vente_id,categorie,libelle,montant,note) SELECT ?,?,?,?,? WHERE EXISTS(SELECT 1 FROM sessionventedirecte WHERE id=?)").bind(id).bind(form_text(&form,"categorie").unwrap_or_else(||"autre".into())).bind(label).bind(amount).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/vente-directe/sessions").into_response())
}

async fn vente_session_charge_supprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path((id, charge_id)): Path<(i64, i64)>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    sqlx::query("DELETE FROM chargeventedirecte WHERE id=? AND session_vente_id=?")
        .bind(charge_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe/sessions").into_response())
}

async fn vente_commande_session(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let session_id = form_i64(&form, "session_vente_id");
    if let Some(session_id) = session_id {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessionventedirecte WHERE id=?")
            .bind(session_id)
            .fetch_one(&state.pool)
            .await?;
        if exists == 0 {
            return Err(AppError::Invalid("Session introuvable".into()));
        }
    }
    sqlx::query("UPDATE commandeventedirecte SET session_vente_id=? WHERE id=?")
        .bind(session_id)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/vente-directe/commandes").into_response())
}

async fn reglages(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    list_page(
        &state,
        &session,
        "Réglages de conduite",
        "Valeurs utilisées pour le calendrier et les alertes.",
        "SELECT cle,valeur,libelle FROM reglage ORDER BY cle",
        &["cle", "valeur", "libelle"],
    )
    .await
}

async fn parametres(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    let params = generic_rows(&state.pool, "SELECT cle,valeur FROM parametre ORDER BY cle").await?;
    let settings = generic_rows(
        &state.pool,
        "SELECT cle,valeur,libelle FROM reglage ORDER BY cle",
    )
    .await?;
    let feed=generic_rows(&state.pool,"SELECT id,categorie,jour_debut,jour_fin,aliment,quantite,unite,note,ordre FROM planaliment ORDER BY ordre,categorie,jour_debut").await?;
    let causes = generic_rows(
        &state.pool,
        "SELECT id,libelle FROM causeperte ORDER BY libelle",
    )
    .await?;
    let demo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM demoobjet")
        .fetch_one(&state.pool)
        .await?;
    let types_elevage: Vec<Value> = auth::TYPES_ELEVAGE
        .iter()
        .map(|(code, libelle)| json!({"code":code,"libelle":libelle}))
        .collect();
    let mut infos = Map::new();
    for row in &params {
        if let Some(key) = row.get("cle").and_then(Value::as_str) {
            infos.insert(
                key.to_string(),
                row.get("valeur").cloned().unwrap_or(Value::Null),
            );
        }
    }
    let mut ctx = context(&session);
    ctx.insert("parametres".into(), Value::Array(params));
    ctx.insert("infos".into(), Value::Object(infos));
    ctx.insert("reglages".into(), Value::Array(settings));
    ctx.insert("plans_aliment".into(), Value::Array(feed));
    ctx.insert("causes".into(), Value::Array(causes));
    let nombre_bandes: i64 = sqlx::query_scalar(
        "SELECT CAST(valeur AS INTEGER) FROM parametre WHERE cle='nombre_bandes'",
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(3);
    let intervalle_bandes_j: i64 = sqlx::query_scalar(
        "SELECT CAST(valeur AS INTEGER) FROM parametre WHERE cle='intervalle_bandes_j'",
    )
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(49);
    let gestation = reglage_i64(&state.pool, "gestation", 115).await?;
    let sevrage = reglage_i64(&state.pool, "sevrage", 28).await?;
    let retour_chaleur = reglage_i64(&state.pool, "chaleur_post_sevrage_j", 5).await?;
    ctx.insert(
        "conduite_bandes".into(),
        json!({"nombre":nombre_bandes,"intervalle_j":intervalle_bandes_j,"cycle_j":gestation+sevrage+retour_chaleur}),
    );
    ctx.insert("demo_actif".into(), json!(demo > 0));
    ctx.insert("types_elevage".into(), Value::Array(types_elevage));
    render(&state, "parametres.html", Value::Object(ctx))
}

async fn conduite_bandes_maj(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let nombre = form_i64(&form, "nombre_bandes")
        .filter(|value| [3, 4, 5, 7, 10, 20, 21].contains(value))
        .ok_or_else(|| AppError::Invalid("Nombre de bandes non reconnu".into()))?;
    let intervalle = form_i64(&form, "intervalle_bandes_j")
        .filter(|value| (7..=70).contains(value))
        .ok_or_else(|| AppError::Invalid("Intervalle entre bandes invalide".into()))?;
    let mut tx = state.pool.begin().await?;
    for (key, value) in [
        ("nombre_bandes", nombre),
        ("intervalle_bandes_j", intervalle),
    ] {
        sqlx::query("INSERT INTO parametre(cle,valeur) VALUES(?,?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur")
            .bind(key)
            .bind(value.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    db::journal(
        &state.pool,
        &session.nom,
        "modifier",
        "conduite_bandes",
        &format!("{nombre} bandes, intervalle {intervalle} jours"),
        "/parametres/conduite-bandes",
    )
    .await;
    Ok(Redirect::to("/parametres#conduite-bandes").into_response())
}

async fn correctifs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    render(&state, "correctifs.html", Value::Object(context(&session)))
}
async fn apropos(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    render(&state, "apropos.html", Value::Object(context(&session)))
}
async fn contact(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    render(&state, "contact.html", Value::Object(context(&session)))
}

async fn list_page(
    state: &AppState,
    session: &SessionData,
    title: &str,
    description: &str,
    sql: &str,
    columns: &[&str],
) -> AppResult<Html<String>> {
    let rows = generic_rows(&state.pool, sql).await?;
    render_list_page(state, session, title, description, rows, columns)
}

fn render_list_page(
    state: &AppState,
    session: &SessionData,
    title: &str,
    description: &str,
    rows: Vec<Value>,
    columns: &[&str],
) -> AppResult<Html<String>> {
    let cols: Vec<Value> = columns
        .iter()
        .map(|key| json!({"key":key,"label":key.replace('_'," ")}))
        .collect();
    let mut ctx = context(session);
    ctx.insert("title".into(), json!(title));
    ctx.insert("description".into(), json!(description));
    ctx.insert("columns".into(), Value::Array(cols));
    ctx.insert("rows".into(), Value::Array(rows));
    render(state, "liste.html", Value::Object(ctx))
}

async fn generic_rows(pool: &SqlitePool, sql: &str) -> AppResult<Vec<Value>> {
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    rows_to_json(rows)
}

fn rows_to_json(rows: Vec<SqliteRow>) -> AppResult<Vec<Value>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut object = Map::new();
        for (index, column) in row.columns().iter().enumerate() {
            let raw = row.try_get_raw(index)?;
            let value = if raw.is_null() {
                Value::Null
            } else {
                match raw.type_info().name() {
                    "INTEGER" => json!(row.try_get::<i64, _>(index)?),
                    "REAL" => json!(row.try_get::<f64, _>(index)?),
                    "BLOB" => json!("[donnée binaire]"),
                    _ => json!(row.try_get::<String, _>(index)?),
                }
            };
            object.insert(column.name().to_string(), value);
        }
        out.push(Value::Object(object));
    }
    Ok(out)
}

async fn compatibility_fallback(
    State(_state): State<AppState>,
    session: Option<Extension<SessionData>>,
    request: axum::extract::Request,
) -> Response {
    let path = request.uri().path().to_string();
    if session.is_none() {
        return Redirect::to("/login").into_response();
    }
    (StatusCode::NOT_IMPLEMENTED,Html(format!("<!doctype html><meta charset='utf-8'><main style='font-family:sans-serif;max-width:760px;margin:60px auto'><h1>Fonction non encore portée</h1><p>La route <code>{path}</code> n’est pas encore reliée à une action Rust. Aucune donnée n’a été modifiée.</p><p><a href='/'>Retour à l’accueil</a></p></main>"))).into_response()
}

#[cfg(test)]
mod gttt_tests {
    use super::*;

    #[tokio::test]
    async fn flux_bande_reunit_destinations_sevrages_et_ventes() -> anyhow::Result<()> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await?;
        let band_id =
            sqlx::query("INSERT INTO bande(code,date_mb,active) VALUES('FLUX-1','2026-01-01',1)")
                .execute(&pool)
                .await?
                .last_insert_rowid();
        let site_id = sqlx::query("INSERT INTO site(code,nom) VALUES('S1','Site 1')")
            .execute(&pool)
            .await?
            .last_insert_rowid();
        let room_id = sqlx::query(
            "INSERT INTO salle(site_id,nom,type,nb_cases,ordre) VALUES(?,'Verraterie','verraterie',0,1)",
        )
        .bind(site_id)
        .execute(&pool)
        .await?
        .last_insert_rowid();
        for (number, reformed, room) in [
            ("V-1", 0, Some(room_id)),
            ("A-1", 0, None),
            ("R-1", 1, None),
        ] {
            let sow_id = sqlx::query("INSERT INTO truie(num_travail,statut,rang,reformee,mere_cochette,salle_id) VALUES(?,'active',1,?,0,?)")
                .bind(number)
                .bind(reformed)
                .bind(room)
                .execute(&pool)
                .await?
                .last_insert_rowid();
            sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nb_sevres,suivi_actif) VALUES('sevrage','2026-01-29',?,?,9,0)")
                .bind(sow_id)
                .bind(band_id)
                .execute(&pool)
                .await?;
        }
        sqlx::query("INSERT INTO venteapport(date,bande_id,nb_porcs) VALUES('2026-07-01',?,20)")
            .bind(band_id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO venteapport(date,lots_json) VALUES('2026-07-08',?)")
            .bind(format!(r#"[{{"bande_id":{band_id},"nb_porcs":5}}]"#))
            .execute(&pool)
            .await?;
        let band =
            sqlx::query_as::<_, Bande>(&format!("SELECT {BAND_FIELDS} FROM bande WHERE id=?"))
                .bind(band_id)
                .fetch_one(&pool)
                .await?;
        let flow = band_flow_summary(&pool, &band).await?;
        assert_eq!(flow["truies_cycle"], 3);
        assert_eq!(flow["verraterie"], 1);
        assert_eq!(flow["attente"], 1);
        assert_eq!(flow["reformees"], 1);
        assert_eq!(flow["sevres"], 27);
        assert_eq!(flow["vendus"], 25);
        assert_eq!(flow["ventes"].as_array().map(Vec::len), Some(2));

        for kind in ["eau", "electricite"] {
            sqlx::query("INSERT INTO compteur_energie(nom,type,unite,rappel_jours,actif) VALUES(?,?,'index',7,1)")
                .bind(format!("Compteur {kind}"))
                .bind(kind)
                .execute(&pool)
                .await?;
        }
        sqlx::query("INSERT INTO tache(titre,echeance,fait) VALUES('Contrôle','2000-01-01',0)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO evenement(type,date,bande_id,suivi_actif,delivrance_ok) VALUES('mise_bas','2026-01-01',?,1,0)")
            .bind(band_id)
            .execute(&pool)
            .await?;
        let case_id =
            sqlx::query("INSERT INTO casesalle(salle_id,nom,nb_max_porcs) VALUES(?,'Alerte',1)")
                .bind(room_id)
                .execute(&pool)
                .await?
                .last_insert_rowid();
        sqlx::query("INSERT INTO inventairecase(case_id,date,nombre) VALUES(?,'2026-08-24',2)")
            .bind(case_id)
            .execute(&pool)
            .await?;
        let alerts = farm_alerts(&pool).await?;
        assert_eq!(alerts["total"], 7);
        let labels = alerts["alertes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|alert| alert["message"].as_str())
            .collect::<Vec<_>>();
        assert!(labels.contains(&"Relevés d’eau attendus"));
        assert!(labels.contains(&"Relevés d’électricité attendus"));
        assert!(labels.contains(&"Cases avec un effectif incohérent"));
        Ok(())
    }

    fn litter(
        live_born: f64,
        stillborn: f64,
        stillborn_rate: Option<f64>,
        weaned: f64,
        adopted: f64,
        removed: f64,
    ) -> GtttLitter {
        GtttLitter {
            sow_number: "T1".into(),
            band: Some("B1".into()),
            farrowing_date: NaiveDate::from_ymd_opt(2026, 1, 1),
            rank: 1,
            gestation: Some(115.0),
            live_born: Some(live_born),
            stillborn: Some(stillborn),
            stillborn_rate,
            weaned: Some(weaned),
            adopted: Some(adopted),
            removed: Some(removed),
        }
    }

    #[test]
    fn taux_mortnes_utilise_mortnes_sur_nes_totaux() {
        let summary = gttt_summary(&[litter(13.0, 1.0, None, 11.0, 0.0, 0.0)]);
        assert_eq!(summary.taux_mortnes, Some(7.1));
    }

    #[test]
    fn mortalite_allaitement_tient_compte_adoptions_retraits() {
        let summary = gttt_summary(&[litter(13.0, 1.0, None, 11.0, 2.0, 1.0)]);
        assert_eq!(summary.mortalite_allaitement, Some(21.4));
    }

    #[test]
    fn sevres_truie_an_utilise_la_periode_reellement_couverte() {
        let first_sow = litter(13.0, 1.0, None, 11.0, 0.0, 0.0);
        let mut second_sow = first_sow.clone();
        second_sow.sow_number = "T2".into();
        let mut first_sow_next = first_sow.clone();
        first_sow_next.farrowing_date = NaiveDate::from_ymd_opt(2026, 7, 1);
        let mut second_sow_next = second_sow.clone();
        second_sow_next.farrowing_date = first_sow_next.farrowing_date;
        let summary = gttt_summary(&[first_sow, second_sow, first_sow_next, second_sow_next]);
        assert_eq!(summary.truies_productives, 2);
        assert_eq!(summary.periode_jours, Some(362));
        assert_eq!(summary.portees_truie_an, Some(2.02));
        assert_eq!(summary.sevres_truie_an, Some(22.2));
    }

    #[test]
    fn montants_francais_reconnaissent_tous_les_signes_comptables() {
        assert_eq!(parse_french_number("12,34-"), Some(-12.34));
        assert_eq!(parse_french_number("-12,34"), Some(-12.34));
        assert_eq!(parse_french_number("(12,34)"), Some(-12.34));
        assert_eq!(parse_french_number("−12,34"), Some(-12.34));
        assert_eq!(parse_french_number("1 234,56"), Some(1234.56));
    }

    #[test]
    fn stades_bande_respectent_toutes_les_frontieres() {
        let schedule = BandSchedule::default();
        assert_eq!(schedule.stage(-116).0, "Planifiée");
        assert_eq!(schedule.stage(-115).0, "Verraterie");
        assert_eq!(schedule.stage(-87).0, "Gestante");
        assert_eq!(schedule.stage(-5).0, "Maternité (préparation)");
        assert_eq!(schedule.stage(0).0, "Maternité");
        assert_eq!(schedule.stage(28).0, "Post-sevrage");
        assert_eq!(schedule.stage(71).0, "Engraissement");
        assert_eq!(schedule.stage(215).0, "Départ / terminé");
    }

    #[test]
    fn calendrier_bande_utilise_les_reglages() {
        let schedule = BandSchedule {
            gestation: 114,
            echo_after_ia: 30,
            maternity_before_farrowing: 7,
            weaning: 26,
            transfer_finishing: 68,
            finishing_feed: 135,
            departure: 205,
        };
        assert_eq!(
            schedule.stages().map(|(_, day)| day),
            [-114, -84, -7, 0, 26, 68, 135, 205]
        );
    }

    #[test]
    fn progression_bande_suit_les_huit_etapes() {
        let schedule = BandSchedule::default();
        assert_eq!(schedule.stage_index(-115), 0);
        assert_eq!(schedule.stage_index(-87), 1);
        assert_eq!(schedule.stage_index(0), 3);
        assert_eq!(schedule.stage_index(28), 4);
        assert_eq!(schedule.stage_index(215), 7);
        assert_eq!(schedule.progression(-115), 0);
        assert_eq!(schedule.progression(215), 100);
        assert_eq!(schedule.progression(300), 100);
    }
}
