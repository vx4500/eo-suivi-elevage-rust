use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Erreur de base de données: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Erreur interne: {0}")]
    Internal(#[from] anyhow::Error),
    #[error("Donnée invalide: {0}")]
    Invalid(String),
    #[error("Accès interdit")]
    Forbidden,
    #[error("Élément introuvable")]
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self, "requête en erreur");
        let (status, message) = match self {
            Self::Invalid(message) => (StatusCode::BAD_REQUEST, message),
            Self::Forbidden => (StatusCode::FORBIDDEN, "Accès interdit".to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "Élément introuvable".to_string()),
            Self::Database(_) | Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Une erreur est survenue. Les détails ont été enregistrés dans le journal."
                    .to_string(),
            ),
        };
        // Le message est aussi porté par `data-app-error` : le script global de base.html
        // le récupère pour l'afficher dans un encart sur la page en cours, sans navigation.
        let escaped = html_escape(&message);
        (
            status,
            Html(format!(
                "<!doctype html><meta charset='utf-8'><main style='font-family:sans-serif;max-width:760px;margin:60px auto'><h1>EO-Suivi</h1><p data-app-error=\"{}\">{}</p><p><a href='/'>Retour à l’accueil</a></p></main>",
                escaped, escaped
            )),
        )
            .into_response()
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub type AppResult<T> = Result<T, AppError>;
