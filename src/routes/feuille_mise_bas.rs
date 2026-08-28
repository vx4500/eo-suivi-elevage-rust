use super::*;

pub(super) async fn page(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> AppResult<Html<String>> {
    if session.role == "engraisseur" {
        return Err(AppError::Forbidden);
    }
    let band = sqlx::query("SELECT id,code,date_mb,site FROM bande WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let band = rows_to_json(vec![band])?.remove(0);
    let blank = query.get("vierge").is_some_and(|v| v == "1");
    let rows = if blank {
        vec![]
    } else {
        rows_to_json(sqlx::query("WITH ids AS (SELECT id FROM truie WHERE bande_code=(SELECT code FROM bande WHERE id=?) AND reformee=0 UNION SELECT truie_id FROM evenement WHERE bande_id=? AND truie_id IS NOT NULL) SELECT t.id,t.num_travail,c.nom AS case_nom FROM ids JOIN truie t ON t.id=ids.id LEFT JOIN evenement e ON e.id=(SELECT e2.id FROM evenement e2 WHERE e2.truie_id=t.id AND e2.bande_id=? AND e2.type='mise_bas' ORDER BY e2.date DESC,e2.id DESC LIMIT 1) LEFT JOIN casesalle c ON c.id=COALESCE(e.case_id,CASE WHEN t.bande_code=(SELECT code FROM bande WHERE id=?) THEN t.case_id END) ORDER BY t.num_travail COLLATE NOCASE,t.id").bind(id).bind(id).bind(id).bind(id).fetch_all(&state.pool).await?)?
    };
    let count = rows.len();
    let mut pages: Vec<Vec<Value>> = rows.chunks(12).map(|c| c.to_vec()).collect();
    if pages.is_empty() {
        pages.push(vec![]);
    }
    for page in &mut pages {
        while page.len() < 12 {
            page.push(json!({}));
        }
    }
    let name: Option<String> =
        sqlx::query_scalar("SELECT valeur FROM parametre WHERE cle='nom_elevage'")
            .fetch_optional(&state.pool)
            .await?
            .flatten();
    let mut ctx = context(&session);
    ctx.insert("bande".into(), band);
    ctx.insert("pages".into(), json!(pages));
    ctx.insert("nom_elevage".into(), json!(name));
    ctx.insert("nombre_truies".into(), json!(count));
    ctx.insert("vierge".into(), json!(blank));
    render(&state, "feuille_mise_bas.html", Value::Object(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::documents::tests::{session, state};
    #[tokio::test]
    async fn bande_selectionnee_sans_doublons_et_pagination() -> anyhow::Result<()> {
        let s = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'BANDE-A'),(2,'BANDE-B');")
            .execute(&s.pool)
            .await?;
        for n in 1..=13 {
            sqlx::query("INSERT INTO truie(id,num_travail,bande_code) VALUES(?,?,'BANDE-A')")
                .bind(n)
                .bind(format!("TRUIE-{n:02}"))
                .execute(&s.pool)
                .await?;
        }
        sqlx::query(
            "INSERT INTO truie(id,num_travail,bande_code) VALUES(14,'AUTRE-TRUIE','BANDE-B')",
        )
        .execute(&s.pool)
        .await?;
        sqlx::raw_sql("INSERT INTO evenement(type,date,truie_id,bande_id) VALUES('insemination','2026-01-01',1,1),('mise_bas','2026-05-01',1,1);").execute(&s.pool).await?;
        let Html(html) = page(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Query(HashMap::new()),
        )
        .await?;
        assert_eq!(html.matches("class=\"sheet\"").count(), 2);
        assert_eq!(html.matches("TRUIE-01").count(), 1);
        assert!(html.contains("TRUIE-13"));
        assert!(!html.contains("AUTRE-TRUIE"));
        assert!(html.contains("EO-Suivi Élevage"));
        assert!(html.contains("window.print()"));
        let Html(blank) = page(
            State(s.clone()),
            Extension(session()),
            Path(1),
            Query(HashMap::from([("vierge".into(), "1".into())])),
        )
        .await?;
        assert_eq!(blank.matches("class=\"sheet\"").count(), 1);
        assert!(!blank.contains("TRUIE-01"));
        assert!(page(
            State(s.clone()),
            Extension(session()),
            Path(999),
            Query(HashMap::new())
        )
        .await
        .is_err());
        let mut restricted = session();
        restricted.role = "engraisseur".into();
        assert!(page(
            State(s),
            Extension(restricted),
            Path(1),
            Query(HashMap::new())
        )
        .await
        .is_err());
        Ok(())
    }
}
