use crate::routes::AppState;
use axum::extract::{Request, State};
use axum::http::header::COOKIE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

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
    rand::thread_rng().fill_bytes(&mut salt_raw);
    let salt = hex::encode(salt_raw);
    let mut output = [0_u8; 32];
    // Compatibilité exacte avec hashlib.pbkdf2_hmac("sha256", ..., 600_000).
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 600_000, &mut output);
    format!("{}${}", salt, hex::encode(output))
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

pub fn new_csrf() -> String {
    uuid::Uuid::new_v4().simple().to_string()
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
        if session.role != "admin"
            && ["/parametres", "/utilisateurs", "/journal", "/sauvegarde", "/maj"]
                .iter()
                .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
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
}
