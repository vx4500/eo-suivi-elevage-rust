//! Analyse des ordres dictés depuis l'application mobile.
//!
//! Le principe retenu : la transcription est faite par un moteur externe
//! (whisper.cpp) qui rend du texte libre, et c'est ici qu'on transforme ce
//! texte en intention structurée. L'analyse est volontairement déterministe
//! — pas de modèle de langue — pour trois raisons : elle tient dans quelques
//! microsecondes, elle tourne sans réseau, et elle ne peut pas inventer un
//! numéro de truie qui n'a pas été prononcé.
//!
//! Rien de ce qui sort d'ici n'est écrit en base : l'appelant renvoie
//! l'analyse au téléphone pour relecture, et n'enregistre qu'après validation.

use serde::Serialize;

/// Durée de conservation des enregistrements audio, en jours. L'audio ne sert
/// qu'à comprendre après coup pourquoi une dictée a été mal comprise ; passé
/// ce délai il est effacé et seul le texte transcrit reste dans le journal.
pub const RETENTION_AUDIO_JOURS: i64 = 10;

/// Longueur minimale d'une racine commune pour rapprocher un mot dicté d'une
/// cause de perte configurée. En dessous, « mort » rapprocherait « mortier ».
const LONGUEUR_RACINE: usize = 5;

/// En dessous de cette longueur, un mot ne porte pas de sens exploitable :
/// articles, liaisons, restes de transcription.
const LONGUEUR_MOT_UTILE: usize = 3;

/// Mots qui annoncent le numéro de l'animal. Les chiffres qui suivent l'un
/// d'eux sont lus comme un numéro de travail, pas comme une quantité.
const MOTS_ANIMAL: [&str; 4] = ["truie", "cochette", "loge", "case"];

/// Mots qui annulent l'énoncé en cours. L'éleveur qui se trompe au cinquième
/// chiffre doit pouvoir repartir à zéro sans reprendre le téléphone.
const MOTS_ANNULATION: [&str; 6] = [
    "annule",
    "annuler",
    "annulation",
    "correction",
    "efface",
    "oublie",
];

/// Mots qui structurent l'énoncé et ne désignent jamais une cause. Sans ce
/// garde-fou, le « truie » qui annonce le numéro se rapprocherait de la cause
/// « Tué par la truie », qui contient le même mot.
const MOTS_STRUCTURE: [&str; 8] = [
    "truie", "cochette", "loge", "case", "porcelet", "porc", "bande", "salle",
];

/// Synonymes de terrain vers le vocabulaire des causes de perte. La table de
/// causes reste celle de l'élevage : on ne fait que rapprocher ce qui se dit
/// à l'oral de ce qui est écrit dans le paramétrage.
const SYNONYMES_CAUSE: [(&str, &str); 16] = [
    ("ecrase", "ecrasement"),
    ("ecraser", "ecrasement"),
    ("couche", "ecrasement"),
    ("chetif", "chetif"),
    ("maigre", "chetif"),
    ("faible", "chetif"),
    ("diarrhee", "diarrhee"),
    ("colique", "diarrhee"),
    ("boiterie", "boiterie"),
    ("splayleg", "splayleg"),
    ("ecart", "ecart de portee"),
    ("momifie", "momifie"),
    ("tue", "tue par la truie"),
    ("mordu", "tue par la truie"),
    ("respiratoire", "respiratoire"),
    ("subite", "mort subite"),
];

/// Ce que l'on a cru comprendre. Chaque champ optionnel non renseigné est
/// signalé dans `manques` : l'application mobile s'en sert pour demander
/// précisément ce qui reste à confirmer, plutôt que de rejeter la dictée.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Analyse {
    pub texte_brut: String,
    pub texte_normalise: String,
    pub num_truie: Option<String>,
    pub quantite: Option<i64>,
    /// Vrai quand la quantité n'a pas été prononcée et qu'on a retenu 1.
    pub quantite_deduite: bool,
    pub cause: Option<String>,
    pub annulation: bool,
    pub manques: Vec<String>,
    pub confiance: &'static str,
}

/// Minuscules, accents retirés, ponctuation transformée en séparateurs.
/// Whisper ponctue ses transcriptions et accentue de façon irrégulière : on
/// compare toujours sur cette forme, jamais sur le texte d'origine.
pub fn normaliser(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for caractere in texte.chars() {
        let remplacement = match caractere.to_ascii_lowercase() {
            'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            autre if autre.is_ascii_alphanumeric() => autre,
            autre if autre.is_lowercase() || autre.is_uppercase() => {
                autre.to_lowercase().next().unwrap_or(' ')
            }
            _ => ' ',
        };
        sortie.push(remplacement);
    }
    sortie.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Retire un pluriel simple pour comparer des racines : « porcelets » et
/// « porcelet » doivent se rejoindre.
fn sans_pluriel(mot: &str) -> &str {
    mot.strip_suffix('s').unwrap_or(mot)
}

/// Un chiffre isolé, tel qu'il ressort d'une dictée chiffre par chiffre.
/// Les variantes sont celles que Whisper produit réellement en français ;
/// on n'accepte ici que des formes sans ambiguïté avec un mot courant.
pub fn mot_vers_chiffre(mot: &str) -> Option<u8> {
    Some(match sans_pluriel(mot) {
        "zero" | "o" | "oh" => 0,
        "un" | "une" => 1,
        "deux" => 2,
        "trois" | "troi" => 3,
        "quatre" | "quatr" => 4,
        "cinq" | "cin" => 5,
        "six" | "sis" => 6,
        "sept" | "set" => 7,
        "huit" | "uit" => 8,
        "neuf" | "nef" => 9,
        _ => return None,
    })
}

/// Une quantité annoncée en toutes lettres. On monte jusqu'à vingt : au-delà
/// on ne perd pas des porcelets un par un, on saisit à l'écran.
fn mot_vers_nombre(mot: &str) -> Option<i64> {
    if let Some(chiffre) = mot_vers_chiffre(mot) {
        return Some(i64::from(chiffre));
    }
    Some(match sans_pluriel(mot) {
        "dix" => 10,
        "onze" => 11,
        "douze" => 12,
        "treize" => 13,
        "quatorze" => 14,
        "quinze" => 15,
        "seize" => 16,
        "vingt" => 20,
        _ => return None,
    })
}

/// Suite de chiffres relevée dans l'énoncé, avec sa position en jetons.
#[derive(Debug, Clone)]
struct Segment {
    position: usize,
    chiffres: String,
}

/// Regroupe les chiffres consécutifs. Whisper agrège de façon imprévisible :
/// « cinq zéro zéro » peut sortir en « 500 », en « 5 0 0 », ou en mélange.
/// En concaténant les jetons voisins, les trois formes donnent la même suite.
fn segments(jetons: &[&str], exclue: Option<usize>) -> Vec<Segment> {
    let mut trouves: Vec<Segment> = Vec::new();
    let mut courant: Option<Segment> = None;
    for (position, jeton) in jetons.iter().enumerate() {
        // Le jeton qui porte la quantité est traité comme un mot ordinaire :
        // il ne rejoint pas le numéro et coupe la suite de chiffres.
        let chiffres = if exclue == Some(position) {
            None
        } else if jeton.chars().all(|c| c.is_ascii_digit()) {
            Some((*jeton).to_string())
        } else {
            mot_vers_chiffre(jeton).map(|chiffre| chiffre.to_string())
        };
        match chiffres {
            Some(chiffres) => match courant.as_mut() {
                Some(segment) => segment.chiffres.push_str(&chiffres),
                None => courant = Some(Segment { position, chiffres }),
            },
            None => {
                if let Some(segment) = courant.take() {
                    trouves.push(segment);
                }
            }
        }
    }
    if let Some(segment) = courant.take() {
        trouves.push(segment);
    }
    trouves
}

/// Position du jeton qui porte la quantité, quand elle est annoncée devant
/// l'objet compté : « deux porcelets écrasés ».
///
/// On ne retient qu'un jeton court — une perte se compte sur un ou deux
/// chiffres — et seulement si des chiffres le précèdent, faute de quoi on
/// prendrait le numéro de la truie pour un nombre de porcelets. Repérer cette
/// position avant tout le reste évite que la quantité soit avalée par le
/// numéro lorsque Whisper colle les deux.
fn position_quantite(jetons: &[&str]) -> Option<usize> {
    let objet = jetons
        .iter()
        .position(|j| matches!(sans_pluriel(j), "porcelet" | "porc"))?;
    let candidat = objet.checked_sub(1)?;
    let jeton = jetons[candidat];
    let court = jeton.chars().all(|c| c.is_ascii_digit()) && jeton.len() <= 2
        || mot_vers_nombre(jeton).is_some() && !jeton.chars().all(|c| c.is_ascii_digit());
    let chiffres_avant = jetons[..candidat]
        .iter()
        .any(|j| j.chars().all(|c| c.is_ascii_digit()) || mot_vers_chiffre(j).is_some());
    (court && mot_vers_nombre(jeton).is_some_and(|n| n > 0) && chiffres_avant).then_some(candidat)
}

/// Rapproche un jeton d'une cause de perte configurée dans l'élevage.
/// On compare des racines : « écrasés » retrouve « Écrasement ».
fn reconnaitre_cause(jeton: &str, causes_normalisees: &[(String, String)]) -> Option<String> {
    let racine = sans_pluriel(jeton);
    if racine.len() < LONGUEUR_MOT_UTILE {
        return None;
    }
    // Un mot du vocabulaire de terrain est reconnu même court — « tué » fait
    // trois lettres. En dehors de cette table, on exige une racine assez
    // longue pour ne pas rapprocher deux mots par leur seul début.
    let synonyme = SYNONYMES_CAUSE
        .iter()
        .find(|(oral, _)| racine.starts_with(oral) || oral.starts_with(racine))
        .map(|(_, canonique)| *canonique);
    let cible = match synonyme {
        Some(canonique) => canonique,
        None if racine.len() >= LONGUEUR_RACINE => racine,
        None => return None,
    };
    // Une cause en plusieurs mots se compare en entier : « tue par la truie »
    // ne se ramène pas à la racine de son premier mot.
    if cible.contains(' ') {
        return causes_normalisees
            .iter()
            .find(|(normalisee, _)| {
                normalisee.contains(cible) || cible.contains(normalisee.as_str())
            })
            .map(|(_, libelle)| libelle.clone());
    }
    causes_normalisees
        .iter()
        .find(|(normalisee, _)| {
            normalisee.split(' ').any(|mot| {
                let mot = sans_pluriel(mot);
                mot.len() >= LONGUEUR_RACINE
                    && (mot.starts_with(&cible[..cible.len().min(LONGUEUR_RACINE)])
                        || cible.starts_with(&mot[..mot.len().min(LONGUEUR_RACINE)]))
            })
        })
        .map(|(_, libelle)| libelle.clone())
}

/// Analyse un énoncé transcrit.
///
/// `causes` est la liste des causes de perte de l'élevage : on ne renvoie
/// jamais une cause qui n'y figure pas, l'enregistrement la refuserait.
/// `longueur_num` est la longueur habituelle des numéros de travail dans le
/// cheptel ; elle sert à séparer le numéro de la quantité quand les deux se
/// suivent, comme dans « truie 500325 deux porcelets écrasés ».
pub fn analyser(texte: &str, causes: &[String], longueur_num: usize) -> Analyse {
    let texte_normalise = normaliser(texte);
    let jetons: Vec<&str> = texte_normalise
        .split(' ')
        .filter(|j| !j.is_empty())
        .collect();
    let causes_normalisees: Vec<(String, String)> = causes
        .iter()
        .map(|libelle| (normaliser(libelle), libelle.clone()))
        .collect();

    let mut analyse = Analyse {
        texte_brut: texte.trim().to_string(),
        texte_normalise: texte_normalise.clone(),
        ..Default::default()
    };

    if jetons.iter().any(|j| MOTS_ANNULATION.contains(j)) {
        analyse.annulation = true;
        analyse.confiance = "annulation";
        return analyse;
    }

    let position_quantite = position_quantite(&jetons);
    let quantite_annoncee = position_quantite.and_then(|index| mot_vers_nombre(jetons[index]));
    let suites = segments(&jetons, position_quantite);
    // Le numéro suit le mot qui désigne l'animal ; à défaut, on prend la
    // première suite de chiffres assez longue pour être un numéro.
    let apres_animal = jetons
        .iter()
        .position(|j| MOTS_ANIMAL.contains(&sans_pluriel(j)))
        .and_then(|index| suites.iter().find(|s| s.position > index));
    let porteur = apres_animal.or_else(|| {
        suites
            .iter()
            .find(|s| s.chiffres.len() >= longueur_num.max(2))
    });

    let mut quantite: Option<i64> = None;
    if let Some(segment) = porteur {
        if segment.chiffres.len() > longueur_num && longueur_num > 0 {
            // Numéro et quantité prononcés d'affilée : la longueur du numéro
            // tranche. Le reliquat est la quantité.
            let (numero, reste) = segment.chiffres.split_at(longueur_num);
            analyse.num_truie = Some(numero.to_string());
            quantite = reste.parse().ok().filter(|n| *n > 0);
        } else {
            analyse.num_truie = Some(segment.chiffres.clone());
            quantite = suites
                .iter()
                .find(|s| s.position > segment.position)
                .and_then(|s| s.chiffres.parse().ok())
                .filter(|n: &i64| *n > 0);
        }
    }

    // La quantité annoncée devant l'objet compté prime sur toute déduction
    // faite à partir de la longueur du numéro.
    analyse.quantite = quantite_annoncee.filter(|n| *n > 0).or(quantite);

    analyse.cause = jetons
        .iter()
        .filter(|jeton| !MOTS_STRUCTURE.contains(&sans_pluriel(jeton)))
        .find_map(|jeton| reconnaitre_cause(jeton, &causes_normalisees));

    // Une perte annoncée sans nombre en vaut une : c'est ainsi qu'on le dit.
    // Le drapeau permet à l'écran de confirmation de le signaler.
    if analyse.quantite.is_none() && analyse.cause.is_some() {
        analyse.quantite = Some(1);
        analyse.quantite_deduite = true;
    }

    if analyse.num_truie.is_none() {
        analyse.manques.push("numero".into());
    }
    if analyse.cause.is_none() {
        analyse.manques.push("cause".into());
    }
    if analyse.quantite.is_none() {
        analyse.manques.push("quantite".into());
    }

    let bonne_longueur = analyse
        .num_truie
        .as_ref()
        .is_some_and(|numero| numero.len() == longueur_num);
    analyse.confiance = if analyse.manques.is_empty() && bonne_longueur && !analyse.quantite_deduite
    {
        "haute"
    } else if analyse.manques.is_empty() {
        "moyenne"
    } else {
        "basse"
    };
    analyse
}

/// Distance d'édition, pour proposer un numéro voisin quand celui qui a été
/// compris n'existe pas au cheptel. Un chiffre mal transcrit ne doit pas
/// obliger à tout redicter.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut ligne: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut precedent = ligne[0];
        ligne[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = precedent + usize::from(ca != cb);
            precedent = ligne[j + 1];
            ligne[j + 1] = substitution.min(ligne[j] + 1).min(ligne[j + 1] + 1);
        }
    }
    ligne[b.len()]
}

/// Les numéros du cheptel les plus proches de celui qui a été compris,
/// classés du plus proche au plus lointain, écarts trop grands exclus.
pub fn voisins(numero: &str, cheptel: &[String], maximum: usize) -> Vec<String> {
    let mut classes: Vec<(usize, &String)> = cheptel
        .iter()
        .map(|candidat| (distance(numero, candidat), candidat))
        .filter(|(ecart, _)| *ecart > 0 && *ecart <= 2)
        .collect();
    classes.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    classes
        .into_iter()
        .take(maximum)
        .map(|(_, numero)| numero.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn causes() -> Vec<String> {
        vec![
            "Écrasement".to_string(),
            "Chétif".to_string(),
            "Diarrhée".to_string(),
            "Écart de portée".to_string(),
        ]
    }

    #[test]
    fn enonce_complet_dicte_chiffre_par_chiffre() {
        let analyse = analyser(
            "truie cinq zéro zéro trois deux cinq, deux porcelets écrasés",
            &causes(),
            6,
        );
        assert_eq!(analyse.num_truie.as_deref(), Some("500325"));
        assert_eq!(analyse.quantite, Some(2));
        assert_eq!(analyse.cause.as_deref(), Some("Écrasement"));
        assert_eq!(analyse.confiance, "haute");
        assert!(analyse.manques.is_empty());
    }

    #[test]
    fn whisper_agrege_les_chiffres_en_nombre() {
        // Même énoncé, transcription agrégée : le résultat doit être identique.
        let analyse = analyser("truie 500325 2 porcelets écrasés", &causes(), 6);
        assert_eq!(analyse.num_truie.as_deref(), Some("500325"));
        assert_eq!(analyse.quantite, Some(2));
        assert_eq!(analyse.cause.as_deref(), Some("Écrasement"));
    }

    #[test]
    fn transcription_partiellement_agregee() {
        let analyse = analyser("truie 500 3 25 deux porcelets écrasés", &causes(), 6);
        assert_eq!(analyse.num_truie.as_deref(), Some("500325"));
        assert_eq!(analyse.quantite, Some(2));
    }

    #[test]
    fn quantite_non_prononcee_vaut_un_et_est_signalee() {
        let analyse = analyser("truie 500325 porcelet écrasé", &causes(), 6);
        assert_eq!(analyse.quantite, Some(1));
        assert!(analyse.quantite_deduite);
        assert_eq!(analyse.confiance, "moyenne");
    }

    #[test]
    fn numero_trop_court_reste_exploitable_mais_peu_sur() {
        let analyse = analyser("truie 5003 deux porcelets écrasés", &causes(), 6);
        assert_eq!(analyse.num_truie.as_deref(), Some("5003"));
        assert_eq!(analyse.confiance, "moyenne");
    }

    #[test]
    fn cause_absente_du_parametrage_nest_pas_inventee() {
        let analyse = analyser("truie 500325 deux porcelets boiteux", &causes(), 6);
        assert_eq!(analyse.cause, None);
        assert!(analyse.manques.contains(&"cause".to_string()));
        assert_eq!(analyse.confiance, "basse");
    }

    #[test]
    fn annulation_interrompt_l_analyse() {
        let analyse = analyser("truie cinq zéro annule", &causes(), 6);
        assert!(analyse.annulation);
        assert_eq!(analyse.num_truie, None);
    }

    #[test]
    fn synonymes_de_terrain() {
        let analyse = analyser("truie 500325 trois porcelets couchés", &causes(), 6);
        assert_eq!(analyse.cause.as_deref(), Some("Écrasement"));
        assert_eq!(analyse.quantite, Some(3));
    }

    #[test]
    fn voisins_proposes_sur_un_chiffre_d_ecart() {
        let cheptel = vec![
            "500325".to_string(),
            "500326".to_string(),
            "500425".to_string(),
            "412200".to_string(),
        ];
        let proches = voisins("500326", &cheptel, 3);
        assert_eq!(proches, vec!["500325".to_string(), "500425".to_string()]);
    }

    /// Les causes livrées avec le schéma. « Tué par la truie » contient le mot
    /// qui annonce le numéro : le vocabulaire de structure ne doit jamais être
    /// pris pour une cause.
    #[test]
    fn le_mot_qui_annonce_l_animal_n_est_pas_une_cause() {
        let livrees: Vec<String> = [
            "Écrasement",
            "Chétif / non conforme",
            "Tué par la truie",
            "Diarrhée",
            "Respiratoire",
            "Mort subite",
            "Boiterie",
            "Autre",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        for (enonce, attendue) in [
            ("truie 500325 deux porcelets écrasés", "Écrasement"),
            (
                "truie 500325 un porcelet tué par la truie",
                "Tué par la truie",
            ),
            ("truie 500325 trois porcelets diarrhée", "Diarrhée"),
            ("truie 500325 un porcelet chétif", "Chétif / non conforme"),
            ("truie 500325 un porcelet mort subite", "Mort subite"),
        ] {
            let analyse = analyser(enonce, &livrees, 6);
            assert_eq!(
                analyse.cause.as_deref(),
                Some(attendue),
                "énoncé : {enonce}"
            );
            assert_eq!(analyse.num_truie.as_deref(), Some("500325"));
        }
    }

    #[test]
    fn normalisation_sans_accents_ni_ponctuation() {
        assert_eq!(
            normaliser("Truie 500325, écrasés !"),
            "truie 500325 ecrases"
        );
    }
}
