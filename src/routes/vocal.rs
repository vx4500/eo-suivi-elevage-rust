//! Saisie vocale depuis l'application mobile.
//!
//! Le parcours tient en deux temps, et c'est volontaire : `analyser` ne fait
//! que comprendre — transcrire, reconnaître, chercher le contexte de la truie
//! — sans rien écrire ; `valider` enregistre, et seulement ce que l'éleveur a
//! relu à l'écran. Une transcription se trompe ; une écriture en base sur la
//! foi d'une transcription se corrigerait à la main pendant des semaines.

use super::*;
use crate::vocal;

/// Nombre de numéros voisins proposés quand le numéro compris est inconnu.
const VOISINS_PROPOSES: usize = 3;

/// Taille maximale d'un envoi audio. Une dictée dure quelques secondes ;
/// au-delà, c'est que le bouton est resté enfoncé dans une poche.
pub(super) const TAILLE_MAX_AUDIO: usize = 8 * 1024 * 1024;

/// Configuration du moteur de transcription, lue à chaque appel pour qu'une
/// installation de whisper.cpp ne demande pas de redémarrer le serveur.
struct Moteur {
    binaire: String,
    modele: String,
    ffmpeg: String,
    /// Nombre de fils de calcul. whisper.cpp en prend quatre par défaut :
    /// sur un serveur qui a plus de cœurs, c'est autant de temps d'attente
    /// gagné pour l'éleveur qui patiente devant son téléphone.
    threads: usize,
}

impl Moteur {
    fn depuis_env() -> Option<Self> {
        let binaire = std::env::var("EO_VOCAL_WHISPER").ok()?;
        let modele = std::env::var("EO_VOCAL_MODELE").ok()?;
        if binaire.trim().is_empty() || modele.trim().is_empty() {
            return None;
        }
        let threads = std::env::var("EO_VOCAL_THREADS")
            .ok()
            .and_then(|valeur| valeur.trim().parse::<usize>().ok())
            .filter(|valeur| *valeur > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            })
            .min(16);
        Some(Self {
            binaire,
            modele,
            ffmpeg: std::env::var("EO_VOCAL_FFMPEG").unwrap_or_else(|_| "ffmpeg".into()),
            threads,
        })
    }
}

/// Transcrit un enregistrement. whisper.cpp n'accepte que du WAV 16 kHz mono :
/// on passe systématiquement par ffmpeg, qui accepte tout ce que le téléphone
/// peut produire et garantit le format attendu.
async fn transcrire(moteur: &Moteur, audio: &[u8]) -> AppResult<String> {
    let base = std::env::temp_dir().join(format!("eo-vocal-{}", uuid::Uuid::new_v4()));
    let entree = base.with_extension("src");
    let converti = base.with_extension("wav");
    tokio::fs::write(&entree, audio)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let nettoyer = |fichiers: Vec<std::path::PathBuf>| async move {
        for fichier in fichiers {
            let _ = tokio::fs::remove_file(fichier).await;
        }
    };

    let conversion = tokio::process::Command::new(&moteur.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(&entree)
        .args(["-ar", "16000", "-ac", "1", "-f", "wav"])
        .arg(&converti)
        .output()
        .await;
    let conversion = match conversion {
        Ok(sortie) if sortie.status.success() => sortie,
        Ok(sortie) => {
            nettoyer(vec![entree, converti]).await;
            tracing::error!(erreur=%String::from_utf8_lossy(&sortie.stderr),"conversion audio impossible");
            return Err(AppError::Invalid(
                "Enregistrement audio illisible. Réessayez la dictée.".into(),
            ));
        }
        Err(erreur) => {
            nettoyer(vec![entree, converti]).await;
            tracing::error!(%erreur, ffmpeg=%moteur.ffmpeg, "ffmpeg introuvable");
            return Err(AppError::Invalid(
                "Conversion audio indisponible sur le serveur (ffmpeg absent).".into(),
            ));
        }
    };
    drop(conversion);

    // `-nt` supprime les horodatages, `-l fr` fige la langue : sans cela
    // Whisper bascule parfois sur l'anglais au milieu d'une suite de chiffres.
    let debut = std::time::Instant::now();
    let threads = moteur.threads.to_string();
    let transcription = tokio::process::Command::new(&moteur.binaire)
        .args([
            "-m",
            &moteur.modele,
            "-l",
            "fr",
            "-nt",
            "-np",
            "-t",
            &threads,
            "-f",
        ])
        .arg(&converti)
        .output()
        .await;
    // Journaliser la durée permet de savoir s'il faut un modèle plus léger :
    // c'est la seule mesure qui compte, celle du serveur de l'élevage.
    tracing::info!(
        millisecondes = debut.elapsed().as_millis(),
        threads = moteur.threads,
        "transcription vocale terminée"
    );
    nettoyer(vec![entree, converti]).await;
    match transcription {
        Ok(sortie) if sortie.status.success() => {
            Ok(String::from_utf8_lossy(&sortie.stdout).trim().to_string())
        }
        Ok(sortie) => {
            tracing::error!(erreur=%String::from_utf8_lossy(&sortie.stderr),"transcription en échec");
            Err(AppError::Invalid(
                "Transcription impossible. Réessayez la dictée.".into(),
            ))
        }
        Err(erreur) => {
            tracing::error!(%erreur, binaire=%moteur.binaire, "moteur de transcription introuvable");
            Err(AppError::Invalid(
                "Moteur de transcription indisponible sur le serveur.".into(),
            ))
        }
    }
}

/// Longueur dominante des numéros de travail du cheptel. C'est elle qui
/// permet de séparer le numéro de la quantité quand les deux sont dictés
/// d'affilée ; un élevage aux numéros courts n'a rien à paramétrer.
async fn longueur_numeros(pool: &SqlitePool) -> AppResult<usize> {
    let longueur: Option<i64> = sqlx::query_scalar(
        "SELECT length(num_travail) FROM truie WHERE reformee=0 AND num_travail<>'' GROUP BY length(num_travail) ORDER BY COUNT(*) DESC,length(num_travail) DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(longueur.unwrap_or(0).max(0) as usize)
}

async fn causes_configurees(pool: &SqlitePool) -> AppResult<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT libelle FROM causeperte ORDER BY libelle COLLATE NOCASE")
            .fetch_all(pool)
            .await?,
    )
}

/// Ce que l'application a besoin de connaître avant la première dictée :
/// le jeton anti-rejeu à renvoyer lors de la validation, les causes que
/// l'élevage accepte, et si le moteur de transcription est bien installé.
pub(super) async fn contexte(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
) -> AppResult<axum::Json<Value>> {
    Ok(axum::Json(json!({
        "csrf_token": session.csrf,
        "causes": causes_configurees(&state.pool).await?,
        "longueur_numeros": longueur_numeros(&state.pool).await?,
        "transcription_disponible": Moteur::depuis_env().is_some(),
        "retention_audio_jours": vocal::RETENTION_AUDIO_JOURS,
        "peut_enregistrer": session.peut_modifier(),
    })))
}

/// Contexte de la portée en cours, tel qu'il sera relu à l'écran. Afficher
/// le nombre de porcelets présents à côté de la perte annoncée est ce qui
/// permet de voir l'erreur avant de valider, pas après.
async fn contexte_truie(pool: &SqlitePool, truie: i64) -> AppResult<Value> {
    let ligne = sqlx::query(
        "SELECT t.id,t.num_travail,t.bande_code,p.id AS portee_id,p.bande_id,p.date AS date_mise_bas,p.presents,e.nes_vifs,p.adoptes,p.retires,p.pertes,p.cloturee FROM truie t LEFT JOIN portee_effectif p ON p.truie_id=t.id AND p.cloturee=0 AND p.date<=date('now') LEFT JOIN evenement e ON e.id=p.id WHERE t.id=? ORDER BY p.date DESC,p.id DESC LIMIT 1",
    )
    .bind(truie)
    .fetch_optional(pool)
    .await?;
    Ok(ligne
        .map(|ligne| rows_to_json(vec![ligne]))
        .transpose()?
        .and_then(|lignes| lignes.into_iter().next())
        .unwrap_or(Value::Null))
}

/// Première étape : comprendre, sans rien enregistrer dans l'élevage.
///
/// Accepte un fichier `audio` (transcrit ici) ou directement un champ
/// `texte` — l'application peut ainsi rejouer une dictée déjà transcrite, et
/// la fonction reste utilisable sur un serveur sans moteur installé.
pub(super) async fn analyser(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    mut multipart: Multipart,
) -> AppResult<axum::Json<Value>> {
    require_writer(&session)?;
    let mut audio: Option<Vec<u8>> = None;
    let mut mime: Option<String> = None;
    let mut texte: Option<String> = None;
    while let Some(champ) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
    {
        match champ.name().unwrap_or_default() {
            "audio" => {
                mime = champ.content_type().map(str::to_string);
                let donnees = champ
                    .bytes()
                    .await
                    .map_err(|e| AppError::Invalid(e.to_string()))?;
                if donnees.len() > TAILLE_MAX_AUDIO {
                    return Err(AppError::Invalid("Enregistrement trop long.".into()));
                }
                if !donnees.is_empty() {
                    audio = Some(donnees.to_vec());
                }
            }
            "texte" => {
                texte = champ
                    .text()
                    .await
                    .ok()
                    .map(|valeur| valeur.trim().to_string())
                    .filter(|valeur| !valeur.is_empty())
            }
            _ => {}
        }
    }

    let texte_brut = match (texte, audio.as_ref()) {
        (Some(texte), _) => texte,
        (None, Some(audio)) => {
            let moteur = Moteur::depuis_env().ok_or_else(|| {
                AppError::Invalid(
                    "Transcription non configurée sur ce serveur (EO_VOCAL_WHISPER, EO_VOCAL_MODELE).".into(),
                )
            })?;
            transcrire(&moteur, audio).await?
        }
        (None, None) => return Err(AppError::Invalid("Aucun énoncé reçu.".into())),
    };

    let causes = causes_configurees(&state.pool).await?;
    let longueur = longueur_numeros(&state.pool).await?;
    let analyse = vocal::analyser(&texte_brut, &causes, longueur);

    // Rapprochement du cheptel : le numéro compris existe-t-il, et sinon
    // quels numéros voisins proposer d'un seul geste à l'écran.
    let mut truie = Value::Null;
    let mut suggestions: Vec<String> = Vec::new();
    let mut truie_id: Option<i64> = None;
    if let Some(numero) = analyse.num_truie.as_ref() {
        truie_id = sqlx::query_scalar(
            "SELECT id FROM truie WHERE num_travail=? AND reformee=0 ORDER BY id DESC LIMIT 1",
        )
        .bind(numero)
        .fetch_optional(&state.pool)
        .await?;
        match truie_id {
            Some(id) => truie = contexte_truie(&state.pool, id).await?,
            None => {
                let cheptel: Vec<String> = sqlx::query_scalar(
                    "SELECT num_travail FROM truie WHERE reformee=0 AND num_travail<>''",
                )
                .fetch_all(&state.pool)
                .await?;
                suggestions = vocal::voisins(numero, &cheptel, VOISINS_PROPOSES);
            }
        }
    }

    let analyse_json = serde_json::to_string(&analyse).unwrap_or_default();
    let saisie_id: i64 = sqlx::query_scalar(
        "INSERT INTO saisievocale(utilisateur,audio,audio_mime,texte_brut,analyse_json,statut,truie_id) VALUES(?,?,?,?,?,?,?) RETURNING id",
    )
    .bind(&session.nom)
    .bind(audio)
    .bind(mime)
    .bind(&texte_brut)
    .bind(&analyse_json)
    .bind(if analyse.annulation {
        "annulee"
    } else {
        "analysee"
    })
    .bind(truie_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(axum::Json(json!({
        "saisie_id": saisie_id,
        "analyse": analyse,
        "truie_trouvee": truie_id.is_some(),
        "truie": truie,
        "suggestions": suggestions,
        "csrf_token": session.csrf,
    })))
}

/// Seconde étape : enregistrer ce que l'éleveur a relu et validé.
///
/// Les valeurs viennent de l'écran de confirmation, pas de l'analyse : si la
/// transcription s'est trompée et que la correction a été faite à l'écran,
/// c'est la correction qui compte. L'enregistrement passe par le même chemin
/// que la saisie manuelle, donc par les mêmes contrôles métier.
pub(super) async fn valider(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<axum::Json<Value>> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let saisie = form_i64(&form, "saisie_id")
        .ok_or_else(|| AppError::Invalid("Saisie vocale inconnue.".into()))?;
    let truie = form_i64(&form, "truie_id")
        .ok_or_else(|| AppError::Invalid("Sélectionnez la truie concernée.".into()))?;
    let statut: Option<String> = sqlx::query_scalar("SELECT statut FROM saisievocale WHERE id=?")
        .bind(saisie)
        .fetch_optional(&state.pool)
        .await?;
    match statut.as_deref() {
        None => return Err(AppError::NotFound),
        // Une double validation créerait deux pertes pour un seul événement :
        // le second appui sur « valider » ne doit rien réécrire.
        Some("validee") => {
            return Err(AppError::Invalid("Cette dictée a déjà été validée.".into()))
        }
        Some(_) => {}
    }

    maternite_suivi::enregistrer_perte(&state.pool, truie, None, &form, Some(28)).await?;
    let perte: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM perteporcelet WHERE truie_id=? ORDER BY id DESC LIMIT 1",
    )
    .bind(truie)
    .fetch_optional(&state.pool)
    .await?;
    sqlx::query("UPDATE saisievocale SET statut='validee',truie_id=?,perte_id=?,valide_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(truie)
        .bind(perte)
        .bind(saisie)
        .execute(&state.pool)
        .await?;
    Ok(axum::Json(json!({
        "enregistre": true,
        "perte_id": perte,
        "truie": contexte_truie(&state.pool, truie).await?,
    })))
}

/// Dictée écartée à la relecture. On garde la trace : ce sont ces lignes-là
/// qu'on relit pour corriger le vocabulaire reconnu.
pub(super) async fn abandonner(
    State(state): State<AppState>,
    Extension(session): Extension<SessionData>,
    Path(id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> AppResult<axum::Json<Value>> {
    require_writer(&session)?;
    verify_csrf(&session, &form)?;
    let touchees =
        sqlx::query("UPDATE saisievocale SET statut='abandonnee' WHERE id=? AND statut<>'validee'")
            .bind(id)
            .execute(&state.pool)
            .await?
            .rows_affected();
    if touchees == 0 {
        return Err(AppError::NotFound);
    }
    Ok(axum::Json(json!({"abandonnee": true})))
}

/// Les dernières dictées, pour comprendre ce qui a été mal compris.
pub(super) async fn journal(State(state): State<AppState>) -> AppResult<axum::Json<Value>> {
    let lignes = generic_rows(
        &state.pool,
        "SELECT s.id,s.created_at,s.utilisateur,s.texte_brut,s.statut,s.truie_id,t.num_travail,s.audio IS NOT NULL AS audio_disponible FROM saisievocale s LEFT JOIN truie t ON t.id=s.truie_id ORDER BY s.created_at DESC,s.id DESC LIMIT 100",
    )
    .await?;
    Ok(axum::Json(json!({"saisies": lignes})))
}

/// Réécoute d'un enregistrement conservé. Passé le délai de rétention,
/// l'audio n'existe plus : seul le texte reste consultable.
pub(super) async fn audio(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Response> {
    let ligne: Option<(Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT audio,audio_mime FROM saisievocale WHERE id=?")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let (Some(audio), mime) = ligne.ok_or(AppError::NotFound)? else {
        return Err(AppError::NotFound);
    };
    let mime = mime.unwrap_or_else(|| "application/octet-stream".into());
    let entete = HeaderValue::from_str(&mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Ok(([(header::CONTENT_TYPE, entete)], audio).into_response())
}

/// Efface les enregistrements audio arrivés au terme de la rétention.
/// La ligne, elle, est conservée : le texte et l'analyse restent lisibles.
pub async fn purger_audio(pool: &SqlitePool) -> anyhow::Result<u64> {
    let effaces = sqlx::query(
        "UPDATE saisievocale SET audio=NULL,audio_purge_at=CURRENT_TIMESTAMP WHERE audio IS NOT NULL AND created_at < datetime('now',?)",
    )
    .bind(format!("-{} day", vocal::RETENTION_AUDIO_JOURS))
    .execute(pool)
    .await?
    .rows_affected();
    if effaces > 0 {
        tracing::info!(
            enregistrements = effaces,
            jours = vocal::RETENTION_AUDIO_JOURS,
            "audio des saisies vocales purgé"
        );
    }
    Ok(effaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::documents::tests::{session, state};

    /// Un élevage minimal : une truie en maternité, une portée de douze
    /// porcelets, une cause de perte configurée.
    async fn elevage() -> anyhow::Result<AppState> {
        let state = state().await;
        sqlx::raw_sql(include_str!("../../migrations/0004_saisie_vocale.sql"))
            .execute(&state.pool)
            .await?;
        sqlx::raw_sql("INSERT INTO bande(id,code) VALUES(1,'B1'); INSERT INTO truie(id,num_travail,bande_code) VALUES(1,'500325','B1'),(2,'500326','B1'); INSERT INTO evenement(id,type,date,truie_id,bande_id,nes_vifs) VALUES(1,'mise_bas',date('now','-3 day'),1,1,12); INSERT OR IGNORE INTO causeperte(libelle) VALUES('Écrasement');")
            .execute(&state.pool)
            .await?;
        Ok(state)
    }

    /// Envoi d'un énoncé déjà transcrit, comme le fera l'application quand
    /// elle rejoue une dictée ou quand la transcription est faite ailleurs.
    async fn enonce(texte: &str) -> Multipart {
        use axum::extract::FromRequest;
        let corps = format!(
            "--LIMITE\r\nContent-Disposition: form-data; name=\"texte\"\r\n\r\n{texte}\r\n--LIMITE--\r\n"
        );
        let requete = axum::http::Request::builder()
            .header("content-type", "multipart/form-data; boundary=LIMITE")
            .body(axum::body::Body::from(corps))
            .unwrap();
        Multipart::from_request(requete, &()).await.unwrap()
    }

    fn validation(saisie: i64, truie: i64, nb: &str) -> HashMap<String, String> {
        HashMap::from([
            ("csrf_token".into(), "test".into()),
            ("saisie_id".into(), saisie.to_string()),
            ("truie_id".into(), truie.to_string()),
            ("nb".into(), nb.into()),
            ("cause".into(), "Écrasement".into()),
        ])
    }

    #[tokio::test]
    async fn l_analyse_n_ecrit_rien_dans_l_elevage_avant_validation() -> anyhow::Result<()> {
        let state = elevage().await?;
        let axum::Json(reponse) = analyser(
            State(state.clone()),
            Extension(session()),
            enonce("truie cinq zéro zéro trois deux cinq deux porcelets écrasés").await,
        )
        .await?;

        assert_eq!(reponse["truie_trouvee"], json!(true));
        assert_eq!(reponse["analyse"]["num_truie"], json!("500325"));
        assert_eq!(reponse["analyse"]["quantite"], json!(2));
        assert_eq!(reponse["analyse"]["cause"], json!("Écrasement"));
        // Le contexte relu à l'écran doit porter l'effectif présent, c'est lui
        // qui rend une erreur visible avant l'enregistrement.
        assert_eq!(reponse["truie"]["presents"], json!(12));
        let pertes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM perteporcelet")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(pertes, 0, "aucune écriture métier avant la validation");

        let saisie = reponse["saisie_id"].as_i64().unwrap();
        let axum::Json(enregistrement) = valider(
            State(state.clone()),
            Extension(session()),
            Form(validation(saisie, 1, "2")),
        )
        .await?;
        assert_eq!(enregistrement["enregistre"], json!(true));
        let (nb, cause): (i64, String) =
            sqlx::query_as("SELECT nb,cause FROM perteporcelet ORDER BY id DESC LIMIT 1")
                .fetch_one(&state.pool)
                .await?;
        assert_eq!((nb, cause.as_str()), (2, "Écrasement"));
        let presents: i64 = sqlx::query_scalar("SELECT presents FROM portee_effectif WHERE id=1")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(presents, 10);
        Ok(())
    }

    #[tokio::test]
    async fn une_meme_dictee_ne_peut_pas_etre_validee_deux_fois() -> anyhow::Result<()> {
        let state = elevage().await?;
        let axum::Json(reponse) = analyser(
            State(state.clone()),
            Extension(session()),
            enonce("truie 500325 deux porcelets écrasés").await,
        )
        .await?;
        let saisie = reponse["saisie_id"].as_i64().unwrap();
        let axum::Json(enregistrement) = valider(
            State(state.clone()),
            Extension(session()),
            Form(validation(saisie, 1, "2")),
        )
        .await?;
        assert_eq!(enregistrement["enregistre"], json!(true));
        assert!(valider(
            State(state.clone()),
            Extension(session()),
            Form(validation(saisie, 1, "2"))
        )
        .await
        .is_err());
        let total: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(nb),0) FROM perteporcelet")
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(total, 2);
        Ok(())
    }

    #[tokio::test]
    async fn un_numero_inconnu_propose_les_voisins_du_cheptel() -> anyhow::Result<()> {
        let state = elevage().await?;
        let axum::Json(reponse) = analyser(
            State(state.clone()),
            Extension(session()),
            enonce("truie 500327 un porcelet écrasé").await,
        )
        .await?;
        assert_eq!(reponse["truie_trouvee"], json!(false));
        assert_eq!(
            reponse["suggestions"],
            json!(["500325".to_string(), "500326".to_string()])
        );
        Ok(())
    }

    #[tokio::test]
    async fn la_validation_reste_soumise_aux_controles_de_la_portee() -> anyhow::Result<()> {
        let state = elevage().await?;
        let axum::Json(reponse) = analyser(
            State(state.clone()),
            Extension(session()),
            enonce("truie 500325 quinze porcelets écrasés").await,
        )
        .await?;
        let saisie = reponse["saisie_id"].as_i64().unwrap();
        // Quinze pertes sur une portée de douze : refusé comme à la main.
        assert!(valider(
            State(state.clone()),
            Extension(session()),
            Form(validation(saisie, 1, "15"))
        )
        .await
        .is_err());
        let statut: String = sqlx::query_scalar("SELECT statut FROM saisievocale WHERE id=?")
            .bind(saisie)
            .fetch_one(&state.pool)
            .await?;
        assert_eq!(statut, "analysee", "la dictée reste corrigeable");
        Ok(())
    }

    #[tokio::test]
    async fn l_audio_s_efface_au_terme_de_la_retention_mais_le_texte_reste() -> anyhow::Result<()> {
        let state = elevage().await?;
        sqlx::query("INSERT INTO saisievocale(id,created_at,audio,texte_brut) VALUES(1,datetime('now',?),X'00FF','ancienne'),(2,datetime('now','-1 day'),X'00FF','récente')")
            .bind(format!("-{} day", vocal::RETENTION_AUDIO_JOURS + 1))
            .execute(&state.pool)
            .await?;
        assert_eq!(purger_audio(&state.pool).await?, 1);
        let restants: Vec<(i64, Option<Vec<u8>>, String)> =
            sqlx::query_as("SELECT id,audio,texte_brut FROM saisievocale ORDER BY id")
                .fetch_all(&state.pool)
                .await?;
        assert_eq!(restants[0].1, None);
        assert_eq!(restants[0].2, "ancienne");
        assert!(restants[1].1.is_some());
        // Purge idempotente : rien de neuf à effacer au passage suivant.
        assert_eq!(purger_audio(&state.pool).await?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn la_longueur_des_numeros_suit_le_cheptel() -> anyhow::Result<()> {
        let state = elevage().await?;
        assert_eq!(longueur_numeros(&state.pool).await?, 6);
        sqlx::query("UPDATE truie SET num_travail='42' WHERE id IN (1,2)")
            .execute(&state.pool)
            .await?;
        assert_eq!(longueur_numeros(&state.pool).await?, 2);
        Ok(())
    }
}
