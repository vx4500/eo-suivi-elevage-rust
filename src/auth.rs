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

const ELEVEUR_ONLY: &[&str] = &["/import", "/import-pdf", "/pharmacie", "/objectif", "/cause"];

const ADMIN_ONLY: &[&str] = &[
    "/parametres",
    "/reglages",
    "/utilisateurs",
    "/journal",
    "/sauvegarde",
    "/administration",
    "/maj",
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
}

impl SessionData {
    pub fn peut_modifier(&self) -> bool {
        matches!(self.role.as_str(), "admin" | "eleveur")
    }

    pub fn est_admin(&self) -> bool {
        self.role == "admin"
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
    path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
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
        "entretien" => Some("entretien"),
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

    let public = path == "/login"
        || path == "/commande"
        || path.starts_with("/commande/")
        || path.starts_with("/desinscription/");
    let session_id = cookie_value(request.headers(), "eo_session");
    let session = session_id
        .as_ref()
        .and_then(|sid| state.sessions.get(sid).map(|entry| entry.value().clone()));

    if !public && session.is_none() {
        return Redirect::to("/login").into_response();
    }

    if let Some(session) = session {
        if session.doit_changer_mdp
            && path != "/mon-compte/mdp"
            && path != "/logout"
        {
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
            && ELEVEUR_ONLY.iter().any(|prefix| path_has_prefix(&path, prefix))
        {
            return Redirect::to("/").into_response();
        }
        if session.role == "salarie" && !salarie_path_allowed(&path, &session.sections) {
            return Redirect::to("/").into_response();
        }
        if session.role != "admin"
            && ADMIN_ONLY.iter().any(|prefix| path_has_prefix(&path, prefix))
        {
            return Redirect::to("/").into_response();
        }
        request.extensions_mut().insert(session);
    }
    next.run(request).await
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
        assert!(salarie_path_allowed("/taches", &[]));
    }

    #[test]
    fn salarie_respecte_ses_sections_personnalisees() {
        let sections = vec!["economique".to_string()];
        assert!(salarie_path_allowed("/economique", &sections));
        assert!(!salarie_path_allowed("/bandes", &sections));
    }
}
