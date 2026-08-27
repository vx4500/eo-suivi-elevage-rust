use crate::routes::AppState;
use axum::extract::{Request, State};
use axum::http::header::COOKIE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use pbkdf2::pbkdf2_hmac;
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use subtle::ConstantTimeEq;

const SALARIE_DEFAUT: &[&str] = &["planning", "bandes", "truies", "charcutiers", "sanitaire"];

const SALARIE_HORS_SECTION_OK: &[&str] = &[
    "/apropos",
    "/correctifs",
    "/mon-compte",
    "/api",
    "/scan",
    "/qr",
    "/calendrier",
    "/taches",
    "/saisie-rapide",
    "/evenement",
    "/mesure",
    "/perte",
    "/declaration",
    "/attente",
    "/template",
    "/export",
    "/cahiers",
    "/engraissement",
    "/transfert",
    "/transferts",
    "/quotidien",
];

const ELEVEUR_ONLY: &[&str] = &[
    "/import",
    "/import-pdf",
    "/pharmacie",
    "/objectif",
    "/cause",
];

const ADMIN_ONLY: &[&str] = &[
    "/parametres",
    "/reglages",
    "/utilisateurs",
    "/journal",
    "/sauvegarde",
    "/administration",
    "/maj",
];

/// Valeur par défaut de `parametre.type_elevage` quand rien n'est enregistré :
/// préserve le comportement historique (cycle complet) pour les bases existantes.
pub const TYPE_ELEVAGE_DEFAUT: &str = "naisseur_engraisseur";

/// Les cinq profils reconnus par le paramétrage « Type d'élevage » (voir Étape 0bis
/// de la spécification). Toute valeur inconnue en base retombe sur le défaut.
pub const TYPES_ELEVAGE: &[(&str, &str)] = &[
    ("naisseur_engraisseur", "Naisseur-engraisseur"),
    ("naisseur", "Naisseur"),
    ("postsevreur", "Post-sevreur seul"),
    ("postsevreur_engraisseur", "Post-sevreur-engraisseur"),
    ("engraisseur", "Engraisseur seul"),
];

#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionData {
    pub uid: i64,
    pub identifiant: String,
    pub nom: String,
    pub role: String,
    pub sections: Vec<String>,
    pub csrf: String,
    pub doit_changer_mdp: bool,
    /// Type d'élevage actif (voir `TYPES_ELEVAGE`) — conditionne l'affichage des
    /// écrans de reproduction/verraterie/maternité. Mis en cache dans la session
    /// à la connexion et rafraîchi en direct quand un admin change le réglage.
    pub type_elevage: String,
    /// Modules optionnels (§0/§2/§4 de la spécification) : désactivés par
    /// défaut pour un petit élevage, activables sans changer d'outil. Même
    /// mécanisme de cache/rafraîchissement en direct que `type_elevage`.
    pub module_genetique: bool,
    pub module_prestataires: bool,
    pub module_charcutiers_rfid: bool,
    pub module_vente_directe: bool,
}

impl SessionData {
    pub fn peut_modifier(&self) -> bool {
        matches!(self.role.as_str(), "admin" | "eleveur")
    }

    pub fn est_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Vrai si le type d'élevage actif conduit des truies (naisseur ou
    /// naisseur-engraisseur) : conditionne l'affichage de la reproduction,
    /// de la verraterie/maternité et de la GTTT.
    pub fn a_truies(&self) -> bool {
        matches!(
            self.type_elevage.as_str(),
            "naisseur_engraisseur" | "naisseur"
        )
    }

    /// Vrai si le type d'élevage actif engraisse des porcs (naisseur-engraisseur,
    /// post-sevreur-engraisseur ou engraisseur seul) : conditionne l'affichage
    /// de l'engraissement et du GMQ/IC de cette phase.
    pub fn engraisse(&self) -> bool {
        matches!(
            self.type_elevage.as_str(),
            "naisseur_engraisseur" | "postsevreur_engraisseur" | "engraisseur"
        )
    }

    /// Vrai si le type d'élevage actif reçoit des animaux achetés (tout sauf le
    /// naisseur pur, qui vend ses porcelets au sevrage).
    pub fn recoit_achats(&self) -> bool {
        matches!(
            self.type_elevage.as_str(),
            "postsevreur" | "postsevreur_engraisseur" | "engraisseur"
        )
    }
}

pub fn hash_password(password: &str) -> String {
    let mut salt_raw = [0_u8; 16];
    OsRng.fill_bytes(&mut salt_raw);
    let salt = hex::encode(salt_raw);
    let mut output = [0_u8; 32];
    // Compatibilité exacte avec hashlib.pbkdf2_hmac("sha256", ..., 600_000).
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 600_000, &mut output);
    format!("{}${}", salt, hex::encode(output))
}

/// PBKDF2 est volontairement exécuté hors des workers asynchrones Tokio.
pub async fn hash_password_async(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|error| anyhow::anyhow!("hachage du mot de passe interrompu: {error}"))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let Some((salt, expected_hex)) = stored.split_once('$') else {
        return false;
    };
    let Ok(expected) = hex::decode(expected_hex) else {
        return false;
    };
    let mut output = vec![0_u8; expected.len()];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 600_000, &mut output);
    output.ct_eq(&expected).into()
}

/// La vérification PBKDF2 est coûteuse en CPU et ne doit pas bloquer une route async.
pub async fn verify_password_async(password: String, stored: String) -> anyhow::Result<bool> {
    tokio::task::spawn_blocking(move || verify_password(&password, &stored))
        .await
        .map_err(|error| anyhow::anyhow!("vérification du mot de passe interrompue: {error}"))
}

pub fn new_secure_token() -> String {
    let mut raw = [0_u8; 32];
    OsRng.fill_bytes(&mut raw);
    hex::encode(raw)
}

pub fn new_csrf() -> String {
    new_secure_token()
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn section_for_path(path: &str) -> Option<&'static str> {
    match path.trim_start_matches('/').split('/').next().unwrap_or("") {
        "planning" => Some("planning"),
        "bandes" | "bande" => Some("bandes"),
        "truies" | "truie" | "recherche" | "inseminations" => Some("truies"),
        "charcutiers" | "charcutier" | "abattoir" | "traitement-charc" => Some("charcutiers"),
        "productivite" | "gttt" => Some("productivite"),
        "ifip" => Some("ifip"),
        "reformes" => Some("reformes"),
        "cochettes" => Some("cochettes"),
        "sanitaire" => Some("sanitaire"),
        "stock" => Some("stock"),
        "economique" => Some("economique"),
        "structure" | "salle" => Some("structure"),
        "effectifs" => Some("effectifs"),
        "archives" => Some("archives"),
        "entretien" | "taches" => Some("entretien"),
        _ => None,
    }
}

fn salarie_path_allowed(path: &str, sections: &[String]) -> bool {
    if matches!(path, "/" | "/logout" | "/contact") {
        return true;
    }
    if let Some(section) = section_for_path(path) {
        return if sections.is_empty() {
            SALARIE_DEFAUT.contains(&section)
        } else {
            sections.iter().any(|allowed| allowed == section)
        };
    }
    SALARIE_HORS_SECTION_OK
        .iter()
        .any(|prefix| path_has_prefix(path, prefix))
}

pub async fn guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if path.starts_with("/static") || path == "/logo" {
        return next.run(request).await;
    }

    let public = !crate::demo_portal::enabled()
        && (path == "/login"
            || path == "/commande"
            || path.starts_with("/commande/")
            || (path.starts_with("/vente-directe/produit/") && path.ends_with("/image"))
            || path.starts_with("/desinscription/"))
        || path == "/login";
    let session_id = cookie_value(request.headers(), "eo_session");
    let session = session_id
        .as_ref()
        .and_then(|sid| state.sessions.get(sid).map(|entry| entry.value().clone()));

    if !public && session.is_none() {
        return Redirect::to("/login").into_response();
    }

    if let Some(session) = session {
        if crate::demo_portal::enabled() {
            if !crate::demo_portal::valid(&state.pool, session.uid, chrono::Utc::now().timestamp())
                .await
            {
                if let Some(sid) = session_id.as_ref() {
                    state.sessions.remove(sid);
                }
                return Redirect::to("/login?err=expire").into_response();
            }
            if crate::demo_portal::blocked(&path) {
                return (
                    axum::http::StatusCode::FORBIDDEN,
                    "Cette opération système est désactivée dans la démonstration.",
                )
                    .into_response();
            }
        }
        if session.doit_changer_mdp && path != "/mon-compte/mdp" && path != "/logout" {
            return Redirect::to("/mon-compte/mdp?force=1").into_response();
        }
        if session.role == "engraisseur"
            && path != "/"
            && !path.starts_with("/engraissement")
            && !path.starts_with("/declaration")
            && path != "/logout"
            && !path.starts_with("/mon-compte")
        {
            return Redirect::to("/engraissement").into_response();
        }
        if !matches!(session.role.as_str(), "admin" | "eleveur")
            && ELEVEUR_ONLY
                .iter()
                .any(|prefix| path_has_prefix(&path, prefix))
        {
            return Redirect::to("/").into_response();
        }
        if session.role == "salarie" && !salarie_path_allowed(&path, &session.sections) {
            return Redirect::to("/").into_response();
        }
        if !session.module_vente_directe && path_has_prefix(&path, "/vente-directe") {
            return Redirect::to("/").into_response();
        }
        if session.role != "admin"
            && ADMIN_ONLY
                .iter()
                .any(|prefix| path_has_prefix(&path, prefix))
        {
            return Redirect::to("/").into_response();
        }
        request.extensions_mut().insert(session);
    }
    let mut response = next.run(request).await;
    if crate::demo_portal::enabled() {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}

fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_un_hash_python_v165() {
        let hash = "00112233445566778899aabbccddeeff$a61cedafd9cc101606a3d49291765ce8062a428aa63377099b70b899d1a0fed5";
        assert!(verify_password("cooperl2026", hash));
        assert!(!verify_password("mauvais", hash));
    }

    #[test]
    fn nouveau_hash_est_verifiable() {
        let hash = hash_password("mot-de-passe-solide");
        assert!(verify_password("mot-de-passe-solide", &hash));
        assert!(!verify_password("autre", &hash));
    }

    #[test]
    fn salarie_est_interdit_par_defaut_hors_section() {
        assert!(!salarie_path_allowed("/route-inconnue", &[]));
        assert!(!salarie_path_allowed("/economique", &[]));
        assert!(salarie_path_allowed("/bandes", &[]));
        assert_eq!(
            salarie_path_allowed("/taches", &[]),
            salarie_path_allowed("/entretien", &[])
        );
        assert!(salarie_path_allowed("/taches", &["entretien".to_string()]));
    }

    #[test]
    fn salarie_respecte_ses_sections_personnalisees() {
        let sections = vec!["economique".to_string()];
        assert!(salarie_path_allowed("/economique", &sections));
        assert!(!salarie_path_allowed("/bandes", &sections));
    }

    fn session_avec_type(type_elevage: &str) -> SessionData {
        SessionData {
            uid: 1,
            identifiant: "test".into(),
            nom: "Test".into(),
            role: "eleveur".into(),
            sections: vec![],
            csrf: "csrf".into(),
            doit_changer_mdp: false,
            type_elevage: type_elevage.into(),
            module_genetique: false,
            module_prestataires: true,
            module_charcutiers_rfid: true,
            module_vente_directe: true,
        }
    }

    #[test]
    fn type_elevage_conditionne_les_ecrans_actifs() {
        let naisseur_engraisseur = session_avec_type("naisseur_engraisseur");
        assert!(naisseur_engraisseur.a_truies());
        assert!(naisseur_engraisseur.engraisse());
        assert!(!naisseur_engraisseur.recoit_achats());

        let naisseur = session_avec_type("naisseur");
        assert!(naisseur.a_truies());
        assert!(!naisseur.engraisse());
        assert!(!naisseur.recoit_achats());

        let postsevreur = session_avec_type("postsevreur");
        assert!(!postsevreur.a_truies());
        assert!(!postsevreur.engraisse());
        assert!(postsevreur.recoit_achats());

        let postsevreur_engraisseur = session_avec_type("postsevreur_engraisseur");
        assert!(!postsevreur_engraisseur.a_truies());
        assert!(postsevreur_engraisseur.engraisse());
        assert!(postsevreur_engraisseur.recoit_achats());

        let engraisseur = session_avec_type("engraisseur");
        assert!(!engraisseur.a_truies());
        assert!(engraisseur.engraisse());
        assert!(engraisseur.recoit_achats());
    }

    #[test]
    fn valeur_inconnue_ne_donne_acces_a_rien() {
        let inconnu = session_avec_type("valeur-jamais-enregistree");
        assert!(!inconnu.a_truies());
        assert!(!inconnu.engraisse());
        assert!(!inconnu.recoit_achats());
    }
}
