use crate::auth::{self, SessionData};
use crate::config::Config;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    Bande, CompteurEnergie, Evenement, ProduitVenteDirecte, ReleveCompteur, Truie, Utilisateur,
};
use axum::extract::{Extension, Form, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Local, NaiveDate};
use dashmap::DashMap;
use minijinja::Environment;
use serde_json::{json, Map, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::HashMap;
use std::sync::Arc;

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
        .route("/logout", get(logout))
        .route("/mon-compte/mdp", get(password_page).post(password_post))
        .route("/bandes", get(bandes))
        .route("/bandes/ajouter", post(bande_ajouter))
        .route("/bande/{id}", get(bande_detail))
        .route("/bande/{id}/imprimer", get(bande_imprimer))
        .route("/export/mise-bas/{id}", get(export_mise_bas))
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
        .route("/truies/affecter-bande", post(truies_affecter_bande))
        .route("/truie/{id}", get(truie_detail))
        .route("/truie/{id}/imprimer", get(truie_imprimer))
        .route("/truie/{id}/bande", post(truie_bande))
        .route("/truie/{id}/reformer", post(truie_reformer))
        .route("/truie/{id}/annuler-sortie", post(truie_annuler_sortie))
        .route("/truie/{id}/mesure", post(truie_mesure))
        .route("/mesure/{id}/supprimer", post(mesure_supprimer))
        .route("/truie/{id}/perte", post(truie_perte))
        .route("/perte/{id}/supprimer", post(perte_supprimer))
        .route("/truie/{id}/cochette", post(truie_cochette))
        .route("/evenement/ajouter", post(evenement_ajouter))
        .route("/evenement/{id}/supprimer", post(evenement_supprimer))
        .route("/inseminations", get(inseminations))
        .route("/inseminations/enregistrer", post(inseminations_enregistrer))
        .route("/recherche", get(recherche))
        .route("/gttt", get(gttt))
        .route("/productivite", get(productivite))
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
        .route("/traitement-charc/{id}/supprimer", post(charcutier_traitement_supprimer))
        .route("/transferts", get(transferts))
        .route("/transferts/porcs", post(transferts_porcs))
        .route("/transferts/truies", post(transferts_truies))
        .route("/transfert/{id}/supprimer", post(transfert_supprimer))
        .route("/effectifs", get(effectifs))
        .route("/effectifs/inventaire", post(effectifs_inventaire))
        .route("/effectifs/inventaire-case", post(effectifs_inventaire_case))
        .route("/etat-donnees", get(etat_donnees))
        .route("/api/bandes-actives", get(api_bandes_actives))
        .route("/api/bandes", get(api_bandes))
        .route("/api/truies", get(api_truies))
        .route("/api/cases", get(api_cases))
        .route("/energie", get(energie))
        .route("/energie/compteur", post(energie_compteur))
        .route("/energie/releve", post(energie_releve))
        .route("/energie/compteur/{id}/rappel", post(energie_rappel))
        .route("/energie/releve/{id}/supprimer", post(energie_releve_supprimer))
        .route("/energie/modele.csv", get(energie_modele_csv))
        .route("/energie/import", post(energie_import))
        .route("/economique", get(economique))
        .route("/economique/aliment", post(economique_aliment))
        .route("/economique/veto", post(economique_veto))
        .route("/economique/vente", post(economique_vente))
        .route("/economique/semence", post(economique_semence))
        .route("/economique/genetique", post(economique_genetique))
        .route("/economique/valorisation", post(economique_valorisation))
        .route("/economique/valorisation/{id}/supprimer", post(economique_valorisation_supprimer))
        .route("/economique/aliment/{id}/supprimer", post(economique_aliment_supprimer))
        .route("/economique/veto/{id}/supprimer", post(economique_veto_supprimer))
        .route("/economique/vente/{id}/supprimer", post(economique_vente_supprimer))
        .route("/economique/semence/{id}/supprimer", post(economique_semence_supprimer))
        .route("/economique/genetique/{id}/supprimer", post(economique_genetique_supprimer))
        .route("/vente-directe", get(vente_directe))
        .route("/vente-directe/produit-ajouter", post(produit_ajouter))
        .route("/vente-directe/produit/{id}", post(produit_modifier))
        .route("/vente-directe/produit/{id}/inventaire", post(produit_inventaire))
        .route("/vente-directe/produit/{id}/deplacer", post(produit_deplacer))
        .route("/vente-directe/reglage-livraison", post(vente_reglage_livraison))
        .route("/vente-directe/session/creer", post(vente_session_creer))
        .route("/vente-directe/session/{id}/activer", post(vente_session_activer))
        .route("/vente-directe/session/{id}/modifier", post(vente_session_modifier))
        .route("/vente-directe/session/{id}/couts", post(vente_session_couts))
        .route("/vente-directe/session/{id}/charge-ajouter", post(vente_session_charge_ajouter))
        .route("/vente-directe/session/{id}/charge/{charge_id}/supprimer", post(vente_session_charge_supprimer))
        .route("/vente-directe/commande/{id}/session", post(vente_commande_session))
        .route("/vente-directe/commande/{id}", get(vente_commande_modifier_page))
        .route("/vente-directe/commande/{id}/modifier", post(vente_commande_modifier))
        .route("/vente-directe/commande/{id}/imprimer", get(vente_commande_imprimer))
        .route("/vente-directe/preparation/imprimer", get(vente_preparation_imprimer))
        .route("/vente-directe/commande/{id}/statut", post(commande_statut))
        .route("/vente-directe/commande/{id}/supprimer", post(commande_supprimer))
        .route("/commande", get(commande_page).post(commande_post))
        .route("/utilisateurs", get(utilisateurs))
        .route("/utilisateurs/creer", post(utilisateur_creer))
        .route("/utilisateurs/{id}/actif", post(utilisateur_actif))
        .route("/utilisateurs/{id}/sections", post(utilisateur_sections))
        .route("/utilisateurs/{id}/mdp", post(utilisateur_mdp))
        .route("/sauvegarde", get(sauvegarde))
        .route("/structure", get(structure))
        .route("/structure/site", post(structure_site))
        .route("/structure/salle", post(structure_salle))
        .route("/structure/case", post(structure_case))
        .route("/structure/salle/{id}/modifier", post(structure_salle_modifier))
        .route("/structure/salle/{id}/ordre", post(structure_salle_ordre))
        .route("/structure/salle/{id}/supprimer", post(structure_salle_supprimer))
        .route("/structure/case/{id}/rfid", post(structure_case_rfid))
        .route("/structure/case/{id}/supprimer", post(structure_case_supprimer))
        .route("/structure/site/{id}/supprimer", post(structure_site_supprimer))
        .route("/taches", get(taches))
        .route("/taches/ajouter", post(tache_ajouter))
        .route("/taches/{id}/fait", post(tache_fait))
        .route("/taches/{id}/supprimer", post(tache_supprimer))
        .route("/sanitaire", get(sanitaire))
        .route("/pharmacie", get(pharmacie))
        .route("/sanitaire/acte/ajouter", post(sanitaire_acte_ajouter))
        .route("/sanitaire/acte/modifier", post(sanitaire_acte_modifier))
        .route("/sanitaire/acte/supprimer", post(sanitaire_acte_supprimer))
        .route("/sanitaire/fait", post(sanitaire_fait))
        .route("/pharmacie/mouvement", post(pharmacie_mouvement))
        .route("/pharmacie/regler", post(pharmacie_regler))
        .route("/planning", get(planning))
        .route("/calendrier.ics", get(calendrier_ics))
        .route("/stock", get(stock))
        .route("/journal", get(journal))
        .route("/entretien", get(entretien).post(entretien_ajouter))
        .route("/entretien/{id}/date", post(entretien_date))
        .route("/entretien/{id}/supprimer", post(entretien_supprimer))
        .route("/engraissement", get(engraissement))
        .route("/declaration", post(declaration_ajouter))
        .route("/declaration/{id}/supprimer", post(declaration_supprimer))
        .route("/abattoir", get(abattoir).post(abattoir_saisie))
        .route("/abattoir/saisie/{id}/supprimer", post(abattoir_saisie_supprimer))
        .route("/cahiers", get(cahiers).post(cahier_ajouter))
        .route("/cahiers/{id}/maj", post(cahier_maj))
        .route("/cahiers/{id}/supprimer", post(cahier_supprimer))
        .route("/quotidien", get(quotidien))
        .route("/quotidien/note", post(quotidien_note))
        .route("/quotidien/ras", post(quotidien_ras))
        .route("/vente-directe/sessions", get(vente_sessions))
        .route("/reglages", get(reglages))
        .route("/parametres", get(parametres))
        .route("/correctifs", get(correctifs))
        .route("/apropos", get(apropos))
        .route("/contact", get(contact))
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

fn form_f64(form: &HashMap<String, String>, key: &str) -> Option<f64> {
    parse_french_number(&form_text(form, key)?)
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
        if auth::verify_password(&password, &user.hash_mdp) {
            sqlx::query("UPDATE utilisateur SET tentatives_echec=0,bloque_jusqu=NULL WHERE id=?")
                .bind(user.id)
                .execute(&state.pool)
                .await?;
            let session_id = uuid::Uuid::new_v4().simple().to_string();
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
                },
            );
            let cookie = Cookie::build(("eo_session", session_id))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
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
    ctx.insert("error".into(), json!(query.get("err").cloned().unwrap_or_default()));
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
    sqlx::query("UPDATE utilisateur SET hash_mdp=?,doit_changer_mdp=0 WHERE id=?")
        .bind(auth::hash_password(&password))
        .bind(session.uid)
        .execute(&state.pool)
        .await?;
    for mut entry in state.sessions.iter_mut() {
        if entry.uid == session.uid {
            entry.doit_changer_mdp = false;
        }
    }
    Ok(Redirect::to("/").into_response())
}

#[derive(serde::Serialize)]
struct BandView {
    id: i64,
    code: String,
    date_mb: Option<String>,
    site: Option<String>,
    age: Option<i64>,
    stade: String,
    prochaine: String,
    truies: i64,
}

fn band_view(band: &Bande, sow_count: i64) -> BandView {
    let today = Local::now().date_naive();
    let date = band
        .date_mb
        .as_deref()
        .and_then(|value| NaiveDate::parse_from_str(&value[..value.len().min(10)], "%Y-%m-%d").ok());
    let age = date.map(|date| (today - date).num_days());
    let (stade, prochaine) = match age {
        None => ("À renseigner", "Renseigner la date de mise-bas"),
        Some(age) if age < -87 => ("Verraterie", "Échographie"),
        Some(age) if age < 0 => ("Gestante", "Mise-bas"),
        Some(age) if age < 28 => ("Maternité", "Sevrage"),
        Some(age) if age < 71 => ("Post-sevrage", "Transfert engraissement"),
        Some(age) if age < 215 => ("Engraissement", "Départ abattoir"),
        Some(_) => ("Départ / terminé", "Cycle terminé"),
    };
    BandView {
        id: band.id,
        code: band.code.clone(),
        date_mb: band.date_mb.clone(),
        site: band.site.clone(),
        age,
        stade: stade.to_string(),
        prochaine: prochaine.to_string(),
        truies: sow_count,
    }
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
    let mut views = Vec::new();
    for band in &bands {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE bande_code=? AND reformee=0")
            .bind(&band.code)
            .fetch_one(&state.pool)
            .await?;
        views.push(band_view(band, count));
    }
    let truies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE reformee=0")
        .fetch_one(&state.pool)
        .await?;
    let sevres: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(nb_sevres),0) FROM evenement WHERE type='sevrage'")
        .fetch_one(&state.pool)
        .await?;
    let vente: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport")
        .fetch_one(&state.pool)
        .await?;
    let aliment: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM livraisonaliment")
        .fetch_one(&state.pool)
        .await?;
    let veto: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatveto")
        .fetch_one(&state.pool)
        .await?;
    let semence: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatsemence")
        .fetch_one(&state.pool)
        .await?;
    let genetique: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(COALESCE(montant_net,montant_ht)),0) AS REAL) FROM achatgenetique")
        .fetch_one(&state.pool)
        .await?;
    let year = Local::now().format("%Y").to_string();
    let year_sales = sqlx::query_as::<_, (i64, f64, f64)>(
        "SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER),CAST(COALESCE(SUM(poids_total),0) AS REAL),CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport WHERE substr(date,1,4)=?",
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
        "SELECT substr(date,1,7) AS mois,CAST(SUM(COALESCE(nb_porcs,0)) AS INTEGER) AS porcs,ROUND(SUM(COALESCE(montant_net,0))/NULLIF(SUM(COALESCE(poids_total,0)),0),3) AS prix_net_kg FROM venteapport WHERE date IS NOT NULL GROUP BY substr(date,1,7) ORDER BY mois DESC LIMIT 12",
    )
    .await?;
    let latest_sales = generic_rows(
        &state.pool,
        "WITH recent AS (SELECT id,date,num_apport,nb_porcs,poids_total,montant_net,ROUND(montant_net/NULLIF(poids_total,0),3) AS prix_net_kg FROM venteapport WHERE poids_total>0 AND montant_net IS NOT NULL ORDER BY date DESC,id DESC LIMIT 10), bounds AS (SELECT MIN(prix_net_kg) AS mini,MAX(prix_net_kg) AS maxi FROM recent) SELECT recent.*,ROUND(CASE WHEN bounds.maxi=bounds.mini THEN 70 ELSE 30+90*(recent.prix_net_kg-bounds.mini)/(bounds.maxi-bounds.mini) END,0) AS hauteur FROM recent CROSS JOIN bounds ORDER BY recent.date,recent.id",
    )
    .await?;
    let latest_average = sqlx::query_as::<_, (f64, f64)>(
        "SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL),CAST(COALESCE(SUM(poids_total),0) AS REAL) FROM (SELECT montant_net,poids_total FROM venteapport WHERE poids_total>0 AND montant_net IS NOT NULL ORDER BY date DESC,id DESC LIMIT 5)",
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
    let mut ctx = context(&session);
    ctx.insert("bandes".into(), serde_json::to_value(views).unwrap_or_default());
    ctx.insert("taches".into(), Value::Array(taches));
    ctx.insert("inseminations".into(), Value::Array(inseminations));
    ctx.insert("prix_tendance".into(), Value::Array(price_trend));
    ctx.insert("dernieres_ventes".into(), Value::Array(latest_sales));
    ctx.insert("annee".into(), json!(year));
    ctx.insert(
        "stats".into(),
        json!({"band_active": bands.len(), "truies": truies, "sevres": sevres, "marge": vente-aliment-veto-semence-genetique,"porcs_vendus_annee":year_sales.0,"prix_net_kg":if year_sales.1>0.0{Some(year_sales.2/year_sales.1)}else{None},"prix_dernieres_ventes":if latest_average.1>0.0{Some(latest_average.0/latest_average.1)}else{None},"morts_annee":year_deaths}),
    );
    Ok(render(&state, "dashboard.html", Value::Object(ctx))?.into_response())
}

const BAND_FIELDS: &str = "id,code,num_officiel,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr";
const BAND_SELECT_ACTIVE: &str = "SELECT id,code,num_officiel,date_mb,site,note,active,cs_truies_saillies,cs_pleines,cs_truies_mb,cs_nt_portee,cs_nv_portee,cs_mn_portee,cs_sevres_portee,cs_total_sevres,cs_tx_pertes_nv,cs_poids_sevrage,cs_gmq_ps,cs_gmq_engr FROM bande WHERE active=1 ORDER BY COALESCE(date_mb,'9999-12-31'),id";

async fn bandes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let mut views = Vec::new();
    for band in bands {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE bande_code=? AND reformee=0")
            .bind(&band.code)
            .fetch_one(&state.pool)
            .await?;
        views.push(band_view(&band, count));
    }
    let mut ctx = context(&session);
    ctx.insert("bandes".into(), serde_json::to_value(views).unwrap_or_default());
    render(&state, "bandes.html", Value::Object(ctx))
}

async fn bande_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let code = form_text(&form, "code").ok_or_else(|| AppError::Invalid("Code obligatoire".into()))?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bande WHERE lower(code)=lower(?) AND active=1")
        .bind(&code)
        .fetch_one(&state.pool)
        .await?;
    if exists > 0 {
        return Err(AppError::Invalid("Cette bande existe déjà".into()));
    }
    sqlx::query("INSERT INTO bande(code,num_officiel,date_mb,site,active) VALUES(?,?,?,?,1)")
        .bind(&code)
        .bind(form_text(&form, "num_officiel"))
        .bind(form_text(&form, "date_mb"))
        .bind(form_text(&form, "site"))
        .execute(&state.pool)
        .await?;
    db::journal(&state.pool, &session.nom, "créer", "bande", &code, "/bandes/ajouter").await;
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
    let nv: i64 = events.iter().map(|e| e.nes_vifs.unwrap_or(0)).sum();
    let sevres: i64 = events.iter().map(|e| e.nb_sevres.unwrap_or(0)).sum();
    let pertes = if nv > 0 {
        (((nv - sevres).max(0) as f64 / nv as f64) * 100.0 * 10.0).round() / 10.0
    } else {
        0.0
    };
    let dates = key_dates(band.date_mb.as_deref());
    let mut ctx = context(&session);
    ctx.insert("bande".into(), serde_json::to_value(&band).unwrap_or_default());
    ctx.insert("truies".into(), serde_json::to_value(&sows).unwrap_or_default());
    ctx.insert("evenements".into(), serde_json::to_value(&events).unwrap_or_default());
    ctx.insert("dates".into(), Value::Array(dates));
    ctx.insert("resume".into(), json!({"truies": sows.len(), "nv": nv, "sevres": sevres, "pertes": pertes}));
    render(&state, "bande.html", Value::Object(ctx))
}

fn key_dates(date_mb: Option<&str>) -> Vec<Value> {
    let Some(date) = date_mb.and_then(|value| NaiveDate::parse_from_str(&value[..value.len().min(10)], "%Y-%m-%d").ok()) else {
        return vec![];
    };
    let today = Local::now().date_naive();
    let stages = [
        ("Insémination", -115), ("Échographie", -87), ("Entrée maternité", -5),
        ("Mise-bas", 0), ("Sevrage", 28), ("Transfert engraissement", 71),
        ("Aliment finition", 140), ("Départ abattoir", 215),
    ];
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
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session, &form)?;
    sqlx::query("UPDATE bande SET active=0,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn bande_desarchiver(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session, &form)?;
    sqlx::query("UPDATE bande SET active=1,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/bande/{id}")).into_response())
}

async fn bande_supprimer(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session, &form)?;
    let linked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM evenement WHERE bande_id=?").bind(id).fetch_one(&state.pool).await?;
    if linked > 0 { return Err(AppError::Invalid("Impossible de supprimer une bande qui contient des événements; archive-la.".into())); }
    sqlx::query("DELETE FROM bande WHERE id=?").bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/bandes").into_response())
}

async fn archives(State(state): State<AppState>, Extension(session): Extension<SessionData>) -> AppResult<Html<String>> {
    list_page(&state, &session, "Bandes archivées", "Historique conservé", "SELECT id,code,date_mb,site,note FROM bande WHERE active=0 ORDER BY date_mb", &["id","code","date_mb","site","note"]).await
}

const TRUIE_FIELDS: &str = "id,num_travail,num_national,rfid,race,date_entree,statut,note,rang,date_naissance,reformee,date_reforme,motif_sortie,mere_cochette,bande_code,salle_id,case_id,perf_nt,perf_nv,perf_mn,perf_sevres,perf_tx_perte";
const TRUIE_SELECT_BY_BAND: &str = "SELECT id,num_travail,num_national,rfid,race,date_entree,statut,note,rang,date_naissance,reformee,date_reforme,motif_sortie,mere_cochette,bande_code,salle_id,case_id,perf_nt,perf_nv,perf_mn,perf_sevres,perf_tx_perte FROM truie WHERE bande_code=? AND reformee=0 ORDER BY num_travail";

async fn truies(
    State(state): State<AppState>, Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let q = query.get("q").cloned().unwrap_or_default();
    let sql = if q.is_empty() {
        format!("SELECT {TRUIE_FIELDS} FROM truie WHERE reformee=0 ORDER BY num_travail")
    } else {
        format!("SELECT {TRUIE_FIELDS} FROM truie WHERE reformee=0 AND (num_travail LIKE ? OR num_national LIKE ? OR rfid LIKE ?) ORDER BY num_travail")
    };
    let sows = if q.is_empty() {
        sqlx::query_as::<_, Truie>(&sql).fetch_all(&state.pool).await?
    } else {
        let pattern = format!("%{q}%");
        sqlx::query_as::<_, Truie>(&sql).bind(&pattern).bind(&pattern).bind(&pattern).fetch_all(&state.pool).await?
    };
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE).fetch_all(&state.pool).await?;
    let mut ctx = context(&session);
    ctx.insert("truies".into(), serde_json::to_value(sows).unwrap_or_default());
    ctx.insert("bandes".into(), serde_json::to_value(bands).unwrap_or_default());
    ctx.insert("q".into(), json!(q));
    render(&state, "truies.html", Value::Object(ctx))
}

async fn truie_ajouter(
    State(state): State<AppState>, Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session, &form)?;
    let number = form_text(&form, "num_travail").ok_or_else(|| AppError::Invalid("N° travail obligatoire".into()))?;
    let duplicate: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM truie WHERE num_travail=? AND reformee=0").bind(&number).fetch_one(&state.pool).await?;
    if duplicate > 0 { return Err(AppError::Invalid("Ce numéro de travail existe déjà".into())); }
    let result = sqlx::query("INSERT INTO truie(num_travail,num_national,rfid,race,date_entree,bande_code,statut,reformee,rang,mere_cochette) VALUES(?,?,?,?,?,?,'active',0,0,0)")
        .bind(&number).bind(form_text(&form,"num_national")).bind(form_text(&form,"rfid"))
        .bind(form_text(&form,"race")).bind(form_text(&form,"date_entree")).bind(form_text(&form,"bande_code"))
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
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM bande WHERE code=? AND active=1",
        )
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
    Ok(Redirect::to("/truies").into_response())
}

async fn truie_detail(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    let sql = format!("SELECT {TRUIE_FIELDS} FROM truie WHERE id=?");
    let sow = sqlx::query_as::<_, Truie>(&sql).bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let events = sqlx::query_as::<_, Evenement>(EVENT_SELECT_BY_SOW).bind(id).fetch_all(&state.pool).await?;
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
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let cases = generic_rows(
        &state.pool,
        "SELECT c.id,s.nom||' · '||c.nom AS label FROM casesalle c JOIN salle s ON s.id=c.salle_id ORDER BY s.ordre,s.nom,c.nom",
    )
    .await?;
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
    ctx.insert("truie".into(), serde_json::to_value(&sow).unwrap_or_default());
    ctx.insert("evenements".into(), serde_json::to_value(events).unwrap_or_default());
    ctx.insert("mesures".into(), Value::Array(mesures));
    ctx.insert("pertes".into(), Value::Array(pertes));
    ctx.insert("bandes".into(), serde_json::to_value(bands).unwrap_or_default());
    ctx.insert("cases".into(), Value::Array(cases));
    ctx.insert("dates".into(), Value::Array(key_dates(date_mb.as_deref())));
    ctx.insert("today".into(), json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state, "truie.html", Value::Object(ctx))
}

async fn truie_bande(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
    sqlx::query("UPDATE truie SET bande_code=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(form_text(&form,"bande_code")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

async fn truie_reformer(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
    let date = form_text(&form,"date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    sqlx::query("UPDATE truie SET reformee=1,statut='reformee',date_reforme=?,motif_sortie=?,updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(date).bind(form_text(&form,"motif")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/truies").into_response())
}

async fn truie_annuler_sortie(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
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
    let date = form_text(&form, "date")
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    sqlx::query("INSERT INTO mesuretruie(truie_id,date,eld,poids,nec,note,periode) VALUES(?,?,?,?,?,?,?)")
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
    Ok(Redirect::to(&sow.map(|value| format!("/truie/{value}#mesures")).unwrap_or_else(|| "/truies".into())).into_response())
}

async fn truie_perte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let nb = form_i64(&form, "nb").unwrap_or(1).max(1);
    let band_id: Option<i64> = sqlx::query_scalar(
        "SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=? ORDER BY b.active DESC,b.id DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    sqlx::query("INSERT INTO perteporcelet(truie_id,bande_id,age_j,nb,cause,date) VALUES(?,?,?,?,?,?)")
        .bind(id)
        .bind(band_id)
        .bind(form_i64(&form, "age_j"))
        .bind(nb)
        .bind(form_text(&form, "cause"))
        .bind(form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string()))
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/truie/{id}#pertes")).into_response())
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
    Ok(Redirect::to(&sow.map(|value| format!("/truie/{value}#pertes")).unwrap_or_else(|| "/truies".into())).into_response())
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
    Ok(Redirect::to(&format!("/truie/{id}")).into_response())
}

const EVENT_SELECT_BY_SOW: &str = "SELECT id,type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,nb_sevres,poids_moyen,produit,motif,resultat,note FROM evenement WHERE truie_id=? ORDER BY date DESC,id DESC";
const EVENT_SELECT_BY_BAND: &str = "SELECT id,type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,nb_sevres,poids_moyen,produit,motif,resultat,note FROM evenement WHERE bande_id=? ORDER BY date DESC,id DESC";

async fn evenement_ajouter(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
    let kind = form_text(&form,"type").ok_or_else(|| AppError::Invalid("Type obligatoire".into()))?;
    let date = form_text(&form,"date").ok_or_else(|| AppError::Invalid("Date obligatoire".into()))?;
    let sow_id = form_i64(&form,"truie_id");
    let mut band_id = form_i64(&form,"bande_id");
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
    sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,nes_totaux,nes_vifs,mort_nes,momifies,chetifs,ecrases,tues_truie,nb_sevres,poids_moyen,adoptes,retires,produit,motif,delai_attente,resultat,nb_doses,heure_debut,heure_fin,note,suivi_actif) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0)")
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
        .bind(form_i64(&form,"nb_doses"))
        .bind(form_text(&form,"heure_debut"))
        .bind(form_text(&form,"heure_fin"))
        .bind(note)
        .execute(&state.pool).await?;
    if kind == "mise_bas" {
        if let Some(sow_id) = sow_id {
            let completed = NaiveDate::parse_from_str(&date[..date.len().min(10)], "%Y-%m-%d")
                .map(|value| value <= Local::now().date_naive())
                .unwrap_or(false);
            sqlx::query("UPDATE truie SET bande_code=COALESCE((SELECT code FROM bande WHERE id=?),bande_code),rang=rang+?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                .bind(band_id).bind(if completed { 1 } else { 0 }).bind(sow_id).execute(&state.pool).await?;
        }
    }
    let target = sow_id.map(|id| format!("/truie/{id}")).unwrap_or_else(|| "/".into());
    Ok(Redirect::to(&target).into_response())
}

async fn evenement_supprimer(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Path(id): Path<i64>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
    let event: Option<(Option<i64>, String, String)> = sqlx::query_as("SELECT truie_id,type,date FROM evenement WHERE id=?").bind(id).fetch_optional(&state.pool).await?;
    let mut tx=state.pool.begin().await?;
    sqlx::query("DELETE FROM evenement WHERE id=?").bind(id).execute(&mut *tx).await?;
    if let Some((Some(sow_id),kind,date))=&event {
        if kind=="mise_bas"&&NaiveDate::parse_from_str(&date[..date.len().min(10)],"%Y-%m-%d").map(|value|value<=Local::now().date_naive()).unwrap_or(false){sqlx::query("UPDATE truie SET rang=MAX(rang-1,0),updated_at=CURRENT_TIMESTAMP WHERE id=?").bind(sow_id).execute(&mut *tx).await?;}
    }
    tx.commit().await?;
    let sow=event.and_then(|value|value.0);
    Ok(Redirect::to(&sow.map(|x|format!("/truie/{x}")).unwrap_or_else(||"/".into())).into_response())
}

async fn inseminations(State(state): State<AppState>, Extension(session): Extension<SessionData>) -> AppResult<Html<String>> {
    let candidates = generic_rows(
        &state.pool,
        "WITH derniere AS (SELECT truie_id,MAX(date) AS date_chaleur FROM evenement WHERE type='chaleur' AND truie_id IS NOT NULL GROUP BY truie_id) SELECT t.id,t.num_travail,t.bande_code,d.date_chaleur,date(d.date_chaleur,'+1 day') AS date_conseillee,(SELECT e.note FROM evenement e WHERE e.truie_id=t.id AND e.type='chaleur' AND e.date=d.date_chaleur ORDER BY e.id DESC LIMIT 1) AS observation FROM derniere d JOIN truie t ON t.id=d.truie_id WHERE t.reformee=0 AND NOT EXISTS(SELECT 1 FROM evenement ia WHERE ia.truie_id=t.id AND ia.type='ia' AND ia.date>=d.date_chaleur) ORDER BY d.date_chaleur,t.num_travail",
    )
    .await?;
    let bands = sqlx::query_as::<_, Bande>(BAND_SELECT_ACTIVE)
        .fetch_all(&state.pool)
        .await?;
    let mut ctx = context(&session);
    ctx.insert("candidates".into(), Value::Array(candidates));
    ctx.insert("bandes".into(), serde_json::to_value(bands).unwrap_or_default());
    ctx.insert("today".into(), json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
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
    let date = form_text(&form, "date")
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    let mut tx = state.pool.begin().await?;
    for id in ids {
        let band_id: Option<i64> = sqlx::query_scalar(
            "SELECT b.id FROM truie t JOIN bande b ON b.code=t.bande_code WHERE t.id=? ORDER BY b.active DESC,b.id DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO evenement(type,date,truie_id,bande_id,produit,nb_doses,note,suivi_actif) VALUES('ia',?,?,?,?,?,?,0)")
            .bind(&date)
            .bind(id)
            .bind(band_id)
            .bind(form_text(&form, "produit"))
            .bind(form_i64(&form, "nb_doses").unwrap_or(1).max(1))
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
        Some(Value::Bool(value)) => if *value { "1".into() } else { "0".into() },
        _ => String::new(),
    }
}

async fn export_mise_bas(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let band: (String, Option<String>) = sqlx::query_as("SELECT code,date_mb FROM bande WHERE id=?")
        .bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let rows = generic_rows(&state.pool,&format!("SELECT t.num_travail,t.rang,t.race,e.nes_totaux,e.nes_vifs,e.mort_nes,e.momifies,e.nb_sevres FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={} AND e.type='mise_bas' ORDER BY t.num_travail,e.date",id)).await?;
    let mut writer=csv::WriterBuilder::new().delimiter(b';').from_writer(Vec::new());
    let band_header=format!("Bande : {}",band.0);let date_header=format!("MB théorique : {}",band.1.unwrap_or_default());
    writer.write_record(["Liste des truies à la mise-bas",band_header.as_str(),date_header.as_str()]).map_err(|error|AppError::Internal(error.into()))?;
    writer.write_record(["N° travail","Rang","Race","NT","NV","Mort-nés","Momifiés","Sevrés"]).map_err(|error|AppError::Internal(error.into()))?;
    for row in rows { let object=row.as_object().expect("generic row object");writer.write_record([csv_value(object.get("num_travail")),csv_value(object.get("rang")),csv_value(object.get("race")),csv_value(object.get("nes_totaux")),csv_value(object.get("nes_vifs")),csv_value(object.get("mort_nes")),csv_value(object.get("momifies")),csv_value(object.get("nb_sevres"))]).map_err(|error|AppError::Internal(error.into()))?; }
    writer.flush().map_err(|error|AppError::Internal(error.into()))?;
    let mut bytes=vec![0xEF,0xBB,0xBF];bytes.extend(writer.into_inner().map_err(|error|AppError::Internal(error.into_error().into()))?);
    let mut headers=HeaderMap::new();headers.insert(header::CONTENT_TYPE,HeaderValue::from_static("text/csv; charset=utf-8"));headers.insert(header::CONTENT_DISPOSITION,HeaderValue::from_str(&format!("attachment; filename=liste_mise_bas_{}.csv",band.0.replace(' ',"_"))).map_err(|error|AppError::Internal(error.into()))?);Ok((headers,bytes).into_response())
}

async fn truies_modele_csv()->Response{let body="\u{feff}num_travail;num_national;rfid;race;date_entree;date_naissance;bande_code;note\r\nT001;FR000000001;250000000001;Large White;2026-01-01;2025-01-01;B1.26;Exemple à supprimer\r\n";let mut headers=HeaderMap::new();headers.insert(header::CONTENT_TYPE,HeaderValue::from_static("text/csv; charset=utf-8"));headers.insert(header::CONTENT_DISPOSITION,HeaderValue::from_static("attachment; filename=modele_import_truies.csv"));(headers,body).into_response()}

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
        return Err(AppError::Invalid("Colonne num_travail manquante".into()));
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    let mut seen = std::collections::HashSet::new();
    let mut preview_rows = Vec::new();
    let mut additions = 0_i64;
    let mut ignored = 0_i64;
    let mut errors = 0_i64;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM importjournal WHERE statut='apercu' AND cree_le<datetime('now','-1 day')")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO importjournal(token,type_import,nom_fichier,statut,cree_par) VALUES(?,'truies',?,'apercu',?)")
        .bind(&token)
        .bind(&filename)
        .bind(session.uid)
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
        let mut action = "ajouter";
        let mut anomaly = None;
        if number.is_empty() {
            action = "erreur";
            anomaly = Some("Numéro de travail manquant".to_string());
            errors += 1;
        } else if !seen.insert(number.to_lowercase()) {
            action = "ignorer";
            anomaly = Some("Doublon dans le fichier".to_string());
            ignored += 1;
        } else {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM truie WHERE lower(trim(num_travail))=lower(trim(?)) AND reformee=0",
            )
            .bind(number)
            .fetch_one(&mut *tx)
            .await?;
            if exists > 0 {
                action = "ignorer";
                anomaly = Some("Truie active déjà présente".to_string());
                ignored += 1;
            } else {
                additions += 1;
            }
        }
        let payload = json!({
            "num_travail": number,
            "num_national": field("num_national"),
            "rfid": field("rfid"),
            "race": field("race"),
            "date_entree": field("date_entree"),
            "date_naissance": field("date_naissance"),
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
        let value = |key: &str| data.get(key).and_then(Value::as_str).filter(|value| !value.is_empty());
        let number = value("num_travail")
            .ok_or_else(|| AppError::Invalid(format!("Numéro absent à la ligne {line}")))?;
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
            .bind(value("date_entree"))
            .bind(value("date_naissance"))
            .bind(value("bande_code"))
            .bind(value("note"))
            .bind(&token)
            .execute(&mut *tx)
            .await?;
        added += 1;
    }
    sqlx::query("UPDATE importjournal SET statut='applique',applique_le=CURRENT_TIMESTAMP WHERE token=?")
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
    sqlx::query("DELETE FROM importjournal WHERE token=? AND statut='apercu' AND (cree_par=? OR ?='admin')")
        .bind(token)
        .bind(session.uid)
        .bind(&session.role)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/truies").into_response())
}

async fn truie_imprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>)->AppResult<Html<String>>{let sow=generic_rows(&state.pool,&format!("SELECT num_travail,num_national,rfid,race,date_naissance,rang,bande_code,statut,note FROM truie WHERE id={id}")).await?.into_iter().next().ok_or(AppError::NotFound)?;let lines=generic_rows(&state.pool,&format!("SELECT date,type,COALESCE(resultat,produit,note,'') AS detail,nes_totaux,nes_vifs,nb_sevres FROM evenement WHERE truie_id={id} ORDER BY date DESC,id DESC")).await?;let mut ctx=context(&session);ctx.insert("title".into(),json!(format!("Fiche truie {}",sow.get("num_travail").and_then(Value::as_str).unwrap_or(""))));ctx.insert("infos".into(),sow);ctx.insert("lignes".into(),Value::Array(lines));render(&state,"impression.html",Value::Object(ctx))}

async fn bande_imprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>)->AppResult<Html<String>>{let band=generic_rows(&state.pool,&format!("SELECT code,num_officiel,date_mb,site,note,active FROM bande WHERE id={id}")).await?.into_iter().next().ok_or(AppError::NotFound)?;let lines=generic_rows(&state.pool,&format!("SELECT 'Truie' AS type,t.num_travail AS reference,'Rang '||t.rang AS detail,NULL AS date FROM truie t JOIN bande b ON b.code=t.bande_code WHERE b.id={id} AND t.reformee=0 UNION ALL SELECT e.type,COALESCE(t.num_travail,''),COALESCE(e.note,e.produit,e.resultat,''),e.date FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.bande_id={id} ORDER BY date DESC,reference")).await?;let mut ctx=context(&session);ctx.insert("title".into(),json!(format!("Fiche bande {}",band.get("code").and_then(Value::as_str).unwrap_or(""))));ctx.insert("infos".into(),band);ctx.insert("lignes".into(),Value::Array(lines));render(&state,"impression.html",Value::Object(ctx))}

fn ics_escape(value:&str)->String{value.replace('\\',"\\\\").replace(';',"\\;").replace(',',"\\,").replace(['\r','\n']," ")}

async fn calendrier_ics(State(state):State<AppState>)->AppResult<Response>{let bands=sqlx::query_as::<_,Bande>(BAND_SELECT_ACTIVE).fetch_all(&state.pool).await?;let mut lines=vec!["BEGIN:VCALENDAR".to_string(),"VERSION:2.0".into(),"PRODID:-//EO-Suivi Elevage Rust//FR".into(),"CALSCALE:GREGORIAN".into(),"METHOD:PUBLISH".into()];let stages=[("Insémination",-115),("Échographie",-87),("Entrée maternité",-5),("Mise-bas",0),("Sevrage",28),("Transfert engraissement",71),("Aliment finition",140),("Départ abattoir",215)];for band in bands{let Some(base)=band.date_mb.as_deref().and_then(|value|NaiveDate::parse_from_str(&value[..value.len().min(10)],"%Y-%m-%d").ok())else{continue};for(name,days)in stages{let day=base+Duration::days(days);lines.push("BEGIN:VEVENT".into());lines.push(format!("UID:bande-{}-{}@eo-suivi-rust",band.id,days+200));lines.push(format!("DTSTAMP:{}T000000Z",Local::now().format("%Y%m%d")));lines.push(format!("DTSTART;VALUE=DATE:{}",day.format("%Y%m%d")));lines.push(format!("SUMMARY:{}",ics_escape(&format!("{} — {}",band.code,name))));lines.push("END:VEVENT".into());}}lines.push("END:VCALENDAR".into());let body=format!("{}\r\n",lines.join("\r\n"));let mut headers=HeaderMap::new();headers.insert(header::CONTENT_TYPE,HeaderValue::from_static("text/calendar; charset=utf-8"));headers.insert(header::CONTENT_DISPOSITION,HeaderValue::from_static("attachment; filename=elevage.ics"));Ok((headers,body).into_response())}

async fn api_bandes_actives(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(&state.pool,"SELECT id,code,date_mb,site FROM bande WHERE active=1 ORDER BY date_mb").await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_truies(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(&state.pool,"SELECT id,num_travail,bande_code,rfid FROM truie WHERE reformee=0 ORDER BY num_travail").await?;
    Ok(axum::Json(Value::Array(rows)))
}

async fn api_bandes(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let rows = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site,active FROM bande ORDER BY active DESC,date_mb,id",
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
        "SELECT 'Truie' AS type,CAST(id AS TEXT) AS reference,COALESCE(num_travail,'')||CASE WHEN bande_code IS NOT NULL THEN ' · bande '||bande_code ELSE '' END AS detail FROM truie WHERE num_travail LIKE ? OR COALESCE(num_national,'') LIKE ? OR COALESCE(rfid,'') LIKE ? UNION ALL SELECT 'Bande',CAST(id AS TEXT),code||COALESCE(' · '||site,'') FROM bande WHERE code LIKE ? OR COALESCE(num_officiel,'') LIKE ? UNION ALL SELECT 'Apport',CAST(id AS TEXT),COALESCE(num_apport,'')||' · '||COALESCE(CAST(nb_porcs AS TEXT),'0')||' porcs' FROM venteapport WHERE COALESCE(num_apport,'') LIKE ? UNION ALL SELECT 'Commande',CAST(id AS TEXT),nom_client||' · '||COALESCE(telephone,'') FROM commandeventedirecte WHERE nom_client LIKE ? OR COALESCE(telephone,'') LIKE ? OR COALESCE(email,'') LIKE ? LIMIT 200",
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
) -> AppResult<Html<String>> {
    let litters = load_gttt_litters(&state.pool, None).await?;
    let summary = gttt_summary(&litters);
    let rank_rows = gttt_rank_rows(&litters);
    let band_codes = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT bande FROM porteerang WHERE bande IS NOT NULL AND trim(bande)<>'' ORDER BY bande",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut band_rows = Vec::with_capacity(band_codes.len());
    for code in band_codes {
        let rows = load_gttt_litters(&state.pool, Some(&code)).await?;
        let mut value = gttt_summary(&rows);
        value
            .as_object_mut()
            .expect("résumé GTTT objet")
            .insert("bande".into(), json!(code));
        band_rows.push(value);
    }
    let mut ctx = context(&session);
    ctx.insert("synthese".into(), summary);
    ctx.insert("rangs".into(), Value::Array(rank_rows));
    ctx.insert("bandes".into(), Value::Array(band_rows));
    render(&state, "gttt.html", Value::Object(ctx))
}

#[derive(Clone, Debug)]
struct GtttLitter {
    rank: i64,
    gestation: Option<f64>,
    live_born: Option<f64>,
    stillborn: Option<f64>,
    stillborn_rate: Option<f64>,
    weaned: Option<f64>,
    adopted: Option<f64>,
    removed: Option<f64>,
}

async fn load_gttt_litters(pool: &SqlitePool, band: Option<&str>) -> AppResult<Vec<GtttLitter>> {
    let sql = if band.is_some() {
        "SELECT rang,duree_gest,nv,mn,tx_mn_nt,sev,ad,re FROM porteerang WHERE bande=? ORDER BY rang,id"
    } else {
        "SELECT rang,duree_gest,nv,mn,tx_mn_nt,sev,ad,re FROM porteerang ORDER BY rang,id"
    };
    let mut query = sqlx::query_as::<
        _,
        (
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
            rank: row.0,
            gestation: row.1,
            live_born: row.2,
            stillborn: row.3,
            stillborn_rate: row.4,
            weaned: row.5,
            adopted: row.6,
            removed: row.7,
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

fn gttt_summary(litters: &[GtttLitter]) -> Value {
    let valid: Vec<&GtttLitter> = litters
        .iter()
        .filter(|litter| litter.live_born.is_some())
        .collect();
    let mean = |values: Vec<f64>| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let total_real = valid.iter().map(|litter| gttt_real_total(litter)).sum::<f64>();
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
    json!({
        "portees": valid.len(),
        "nes_totaux_moy": mean(valid.iter().map(|litter| gttt_real_total(litter)).collect::<Vec<_>>()).map(|value| (value * 100.0).round() / 100.0),
        "nes_vifs_moy": mean(valid.iter().filter_map(|litter| litter.live_born).collect::<Vec<_>>()).map(|value| (value * 100.0).round() / 100.0),
        "sevres_moy": mean(litters.iter().filter_map(|litter| litter.weaned).collect::<Vec<_>>()).map(|value| (value * 100.0).round() / 100.0),
        "taux_mortnes": (total_real > 0.0).then(|| (total_stillborn / total_real * 1000.0).round() / 10.0),
        "mortalite_allaitement": (available > 0.0).then(|| ((losses.max(0.0) / available) * 1000.0).round() / 10.0),
        "total_sevres": litters.iter().map(|litter| litter.weaned.unwrap_or(0.0)).sum::<f64>().round() as i64,
        "gestation_moy": mean(litters.iter().filter_map(|litter| litter.gestation).collect::<Vec<_>>()).map(|value| (value * 10.0).round() / 10.0),
    })
}

fn gttt_rank_rows(litters: &[GtttLitter]) -> Vec<Value> {
    let mut ranks: Vec<i64> = litters.iter().map(|litter| litter.rank).collect();
    ranks.sort_unstable();
    ranks.dedup();
    ranks
        .into_iter()
        .map(|rank| {
            let selected = litters
                .iter()
                .filter(|litter| litter.rank == rank)
                .cloned()
                .collect::<Vec<_>>();
            let mut value = gttt_summary(&selected);
            value
                .as_object_mut()
                .expect("résumé GTTT objet")
                .insert("rang".into(), json!(rank));
            value
        })
        .collect()
}

async fn productivite(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    let months=query.get("mois").and_then(|value|value.parse::<i64>().ok()).filter(|value|matches!(value,6|12|24|36|60|120)).unwrap_or(24);
    let cutoff=format!("-{months} months");
    let rows=sqlx::query("SELECT b.id,b.code,b.date_mb,b.site,b.cs_truies_saillies,b.cs_pleines,b.cs_truies_mb,b.cs_nv_portee,b.cs_sevres_portee,b.cs_total_sevres,b.cs_tx_pertes_nv,b.cs_poids_sevrage,b.cs_gmq_ps,b.cs_gmq_engr,(SELECT MIN(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='ia') AS premiere_ia_reelle,(SELECT MIN(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='mise_bas' AND e.date<=date('now')) AS premiere_mb_reelle,(SELECT MAX(e.date) FROM evenement e WHERE e.bande_id=b.id AND e.type='sevrage') AS dernier_sevrage_reel,(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type='echo') AS echos,(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type='echo' AND lower(COALESCE(e.resultat,'')) IN('positive','positif','pleine','oui')) AS echos_positives,ROUND(100.0*(SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type='echo' AND lower(COALESCE(e.resultat,'')) IN('positive','positif','pleine','oui'))/NULLIF((SELECT COUNT(*) FROM evenement e WHERE e.bande_id=b.id AND e.type='echo'),0),1) AS taux_pleines_echo,ROUND(100.0*COALESCE(b.cs_truies_mb,0)/NULLIF(b.cs_truies_saillies,0),1) AS taux_mb_saillies FROM bande b WHERE b.date_mb IS NULL OR b.date_mb>=date('now',?) ORDER BY b.date_mb IS NULL,b.date_mb,b.id").bind(cutoff).fetch_all(&state.pool).await?;
    let mut ctx=context(&session);ctx.insert("bandes".into(),Value::Array(rows_to_json(rows)?));ctx.insert("mois".into(),json!(months));render(&state,"productivite.html",Value::Object(ctx))
}

async fn parameter_f64(pool:&SqlitePool,key:&str,default:f64)->AppResult<f64>{let value:Option<String>=sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle=?").bind(key).fetch_optional(pool).await?.flatten();Ok(value.and_then(|value|parse_french_number(&value)).unwrap_or(default))}
async fn parameter_list(pool:&SqlitePool,key:&str,defaults:&[&str])->AppResult<Vec<String>>{let value:Option<String>=sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle=?").bind(key).fetch_optional(pool).await?.flatten();Ok(value.filter(|value|!value.trim().is_empty()).map(|value|value.split(',').map(str::trim).filter(|value|!value.is_empty()).map(str::to_string).collect()).unwrap_or_else(||defaults.iter().map(|value|(*value).to_string()).collect()))}

async fn reformes_seuils(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let keys=["seuil_nv_min","seuil_sevres_min","seuil_retours_max","seuil_ecrases_max","seuil_rang_max","seuil_chetifs_max"];let mut tx=state.pool.begin().await?;for key in keys{if let Some(value)=form_f64(&form,key).filter(|value|value.is_finite()&&*value>=0.0){sqlx::query("INSERT INTO parametre(cle,valeur) VALUES(?,?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(key).bind(value.to_string()).execute(&mut *tx).await?;}}tx.commit().await?;Ok(Redirect::to("/reformes").into_response())}
async fn reformes_criteres(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let allowed=["nv","sevres","retours","ecrases","rang","chetifs"];let selected=allowed.iter().filter(|code|form.contains_key(&format!("crit_{code}"))).copied().collect::<Vec<_>>();if selected.is_empty(){return Err(AppError::Invalid("Sélectionne au moins un critère".into()))}sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('reforme_criteres',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(selected.join(",")).execute(&state.pool).await?;Ok(Redirect::to("/reformes").into_response())}
async fn cochettes_criteres(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let allowed=["nv","sevres","ecrases","retours","rang","chetifs","issf"];let selected=allowed.iter().filter(|code|form.contains_key(&format!("crit_{code}"))).take(4).copied().collect::<Vec<_>>();if selected.is_empty(){return Err(AppError::Invalid("Sélectionne au moins un critère".into()))}sqlx::query("INSERT INTO parametre(cle,valeur) VALUES('cochette_criteres',?) ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur").bind(selected.join(",")).execute(&state.pool).await?;Ok(Redirect::to("/cochettes").into_response())}

async fn reformes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let seuils=json!({
        "seuil_nv_min":parameter_f64(&state.pool,"seuil_nv_min",13.0).await?,
        "seuil_sevres_min":parameter_f64(&state.pool,"seuil_sevres_min",11.0).await?,
        "seuil_retours_max":parameter_f64(&state.pool,"seuil_retours_max",2.0).await?,
        "seuil_ecrases_max":parameter_f64(&state.pool,"seuil_ecrases_max",4.0).await?,
        "seuil_rang_max":parameter_f64(&state.pool,"seuil_rang_max",7.0).await?,
        "seuil_chetifs_max":parameter_f64(&state.pool,"seuil_chetifs_max",20.0).await?,
    });
    let criteria=parameter_list(&state.pool,"reforme_criteres",&["nv","sevres","retours","ecrases","rang","chetifs"]).await?;
    let raw=generic_rows(&state.pool,"SELECT id,num_travail,bande_code,rang,perf_nv,perf_sevres,nb_retours,tx_chetifs,CAST(COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.truie_id=t.id AND lower(COALESCE(p.cause,'')) LIKE '%cras%'),0) AS INTEGER) AS ecrases FROM truie t WHERE reformee=0 ORDER BY num_travail").await?;
    let mut rows=Vec::new();
    for mut row in raw{let object=row.as_object_mut().expect("truie objet");let mut reasons=Vec::new();let f=|key:&str|object.get(key).and_then(Value::as_f64);let i=|key:&str|object.get(key).and_then(Value::as_i64).map(|value|value as f64);if criteria.iter().any(|c|c=="nv")&&f("perf_nv").is_some_and(|v|v<seuils["seuil_nv_min"].as_f64().unwrap_or(13.0)){reasons.push("nés vifs bas")};if criteria.iter().any(|c|c=="sevres")&&f("perf_sevres").is_some_and(|v|v<seuils["seuil_sevres_min"].as_f64().unwrap_or(11.0)){reasons.push("sevrés bas")};if criteria.iter().any(|c|c=="retours")&&i("nb_retours").is_some_and(|v|v>seuils["seuil_retours_max"].as_f64().unwrap_or(2.0)){reasons.push("retours élevés")};if criteria.iter().any(|c|c=="ecrases")&&i("ecrases").is_some_and(|v|v>seuils["seuil_ecrases_max"].as_f64().unwrap_or(4.0)){reasons.push("écrasés élevés")};if criteria.iter().any(|c|c=="rang")&&i("rang").is_some_and(|v|v>seuils["seuil_rang_max"].as_f64().unwrap_or(7.0)){reasons.push("rang élevé")};if criteria.iter().any(|c|c=="chetifs")&&f("tx_chetifs").is_some_and(|v|v>seuils["seuil_chetifs_max"].as_f64().unwrap_or(20.0)){reasons.push("chétifs élevés")};if!reasons.is_empty(){object.insert("raisons".into(),json!(reasons.join(", ")));object.insert("score".into(),json!(reasons.len()));rows.push(row)}}
    rows.sort_by_key(|row|std::cmp::Reverse(row.get("score").and_then(Value::as_u64).unwrap_or_default()));
    let exits=generic_rows(&state.pool,"SELECT id,num_travail,date_reforme,motif_sortie,rang FROM truie WHERE reformee=1 ORDER BY date_reforme DESC,id DESC LIMIT 200").await?;
    let mut ctx=context(&session);ctx.insert("seuils".into(),seuils);ctx.insert("criteres".into(),json!(criteria));ctx.insert("candidates".into(),Value::Array(rows));ctx.insert("sorties".into(),Value::Array(exits));render(&state,"reformes.html",Value::Object(ctx))
}

async fn cochettes(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let criteria=parameter_list(&state.pool,"cochette_criteres",&["nv","sevres","ecrases","retours"]).await?;
    let averages=generic_rows(&state.pool,"SELECT ROUND(AVG(perf_nv),2) AS nv,ROUND(AVG(perf_sevres),2) AS sevres,ROUND(AVG(issf),2) AS issf,ROUND(AVG(tx_chetifs),2) AS chetifs FROM truie WHERE reformee=0").await?.into_iter().next().unwrap_or_else(||json!({}));
    let rows=generic_rows(&state.pool,"SELECT id,num_travail,bande_code,mere_cochette,rang,perf_nv,perf_sevres,perf_tx_perte,nb_retours,issf,tx_chetifs,CAST(COALESCE((SELECT SUM(p.nb) FROM perteporcelet p WHERE p.truie_id=t.id AND lower(COALESCE(p.cause,'')) LIKE '%cras%'),0) AS INTEGER) AS ecrases FROM truie t WHERE reformee=0 AND (perf_nv IS NOT NULL OR perf_sevres IS NOT NULL) ORDER BY num_travail").await?;
    let threshold=criteria.len().saturating_sub(1).max(1);let mut candidates=Vec::new();let mut designated=Vec::new();
    for mut row in rows{let is_designated=row.get("mere_cochette").and_then(Value::as_i64)==Some(1);if is_designated{designated.push(row.clone())}let object=row.as_object_mut().expect("truie objet");let mut details=Vec::new();let f=|key:&str|object.get(key).and_then(Value::as_f64);let i=|key:&str|object.get(key).and_then(Value::as_i64);if criteria.iter().any(|c|c=="nv")&&f("perf_nv").is_some_and(|v|v>=averages["nv"].as_f64().unwrap_or(0.0)){details.push("nés vifs")};if criteria.iter().any(|c|c=="sevres")&&f("perf_sevres").is_some_and(|v|v>=averages["sevres"].as_f64().unwrap_or(0.0)){details.push("capacité maternelle")};if criteria.iter().any(|c|c=="ecrases")&&i("ecrases").unwrap_or_default()<=1{details.push("peu d’écrasés")};if criteria.iter().any(|c|c=="retours")&&i("nb_retours").unwrap_or_default()<=1{details.push("fertilité")};if criteria.iter().any(|c|c=="rang")&&i("rang").unwrap_or_default()>=3{details.push("longévité")};if criteria.iter().any(|c|c=="chetifs")&&f("tx_chetifs").is_some_and(|v|v<=averages["chetifs"].as_f64().unwrap_or(v)){details.push("homogénéité")};if criteria.iter().any(|c|c=="issf")&&f("issf").is_some_and(|v|v<=averages["issf"].as_f64().unwrap_or(v)){details.push("ISSF")};if details.len()>=threshold{object.insert("score".into(),json!(details.len()));object.insert("details".into(),json!(details.join(", ")));candidates.push(row)}}
    candidates.sort_by_key(|row|std::cmp::Reverse(row.get("score").and_then(Value::as_u64).unwrap_or_default()));let mut ctx=context(&session);ctx.insert("criteres".into(),json!(criteria));ctx.insert("moyennes".into(),averages);ctx.insert("candidates".into(),Value::Array(candidates));ctx.insert("designees".into(),Value::Array(designated));render(&state,"cochettes.html",Value::Object(ctx))
}

async fn ifip(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let references=generic_rows(&state.pool,"SELECT id,libelle,cle,annee,moyenne,tiers_sup,sens,decimales,ordre,CASE cle WHEN 'nes_vifs' THEN (SELECT ROUND(AVG(cs_nv_portee),2) FROM bande WHERE cs_nv_portee IS NOT NULL) WHEN 'sevres_portee' THEN (SELECT ROUND(AVG(cs_sevres_portee),2) FROM bande WHERE cs_sevres_portee IS NOT NULL) WHEN 'tx_pertes_allait' THEN (SELECT ROUND(AVG(cs_tx_pertes_nv),2) FROM bande WHERE cs_tx_pertes_nv IS NOT NULL) WHEN 'poids_sevrage' THEN (SELECT ROUND(AVG(cs_poids_sevrage),2) FROM bande WHERE cs_poids_sevrage IS NOT NULL) WHEN 'gmq_ps' THEN (SELECT ROUND(AVG(cs_gmq_ps),2) FROM bande WHERE cs_gmq_ps IS NOT NULL) WHEN 'gmq_engr' THEN (SELECT ROUND(AVG(cs_gmq_engr),2) FROM bande WHERE cs_gmq_engr IS NOT NULL) ELSE NULL END AS valeur_elevage FROM referenceifip ORDER BY ordre,id").await?;
    let mut ctx=context(&session);ctx.insert("references".into(),Value::Array(references));render(&state,"ifip.html",Value::Object(ctx))
}

async fn ifip_maj(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let id=form_i64(&form,"id").ok_or_else(||AppError::Invalid("Référence manquante".into()))?;let sense=match form.get("sens").map(String::as_str){Some("bas")=>"bas",_=>"haut"};sqlx::query("UPDATE referenceifip SET moyenne=?,tiers_sup=?,annee=?,sens=? WHERE id=?").bind(form_f64(&form,"moyenne")).bind(form_f64(&form,"tiers_sup")).bind(form_text(&form,"annee")).bind(sense).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/ifip").into_response())}

async fn charcutiers(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
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
    let band: Option<String> = sqlx::query_scalar("SELECT bande_code FROM porccharcutier WHERE id=?")
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
        .bind(form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string()))
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
    let animal: Option<i64> = sqlx::query_scalar(
        "SELECT charcutier_id FROM traitementcharcutier WHERE id=?",
    )
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
        "SELECT si.code AS site,s.id AS salle_id,s.nom AS salle,c.id AS case_id,c.nom AS case_nom,c.nb_max_porcs,CAST(COALESCE((SELECT SUM(CASE WHEN t.case_dest_id=c.id THEN COALESCE(t.nombre,0) ELSE -COALESCE(t.nombre,0) END) FROM transfert t WHERE t.espece='porc' AND (t.case_dest_id=c.id OR t.case_source_id=c.id)),0)-COALESCE((SELECT SUM(d.nombre) FROM declarationmort d WHERE d.case_id=c.id),0) AS INTEGER) AS porcs,(SELECT COUNT(*) FROM truie tr WHERE tr.reformee=0 AND tr.case_id=c.id) AS nb_truies,(SELECT GROUP_CONCAT(tr.num_travail,', ') FROM truie tr WHERE tr.reformee=0 AND tr.case_id=c.id) AS truies FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.code,COALESCE(s.ordre,0),s.nom,c.nom",
    )
    .await?;
    let mut bands = generic_rows(
        &state.pool,
        "SELECT id,code,date_mb,site FROM bande WHERE active=1 ORDER BY date_mb,code",
    )
    .await?;
    for band in &mut bands {
        let object = band.as_object_mut().expect("generic row object");
        let id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        let code = object.get("code").and_then(Value::as_str).unwrap_or_default().to_string();
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
    ctx.insert("today".into(), json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state, "transferts.html", Value::Object(ctx))
}

async fn case_pig_count(pool: &SqlitePool, case_id: i64) -> AppResult<i64> {
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
        "SELECT CAST(COALESCE(SUM(CASE WHEN case_dest_id=? THEN COALESCE(nombre,0) ELSE -COALESCE(nombre,0) END),0) AS INTEGER) FROM transfert WHERE espece='porc' AND (case_dest_id=? OR case_source_id=?) AND (? IS NULL OR date>?)",
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
    Ok((base + movements - deaths).max(0))
}

async fn remaining_band_pigs(pool: &SqlitePool, band_id: i64, code: &str) -> AppResult<i64> {
    let stock_date: Option<String> = sqlx::query_scalar(
        "SELECT MAX(date) FROM mouvementstock WHERE est_stock=1 AND bande_code=?",
    )
    .bind(code)
    .fetch_one(pool)
    .await?;
    let Some(stock_date) = stock_date else { return Ok(0) };
    let base = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM mouvementstock WHERE est_stock=1 AND bande_code=? AND date=? AND lower(COALESCE(libelle,'')) NOT LIKE '%truie%' AND lower(COALESCE(libelle,'')) NOT LIKE '%pleine%' AND lower(COALESCE(libelle,'')) NOT LIKE '%lactation%'",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let deaths = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM declarationmort WHERE bande_code=? AND date>=?",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let slaughter_deaths = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM porccharcutier WHERE bande_code=? AND date_mort IS NOT NULL AND date_mort>=?",
    )
    .bind(code)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let sold = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(CASE WHEN lots_json IS NOT NULL AND json_valid(lots_json) AND json_array_length(lots_json)>=2 THEN (SELECT COALESCE(SUM(CAST(json_extract(j.value,'$.nb_porcs') AS INTEGER)),0) FROM json_each(v.lots_json) j WHERE COALESCE(CAST(json_extract(j.value,'$.bande_id') AS INTEGER),v.bande_id)=?) WHEN bande_id=? THEN COALESCE(nb_porcs,0) ELSE 0 END),0) AS INTEGER) FROM venteapport v WHERE date>=?",
    )
    .bind(band_id)
    .bind(band_id)
    .bind(&stock_date)
    .fetch_one(pool)
    .await?;
    let transferred = sqlx::query_scalar::<_, i64>(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM transfert WHERE espece='porc' AND bande_id=?",
    )
    .bind(band_id)
    .fetch_one(pool)
    .await?;
    Ok((base - deaths - slaughter_deaths - sold - transferred).max(0))
}

async fn transferts_porcs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let source = form_text(&form, "source").ok_or_else(|| AppError::Invalid("Source obligatoire".into()))?;
    let destination = form_i64(&form, "case_dest_id").ok_or_else(|| AppError::Invalid("Case de destination obligatoire".into()))?;
    let number = form_i64(&form, "nombre").filter(|value| *value > 0).ok_or_else(|| AppError::Invalid("Nombre invalide".into()))?;
    let date = form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
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
            return Err(AppError::Invalid(format!("Capacité dépassée : {} place(s) disponible(s)", (capacity - present).max(0))));
        }
    }
    let (kind, raw_id) = source.split_once(':').ok_or_else(|| AppError::Invalid("Source invalide".into()))?;
    let source_id = raw_id.parse::<i64>().map_err(|_| AppError::Invalid("Source invalide".into()))?;
    let mut band_id = None;
    let mut source_case = None;
    let mut source_room = None;
    match kind {
        "bande" => {
            let code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=? AND active=1")
                .bind(source_id).fetch_optional(&state.pool).await?
                .ok_or_else(|| AppError::Invalid("Bande source introuvable".into()))?;
            let available = remaining_band_pigs(&state.pool, source_id, &code).await?;
            if number > available {
                return Err(AppError::Invalid(format!("Effectif insuffisant : {available} porc(s) disponible(s)")));
            }
            band_id = Some(source_id);
        }
        "case" => {
            if source_id == destination {
                return Err(AppError::Invalid("La source et la destination sont identiques".into()));
            }
            let room: i64 = sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
                .bind(source_id).fetch_optional(&state.pool).await?
                .ok_or_else(|| AppError::Invalid("Case source introuvable".into()))?;
            let available = case_pig_count(&state.pool, source_id).await?;
            if number > available {
                return Err(AppError::Invalid(format!("Effectif insuffisant : {available} porc(s) disponible(s)")));
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
    let destination = form_i64(&form, "case_dest_id").ok_or_else(|| AppError::Invalid("Case de destination obligatoire".into()))?;
    let destination_room: i64 = sqlx::query_scalar("SELECT salle_id FROM casesalle WHERE id=?")
        .bind(destination).fetch_optional(&state.pool).await?
        .ok_or_else(|| AppError::Invalid("Case de destination introuvable".into()))?;
    let date = form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
    let mut tx = state.pool.begin().await?;
    for id in ids {
        let current = sqlx::query_as::<_, (Option<i64>, Option<i64>)>("SELECT t.case_id,COALESCE(t.salle_id,c.salle_id) FROM truie t LEFT JOIN casesalle c ON c.id=t.case_id WHERE t.id=? AND t.reformee=0")
            .bind(id).fetch_optional(&mut *tx).await?;
        let Some((source_case, source_room)) = current else { continue };
        if source_case == Some(destination) { continue }
        sqlx::query("UPDATE truie SET case_id=?,salle_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
            .bind(destination).bind(destination_room).bind(id).execute(&mut *tx).await?;
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
                let latest: Option<i64> = sqlx::query_scalar("SELECT MAX(id) FROM transfert WHERE espece='truie' AND truie_id=?")
                    .bind(sow_id).fetch_one(&mut *tx).await?;
                if latest == Some(id) {
                    sqlx::query("UPDATE truie SET case_id=?,salle_id=?,updated_at=CURRENT_TIMESTAMP WHERE id=?")
                        .bind(source_case).bind(source_room).bind(sow_id).execute(&mut *tx).await?;
                }
            }
        }
        sqlx::query("DELETE FROM transfert WHERE id=?").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
    }
    Ok(Redirect::to("/transferts").into_response())
}

async fn effectifs(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let stock_date: Option<String> = sqlx::query_scalar("SELECT MAX(date) FROM mouvementstock WHERE est_stock=1")
        .fetch_one(&state.pool).await?;
    let snapshot = if let Some(ref day) = stock_date {
        let rows = sqlx::query("SELECT COALESCE(NULLIF(trim(REPLACE(lower(COALESCE(libelle,'stock')),'stock','')),''),'autres') AS stade,CAST(COALESCE(SUM(nombre),0) AS INTEGER) AS animaux,ROUND(COALESCE(SUM(montant),0),2) AS valeur FROM mouvementstock WHERE est_stock=1 AND date=? GROUP BY stade ORDER BY stade")
            .bind(day).fetch_all(&state.pool).await?;
        rows_to_json(rows)?
    } else { Vec::new() };
    let movements = generic_rows(&state.pool,"SELECT id,date,bande_code,code_ifip,nombre,poids,montant,libelle,destination,type_saisie FROM mouvementstock WHERE est_stock=0 ORDER BY date DESC,id DESC LIMIT 80").await?;
    let losses = generic_rows(&state.pool,"SELECT bande_code,CAST(COALESCE(SUM(CASE WHEN code_ifip='39' THEN nombre ELSE 0 END),0) AS INTEGER) AS pertes_ps,CAST(COALESCE(SUM(CASE WHEN code_ifip='49' THEN nombre ELSE 0 END),0) AS INTEGER) AS pertes_engraissement FROM mouvementstock WHERE code_ifip IN('39','49') AND bande_code IS NOT NULL GROUP BY bande_code ORDER BY bande_code").await?;
    let mut cases = generic_rows(&state.pool,"SELECT c.id,c.nom,s.nom AS salle,COALESCE(si.nom,si.code) AS site,c.nb_max_porcs FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY COALESCE(si.nom,si.code),s.ordre,c.nom").await?;
    for case in &mut cases {
        let object=case.as_object_mut().expect("case objet");
        let id=object.get("id").and_then(Value::as_i64).unwrap_or_default();
        object.insert("effectif".into(),json!(case_pig_count(&state.pool,id).await?));
    }
    let case_inventories=generic_rows(&state.pool,"SELECT i.id,i.date,i.nombre,i.note,i.cree_par,c.nom AS case_nom,s.nom AS salle,COALESCE(si.nom,si.code) AS site FROM inventairecase i JOIN casesalle c ON c.id=i.case_id JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY i.date DESC,i.id DESC LIMIT 100").await?;
    let mut bands = generic_rows(&state.pool,"SELECT id,code,date_mb,site FROM bande WHERE active=1 ORDER BY date_mb,code").await?;
    for band in &mut bands {
        let object = band.as_object_mut().expect("generic row object");
        let id = object.get("id").and_then(Value::as_i64).unwrap_or_default();
        let code = object.get("code").and_then(Value::as_str).unwrap_or_default().to_string();
        object.insert("effectif".into(), json!(remaining_band_pigs(&state.pool,id,&code).await?));
    }
    let total_animals: i64 = snapshot.iter().filter_map(|row| row.get("animaux").and_then(Value::as_i64)).sum();
    let total_value: f64 = snapshot.iter().filter_map(|row| row.get("valeur").and_then(Value::as_f64)).sum();
    let mut ctx = context(&session);
    ctx.insert("date_stock".into(), json!(stock_date));
    ctx.insert("snapshot".into(), Value::Array(snapshot));
    ctx.insert("mouvements".into(), Value::Array(movements));
    ctx.insert("pertes".into(), Value::Array(losses));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("cases".into(),Value::Array(cases));
    ctx.insert("inventaires_cases".into(),Value::Array(case_inventories));
    ctx.insert("total_animaux".into(), json!(total_animals));
    ctx.insert("total_valeur".into(), json!(total_value));
    ctx.insert("today".into(), json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state,"effectifs.html",Value::Object(ctx))
}

async fn effectifs_inventaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let band_id = form_i64(&form,"bande_id").ok_or_else(||AppError::Invalid("Bande obligatoire".into()))?;
    let code: String = sqlx::query_scalar("SELECT code FROM bande WHERE id=?")
        .bind(band_id).fetch_optional(&state.pool).await?
        .ok_or_else(||AppError::Invalid("Bande introuvable".into()))?;
    let number = form_i64(&form,"nombre").filter(|value|*value>=0).ok_or_else(||AppError::Invalid("Effectif invalide".into()))?;
    let date = form_text(&form,"date").unwrap_or_else(||Local::now().date_naive().format("%Y-%m-%d").to_string());
    sqlx::query("INSERT INTO mouvementstock(code_ifip,date,bande_code,nombre,poids,montant,libelle,destination,type_saisie,est_stock) VALUES(NULL,?,?,?,?,?,?,NULL,'inventaire',1)")
        .bind(&date).bind(&code).bind(number).bind(form_f64(&form,"poids")).bind(form_f64(&form,"montant")).bind(form_text(&form,"libelle").unwrap_or_else(||"stock porcs".into())).execute(&state.pool).await?;
    Ok(Redirect::to("/effectifs").into_response())
}

async fn effectifs_inventaire_case(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session,&form)?;
    let case_id=form_i64(&form,"case_id").ok_or_else(||AppError::Invalid("Case obligatoire".into()))?;
    let number=form_i64(&form,"nombre").filter(|value|*value>=0).ok_or_else(||AppError::Invalid("Effectif invalide".into()))?;
    let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM casesalle WHERE id=?").bind(case_id).fetch_one(&state.pool).await?;
    if exists==0{return Err(AppError::Invalid("Case introuvable".into()))}
    sqlx::query("INSERT INTO inventairecase(case_id,date,nombre,note,cree_par) VALUES(?,?,?,?,?)")
        .bind(case_id)
        .bind(form_text(&form,"date").unwrap_or_else(||Local::now().date_naive().format("%Y-%m-%d").to_string()))
        .bind(number).bind(form_text(&form,"note")).bind(&session.identifiant).execute(&state.pool).await?;
    Ok(Redirect::to("/effectifs").into_response())
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
        "SELECT 'Doublons de numéros de truies actives' AS controle,COUNT(*) AS anomalies FROM (SELECT num_travail FROM truie WHERE reformee=0 GROUP BY lower(trim(num_travail)) HAVING COUNT(*)>1) UNION ALL SELECT 'Événements sans truie',COUNT(*) FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id WHERE e.truie_id IS NOT NULL AND t.id IS NULL UNION ALL SELECT 'Événements sans bande',COUNT(*) FROM evenement e LEFT JOIN bande b ON b.id=e.bande_id WHERE e.bande_id IS NOT NULL AND b.id IS NULL UNION ALL SELECT 'Transferts vers une case absente',COUNT(*) FROM transfert t LEFT JOIN casesalle c ON c.id=t.case_dest_id WHERE t.case_dest_id IS NOT NULL AND c.id IS NULL UNION ALL SELECT 'Bandes actives sans date de mise-bas',COUNT(*) FROM bande WHERE active=1 AND (date_mb IS NULL OR trim(date_mb)='') UNION ALL SELECT 'Truies actives sans bande',COUNT(*) FROM truie WHERE reformee=0 AND (bande_code IS NULL OR trim(bande_code)='')",
        &["controle", "anomalies"],
    )
    .await
}

async fn energie(
    State(state): State<AppState>, Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let meters = sqlx::query_as::<_,CompteurEnergie>("SELECT id,nom,type,site_id,unite,rappel_jours,actif,note FROM compteur_energie WHERE actif=1 ORDER BY type,nom")
        .fetch_all(&state.pool).await?;
    let sites = generic_rows(&state.pool,"SELECT id,code,nom FROM site ORDER BY COALESCE(nom,code)").await?;
    let mut data = Vec::new();
    for meter in &meters {
        let mut readings = sqlx::query_as::<_,ReleveCompteur>("SELECT id,compteur_id,date_releve,valeur_index,bandes,note,remplacement_compteur FROM releve_compteur WHERE compteur_id=? ORDER BY date_releve,id")
            .bind(meter.id).fetch_all(&state.pool).await?;
        let mut previous: Option<f64> = None;
        let mut enriched = Vec::new();
        for reading in &readings {
            let consumption = if reading.remplacement_compteur { None } else { previous.map(|value| reading.valeur_index-value) };
            previous = Some(reading.valeur_index);
            let mut value = serde_json::to_value(reading).unwrap_or_default();
            value.as_object_mut().unwrap().insert("conso".into(),json!(consumption));
            enriched.push(value);
        }
        enriched.reverse(); readings.reverse();
        let last_date = readings.first().and_then(|r|NaiveDate::parse_from_str(&r.date_releve[..r.date_releve.len().min(10)],"%Y-%m-%d").ok());
        let due = if meter.r#type=="eau" { meter.rappel_jours.map(|days|last_date.unwrap_or(Local::now().date_naive()-Duration::days(days))+Duration::days(days)) } else { None };
        let overdue = due.map(|date| (Local::now().date_naive()-date).num_days().max(0));
        let site: Option<String> = if let Some(id)=meter.site_id { sqlx::query_scalar("SELECT COALESCE(nom,code) FROM site WHERE id=?").bind(id).fetch_optional(&state.pool).await? } else { None };
        data.push(json!({"compteur":meter,"releves":enriched,"site":site,"alerte":overdue.map(|d|d>=0 && due.unwrap()<=Local::now().date_naive()).unwrap_or(false),"jours_retard":overdue}));
    }
    let mut ctx=context(&session); ctx.insert("compteurs".into(),serde_json::to_value(meters).unwrap_or_default()); ctx.insert("sites".into(),Value::Array(sites)); ctx.insert("data".into(),Value::Array(data)); ctx.insert("today".into(),json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state,"energie.html",Value::Object(ctx))
}

async fn energie_compteur(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{
    require_writer(&session)?;verify_csrf(&session,&form)?;let name=form_text(&form,"nom").ok_or_else(||AppError::Invalid("Nom obligatoire".into()))?;let kind=if form.get("type").map(String::as_str)==Some("electricite"){"electricite"}else{"eau"};
    let unit=if kind=="electricite"{"kWh"}else{"m³"};sqlx::query("INSERT INTO compteur_energie(nom,type,site_id,unite,rappel_jours,actif,note) SELECT ?,?,?,?,?,1,? WHERE NOT EXISTS(SELECT 1 FROM compteur_energie WHERE actif=1 AND type=? AND lower(trim(nom))=lower(trim(?)) AND COALESCE(site_id,-1)=COALESCE(?,-1))").bind(&name).bind(kind).bind(form_i64(&form,"site_id")).bind(unit).bind(if kind=="eau"{form_i64(&form,"rappel_jours")}else{None}).bind(form_text(&form,"note")).bind(kind).bind(&name).bind(form_i64(&form,"site_id")).execute(&state.pool).await?;Ok(Redirect::to("/energie").into_response())
}

async fn energie_releve(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{
    require_writer(&session)?;verify_csrf(&session,&form)?;let meter_id=form_i64(&form,"compteur_id").ok_or_else(||AppError::Invalid("Compteur obligatoire".into()))?;let date=form_text(&form,"date_releve").unwrap_or_else(||Local::now().date_naive().format("%Y-%m-%d").to_string());let index=form_f64(&form,"index").ok_or_else(||AppError::Invalid("Index invalide".into()))?;let replacement=form.contains_key("remplacement_compteur");
    let site:Option<String>=sqlx::query_scalar("SELECT COALESCE(s.code,s.nom) FROM compteur_energie c LEFT JOIN site s ON s.id=c.site_id WHERE c.id=?").bind(meter_id).fetch_optional(&state.pool).await?.flatten();
    let bands=present_bands(&state.pool,site.as_deref(),&date).await?;sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index,bandes,note,remplacement_compteur) VALUES(?,?,?,?,?,?)").bind(meter_id).bind(&date).bind(index).bind(if bands.is_empty(){None}else{Some(bands.join(","))}).bind(form_text(&form,"note").or_else(||replacement.then(||"Compteur remplacé – nouvel index de départ".into()))).bind(replacement).execute(&state.pool).await?;Ok(Redirect::to(&format!("/energie#compteur-{meter_id}")).into_response())
}

async fn present_bands(pool:&SqlitePool,site:Option<&str>,day:&str)->AppResult<Vec<String>>{let Some(day)=NaiveDate::parse_from_str(&day[..day.len().min(10)],"%Y-%m-%d").ok() else{return Ok(vec![])};let rows=generic_rows(pool,"SELECT code,date_mb,site FROM bande WHERE date_mb IS NOT NULL").await?;let mut out=Vec::new();for row in rows{let obj=row.as_object().unwrap();let code=obj.get("code").and_then(Value::as_str).unwrap_or("");let mb=obj.get("date_mb").and_then(Value::as_str).and_then(|v|NaiveDate::parse_from_str(&v[..v.len().min(10)],"%Y-%m-%d").ok());let row_site=obj.get("site").and_then(Value::as_str);if let Some(mb)=mb{if site.is_some()&&row_site.is_some()&&site!=row_site{continue}if day>=mb-Duration::days(115)&&day<=mb+Duration::days(225)&&!code.is_empty(){out.push(code.to_string())}}}out.sort();out.dedup();Ok(out)}

async fn energie_rappel(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("UPDATE compteur_energie SET rappel_jours=? WHERE id=? AND type='eau'").bind(form_i64(&form,"rappel_jours")).bind(id).execute(&state.pool).await?;Ok(Redirect::to(&format!("/energie#compteur-{id}")).into_response())}
async fn energie_releve_supprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let meter:Option<i64>=sqlx::query_scalar("SELECT compteur_id FROM releve_compteur WHERE id=?").bind(id).fetch_optional(&state.pool).await?;sqlx::query("DELETE FROM releve_compteur WHERE id=?").bind(id).execute(&state.pool).await?;Ok(Redirect::to(&meter.map(|x|format!("/energie#compteur-{x}")).unwrap_or_else(||"/energie".into())).into_response())}

async fn energie_modele_csv()->Response{let body="\u{feff}type_compteur;nom_compteur;site;date_releve;index;unite;rappel_jours;remplacement_compteur;note\r\neau;Compteur général;Site principal;2026-08-16;12345,6;m³;7;;Relevé hebdomadaire\r\n";let mut headers=HeaderMap::new();headers.insert(header::CONTENT_TYPE,HeaderValue::from_static("text/csv; charset=utf-8"));headers.insert(header::CONTENT_DISPOSITION,HeaderValue::from_static("attachment; filename=modele_import_eau_electricite.csv"));(headers,body).into_response()}

async fn energie_import(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_writer(&session)?;
    let mut data = None;
    let mut csrf = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::Invalid(error.to_string()))?
    {
        match field.name() {
            Some("fichier") => {
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
        let date = row
            .get("date_releve")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::Invalid("date_releve manquante".into()))?;
        NaiveDate::parse_from_str(&date[..date.len().min(10)], "%Y-%m-%d")
            .map_err(|_| AppError::Invalid(format!("date_releve invalide : {date}")))?;
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
        let duplicate: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM releve_compteur WHERE compteur_id=? AND date_releve=? AND ABS(valeur_index-?)<0.000001",
        )
        .bind(meter_id)
        .bind(date)
        .bind(index)
        .fetch_one(&mut *transaction)
        .await?;
        if duplicate == 0 {
            sqlx::query("INSERT INTO releve_compteur(compteur_id,date_releve,valeur_index,note,remplacement_compteur) VALUES(?,?,?,?,?)")
                .bind(meter_id)
                .bind(date)
                .bind(index)
                .bind(row.get("note"))
                .bind(matches!(row.get("remplacement_compteur").map(|value| value.to_lowercase()).as_deref(), Some("oui" | "1" | "true" | "x")))
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
) -> AppResult<Html<String>> {
    let ventes_total: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport").fetch_one(&state.pool).await?;
    let aliment: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM livraisonaliment").fetch_one(&state.pool).await?;
    let veto: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatveto").fetch_one(&state.pool).await?;
    let semence: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_ht),0) AS REAL) FROM achatsemence").fetch_one(&state.pool).await?;
    let genetique: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(COALESCE(montant_net,montant_ht)),0) AS REAL) FROM achatgenetique").fetch_one(&state.pool).await?;
    let bands = generic_rows(&state.pool,"SELECT id,code,date_mb,site FROM bande ORDER BY active DESC,date_mb IS NULL,date_mb,id").await?;
    let band_results = generic_rows(
        &state.pool,
        "WITH ventes AS (SELECT bande_id,SUM(COALESCE(nb_porcs,0)) AS porcs,SUM(COALESCE(poids_total,0)) AS poids,SUM(COALESCE(montant_net,0)) AS recettes FROM venteapport GROUP BY bande_id),aliment AS (SELECT bande_id,SUM(COALESCE(montant_ht,0)) AS cout FROM livraisonaliment GROUP BY bande_id),veto AS (SELECT bande_id,SUM(COALESCE(montant_ht,0)) AS cout FROM achatveto GROUP BY bande_id),semence AS (SELECT bande_id,SUM(COALESCE(montant_ht,0)) AS cout FROM achatsemence GROUP BY bande_id),genetique AS (SELECT bande_code,SUM(COALESCE(montant_net,montant_ht,0)) AS cout FROM achatgenetique GROUP BY bande_code) SELECT b.id,b.code,b.site,CAST(COALESCE(v.porcs,0) AS INTEGER) AS porcs,ROUND(COALESCE(v.poids,0),1) AS poids,ROUND(COALESCE(v.recettes,0),2) AS recettes,ROUND(COALESCE(a.cout,0),2) AS aliment,ROUND(COALESCE(vt.cout,0),2) AS veto,ROUND(COALESCE(se.cout,0),2) AS semence,ROUND(COALESCE(g.cout,0),2) AS genetique,ROUND(COALESCE(v.recettes,0)-COALESCE(a.cout,0)-COALESCE(vt.cout,0)-COALESCE(se.cout,0)-COALESCE(g.cout,0),2) AS marge,ROUND((COALESCE(a.cout,0)+COALESCE(vt.cout,0)+COALESCE(se.cout,0)+COALESCE(g.cout,0))/NULLIF(v.porcs,0),2) AS cout_par_porc,ROUND(COALESCE(v.recettes,0)/NULLIF(v.poids,0),3) AS prix_net_kg FROM bande b LEFT JOIN ventes v ON v.bande_id=b.id LEFT JOIN aliment a ON a.bande_id=b.id LEFT JOIN veto vt ON vt.bande_id=b.id LEFT JOIN semence se ON se.bande_id=b.id LEFT JOIN genetique g ON g.bande_code=b.code WHERE v.porcs IS NOT NULL OR a.cout IS NOT NULL OR vt.cout IS NOT NULL OR se.cout IS NOT NULL OR g.cout IS NOT NULL ORDER BY b.date_mb IS NULL,b.date_mb,b.id",
    )
    .await?;
    let ventes = generic_rows(&state.pool,"SELECT id,date,num_apport,nb_porcs,poids_total,ROUND(montant_net/NULLIF(poids_total,0),3) AS prix_net_kg,montant_net,tmp,total_retenues,tx_qualification,nb_hors_poids,nb_tmp_bas FROM venteapport ORDER BY date DESC,id DESC LIMIT 50").await?;
    let achats = generic_rows(&state.pool,"SELECT id,date,'aliment' AS categorie,produit AS libelle,tonnage AS quantite,montant_ht FROM livraisonaliment UNION ALL SELECT id,date,'vétérinaire',produit,quantite,montant_ht FROM achatveto UNION ALL SELECT id,date,'semence',designation,nb_doses,montant_ht FROM achatsemence UNION ALL SELECT id,date,'génétique',designation,nb_animaux,COALESCE(montant_net,montant_ht) FROM achatgenetique ORDER BY date DESC,id DESC LIMIT 50").await?;
    let valuations = generic_rows(&state.pool,"SELECT id,num_apport,date,libelle,montant,categorie,CASE WHEN lower(COALESCE(categorie,''))='retenue' THEN 1 ELSE 0 END AS est_retenue FROM valorisationapport ORDER BY date DESC,id DESC LIMIT 200").await?;
    let monthly = generic_rows(&state.pool,"WITH RECURSIVE mois(m) AS (SELECT date('now','start of month','-11 months') UNION ALL SELECT date(m,'+1 month') FROM mois WHERE m<date('now','start of month')),depenses AS (SELECT substr(date,1,7) AS m,SUM(COALESCE(montant_ht,0)) AS montant FROM livraisonaliment GROUP BY m UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_ht,0)) FROM achatveto GROUP BY substr(date,1,7) UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_ht,0)) FROM achatsemence GROUP BY substr(date,1,7) UNION ALL SELECT substr(date,1,7),SUM(COALESCE(montant_net,montant_ht,0)) FROM achatgenetique GROUP BY substr(date,1,7)),revenus AS (SELECT substr(date,1,7) AS m,SUM(COALESCE(montant_net,0)) AS montant,SUM(COALESCE(poids_total,0)) AS poids FROM venteapport GROUP BY m) SELECT substr(m.m,1,7) AS mois,ROUND(COALESCE((SELECT SUM(d.montant) FROM depenses d WHERE d.m=substr(m.m,1,7)),0),2) AS depenses,ROUND(COALESCE(r.montant,0),2) AS revenus,ROUND(r.montant/NULLIF(r.poids,0),3) AS prix_net_kg FROM mois m LEFT JOIN revenus r ON r.m=substr(m.m,1,7) ORDER BY m.m").await?;
    let unallocated = generic_rows(&state.pool,"SELECT 'Aliment' AS categorie,ROUND(COALESCE(SUM(montant_ht),0),2) AS montant FROM livraisonaliment WHERE bande_id IS NULL UNION ALL SELECT 'Vétérinaire',ROUND(COALESCE(SUM(montant_ht),0),2) FROM achatveto WHERE bande_id IS NULL UNION ALL SELECT 'Semence',ROUND(COALESCE(SUM(montant_ht),0),2) FROM achatsemence WHERE bande_id IS NULL UNION ALL SELECT 'Génétique',ROUND(COALESCE(SUM(COALESCE(montant_net,montant_ht)),0),2) FROM achatgenetique WHERE bande_code IS NULL OR trim(bande_code)=''").await?;
    let total_weight: f64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(poids_total),0) AS REAL) FROM venteapport").fetch_one(&state.pool).await?;
    let total_pigs: i64 = sqlx::query_scalar("SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER) FROM venteapport").fetch_one(&state.pool).await?;
    let mut ctx = context(&session);
    ctx.insert("totaux".into(),json!({"ventes":ventes_total,"aliment":aliment,"veto":veto,"semence":semence,"genetique":genetique,"marge":ventes_total-aliment-veto-semence-genetique,"porcs":total_pigs,"prix_net_kg":if total_weight>0.0{Some(ventes_total/total_weight)}else{None}}));
    ctx.insert("bandes".into(),Value::Array(bands));
    ctx.insert("resultats_bandes".into(),Value::Array(band_results));
    ctx.insert("ventes".into(),Value::Array(ventes));
    ctx.insert("achats".into(),Value::Array(achats));
    ctx.insert("valorisations".into(),Value::Array(valuations));
    ctx.insert("mensuel".into(),Value::Array(monthly));
    ctx.insert("non_affectes".into(),Value::Array(unallocated));
    ctx.insert("today".into(),json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state,"economique.html",Value::Object(ctx))
}

async fn economique_aliment(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let amount=economic_amount(&form,"montant_ht").ok_or_else(||AppError::Invalid("Montant HT obligatoire".into()))?;sqlx::query("INSERT INTO livraisonaliment(date,fournisseur,produit,silo,tonnage,pu_ht,montant_ht,num_facture,site,bande_id) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(form_text(&form,"date")).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"produit")).bind(form_text(&form,"silo")).bind(form_f64(&form,"tonnage")).bind(form_f64(&form,"pu_ht")).bind(amount).bind(form_text(&form,"num_facture")).bind(form_text(&form,"site")).bind(form_i64(&form,"bande_id")).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}
async fn economique_veto(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let amount=economic_amount(&form,"montant_ht").ok_or_else(||AppError::Invalid("Montant HT obligatoire".into()))?;sqlx::query("INSERT INTO achatveto(date,produit,quantite,pu_ht,montant_ht,num_facture,delai_attente,fournisseur,site,bande_id) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(form_text(&form,"date")).bind(form_text(&form,"produit")).bind(form_f64(&form,"quantite")).bind(form_f64(&form,"pu_ht")).bind(amount).bind(form_text(&form,"num_facture")).bind(form_i64(&form,"delai_attente")).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"site")).bind(form_i64(&form,"bande_id")).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}
async fn economique_vente(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let number=form_i64(&form,"nb_porcs").filter(|value|*value>=0);let weight=form_f64(&form,"poids_total").filter(|value|*value>=0.0);let average=match(number,weight){(Some(n),Some(w))if n>0=>Some(w/n as f64),_=>form_f64(&form,"poids_moyen")};let amount=economic_amount(&form,"montant_net").ok_or_else(||AppError::Invalid("Montant net obligatoire".into()))?;sqlx::query("INSERT INTO venteapport(date,num_apport,bande_id,nb_porcs,poids_total,poids_moyen,prix_moyen,plus_value,montant_net,tmp,tx_qualification,nb_hors_poids,nb_tmp_bas,nb_g2,nb_tatouage,nb_qualifies,nb_livres,muscle_gamme,muscle_lot,total_retenues) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(form_text(&form,"date")).bind(form_text(&form,"num_apport")).bind(form_i64(&form,"bande_id")).bind(number).bind(weight).bind(average).bind(form_f64(&form,"prix_moyen")).bind(form_f64(&form,"plus_value")).bind(amount).bind(form_f64(&form,"tmp")).bind(form_f64(&form,"tx_qualification")).bind(form_i64(&form,"nb_hors_poids")).bind(form_i64(&form,"nb_tmp_bas")).bind(form_i64(&form,"nb_g2")).bind(form_i64(&form,"nb_tatouage")).bind(form_i64(&form,"nb_qualifies")).bind(form_i64(&form,"nb_livres")).bind(form_f64(&form,"muscle_gamme")).bind(form_f64(&form,"muscle_lot")).bind(form_f64(&form,"total_retenues")).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}
async fn economique_semence(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let amount=economic_amount(&form,"montant_ht").ok_or_else(||AppError::Invalid("Montant HT obligatoire".into()))?;let ttc=economic_amount(&form,"montant_ttc");sqlx::query("INSERT INTO achatsemence(date,num_facture,fournisseur,designation,nb_doses,montant_ht,montant_ttc,bande_id,note) VALUES(?,?,?,?,?,?,?,?,?)").bind(form_text(&form,"date")).bind(form_text(&form,"num_facture")).bind(form_text(&form,"fournisseur")).bind(form_text(&form,"designation")).bind(form_i64(&form,"nb_doses")).bind(amount).bind(ttc).bind(form_i64(&form,"bande_id")).bind(form_text(&form,"note")).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}
async fn economique_genetique(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let amount=economic_amount(&form,"montant_net").or_else(||economic_amount(&form,"montant_ht")).ok_or_else(||AppError::Invalid("Montant obligatoire".into()))?;sqlx::query("INSERT INTO achatgenetique(date,num_facture,fournisseur,designation,nb_animaux,poids_total,prix_moyen,montant_ht,montant_net,bande_code,note) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(form_text(&form,"date")).bind(form_text(&form,"num_facture")).bind(form_text(&form,"fournisseur").unwrap_or_else(||"Cooperl".into())).bind(form_text(&form,"designation")).bind(form_i64(&form,"nb_animaux")).bind(form_f64(&form,"poids_total")).bind(form_f64(&form,"prix_moyen")).bind(None::<f64>).bind(amount).bind(form_text(&form,"bande_code")).bind(form_text(&form,"note")).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}

async fn economique_valorisation(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let label=form_text(&form,"libelle").ok_or_else(||AppError::Invalid("Libellé obligatoire".into()))?;let lower=label.to_lowercase();let forced_retention=["équarrissage","equarrissage","groupement","cvee","contribution sanitaire","cotisation"].iter().any(|needle|lower.contains(needle));let category=if forced_retention||form.get("categorie").map(String::as_str)==Some("retenue"){"retenue"}else{"valorisation"};let amount=form_f64(&form,"montant").ok_or_else(||AppError::Invalid("Montant obligatoire".into()))?;let stored=if category=="retenue"{-amount.abs()}else{amount};let number=form_text(&form,"num_apport");let mut tx=state.pool.begin().await?;sqlx::query("INSERT INTO valorisationapport(num_apport,date,libelle,montant,categorie) VALUES(?,?,?,?,?)").bind(&number).bind(form_text(&form,"date")).bind(label).bind(stored).bind(category).execute(&mut *tx).await?;if let Some(number)=number{sqlx::query("UPDATE venteapport SET total_retenues=(SELECT ROUND(COALESCE(SUM(ABS(montant)),0),2) FROM valorisationapport WHERE num_apport=? AND lower(COALESCE(categorie,''))='retenue') WHERE num_apport=?").bind(&number).bind(&number).execute(&mut *tx).await?;}tx.commit().await?;Ok(Redirect::to("/economique").into_response())}

macro_rules! delete_handler{($name:ident,$table:literal)=>{async fn $name(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query(concat!("DELETE FROM ",$table," WHERE id=?")).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/economique").into_response())}}}
delete_handler!(economique_aliment_supprimer,"livraisonaliment");delete_handler!(economique_veto_supprimer,"achatveto");delete_handler!(economique_vente_supprimer,"venteapport");delete_handler!(economique_semence_supprimer,"achatsemence");delete_handler!(economique_genetique_supprimer,"achatgenetique");

async fn economique_valorisation_supprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let number:Option<String>=sqlx::query_scalar("SELECT num_apport FROM valorisationapport WHERE id=?").bind(id).fetch_optional(&state.pool).await?.flatten();let mut tx=state.pool.begin().await?;sqlx::query("DELETE FROM valorisationapport WHERE id=?").bind(id).execute(&mut *tx).await?;if let Some(number)=number{sqlx::query("UPDATE venteapport SET total_retenues=(SELECT ROUND(COALESCE(SUM(ABS(montant)),0),2) FROM valorisationapport WHERE num_apport=? AND lower(COALESCE(categorie,''))='retenue') WHERE num_apport=?").bind(&number).bind(&number).execute(&mut *tx).await?;}tx.commit().await?;Ok(Redirect::to("/economique").into_response())}

async fn vente_directe(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{
    require_writer(&session)?;
    let products=generic_rows(&state.pool,"SELECT id,nom,prix,unite,actif,ordre,quantite_disponible FROM produitventedirecte ORDER BY ordre,nom").await?;
    let orders=generic_rows(&state.pool,"SELECT c.id,c.cree_le,c.nom_client,c.telephone,c.email,c.notes,c.statut,c.total,c.session_vente_id,s.nom AS session_nom,(SELECT GROUP_CONCAT(l.nom_produit||' × '||l.quantite,', ') FROM lignecommandeventedirecte l WHERE l.commande_id=c.id) AS lignes FROM commandeventedirecte c LEFT JOIN sessionventedirecte s ON s.id=c.session_vente_id ORDER BY c.cree_le DESC,c.id DESC LIMIT 500").await?;
    let sessions=generic_rows(&state.pool,"SELECT id,nom,date_livraison,active FROM sessionventedirecte ORDER BY active DESC,id DESC").await?;
    let settings=generic_rows(&state.pool,"SELECT date_livraison,texte_livraison FROM reglageventedirecte WHERE id=1").await?.into_iter().next().unwrap_or_else(||json!({"date_livraison":null,"texte_livraison":null}));
    let mut ctx=context(&session);
    ctx.insert("produits".into(),Value::Array(products));
    ctx.insert("commandes".into(),Value::Array(orders));
    ctx.insert("sessions_vente".into(),Value::Array(sessions));
    ctx.insert("reglage".into(),settings);
    render(&state,"vente_directe.html",Value::Object(ctx))
}
async fn produit_ajouter(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let name=form_text(&form,"nom").ok_or_else(||AppError::Invalid("Nom obligatoire".into()))?;let price=form_f64(&form,"prix").ok_or_else(||AppError::Invalid("Prix invalide".into()))?;let order:i64=sqlx::query_scalar("SELECT COALESCE(MAX(ordre),0)+1 FROM produitventedirecte").fetch_one(&state.pool).await?;sqlx::query("INSERT INTO produitventedirecte(nom,prix,unite,actif,ordre,quantite_disponible) VALUES(?,?,?,1,?,?)").bind(name).bind(price).bind(form_text(&form,"unite").unwrap_or_else(||"kg".into())).bind(order).bind(form_f64(&form,"quantite_disponible")).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe").into_response())}
async fn produit_modifier(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let name=form_text(&form,"nom").ok_or_else(||AppError::Invalid("Nom obligatoire".into()))?;let price=form_f64(&form,"prix").filter(|value|*value>=0.0).ok_or_else(||AppError::Invalid("Prix invalide".into()))?;let unit=if form.get("unite").map(String::as_str)==Some("pièce"){"pièce"}else{"kg"};sqlx::query("UPDATE produitventedirecte SET nom=?,prix=?,unite=?,actif=? WHERE id=?").bind(name).bind(price).bind(unit).bind(form.contains_key("actif")).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe#produits").into_response())}

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
            stock.map(|value| value.to_string()).unwrap_or_else(|| "illimité".into())
        ),
        "/vente-directe",
    )
    .await;
    Ok(Redirect::to("/vente-directe#produits").into_response())
}

async fn produit_deplacer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let current:Option<i64>=sqlx::query_scalar("SELECT ordre FROM produitventedirecte WHERE id=?").bind(id).fetch_optional(&state.pool).await?;if let Some(current)=current{let direction=form.get("direction").map(String::as_str).unwrap_or("");let other:Option<(i64,i64)>=match direction{"haut"=>sqlx::query_as("SELECT id,ordre FROM produitventedirecte WHERE ordre<? ORDER BY ordre DESC LIMIT 1").bind(current).fetch_optional(&state.pool).await?,"bas"=>sqlx::query_as("SELECT id,ordre FROM produitventedirecte WHERE ordre>? ORDER BY ordre LIMIT 1").bind(current).fetch_optional(&state.pool).await?,_=>None};if let Some((other_id,other_order))=other{let mut tx=state.pool.begin().await?;sqlx::query("UPDATE produitventedirecte SET ordre=? WHERE id=?").bind(other_order).bind(id).execute(&mut *tx).await?;sqlx::query("UPDATE produitventedirecte SET ordre=? WHERE id=?").bind(current).bind(other_id).execute(&mut *tx).await?;tx.commit().await?;}}Ok(Redirect::to("/vente-directe#produits").into_response())}

async fn vente_reglage_livraison(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison,texte_livraison) VALUES(1,?,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison,texte_livraison=excluded.texte_livraison").bind(form_text(&form,"date_livraison")).bind(form_text(&form,"texte_livraison")).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe").into_response())}

async fn commande_statut(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let new_status=form_text(&form,"statut").unwrap_or_else(||"nouvelle".into());if !matches!(new_status.as_str(),"nouvelle"|"validee"|"preparation"|"prete"|"livree"|"annulee"){return Err(AppError::Invalid("Statut invalide".into()))}let mut tx=state.pool.begin().await?;let old:Option<String>=sqlx::query_scalar("SELECT statut FROM commandeventedirecte WHERE id=?").bind(id).fetch_optional(&mut *tx).await?;let Some(old)=old else{return Err(AppError::NotFound)};let lines=sqlx::query_as::<_,(Option<i64>,f64)>("SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?").bind(id).fetch_all(&mut *tx).await?;if new_status=="annulee"&&old!="annulee"{for(product_id,quantity)in &lines{if let Some(product_id)=product_id{sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;}}}else if old=="annulee"&&new_status!="annulee"{for(product_id,quantity)in &lines{if let Some(product_id)=product_id{let stock:Option<f64>=sqlx::query_scalar("SELECT quantite_disponible FROM produitventedirecte WHERE id=?").bind(product_id).fetch_optional(&mut *tx).await?.flatten();if stock.is_some_and(|value|value<*quantity){return Err(AppError::Invalid("Stock insuffisant pour réactiver la commande".into()))}}}for(product_id,quantity)in &lines{if let Some(product_id)=product_id{sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;}}}sqlx::query("UPDATE commandeventedirecte SET statut=? WHERE id=?").bind(new_status).bind(id).execute(&mut *tx).await?;tx.commit().await?;Ok(Redirect::to("/vente-directe").into_response())}

async fn commande_supprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let mut tx=state.pool.begin().await?;let status:Option<String>=sqlx::query_scalar("SELECT statut FROM commandeventedirecte WHERE id=?").bind(id).fetch_optional(&mut *tx).await?;if status.as_deref().is_some_and(|value|value!="annulee"){let lines=sqlx::query_as::<_,(Option<i64>,f64)>("SELECT produit_id,quantite FROM lignecommandeventedirecte WHERE commande_id=?").bind(id).fetch_all(&mut *tx).await?;for(product_id,quantity)in lines{if let Some(product_id)=product_id{sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible+? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product_id).execute(&mut *tx).await?;}}}sqlx::query("DELETE FROM commandeventedirecte WHERE id=?").bind(id).execute(&mut *tx).await?;tx.commit().await?;Ok(Redirect::to("/vente-directe").into_response())}

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
    if !matches!(status.as_str(), "nouvelle" | "validee" | "preparation" | "prete" | "livree" | "annulee") {
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
        "SELECT id,nom,prix,unite,actif,ordre,quantite_disponible FROM produitventedirecte ORDER BY ordre,nom",
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
        if status != "annulee" && product.quantite_disponible.is_some_and(|stock| quantity > stock) {
            return Err(AppError::Invalid(format!("Stock insuffisant pour {}", product.nom)));
        }
        let line_total = (quantity * product.prix * 100.0).round() / 100.0;
        total += line_total;
        lines.push((product, quantity, line_total));
    }
    if lines.is_empty() {
        return Err(AppError::Invalid("La commande doit contenir au moins un produit".into()));
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
    db::journal(&state.pool,&session.nom,"modifier","commande_vente_directe",&format!("commande {id}"),&format!("/vente-directe/commande/{id}")).await;
    Ok(Redirect::to("/vente-directe").into_response())
}

async fn vente_commande_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let order = generic_rows(&state.pool,&format!("SELECT c.id,c.cree_le,c.nom_client,c.telephone,c.email,c.notes,c.statut,c.total,s.nom AS session_nom,s.date_livraison FROM commandeventedirecte c LEFT JOIN sessionventedirecte s ON s.id=c.session_vente_id WHERE c.id={id}")).await?;
    let Some(order)=order.into_iter().next() else{return Err(AppError::NotFound)};
    let lines=generic_rows(&state.pool,&format!("SELECT nom_produit,prix_unitaire,unite,quantite,total_ligne FROM lignecommandeventedirecte WHERE commande_id={id} ORDER BY id")).await?;
    let mut ctx=context(&session);ctx.insert("commande".into(),order);ctx.insert("lignes".into(),Value::Array(lines));render(&state,"vente_commande_impression.html",Value::Object(ctx))
}

async fn vente_preparation_imprimer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    require_writer(&session)?;
    let session_id = query.get("session_id").and_then(|value| value.parse::<i64>().ok());
    let session_id = match session_id {
        Some(value) => Some(value),
        None => sqlx::query_scalar("SELECT id FROM sessionventedirecte WHERE active=1 ORDER BY id DESC LIMIT 1").fetch_optional(&state.pool).await?,
    };
    let Some(session_id)=session_id else{return Err(AppError::Invalid("Aucune session de vente active".into()))};
    let sale_session=generic_rows(&state.pool,&format!("SELECT id,nom,date_livraison,nb_porcs,bande_reference FROM sessionventedirecte WHERE id={session_id}")).await?;
    let Some(sale_session)=sale_session.into_iter().next() else{return Err(AppError::NotFound)};
    let products=generic_rows(&state.pool,&format!("SELECT l.nom_produit,l.unite,ROUND(SUM(l.quantite),2) AS quantite,COUNT(DISTINCT c.id) AS commandes FROM lignecommandeventedirecte l JOIN commandeventedirecte c ON c.id=l.commande_id WHERE c.session_vente_id={session_id} AND c.statut<>'annulee' GROUP BY l.nom_produit,l.unite ORDER BY l.nom_produit")).await?;
    let orders=generic_rows(&state.pool,&format!("SELECT c.id,c.nom_client,c.telephone,c.notes,c.statut,c.total,(SELECT GROUP_CONCAT(l.nom_produit||' × '||l.quantite,', ') FROM lignecommandeventedirecte l WHERE l.commande_id=c.id) AS lignes FROM commandeventedirecte c WHERE c.session_vente_id={session_id} AND c.statut<>'annulee' ORDER BY c.nom_client,c.id")).await?;
    let mut ctx=context(&session);ctx.insert("session_vente".into(),sale_session);ctx.insert("produits".into(),Value::Array(products));ctx.insert("commandes".into(),Value::Array(orders));render(&state,"vente_preparation.html",Value::Object(ctx))
}

async fn commande_page(State(state):State<AppState>,Query(query):Query<HashMap<String,String>>)->AppResult<Html<String>>{
    let products=sqlx::query_as::<_,ProduitVenteDirecte>("SELECT id,nom,prix,unite,actif,ordre,quantite_disponible FROM produitventedirecte WHERE actif=1 AND (quantite_disponible IS NULL OR quantite_disponible>0) ORDER BY ordre,nom").fetch_all(&state.pool).await?;
    let settings=generic_rows(&state.pool,"SELECT date_livraison,texte_livraison FROM reglageventedirecte WHERE id=1").await?.into_iter().next().unwrap_or_else(||json!({"date_livraison":null,"texte_livraison":null}));
    let active=generic_rows(&state.pool,"SELECT id,nom,date_livraison FROM sessionventedirecte WHERE active=1 ORDER BY id DESC LIMIT 1").await?.into_iter().next().unwrap_or(Value::Null);
    render(&state,"commande.html",json!({"produits":products,"reglage":settings,"session_active":active,"ok":query.contains_key("ok"),"error":query.get("err").cloned().unwrap_or_default()}))
}

async fn commande_post(State(state):State<AppState>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{
    if form_text(&form,"website").is_some(){return Ok(Redirect::to("/commande?ok=1").into_response())}
    let name=form_text(&form,"nom_client").filter(|value|value.len()<=160).ok_or_else(||AppError::Invalid("Nom obligatoire".into()))?;
    let phone=form_text(&form,"telephone").filter(|value|value.len()<=40).ok_or_else(||AppError::Invalid("Téléphone obligatoire".into()))?;
    let mut tx=state.pool.begin().await?;
    let products=sqlx::query_as::<_,ProduitVenteDirecte>("SELECT id,nom,prix,unite,actif,ordre,quantite_disponible FROM produitventedirecte WHERE actif=1 ORDER BY ordre,nom").fetch_all(&mut *tx).await?;
    let mut lines=Vec::new();let mut total=0.0;
    for product in products{let quantity=form_f64(&form,&format!("q_{}",product.id)).unwrap_or(0.0);if quantity<=0.0{continue}if quantity>10_000.0{return Err(AppError::Invalid("Quantité invalide".into()))}if product.quantite_disponible.is_some_and(|stock|quantity>stock){return Ok(Redirect::to("/commande?err=stock-insuffisant").into_response())}let line_total=(quantity*product.prix*100.0).round()/100.0;total+=line_total;lines.push((product,quantity,line_total))}
    if lines.is_empty(){return Ok(Redirect::to("/commande?err=commande-vide").into_response())}
    let session_id:Option<i64>=sqlx::query_scalar("SELECT id FROM sessionventedirecte WHERE active=1 ORDER BY date_creation DESC,id DESC LIMIT 1").fetch_optional(&mut *tx).await?;
    let token=uuid::Uuid::new_v4().simple().to_string();let email=form_text(&form,"email");
    let client_id=if let Some(id)=sqlx::query_scalar::<_,i64>("SELECT id FROM clientventedirecte WHERE (? IS NOT NULL AND email=?) OR telephone=? LIMIT 1").bind(&email).bind(&email).bind(&phone).fetch_optional(&mut *tx).await?{sqlx::query("UPDATE clientventedirecte SET nom=?,email=?,telephone=? WHERE id=?").bind(&name).bind(&email).bind(&phone).bind(id).execute(&mut *tx).await?;id}else{sqlx::query("INSERT INTO clientventedirecte(nom,email,telephone,newsletter_email,newsletter_sms,cree_le,token_desinscription) VALUES(?,?,?,0,0,CURRENT_TIMESTAMP,?)").bind(&name).bind(&email).bind(&phone).bind(token).execute(&mut *tx).await?.last_insert_rowid()};
    let order_id=sqlx::query("INSERT INTO commandeventedirecte(client_id,session_vente_id,nom_client,telephone,email,notes,statut,total,cree_le) VALUES(?,?,?,?,?,?,'nouvelle',?,CURRENT_TIMESTAMP)").bind(client_id).bind(session_id).bind(&name).bind(&phone).bind(&email).bind(form_text(&form,"notes")).bind(total).execute(&mut *tx).await?.last_insert_rowid();
    for(product,quantity,line_total)in lines{sqlx::query("INSERT INTO lignecommandeventedirecte(commande_id,produit_id,nom_produit,prix_unitaire,unite,quantite,total_ligne) VALUES(?,?,?,?,?,?,?)").bind(order_id).bind(product.id).bind(&product.nom).bind(product.prix).bind(&product.unite).bind(quantity).bind(line_total).execute(&mut *tx).await?;sqlx::query("UPDATE produitventedirecte SET quantite_disponible=quantite_disponible-? WHERE id=? AND quantite_disponible IS NOT NULL").bind(quantity).bind(product.id).execute(&mut *tx).await?;}
    tx.commit().await?;Ok(Redirect::to("/commande?ok=1").into_response())
}

async fn utilisateurs(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{if!session.est_admin(){return Err(AppError::Forbidden)}let users=sqlx::query_as::<_,Utilisateur>("SELECT id,identifiant,nom,prenom,hash_mdp,role,actif,sections,doit_changer_mdp,tentatives_echec,bloque_jusqu FROM utilisateur ORDER BY identifiant").fetch_all(&state.pool).await?;let mut ctx=context(&session);ctx.insert("utilisateurs".into(),serde_json::to_value(users).unwrap_or_default());render(&state,"utilisateurs.html",Value::Object(ctx))}
async fn utilisateur_creer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{if!session.est_admin(){return Err(AppError::Forbidden)}verify_csrf(&session,&form)?;let id=form_text(&form,"identifiant").ok_or_else(||AppError::Invalid("Identifiant obligatoire".into()))?;let password=form.get("mdp").cloned().unwrap_or_default();if password.len()<8{return Err(AppError::Invalid("Mot de passe: 8 caractères minimum".into()))}let role=form.get("role").map(String::as_str).unwrap_or("salarie");if!matches!(role,"admin"|"eleveur"|"salarie"|"engraisseur"){return Err(AppError::Invalid("Rôle invalide".into()))}sqlx::query("INSERT INTO utilisateur(identifiant,nom,prenom,hash_mdp,role,actif,doit_changer_mdp) VALUES(?,?,?,?,?,1,1)").bind(id).bind(form_text(&form,"nom")).bind(form_text(&form,"prenom")).bind(auth::hash_password(&password)).bind(role).execute(&state.pool).await?;Ok(Redirect::to("/utilisateurs").into_response())}
async fn utilisateur_actif(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{if!session.est_admin(){return Err(AppError::Forbidden)}verify_csrf(&session,&form)?;sqlx::query("UPDATE utilisateur SET actif=CASE actif WHEN 1 THEN 0 ELSE 1 END WHERE id=? AND identifiant<>'admin'").bind(id).execute(&state.pool).await?;Ok(Redirect::to("/utilisateurs").into_response())}

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
    sqlx::query("UPDATE utilisateur SET hash_mdp=?,doit_changer_mdp=1,tentatives_echec=0,bloque_jusqu=NULL WHERE id=?")
        .bind(auth::hash_password(&password))
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/utilisateurs").into_response())
}

async fn sauvegarde(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Response>{if!session.est_admin(){return Err(AppError::Forbidden)}sqlx::query("PRAGMA wal_checkpoint(FULL)").execute(&state.pool).await?;let bytes=tokio::fs::read(&state.config.db_path).await.map_err(anyhow::Error::from)?;let filename=format!("elevage_sauvegarde_{}.db",Local::now().date_naive());let mut headers=HeaderMap::new();headers.insert(header::CONTENT_TYPE,HeaderValue::from_static("application/x-sqlite3"));headers.insert(header::CONTENT_DISPOSITION,HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).map_err(|e|AppError::Internal(e.into()))?);Ok((headers,bytes).into_response())}

async fn structure(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let sites = generic_rows(&state.pool, "SELECT id,code,nom FROM site ORDER BY COALESCE(nom,code)").await?;
    let rooms = generic_rows(&state.pool, "SELECT s.id,s.site_id,s.nom,s.type,s.rfid,s.nb_cases,s.ordre,COALESCE(si.nom,si.code) AS site FROM salle s JOIN site si ON si.id=s.site_id ORDER BY COALESCE(si.nom,si.code),s.ordre,s.nom").await?;
    let cases = generic_rows(&state.pool, "SELECT c.id,c.salle_id,c.nom,c.rfid,c.nb_max_porcs,c.num_vanne,s.nom AS salle,COALESCE(si.nom,si.code) AS site FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY COALESCE(si.nom,si.code),s.ordre,c.nom").await?;
    let mut ctx = context(&session);
    ctx.insert("sites".into(), Value::Array(sites));
    ctx.insert("salles".into(), Value::Array(rooms));
    ctx.insert("cases".into(), Value::Array(cases));
    render(&state, "structure.html", Value::Object(ctx))
}
async fn structure_site(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let code=form_text(&form,"code").ok_or_else(||AppError::Invalid("Code obligatoire".into()))?;sqlx::query("INSERT INTO site(code,nom) VALUES(?,?)").bind(code).bind(form_text(&form,"nom")).execute(&state.pool).await?;Ok(Redirect::to("/structure").into_response())}
async fn structure_salle(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("INSERT INTO salle(site_id,nom,type,rfid,nb_cases,ordre) VALUES(?,?,?,?,0,COALESCE((SELECT MAX(ordre)+1 FROM salle WHERE site_id=?),0))").bind(form_i64(&form,"site_id")).bind(form_text(&form,"nom")).bind(form_text(&form,"type")).bind(form_text(&form,"rfid")).bind(form_i64(&form,"site_id")).execute(&state.pool).await?;Ok(Redirect::to("/structure").into_response())}
async fn structure_case(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("INSERT INTO casesalle(salle_id,nom,rfid,nb_max_porcs,num_vanne) VALUES(?,?,?,?,?)").bind(form_i64(&form,"salle_id")).bind(form_text(&form,"nom")).bind(form_text(&form,"rfid")).bind(form_i64(&form,"nb_max_porcs")).bind(form_text(&form,"num_vanne")).execute(&state.pool).await?;Ok(Redirect::to("/structure").into_response())}

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
    sqlx::query("UPDATE casesalle SET rfid=?,num_vanne=?,nb_max_porcs=? WHERE id=?")
        .bind(form_text(&form, "rfid"))
        .bind(form_text(&form, "num_vanne"))
        .bind(form_i64(&form, "nb_max_porcs"))
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
    let used: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM transfert WHERE case_source_id=? OR case_dest_id=?)+(SELECT COUNT(*) FROM declarationmort WHERE case_id=?)+(SELECT COUNT(*) FROM truie WHERE case_id=?)")
        .bind(id).bind(id).bind(id).bind(id).fetch_one(&state.pool).await?;
    if used > 0 {
        return Err(AppError::Invalid("Cette case contient un historique ou des animaux et ne peut pas être supprimée".into()));
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
    let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM casesalle WHERE salle_id=?")
        .bind(id).fetch_one(&state.pool).await?;
    if children > 0 {
        return Err(AppError::Invalid("Supprime ou déplace d'abord les cases de cette salle".into()));
    }
    sqlx::query("DELETE FROM salle WHERE id=?")
        .bind(id).execute(&state.pool).await?;
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
    let children: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM salle WHERE site_id=?")
        .bind(id).fetch_one(&state.pool).await?;
    if children > 0 {
        return Err(AppError::Invalid("Supprime d'abord les salles de ce site".into()));
    }
    sqlx::query("DELETE FROM site WHERE id=?")
        .bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/structure").into_response())
}

async fn taches(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{list_page(&state,&session,"Tâches et réparations","Échéances et suivi","SELECT id,titre,type,bande_code,salle,echeance,fait,note,cree_le FROM tache ORDER BY fait,echeance,cree_le DESC",&["id","titre","type","bande_code","salle","echeance","fait","note"]).await}
async fn tache_ajouter(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{verify_csrf(&session,&form)?;sqlx::query("INSERT INTO tache(titre,type,bande_code,salle,echeance,note,fait,cree_le) VALUES(?,?,?,?,?,?,0,CURRENT_TIMESTAMP)").bind(form_text(&form,"titre")).bind(form_text(&form,"type")).bind(form_text(&form,"bande_code")).bind(form_text(&form,"salle")).bind(form_text(&form,"echeance")).bind(form_text(&form,"note")).execute(&state.pool).await?;Ok(Redirect::to("/taches").into_response())}
async fn tache_fait(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{verify_csrf(&session,&form)?;sqlx::query("UPDATE tache SET fait=CASE fait WHEN 1 THEN 0 ELSE 1 END WHERE id=?").bind(id).execute(&state.pool).await?;Ok(Redirect::to("/taches").into_response())}
async fn tache_supprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{verify_csrf(&session,&form)?;sqlx::query("DELETE FROM tache WHERE id=?").bind(id).execute(&state.pool).await?;Ok(Redirect::to("/taches").into_response())}

async fn sanitaire(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let protocols = generic_rows(&state.pool, "SELECT id,libelle,cible,reference,jour,produit,dose,unite,voie,duree_j,delai_attente,aiguille,preconisations,note FROM acteprotocole WHERE actif=1 ORDER BY cible,jour,id").await?;
    let bands = generic_rows(&state.pool, "SELECT id,code,date_mb FROM bande WHERE active=1 ORDER BY date_mb,code").await?;
    let completed = generic_rows(&state.pool, "SELECT ar.id,ar.date_realise,b.code AS bande,a.libelle,a.produit,ar.note FROM acterealise ar JOIN bande b ON b.id=ar.bande_id JOIN acteprotocole a ON a.id=ar.acte_id ORDER BY ar.date_realise DESC,ar.id DESC LIMIT 250").await?;
    let mut ctx = context(&session);
    ctx.insert("protocoles".into(), Value::Array(protocols));
    ctx.insert("bandes".into(), Value::Array(bands));
    ctx.insert("realises".into(), Value::Array(completed));
    ctx.insert("today".into(), json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state, "sanitaire.html", Value::Object(ctx))
}

async fn sanitaire_acte_ajouter(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let label = form_text(&form, "libelle").ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    let target = form_text(&form, "cible").ok_or_else(|| AppError::Invalid("Cible obligatoire".into()))?;
    let reference = form_text(&form, "reference").unwrap_or_else(|| "mise_bas".into());
    let day = form_i64(&form, "jour").unwrap_or(0);
    sqlx::query("INSERT INTO acteprotocole(libelle,cible,reference,jour,produit,dose,unite,voie,duree_j,delai_attente,aiguille,preconisations,note,actif) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,1)")
        .bind(label).bind(target).bind(reference).bind(day)
        .bind(form_text(&form,"produit")).bind(form_text(&form,"dose")).bind(form_text(&form,"unite"))
        .bind(form_text(&form,"voie")).bind(form_i64(&form,"duree_j")).bind(form_i64(&form,"delai_attente"))
        .bind(form_text(&form,"aiguille")).bind(form_text(&form,"preconisations")).bind(form_text(&form,"note"))
        .execute(&state.pool).await?;
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
    let label = form_text(&form, "libelle").ok_or_else(|| AppError::Invalid("Libellé obligatoire".into()))?;
    sqlx::query("UPDATE acteprotocole SET libelle=?,cible=?,reference=?,jour=?,produit=?,dose=?,unite=?,voie=?,duree_j=?,delai_attente=?,aiguille=?,preconisations=?,note=? WHERE id=?")
        .bind(label).bind(form_text(&form,"cible")).bind(form_text(&form,"reference")).bind(form_i64(&form,"jour").unwrap_or(0))
        .bind(form_text(&form,"produit")).bind(form_text(&form,"dose")).bind(form_text(&form,"unite")).bind(form_text(&form,"voie"))
        .bind(form_i64(&form,"duree_j")).bind(form_i64(&form,"delai_attente")).bind(form_text(&form,"aiguille"))
        .bind(form_text(&form,"preconisations")).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn sanitaire_acte_supprimer(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_writer(&session)?; verify_csrf(&session,&form)?;
    let id=form_i64(&form,"id").ok_or_else(||AppError::Invalid("Acte manquant".into()))?;
    sqlx::query("UPDATE acteprotocole SET actif=0 WHERE id=?").bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn sanitaire_fait(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    verify_csrf(&session,&form)?;
    let act=form_i64(&form,"acte_id").ok_or_else(||AppError::Invalid("Acte manquant".into()))?;
    let band=form_i64(&form,"bande_id").ok_or_else(||AppError::Invalid("Bande manquante".into()))?;
    let date=form_text(&form,"date_realise").unwrap_or_else(||Local::now().date_naive().format("%Y-%m-%d").to_string());
    sqlx::query("INSERT INTO acterealise(acte_id,bande_id,date_realise,note) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM acteprotocole WHERE id=? AND actif=1) AND EXISTS(SELECT 1 FROM bande WHERE id=?)")
        .bind(act).bind(band).bind(date).bind(form_text(&form,"note")).bind(act).bind(band).execute(&state.pool).await?;
    Ok(Redirect::to("/sanitaire").into_response())
}

async fn pharmacie(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if !matches!(session.role.as_str(), "admin" | "eleveur") { return Err(AppError::Forbidden); }
    let products=generic_rows(&state.pool,"SELECT id,produit,stock_actuel,unite,seuil_alerte,maj,note,CASE WHEN seuil_alerte IS NOT NULL AND stock_actuel<=seuil_alerte THEN 1 ELSE 0 END AS alerte FROM produitpharmacie ORDER BY alerte DESC,produit").await?;
    let movements=generic_rows(&state.pool,"SELECT id,produit,date,type,quantite,bande_code,note FROM mouvementpharmacie ORDER BY date DESC,id DESC LIMIT 300").await?;
    let bands=generic_rows(&state.pool,"SELECT code FROM bande WHERE active=1 ORDER BY date_mb,code").await?;
    let mut ctx=context(&session);ctx.insert("produits".into(),Value::Array(products));ctx.insert("mouvements".into(),Value::Array(movements));ctx.insert("bandes".into(),Value::Array(bands));ctx.insert("today".into(),json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));
    render(&state,"pharmacie.html",Value::Object(ctx))
}

async fn pharmacie_mouvement(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    if !matches!(session.role.as_str(),"admin"|"eleveur"){return Err(AppError::Forbidden)} verify_csrf(&session,&form)?;
    let product=form_text(&form,"produit").ok_or_else(||AppError::Invalid("Produit obligatoire".into()))?;
    let quantity=form_f64(&form,"quantite").filter(|value|*value>=0.0).ok_or_else(||AppError::Invalid("Quantité invalide".into()))?;
    let kind=form.get("type").map(String::as_str).unwrap_or("sortie");
    if !matches!(kind,"entree"|"sortie"|"inventaire"){return Err(AppError::Invalid("Type de mouvement invalide".into()))}
    let mut tx=state.pool.begin().await?;
    sqlx::query("INSERT INTO produitpharmacie(produit,stock_actuel,unite,maj) SELECT ?,0,?,CURRENT_TIMESTAMP WHERE NOT EXISTS(SELECT 1 FROM produitpharmacie WHERE lower(produit)=lower(?))")
        .bind(&product).bind(form_text(&form,"unite").unwrap_or_else(||"doses".into())).bind(&product).execute(&mut *tx).await?;
    let stock: f64=sqlx::query_scalar("SELECT CAST(COALESCE(stock_actuel,0) AS REAL) FROM produitpharmacie WHERE lower(produit)=lower(?) LIMIT 1").bind(&product).fetch_one(&mut *tx).await?;
    let new_stock=match kind{"entree"=>stock+quantity,"inventaire"=>quantity,_ if quantity<=stock=>stock-quantity,_=>return Err(AppError::Invalid(format!("Stock insuffisant : {stock}")))};
    sqlx::query("UPDATE produitpharmacie SET stock_actuel=?,maj=CURRENT_TIMESTAMP WHERE lower(produit)=lower(?)").bind(new_stock).bind(&product).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mouvementpharmacie(produit,date,type,quantite,note,bande_code) VALUES(?,?,?,?,?,?)").bind(&product).bind(form_text(&form,"date")).bind(kind).bind(quantity).bind(form_text(&form,"note")).bind(form_text(&form,"bande_code")).execute(&mut *tx).await?;
    tx.commit().await?;Ok(Redirect::to("/pharmacie").into_response())
}

async fn pharmacie_regler(
    State(state): State<AppState>, Extension(session): Extension<SessionData>, Form(form): Form<HashMap<String,String>>,
) -> AppResult<Response> {
    if !matches!(session.role.as_str(),"admin"|"eleveur"){return Err(AppError::Forbidden)} verify_csrf(&session,&form)?;
    let id=form_i64(&form,"id").ok_or_else(||AppError::Invalid("Produit manquant".into()))?;
    sqlx::query("UPDATE produitpharmacie SET unite=?,seuil_alerte=?,note=?,maj=CURRENT_TIMESTAMP WHERE id=?").bind(form_text(&form,"unite")).bind(form_f64(&form,"seuil_alerte")).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;
    Ok(Redirect::to("/pharmacie").into_response())
}
async fn planning(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{list_page(&state,&session,"Planning","Événements à venir et récents","SELECT e.id,e.date,e.type,t.num_travail,b.code AS bande,e.produit,e.note FROM evenement e LEFT JOIN truie t ON t.id=e.truie_id LEFT JOIN bande b ON b.id=e.bande_id ORDER BY e.date DESC LIMIT 250",&["date","type","num_travail","bande","produit","note"]).await}
async fn stock(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{list_page(&state,&session,"Stocks et mouvements","Derniers mouvements enregistrés","SELECT id,date,bande_code,nombre,poids,montant,libelle,destination,type_saisie,est_stock FROM mouvementstock ORDER BY date DESC,id DESC LIMIT 500",&["date","bande_code","nombre","poids","montant","libelle","destination","type_saisie","est_stock"]).await}
async fn journal(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{if!session.est_admin(){return Err(AppError::Forbidden)}list_page(&state,&session,"Journal d'activité","Traçabilité des opérations","SELECT id,horodatage,utilisateur,action,objet,detail,chemin FROM journal ORDER BY horodatage DESC,id DESC LIMIT 1000",&["horodatage","utilisateur","action","objet","detail","chemin"]).await}

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
        .bind(form_text(&form, "derniere_date"))
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
    let date = form_text(&form, "derniere_date")
        .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
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
            "SELECT id,code,instructions,poids_cible FROM bande WHERE active=1 AND engraisseur_id={} ORDER BY date_mb,code",
            session.uid
        )
    } else {
        "SELECT id,code,instructions,poids_cible FROM bande WHERE active=1 ORDER BY date_mb,code"
            .to_string()
    };
    let bands = generic_rows(&state.pool, &band_sql).await?;
    let cases = generic_rows(
        &state.pool,
        "SELECT c.id,COALESCE(si.nom,si.code)||' · '||s.nom||' · '||c.nom AS nom FROM casesalle c JOIN salle s ON s.id=c.salle_id JOIN site si ON si.id=s.site_id ORDER BY si.nom,s.ordre,c.nom",
    )
    .await?;
    let mut ctx = context(&session);
    ctx.insert("declarations".into(), Value::Array(declarations));
    ctx.insert("bandes".into(), Value::Array(bands));
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
    if let Some(case_id) = case_id {
        let present = case_pig_count(&state.pool, case_id).await?;
        if number > present {
            return Err(AppError::Invalid(format!(
                "Effectif insuffisant dans la case : {present} porc(s) présent(s)"
            )));
        }
    }
    sqlx::query("INSERT INTO declarationmort(bande_code,date,stade,case_id,cause,poids,nombre,declare_par,note) VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(&band)
        .bind(form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string()))
        .bind(form_text(&form, "stade"))
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
        "SELECT v.id,v.date,v.num_apport,b.code AS bande,v.nb_porcs,v.poids_total,v.poids_moyen,v.tmp,v.tx_qualification,v.plus_value,v.montant_net FROM venteapport v LEFT JOIN bande b ON b.id=v.bande_id ORDER BY v.date DESC,v.id DESC LIMIT 250",
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
    let total_abattus: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER) FROM venteapport",
    )
    .fetch_one(&state.pool)
    .await?;
    let net_total: f64 =
        sqlx::query_scalar("SELECT CAST(COALESCE(SUM(montant_net),0) AS REAL) FROM venteapport")
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
    let prix_net_kg: Option<f64> = sqlx::query_scalar(
        "SELECT SUM(COALESCE(montant_net,0))/NULLIF(SUM(COALESCE(poids_total,0)),0) FROM venteapport",
    )
    .fetch_one(&state.pool)
    .await?;
    let total_saisies: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nombre),0) AS INTEGER) FROM saisieabattoir",
    )
    .fetch_one(&state.pool)
    .await?;
    let synthesis = generic_rows(
        &state.pool,
        "WITH v AS (SELECT bande_id,SUM(COALESCE(nb_porcs,0)) AS porcs,SUM(COALESCE(poids_total,0)) AS poids,SUM(COALESCE(montant_net,0)) AS net,SUM(COALESCE(tmp,0)*COALESCE(nb_porcs,0))/NULLIF(SUM(COALESCE(nb_porcs,0)),0) AS tmp FROM venteapport GROUP BY bande_id),s AS (SELECT bande_code,SUM(COALESCE(nombre,0)) AS saisies FROM saisieabattoir GROUP BY bande_code) SELECT b.id,b.code,CAST(COALESCE(v.porcs,0) AS INTEGER) AS porcs,ROUND(v.poids/NULLIF(v.porcs,0),1) AS poids_moyen,ROUND(v.tmp,2) AS tmp,ROUND(v.net/NULLIF(v.poids,0),3) AS prix_net_kg,ROUND(v.net,2) AS net,CAST(COALESCE(s.saisies,0) AS INTEGER) AS saisies FROM bande b LEFT JOIN v ON v.bande_id=b.id LEFT JOIN s ON s.bande_code=b.code WHERE v.porcs IS NOT NULL OR s.saisies IS NOT NULL ORDER BY b.date_mb DESC,b.id DESC",
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
        json!({"total_abattus":total_abattus,"net_total":net_total,"poids_moy":poids_moy,"tmp_moy":tmp_moy,"prix_net_kg":prix_net_kg,"total_saisies":total_saisies}),
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
        .bind(form_text(&form, "date").unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string()))
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

async fn cahiers(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    if session.role == "salarie" {
        return Err(AppError::Forbidden);
    }
    let cahiers = generic_rows(
        &state.pool,
        "SELECT id,nom,valeur_par_porc,actif,note FROM cahiercharges ORDER BY actif DESC,nom",
    )
    .await?;
    let reels = generic_rows(
        &state.pool,
        "SELECT libelle,ROUND(SUM(montant),2) AS montant,COUNT(DISTINCT num_apport) AS apports FROM valorisationapport WHERE COALESCE(categorie,'valorisation')<>'retenue' GROUP BY libelle ORDER BY montant DESC",
    )
    .await?;
    let total_porcs: i64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(nb_porcs),0) AS INTEGER) FROM venteapport",
    )
    .fetch_one(&state.pool)
    .await?;
    let mut ctx = context(&session);
    ctx.insert("cahiers".into(), Value::Array(cahiers));
    ctx.insert("reels".into(), Value::Array(reels));
    ctx.insert("total_porcs".into(), json!(total_porcs));
    render(&state, "cahiers.html", Value::Object(ctx))
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
    Ok(Redirect::to("/cahiers").into_response())
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
    Ok(Redirect::to("/cahiers").into_response())
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
    Ok(Redirect::to("/cahiers").into_response())
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
    let rows = sqlx::query("SELECT id,date,horodatage,categorie,salle_nom,element,statut,note,utilisateur FROM controlequotidien WHERE date=? ORDER BY horodatage DESC,id DESC")
        .bind(&jour)
        .fetch_all(&state.pool)
        .await?;
    let mut ctx = context(&session);
    ctx.insert("jour".into(), json!(jour));
    ctx.insert("controles".into(), Value::Array(rows_to_json(rows)?));
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
    let sessions=generic_rows(&state.pool,"SELECT s.id,s.nom,s.date_creation,s.date_livraison,s.nb_porcs,s.bande_reference,s.active,s.notes,COUNT(DISTINCT c.id) AS commandes,ROUND(COALESCE(SUM(CASE WHEN c.statut<>'annulee' THEN c.total ELSE 0 END),0),2) AS chiffre_affaires,ROUND(COALESCE((SELECT SUM(ch.montant) FROM chargeventedirecte ch WHERE ch.session_vente_id=s.id),0),2) AS charges,ROUND(COALESCE(ce.semence,0)+COALESCE(ce.gestation,0)+COALESCE(ce.maternite,0)+COALESCE(ce.post_sevrage,0)+COALESCE(ce.engraissement,0)+COALESCE(ce.veto_autres,0),2) AS cout_elevage,ROUND(COALESCE(SUM(CASE WHEN c.statut<>'annulee' THEN c.total ELSE 0 END),0)-COALESCE((SELECT SUM(ch.montant) FROM chargeventedirecte ch WHERE ch.session_vente_id=s.id),0)-COALESCE(ce.semence,0)-COALESCE(ce.gestation,0)-COALESCE(ce.maternite,0)-COALESCE(ce.post_sevrage,0)-COALESCE(ce.engraissement,0)-COALESCE(ce.veto_autres,0),2) AS marge,COALESCE(ce.semence,0) AS semence,COALESCE(ce.gestation,0) AS gestation,COALESCE(ce.maternite,0) AS maternite,COALESCE(ce.post_sevrage,0) AS post_sevrage,COALESCE(ce.engraissement,0) AS engraissement,COALESCE(ce.veto_autres,0) AS veto_autres FROM sessionventedirecte s LEFT JOIN commandeventedirecte c ON c.session_vente_id=s.id LEFT JOIN coutelevageventedirecte ce ON ce.session_vente_id=s.id GROUP BY s.id ORDER BY s.active DESC,s.date_livraison DESC,s.id DESC").await?;
    let charges=generic_rows(&state.pool,"SELECT id,session_vente_id,categorie,libelle,montant,note FROM chargeventedirecte ORDER BY id DESC").await?;
    let bands=generic_rows(&state.pool,"SELECT code,date_mb FROM bande ORDER BY active DESC,date_mb DESC,code").await?;
    let mut ctx=context(&session);ctx.insert("sessions_vente".into(),Value::Array(sessions));ctx.insert("charges".into(),Value::Array(charges));ctx.insert("bandes".into(),Value::Array(bands));ctx.insert("today".into(),json!(Local::now().date_naive().format("%Y-%m-%d").to_string()));render(&state,"vente_sessions.html",Value::Object(ctx))
}

async fn vente_session_creer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let name=form_text(&form,"nom").ok_or_else(||AppError::Invalid("Nom de session obligatoire".into()))?.chars().take(160).collect::<String>();let mut tx=state.pool.begin().await?;sqlx::query("UPDATE sessionventedirecte SET active=0 WHERE active=1").execute(&mut *tx).await?;let id=sqlx::query("INSERT INTO sessionventedirecte(nom,date_creation,date_livraison,nb_porcs,bande_reference,active,notes) VALUES(?,date('now'),?,?,?,1,?)").bind(name).bind(form_text(&form,"date_livraison")).bind(form_i64(&form,"nb_porcs").unwrap_or(0).max(0)).bind(form_text(&form,"bande_reference")).bind(form_text(&form,"notes")).execute(&mut *tx).await?.last_insert_rowid();sqlx::query("INSERT OR IGNORE INTO coutelevageventedirecte(session_vente_id) VALUES(?)").bind(id).execute(&mut *tx).await?;sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison").bind(form_text(&form,"date_livraison")).execute(&mut *tx).await?;tx.commit().await?;Ok(Redirect::to("/vente-directe/sessions").into_response())}

async fn vente_session_activer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let row:Option<(Option<String>,)>=sqlx::query_as("SELECT date_livraison FROM sessionventedirecte WHERE id=?").bind(id).fetch_optional(&state.pool).await?;let Some((date,))=row else{return Err(AppError::NotFound)};let mut tx=state.pool.begin().await?;sqlx::query("UPDATE sessionventedirecte SET active=CASE WHEN id=? THEN 1 ELSE 0 END").bind(id).execute(&mut *tx).await?;sqlx::query("INSERT INTO reglageventedirecte(id,date_livraison) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET date_livraison=excluded.date_livraison").bind(date).execute(&mut *tx).await?;tx.commit().await?;Ok(Redirect::to("/vente-directe/sessions").into_response())}

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
    let active: Option<bool> = sqlx::query_scalar("SELECT active FROM sessionventedirecte WHERE id=?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(active) = active else {
        return Err(AppError::NotFound);
    };
    let delivery_date = form_text(&form, "date_livraison");
    sqlx::query("UPDATE sessionventedirecte SET nom=?,date_livraison=?,nb_porcs=?,bande_reference=?,notes=? WHERE id=?")
        .bind(name)
        .bind(&delivery_date)
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

async fn vente_session_couts(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("INSERT INTO coutelevageventedirecte(session_vente_id,semence,gestation,maternite,post_sevrage,engraissement,veto_autres) VALUES(?,?,?,?,?,?,?) ON CONFLICT(session_vente_id) DO UPDATE SET semence=excluded.semence,gestation=excluded.gestation,maternite=excluded.maternite,post_sevrage=excluded.post_sevrage,engraissement=excluded.engraissement,veto_autres=excluded.veto_autres").bind(id).bind(form_f64(&form,"semence").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"gestation").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"maternite").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"post_sevrage").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"engraissement").unwrap_or(0.0).max(0.0)).bind(form_f64(&form,"veto_autres").unwrap_or(0.0).max(0.0)).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe/sessions").into_response())}

async fn vente_session_charge_ajouter(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let label=form_text(&form,"libelle").ok_or_else(||AppError::Invalid("Libellé obligatoire".into()))?;let amount=form_f64(&form,"montant").filter(|value|*value>=0.0).ok_or_else(||AppError::Invalid("Montant invalide".into()))?;sqlx::query("INSERT INTO chargeventedirecte(session_vente_id,categorie,libelle,montant,note) SELECT ?,?,?,?,? WHERE EXISTS(SELECT 1 FROM sessionventedirecte WHERE id=?)").bind(id).bind(form_text(&form,"categorie").unwrap_or_else(||"autre".into())).bind(label).bind(amount).bind(form_text(&form,"note")).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe/sessions").into_response())}

async fn vente_session_charge_supprimer(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path((id,charge_id)):Path<(i64,i64)>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;sqlx::query("DELETE FROM chargeventedirecte WHERE id=? AND session_vente_id=?").bind(charge_id).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe/sessions").into_response())}

async fn vente_commande_session(State(state):State<AppState>,Extension(session):Extension<SessionData>,Path(id):Path<i64>,Form(form):Form<HashMap<String,String>>)->AppResult<Response>{require_writer(&session)?;verify_csrf(&session,&form)?;let session_id=form_i64(&form,"session_vente_id");if let Some(session_id)=session_id{let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessionventedirecte WHERE id=?").bind(session_id).fetch_one(&state.pool).await?;if exists==0{return Err(AppError::Invalid("Session introuvable".into()))}}sqlx::query("UPDATE commandeventedirecte SET session_vente_id=? WHERE id=?").bind(session_id).bind(id).execute(&state.pool).await?;Ok(Redirect::to("/vente-directe").into_response())}

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
    list_page(
        &state,
        &session,
        "Paramètres",
        "Paramètres techniques conservés dans la base. Les secrets de communication ne sont jamais affichés ici.",
        "SELECT cle,valeur FROM parametre ORDER BY cle",
        &["cle", "valeur"],
    )
    .await
}

async fn correctifs(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{render(&state,"correctifs.html",Value::Object(context(&session)))}
async fn apropos(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{render(&state,"apropos.html",Value::Object(context(&session)))}
async fn contact(State(state):State<AppState>,Extension(session):Extension<SessionData>)->AppResult<Html<String>>{render(&state,"contact.html",Value::Object(context(&session)))}

async fn list_page(state:&AppState,session:&SessionData,title:&str,description:&str,sql:&str,columns:&[&str])->AppResult<Html<String>>{
    let rows=generic_rows(&state.pool,sql).await?;
    render_list_page(state,session,title,description,rows,columns)
}

fn render_list_page(state:&AppState,session:&SessionData,title:&str,description:&str,rows:Vec<Value>,columns:&[&str])->AppResult<Html<String>>{
    let cols:Vec<Value>=columns.iter().map(|key|json!({"key":key,"label":key.replace('_'," ")})).collect();
    let mut ctx=context(session);
    ctx.insert("title".into(),json!(title));
    ctx.insert("description".into(),json!(description));
    ctx.insert("columns".into(),Value::Array(cols));
    ctx.insert("rows".into(),Value::Array(rows));
    render(state,"liste.html",Value::Object(ctx))
}

async fn generic_rows(pool:&SqlitePool,sql:&str)->AppResult<Vec<Value>>{
    let rows=sqlx::query(sql).fetch_all(pool).await?;
    rows_to_json(rows)
}

fn rows_to_json(rows:Vec<SqliteRow>)->AppResult<Vec<Value>>{
    let mut out=Vec::with_capacity(rows.len());
    for row in rows{
        let mut object=Map::new();
        for(index,column)in row.columns().iter().enumerate(){
            let raw=row.try_get_raw(index)?;
            let value=if raw.is_null(){
                Value::Null
            }else{
                match raw.type_info().name(){
                    "INTEGER"=>json!(row.try_get::<i64,_>(index)?),
                    "REAL"=>json!(row.try_get::<f64,_>(index)?),
                    "BLOB"=>json!("[donnée binaire]"),
                    _=>json!(row.try_get::<String,_>(index)?),
                }
            };
            object.insert(column.name().to_string(),value);
        }
        out.push(Value::Object(object));
    }
    Ok(out)
}

async fn compatibility_fallback(
    State(_state):State<AppState>,
    session:Option<Extension<SessionData>>,
    request:axum::extract::Request,
)->Response{
    let path=request.uri().path().to_string();
    if session.is_none(){return Redirect::to("/login").into_response()}
    (StatusCode::NOT_IMPLEMENTED,Html(format!("<!doctype html><meta charset='utf-8'><main style='font-family:sans-serif;max-width:760px;margin:60px auto'><h1>Fonction non encore portée</h1><p>La route <code>{path}</code> n’est pas encore reliée à une action Rust. Aucune donnée n’a été modifiée.</p><p><a href='/'>Retour à l’accueil</a></p></main>"))).into_response()
}

#[cfg(test)]
mod gttt_tests {
    use super::*;

    fn litter(
        live_born: f64,
        stillborn: f64,
        stillborn_rate: Option<f64>,
        weaned: f64,
        adopted: f64,
        removed: f64,
    ) -> GtttLitter {
        GtttLitter {
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
        assert_eq!(summary["taux_mortnes"], json!(7.1));
    }

    #[test]
    fn mortalite_allaitement_tient_compte_adoptions_retraits() {
        let summary = gttt_summary(&[litter(13.0, 1.0, None, 11.0, 2.0, 1.0)]);
        assert_eq!(summary["mortalite_allaitement"], json!(21.4));
    }

    #[test]
    fn montants_francais_reconnaissent_tous_les_signes_comptables() {
        assert_eq!(parse_french_number("12,34-"), Some(-12.34));
        assert_eq!(parse_french_number("-12,34"), Some(-12.34));
        assert_eq!(parse_french_number("(12,34)"), Some(-12.34));
        assert_eq!(parse_french_number("−12,34"), Some(-12.34));
        assert_eq!(parse_french_number("1 234,56"), Some(1234.56));
    }
}
