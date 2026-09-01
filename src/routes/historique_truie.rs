use super::*;

/// Chaque sevrage/perte appartient à l'intervalle entre deux mises-bas,
/// même si une bande est réutilisée ou n'a pas été renseignée.
pub(super) async fn portees(pool: &SqlitePool, sow: &Truie) -> AppResult<Vec<Value>> {
    let rows = sqlx::query(
        r#"
SELECT e.id,e.date,b.code AS bande,e.nes_totaux,e.nes_vifs,e.mort_nes,e.momifies,
 e.heure_debut,e.heure_fin,e.delivrance_ok,e.suivi_actif,
 ROW_NUMBER() OVER(ORDER BY e.date DESC,e.id DESC)-1 AS plus_recentes,
 pe.date_sevrage,pe.nb_sevres,pe.poids_moyen,pe.eld_entree AS eld1,pe.eld_sortie AS eld2,
 pe.adoptes,pe.retires,pe.pertes,pe.presents,pe.cloturee
FROM evenement e JOIN portee_effectif pe ON pe.id=e.id LEFT JOIN bande b ON b.id=e.bande_id
WHERE e.truie_id=? AND e.date<=date('now') ORDER BY e.date DESC,e.id DESC
"#,
    )
    .bind(sow.id)
    .fetch_all(pool)
    .await?;
    let mut rows = rows_to_json(rows)?;
    let imported = sqlx::query("SELECT id,rang,bande,nv AS nes_vifs,mn AS mort_nes,sev AS nb_sevres,ad AS adoptes,re AS retires,duree_gest,tx_pertes,eld1,eld2 FROM porteerang WHERE num_travail=? ORDER BY rang DESC,id DESC")
        .bind(&sow.num_travail).fetch_all(pool).await?;
    let imported = rows_to_json(imported)?;
    let mut used = HashSet::new();
    let mut used_ranks = HashSet::new();
    for i in 0..rows.len() {
        let band = rows[i]["bande"].as_str();
        let matches: Vec<_> = imported
            .iter()
            .filter(|p| band.is_some() && p["bande"].as_str() == band)
            .collect();
        let unique =
            band.is_some() && rows.iter().filter(|r| r["bande"].as_str() == band).count() == 1;
        if unique && matches.len() == 1 {
            let p = matches[0];
            used.insert(p["id"].as_i64().unwrap());
            rows[i]["rang"] = p["rang"].clone();
            used_ranks.insert(p["rang"].as_i64().unwrap());
            for key in [
                "nes_vifs",
                "mort_nes",
                "adoptes",
                "retires",
                "nb_sevres",
                "duree_gest",
                "tx_pertes",
                "eld1",
                "eld2",
            ] {
                if rows[i][key].is_null() {
                    rows[i][key] = p[key].clone();
                }
            }
            rows[i]["source"] = json!("Événements et historique importé");
        } else {
            rows[i]["source"] = json!("Événements enregistrés");
        }
        for key in ["adoptes", "retires"] {
            if rows[i][key].is_null() {
                rows[i][key] = json!(0);
            }
        }
    }
    // Ne pas renuméroter à partir de 1 un historique partiel (ex. rangs 5 et 6).
    for row in &mut rows {
        if row["rang"].is_null() {
            let rank = sow.rang - row["plus_recentes"].as_i64().unwrap_or(0);
            if rank > 0
                && !used_ranks.contains(&rank)
                && !imported.iter().any(|p| p["rang"].as_i64() == Some(rank))
            {
                row["rang"] = json!(rank);
                row["rang_estime"] = json!(true);
                used_ranks.insert(rank);
            }
        }
    }
    for mut p in imported {
        if !used.contains(&p["id"].as_i64().unwrap()) {
            p["source"] = json!("Historique importé");
            p["id"] = Value::Null;
            rows.push(p);
        }
    }
    rows.sort_by(|a, b| {
        b["rang"]
            .as_i64()
            .cmp(&a["rang"].as_i64())
            .then_with(|| b["date"].as_str().cmp(&a["date"].as_str()))
    });
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::documents::tests::{session, state};

    async fn sow(pool: &SqlitePool) -> Truie {
        sqlx::query_as(&format!("SELECT {TRUIE_FIELDS} FROM truie WHERE id=1"))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn historique_separe_cycles_et_affiche_presents_au_resume() -> anyhow::Result<()> {
        let state = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'); INSERT INTO truie(id,num_travail,rang,perf_sevres) VALUES(1,'T1',6,99),(2,'T2',1,NULL); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs,nb_sevres) VALUES(1,'mise_bas','2025-01-01',1,1,10,NULL),(2,'sevrage','2025-01-29',1,1,NULL,0),(3,'mise_bas','2025-07-01',1,1,12,NULL),(4,'mise_bas','2025-07-01',2,1,10,NULL); INSERT INTO perteporcelet(truie_id,bande_id,date,nb) VALUES(1,1,'2025-01-05',10),(1,1,'2025-07-05',2); INSERT INTO adoptionporcelet(date,source_id,destination_id,nombre) VALUES('2025-07-06',4,3,2),('2025-07-07',3,4,1);").execute(&state.pool).await?;
        let rows = portees(&state.pool, &sow(&state.pool).await).await?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["rang"], 6);
        assert_eq!(rows[0]["presents"], 11);
        assert_eq!(rows[0]["pertes"], 2);
        assert!(rows[0]["nb_sevres"].is_null());
        assert_eq!(rows[1]["rang"], 5);
        assert_eq!(rows[1]["nb_sevres"], 0);
        assert_eq!(rows[1]["pertes"], 10);
        assert_eq!(rows[1]["presents"], 0);
        let Html(mat) = maternite(
            State(state.clone()),
            Extension(session()),
            Query(HashMap::from([("bande_id".into(), "1".into())])),
        )
        .await?;
        let card = mat
            .split("id=\"truie-1\"")
            .nth(1)
            .unwrap()
            .split("id=\"truie-2\"")
            .next()
            .unwrap();
        assert!(card.contains("data-indicateur=\"presents\">11</b>"));
        assert!(!card.contains("2025-01-05"));
        let Html(html) = truie_detail(State(state), Extension(session()), Path(1)).await?;
        let resume = html
            .split("id=\"resume\"")
            .nth(1)
            .unwrap()
            .split("data-name=\"reproduction\"")
            .next()
            .unwrap();
        assert!(resume.contains("data-indicateur=\"presents\">11</b>"));
        assert!(!resume.contains("Sevrés"));
        let history = html.split("id=\"historique\"").nth(1).unwrap();
        assert!(history.contains("Historique par rang de portée"));
        assert!(history.contains("data-indicateur=\"sevres\">0</td>"));
        assert!(!history.contains("data-indicateur=\"sevres\">99"));
        Ok(())
    }

    #[tokio::test]
    async fn rangs_importes_conserves_et_sevrage_sans_bande() -> anyhow::Result<()> {
        let state = state().await;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'); INSERT INTO truie(id,num_travail,rang) VALUES(1,'T1',6); INSERT INTO porteerang(num_travail,rang,bande,nv,sev,ad,re,eld1) VALUES('T1',4,'ANCIENNE',13,11,1,0,14),('T1',6,'B1',12,10,0,0,15); INSERT INTO evenement(type,date,truie_id,bande_id,nes_vifs,nb_sevres) VALUES('mise_bas','2025-01-01',1,NULL,9,NULL),('sevrage','2025-01-29',1,NULL,NULL,9),('mise_bas','2025-07-01',1,1,12,NULL),('sevrage','2025-07-29',1,1,NULL,10);").execute(&state.pool).await?;
        let rows = portees(&state.pool, &sow(&state.pool).await).await?;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["rang"], 6);
        assert!(rows[0]["rang_estime"].is_null());
        assert_eq!(rows[0]["nb_sevres"], 10);
        assert_eq!(rows[0]["eld1"], 15.0);
        assert_eq!(rows[1]["rang"], 5);
        assert_eq!(rows[1]["nb_sevres"], 9);
        assert_eq!(rows[2]["rang"], 4);
        assert_eq!(rows[2]["nb_sevres"], 11.0);
        assert!(rows[2]["date"].is_null());
        Ok(())
    }

    #[tokio::test]
    async fn sevrage_futur_et_absence_de_portee_ne_fabriquent_pas_un_effectif() -> anyhow::Result<()>
    {
        let state = state().await;
        sqlx::query("INSERT INTO truie(id,num_travail,rang,perf_sevres) VALUES(1,'T1',1,10)")
            .execute(&state.pool)
            .await?;
        assert!(portees(&state.pool, &sow(&state.pool).await)
            .await?
            .is_empty());
        sqlx::raw_sql("INSERT INTO evenement(type,date,truie_id,nes_vifs,nb_sevres) VALUES('mise_bas','2025-01-01',1,12,NULL),('sevrage','2999-01-01',1,NULL,12);").execute(&state.pool).await?;
        let rows = portees(&state.pool, &sow(&state.pool).await).await?;
        assert_eq!(rows[0]["presents"], 12);
        assert!(rows[0]["nb_sevres"].is_null());
        Ok(())
    }
}
