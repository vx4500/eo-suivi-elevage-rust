use super::*;
fn require_demo() -> AppResult<()> {
    if crate::demo_portal::enabled() {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}
pub(super) async fn acces(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<Html<String>> {
    require_demo()?;
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    crate::demo_portal::prune(&state.pool)
        .await
        .map_err(AppError::Internal)?;
    let mut ctx = context(&session);
    ctx.insert("acces".into(),Value::Array(generic_rows(&state.pool,"SELECT u.id,u.nom,u.identifiant,datetime(d.expire,'unixepoch') AS expiration,u.actif FROM demo_acces d JOIN utilisateur u ON u.id=d.utilisateur_id ORDER BY d.cree DESC").await?));
    ctx.insert("suggestions".into(),Value::Array(generic_rows(&state.pool,"SELECT s.id,u.nom,s.message,s.page,datetime(s.cree,'unixepoch') AS date FROM demo_suggestion s JOIN utilisateur u ON u.id=s.utilisateur_id ORDER BY s.id DESC LIMIT 200").await?));
    render(&state, "demo_acces.html", Value::Object(ctx))
}
pub(super) async fn creer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    require_demo()?;
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    let nom = form_text(&form, "nom")
        .filter(|s| s.len() <= 100)
        .ok_or_else(|| AppError::Invalid("Nom obligatoire (100 caractères maximum)".into()))?;
    let identifiant = form_text(&form, "identifiant")
        .filter(|s| s.len() <= 100)
        .ok_or_else(|| AppError::Invalid("Identifiant obligatoire".into()))?;
    let password = auth::new_secure_token();
    let hash = auth::hash_password_async(password.clone())
        .await
        .map_err(AppError::Internal)?;
    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin().await?;
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM utilisateur WHERE identifiant=?")
        .bind(&identifiant)
        .fetch_one(&mut *tx)
        .await?;
    if exists > 0 {
        return Err(AppError::Invalid("Cet identifiant existe déjà".into()));
    }
    let id=sqlx::query("INSERT INTO utilisateur(identifiant,nom,hash_mdp,role,actif,doit_changer_mdp) VALUES(?,?,?,'eleveur',1,0)").bind(&identifiant).bind(nom).bind(hash).execute(&mut *tx).await?.last_insert_rowid();
    sqlx::query("INSERT INTO demo_acces(utilisateur_id,expire,cree) VALUES(?,?,?)")
        .bind(id)
        .bind(now + 48 * 3600)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut ctx = context(&session);
    ctx.insert("identifiant_cree".into(), json!(identifiant));
    ctx.insert("mot_de_passe".into(), json!(password));
    render(&state, "demo_acces.html", Value::Object(ctx))
}
pub(super) async fn revoquer(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_demo()?;
    if !session.est_admin() {
        return Err(AppError::Forbidden);
    }
    verify_csrf(&session, &form)?;
    sqlx::query("UPDATE utilisateur SET actif=0 WHERE id=? AND id IN(SELECT utilisateur_id FROM demo_acces)").bind(id).execute(&state.pool).await?;
    state.sessions.retain(|_, s| s.uid != id);
    Ok(Redirect::to("/demo/acces").into_response())
}
pub(super) async fn suggestion(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<Response> {
    require_demo()?;
    verify_csrf(&session, &form)?;
    let message = form_text(&form, "message")
        .filter(|s| s.len() <= 4000)
        .ok_or_else(|| AppError::Invalid("Suggestion obligatoire, 4000 octets maximum".into()))?;
    let page = form_text(&form, "page")
        .filter(|s| s.starts_with('/') && s.len() <= 200)
        .unwrap_or_else(|| "/".into());
    let now = chrono::Utc::now().timestamp();
    let mut tx = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM demo_suggestion WHERE utilisateur_id=? AND cree>?",
    )
    .bind(session.uid)
    .bind(now - 86400)
    .fetch_one(&mut *tx)
    .await?;
    if count >= 30 {
        return Err(AppError::Invalid(
            "Limite de 30 suggestions par jour atteinte".into(),
        ));
    }
    sqlx::query("INSERT INTO demo_suggestion(utilisateur_id,message,page,cree) VALUES(?,?,?,?)")
        .bind(session.uid)
        .bind(message)
        .bind(page)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Html("<meta charset=utf-8><h1>Merci pour votre suggestion !</h1><p>Elle a été enregistrée pour Emmanuel ORY.</p><a href='/'>Revenir à la démonstration</a>").into_response())
}
