use super::*;

pub(super) fn catalogue() -> AppResult<Vec<Value>> {
    serde_json::from_str(include_str!("../../resources/documents-elevage.json"))
        .map_err(|e| AppError::Internal(e.into()))
}
fn valid_item(key: &str) -> AppResult<()> {
    if catalogue()?.iter().any(|group| {
        group["items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["id"].as_str() == Some(key)))
    }) {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}
fn file_type(bytes: &[u8]) -> AppResult<(&'static str, &'static str)> {
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::Invalid(
            "Choisissez un fichier non vide de 8 Mo maximum.".into(),
        ));
    }
    if bytes.starts_with(b"%PDF-") {
        Ok(("application/pdf", "pdf"))
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok(("image/png", "png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok(("image/jpeg", "jpg"))
    } else {
        Err(AppError::Invalid(
            "Formats acceptés : PDF, photo JPEG ou PNG.".into(),
        ))
    }
}

pub(super) async fn page(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    let can_manage = matches!(session.role.as_str(), "admin" | "eleveur");
    let mut groups = catalogue()?;
    // Les justificatifs peuvent contenir des salaires ou données sanitaires :
    // ne pas exposer même leurs noms aux comptes salariés/prestataires.
    let files = if can_manage {
        generic_rows(&state.pool, "SELECT id,document_key,nom,cree_le,LENGTH(contenu) AS taille FROM documentjustificatif ORDER BY cree_le DESC,id DESC").await?
    } else {
        Vec::new()
    };
    for group in &mut groups {
        if let Some(items) = group["items"].as_array_mut() {
            for item in items {
                let matching: Vec<_> = files
                    .iter()
                    .filter(|f| f["document_key"] == item["id"])
                    .cloned()
                    .collect();
                item["fichiers"] = json!(matching);
            }
        }
    }
    let mut ctx = context(&session);
    ctx.insert("document_groups".into(), json!(groups));
    ctx.insert("documents_manage".into(), json!(can_manage));
    render(&state, "documents_obligatoires.html", Value::Object(ctx))
}

pub(super) async fn upload(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(key): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    valid_item(&key)?;
    let mut form = HashMap::new();
    let mut file = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
    {
        match field.name() {
            Some("csrf_token") => {
                form.insert(
                    "csrf_token".into(),
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::Invalid(e.to_string()))?,
                );
            }
            Some("fichier") => {
                if file.is_some() {
                    return Err(AppError::Invalid("Importez un fichier à la fois.".into()));
                }
                let name: String = field
                    .file_name()
                    .unwrap_or("document")
                    .chars()
                    .filter(|c| !c.is_control() && !['/', '\\'].contains(c))
                    .take(180)
                    .collect();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Invalid(e.to_string()))?;
                file = Some((name, bytes));
            }
            _ => {}
        }
    }
    verify_csrf(&session, &form)?;
    let (name, bytes) = file.ok_or_else(|| AppError::Invalid("Fichier manquant.".into()))?;
    let (mime, _) = file_type(&bytes)?;
    sqlx::query("INSERT INTO documentjustificatif(document_key,nom,mime,contenu,sha256,cree_par) VALUES(?,?,?,?,?,?) ON CONFLICT(document_key,sha256) DO NOTHING")
        .bind(&key).bind(if name.trim().is_empty() { "document" } else { &name }).bind(mime).bind(bytes.as_ref()).bind(contenu_sha256(&bytes)).bind(session.uid).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/documents-obligatoires#doc-{key}")).into_response())
}

pub(super) async fn download(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    let (bytes,): (Vec<u8>,) =
        sqlx::query_as("SELECT contenu FROM documentjustificatif WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let (mime, ext) = file_type(&bytes)?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"document-{id}.{ext}\""),
            ),
            (header::CACHE_CONTROL, "private, no-store".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (header::CONTENT_SECURITY_POLICY, "sandbox".into()),
        ],
        bytes,
    )
        .into_response())
}

pub(super) async fn delete(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_economic_import(&session)?;
    verify_csrf(&session, &form)?;
    let key: String =
        sqlx::query_scalar("DELETE FROM documentjustificatif WHERE id=? RETURNING document_key")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/documents-obligatoires#doc-{key}")).into_response())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    pub(crate) fn session() -> SessionData {
        SessionData {
            uid: 1,
            identifiant: "test".into(),
            nom: "Test".into(),
            role: "admin".into(),
            sections: vec![],
            csrf: "test".into(),
            doit_changer_mdp: false,
            type_elevage: "naisseur_engraisseur".into(),
            module_genetique: false,
            module_prestataires: false,
            module_charcutiers_rfid: false,
            module_vente_directe: false,
        }
    }
    pub(crate) async fn state() -> AppState {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0002_ventelot.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO utilisateur(id,identifiant,nom,role,hash_mdp) VALUES(1,'test','Test','admin','unused')").execute(&pool).await.unwrap();
        AppState::new(
            Config {
                bind: "127.0.0.1:8080".parse().unwrap(),
                db_path: "/tmp/documents-test.db".into(),
                secure_cookies: false,
            },
            pool,
            crate::templates::build().unwrap(),
        )
    }
    async fn multipart(csrf: &str, content: &str) -> Multipart {
        use axum::extract::FromRequest;
        let body=format!("--BOUNDARY\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{csrf}\r\n--BOUNDARY\r\nContent-Disposition: form-data; name=\"fichier\"; filename=\"preuve.pdf\"\r\nContent-Type: application/pdf\r\n\r\n{content}\r\n--BOUNDARY--\r\n");
        let request = axum::http::Request::builder()
            .header(
                header::CONTENT_TYPE,
                "multipart/form-data; boundary=BOUNDARY",
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        Multipart::from_request(request, &()).await.unwrap()
    }
    #[tokio::test]
    async fn import_prive_dedoublonnage_telechargement_suppression() {
        let state = state().await;
        let key = "doc-01-01".to_string();
        assert!(upload(
            State(state.clone()),
            Extension(session()),
            Path(key.clone()),
            multipart("incorrect", "%PDF-1.4\n").await
        )
        .await
        .is_err());
        assert!(upload(
            State(state.clone()),
            Extension(session()),
            Path("inconnu".into()),
            multipart("test", "%PDF-1.4\n").await
        )
        .await
        .is_err());
        assert!(upload(
            State(state.clone()),
            Extension(session()),
            Path(key.clone()),
            multipart("test", "<html>refuse</html>").await
        )
        .await
        .is_err());
        for _ in 0..2 {
            upload(
                State(state.clone()),
                Extension(session()),
                Path(key.clone()),
                multipart("test", "%PDF-1.4\n").await,
            )
            .await
            .unwrap();
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documentjustificatif")
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let response = download(State(state.clone()), Extension(session()), Path(1))
            .await
            .unwrap();
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
        assert!(response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment;"));
        let bytes = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"%PDF-1.4\n");
        let Html(html) = page(State(state.clone()), Extension(session()))
            .await
            .unwrap();
        assert!(html.contains("preuve.pdf"));
        assert!(!html.contains("Ce que cette liste couvre"));
        let mut worker = session();
        worker.role = "salarie".into();
        let Html(html) = page(State(state.clone()), Extension(worker.clone()))
            .await
            .unwrap();
        assert!(!html.contains("preuve.pdf"));
        assert!(
            download(State(state.clone()), Extension(worker.clone()), Path(1))
                .await
                .is_err()
        );
        assert!(upload(
            State(state.clone()),
            Extension(worker),
            Path(key),
            multipart("test", "%PDF-1.4\n").await
        )
        .await
        .is_err());
        assert!(delete(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(HashMap::new())
        )
        .await
        .is_err());
        delete(
            State(state.clone()),
            Extension(session()),
            Path(1),
            Form(HashMap::from([("csrf_token".into(), "test".into())])),
        )
        .await
        .unwrap();
        assert!(
            download(State(state.clone()), Extension(session()), Path(1))
                .await
                .is_err()
        );
    }
    #[test]
    fn fichiers_et_cles_valides() {
        assert!(file_type(b"<html>test</html>").is_err());
        assert!(file_type(b"").is_err());
        assert_eq!(file_type(b"%PDF-1.4\n").unwrap().1, "pdf");
        assert!(file_type(&vec![0; 8 * 1024 * 1024 + 1]).is_err());
        let groups = catalogue().unwrap();
        let keys: Vec<_> = groups
            .iter()
            .flat_map(|g| g["items"].as_array().unwrap())
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(keys.iter().collect::<HashSet<_>>().len(), 65);
        assert!(valid_item(keys[0]).is_ok());
        assert!(valid_item("../unknown").is_err());
    }
}
