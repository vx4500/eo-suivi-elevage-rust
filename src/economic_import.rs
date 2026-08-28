use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_PDF_PAGES: usize = 30;
const MAX_DECOMPRESSED_PAGE: usize = 2 * 1024 * 1024;

/// Motif d'une ligne produit aliment Cooperl (silo, tonnage, PU, montant).
/// Partagé entre la détection du type de document et son analyse : certains
/// bordereaux (constaté sur des factures récentes) n'exposent plus l'en-tête
/// de tableau « Désignation produit / Silos » dans le texte extrait par
/// lopdf — probablement du texte positionné hors flux principal — alors que
/// les lignes produit elles-mêmes restent extraites normalement.
const ALIMENT_ROW_PATTERN: &str = r"(?m)^(.+?)\s+(MI|FE|GR|FM|\([0-9]+\))\s+(?:([0-9]{1,3})\s+)?([0-9]+[.,][0-9]+)(-?)\s*\*?\s+(?:[0-9.,]+\s+)?([0-9.,]+)\s+[0-9]+\s+([0-9.,]+)(-?)\s*$";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportLine {
    pub kind: String,
    pub date: Option<String>,
    pub reference: Option<String>,
    pub label: String,
    pub quantity: Option<f64>,
    pub unit_price: Option<f64>,
    pub amount: Option<f64>,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone)]
pub struct ImportDocument {
    pub document_type: String,
    pub lines: Vec<ImportLine>,
    pub warnings: Vec<String>,
}

pub fn extract_pdf_text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.starts_with(b"%PDF-") {
        return Err("Le fichier ne possède pas une signature PDF valide".into());
    }
    let document = lopdf::Document::load_mem(bytes)
        .or_else(|_| match repair_startxref_offset(bytes) {
            Some(patched) => lopdf::Document::load_mem(&patched),
            None => lopdf::Document::load_mem(bytes),
        })
        .map_err(|_| "Le PDF est illisible, chiffré ou endommagé".to_string())?;
    let pages: Vec<u32> = document.get_pages().keys().copied().collect();
    if pages.is_empty() {
        return Err("Le PDF ne contient aucune page".into());
    }
    if pages.len() > MAX_PDF_PAGES {
        return Err(format!(
            "Le PDF contient trop de pages (maximum {MAX_PDF_PAGES})"
        ));
    }
    let text = document
        .extract_text_with_limit(&pages, MAX_DECOMPRESSED_PAGE)
        .map_err(|_| "Le contenu du PDF est illisible ou trop volumineux".to_string())?;
    if text.trim().chars().count() < 40 {
        return Err(
            "Ce PDF semble être un scan sans texte. L'OCR des photos sera ajouté dans une prochaine version"
                .into(),
        );
    }
    Ok(text)
}

/// Certains logiciels d'export (constaté avec « LDPRX » sur un bordereau
/// Cooperl) écrivent un octet `startxref` qui ne pointe pas exactement sur le
/// mot-clé `xref` — probablement un bug de génération sur leur normalisation
/// des fins de ligne. lopdf refuse alors le fichier entier
/// (`ParseError::InvalidTrailer`) alors que le contenu est par ailleurs
/// intact. Si la table `xref` classique existe bien dans le fichier, on
/// corrige uniquement le nombre après `startxref` pour qu'il pointe dessus,
/// et on retente le chargement. Ne traite pas les xref sous forme de flux
/// (PDF avec compression d'objets), qui n'ont pas de mot-clé `xref` littéral.
fn repair_startxref_offset(bytes: &[u8]) -> Option<Vec<u8>> {
    let start_marker = b"startxref";
    let start_pos = bytes
        .windows(start_marker.len())
        .rposition(|window| window == start_marker)?;
    let digits_start = bytes[start_pos + start_marker.len()..]
        .iter()
        .position(|byte| byte.is_ascii_digit())
        .map(|offset| start_pos + start_marker.len() + offset)?;
    let digits_end = bytes[digits_start..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map(|offset| digits_start + offset)?;
    let declared: usize = std::str::from_utf8(&bytes[digits_start..digits_end])
        .ok()?
        .parse()
        .ok()?;

    // Le mot-clé xref est toujours en tête de ligne ; on cherche la dernière
    // occurrence avant `startxref`, pour ignorer le "xref" contenu dans
    // "startxref" lui-même et dans le contenu des pages.
    let search_area = &bytes[..start_pos];
    let xref_pos = [b"\nxref\r\n".as_slice(), b"\nxref\n".as_slice()]
        .into_iter()
        .filter_map(|pattern| {
            search_area
                .windows(pattern.len())
                .rposition(|window| window == pattern)
                .map(|position| position + 1) // avance sur le '\n' de tête
        })
        .max()?;

    if xref_pos == declared {
        return None; // l'offset déclaré était déjà correct, rien à réparer
    }

    let mut patched = bytes.to_vec();
    patched.splice(digits_start..digits_end, xref_pos.to_string().into_bytes());
    Some(patched)
}

pub fn parse_document(text: &str) -> Result<ImportDocument, String> {
    let normalized = text
        .replace(['\u{2212}', '\u{2013}', '\u{2014}'], "-")
        .replace('\u{00a0}', " ");
    let upper = normalized.to_uppercase();
    if (upper.contains("AUTORENOUVELLEMENT") || is_semence(&upper))
        && !upper
            .split_whitespace()
            .collect::<String>()
            .contains("ANIMAUXREPRODUCTEURS")
    {
        return parse_semence(&normalized);
    }
    if upper.contains("SYNTHESE DES INDICES")
        || upper.contains("SYNTHÈSE DES INDICES")
        || (upper.contains("UNIPORC") && upper.contains("TMP") && upper.contains("TATOUAGE"))
        || upper.contains("BORDEREAU DE PESEE")
        || upper.contains("BORDEREAU DE PESÉE")
    {
        return parse_synthese(&normalized);
    }
    let compact: String = upper
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if compact.contains("ANIMAUXREPRODUCTEURS")
        || (compact.contains("COCHETTE") && compact.contains("REPRODUCTEURS"))
    {
        return parse_genetique(&normalized);
    }
    // Les duplicatas Cooperl les plus récents perdent le libellé « APPORT
    // DE » lors de l'extraction lopdf : restent toutefois PORCS
    // CHARCUTIERS, le numéro de bon et la semaine. Ne pas exiger un mot qui
    // n'existe visuellement que dans le fond de page du PDF.
    if upper.contains("CHARCUTIERS")
        && (upper.contains("APPORT") || (upper.contains("BON N") && upper.contains("SEMAINE N")))
    {
        return parse_apport(&normalized);
    }
    if upper.contains("PRODUITS VETERINAIRES") || upper.contains("PRODUITS VÉTÉRINAIRES") {
        return parse_veto(&normalized);
    }
    if upper.contains("ALIMENTS")
        && (upper.contains("SILOS")
            || upper.contains("DÉSIGNATION PRODUIT")
            || upper.contains("DESIGNATION PRODUIT")
            || Regex::new(ALIMENT_ROW_PATTERN).is_ok_and(|regex| regex.is_match(&normalized)))
    {
        return parse_aliment(&normalized);
    }
    Err("Type de document non reconnu. Formats pris en charge : aliment Cooperl, vétérinaire, semence, génétique, apport et synthèse Uniporc".into())
}

fn capture(text: &str, pattern: &str, group: usize) -> Option<String> {
    Regex::new(pattern)
        .ok()?
        .captures(text)?
        .get(group)
        .map(|value| value.as_str().trim().to_string())
}

fn captures_all(text: &str, pattern: &str, group: usize) -> Vec<String> {
    let Ok(regex) = Regex::new(pattern) else {
        return Vec::new();
    };
    regex
        .captures_iter(text)
        .filter_map(|captures| captures.get(group).map(|value| value.as_str().to_string()))
        .collect()
}

fn number(raw: &str) -> Option<f64> {
    let mut value = raw
        .trim()
        .replace(['\u{00a0}', '\u{202f}', ' ', '\t'], "")
        .replace(['\u{2212}', '\u{2013}', '\u{2014}'], "-")
        .replace(['(', ')'], "");
    let negative = value.starts_with('-') || value.ends_with('-') || raw.trim().starts_with('(');
    value = value.trim_matches('-').to_string();
    if value.contains(',') && value.contains('.') {
        if value.rfind(',') > value.rfind('.') {
            value = value.replace('.', "").replace(',', ".");
        } else {
            value = value.replace(',', "");
        }
    } else if value.contains(',') {
        value = value.replace(',', ".");
    }
    value
        .parse::<f64>()
        .ok()
        .map(|parsed| if negative { -parsed.abs() } else { parsed })
        .filter(|parsed| parsed.is_finite())
}

fn integer(raw: &str) -> Option<i64> {
    let value = number(raw)?.round();
    (value >= i64::MIN as f64 && value <= i64::MAX as f64).then_some(value as i64)
}

fn iso_date(raw: &str) -> Option<String> {
    let normalized = raw.trim().replace('.', "/");
    // `%Y` de chrono accepte aussi une année à 2 chiffres (il n'exige pas 4
    // chiffres à la lecture), donc "27/07/26" est déjà accepté par ce
    // premier format — silencieusement interprété comme l'an 26 et non
    // 2026. Un vrai bug trouvé en analysant de vrais bordereaux Cooperl, qui
    // écrivent systématiquement leurs dates sur deux chiffres : le seul test
    // existant avant cette page utilisait une année à 4 chiffres et ne
    // l'exerçait donc jamais. On applique le même correctif de siècle quel
    // que soit le format ayant matché — toutes les dates de l'application
    // appartiennent au 21e siècle.
    let date = NaiveDate::parse_from_str(&normalized, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(&normalized, "%d/%m/%y"))
        .ok()?;
    let year = if date.year() < 100 {
        date.year() + 2000
    } else {
        date.year()
    };
    NaiveDate::from_ymd_opt(year, date.month(), date.day())
        .map(|date| date.format("%Y-%m-%d").to_string())
}

fn document_sign(text: &str) -> f64 {
    let upper = text.to_uppercase();
    if Regex::new(r"\bAVOIR\b|NOTE\s+DE\s+CREDIT|NOTE\s+DE\s+CRÉDIT|CREDIT\s+NOTE")
        .is_ok_and(|regex| regex.is_match(&upper))
    {
        -1.0
    } else {
        1.0
    }
}

/// Contexte du bon auquel appartient une ligne, et non le premier site du PDF.
fn delivery_context(text: &str, end: usize) -> (Option<String>, Option<String>, Option<String>) {
    let prefix = &text[..end];
    let mut delivery = None;
    let mut order = None;
    let mut destination = None;
    for line in prefix.lines() {
        let upper = line.to_uppercase();
        if upper.contains("BON") && (upper.contains("LIVRAISON") || upper.contains("BON N")) {
            delivery = capture(line, r"(?i)du\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})", 1)
                .and_then(|v| iso_date(&v));
            order = None;
        }
        if upper.contains("COMMANDE") {
            order = capture(
                line,
                r"(?i)(?:du|date\s*:?)\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
                1,
            )
            .and_then(|v| iso_date(&v));
        }
        if upper.contains("LIVRÉ CHEZ") || upper.contains("LIVRE CHEZ") {
            destination = Some(line.trim().to_string());
        }
    }
    (delivery, order, destination)
}

fn parse_aliment(text: &str) -> Result<ImportDocument, String> {
    // Le numéro de facture est normalement précédé de « FACTURE N° », mais
    // certains bordereaux (constaté sur une facture aliment réelle, 38.pdf)
    // ne conservent que la forme abrégée « Fact.N° » à l'extraction lopdf —
    // le mot complet « FACTURE » disparaît du texte, probablement rendu hors
    // flux principal. « FACT(?:URE)? » couvre les deux cas.
    let reference = capture(
        text,
        r"(?i)FACT(?:URE)?\.?\s*N[°ºo:]*\s*([0-9][0-9. ]{4,})",
        1,
    )
    .map(|value| value.replace(['.', ' '], ""));
    let date = capture(
        text,
        r"(?is)Bon de livraison.*?du\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
        1,
    )
    .or_else(|| {
        capture(
            text,
            r"(?is)DATE\s*:\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
            1,
        )
    })
    .and_then(|value| iso_date(&value));
    let regex = Regex::new(ALIMENT_ROW_PATTERN)
        .map_err(|error| format!("analyse aliment indisponible: {error}"))?;
    let credit = document_sign(text);
    let mut lines = Vec::new();
    for row in regex.captures_iter(text) {
        let (delivery, order, destination) = delivery_context(text, row.get(0).unwrap().start());
        let product = format!(
            "{} {}",
            row[1].split_whitespace().collect::<Vec<_>>().join(" "),
            &row[2]
        );
        let line_sign = if !row[5].is_empty() || !row[8].is_empty() {
            -1.0
        } else {
            credit
        };
        let tonnage = number(&row[4]).map(|value| value.abs() * line_sign);
        let unit_price = number(&row[6]);
        let amount = number(&row[7]).map(|value| value.abs() * line_sign);
        lines.push(ImportLine {
            kind: "aliment".into(),
            date: delivery.clone().or_else(||date.clone()),
            reference: reference.clone(),
            label: product.clone(),
            quantity: tonnage,
            unit_price,
            amount,
            details: json!({
                "date_commande":order,"date_livraison":delivery,"destination":destination,"source_ligne":row.get(0).unwrap().start(),
                "fournisseur": "Cooperl Nutrition",
                "produit": product,
                "silo": row.get(3).map(|v|v.as_str()),
                "presentation": row[2].to_string(),
                "tonnage": tonnage,
                "pu_ht": unit_price,
                "montant_ht": amount,
                "num_facture": reference,
            }),
        });
    }
    finish_document("aliment", lines, reference, date)
}

fn parse_veto(text: &str) -> Result<ImportDocument, String> {
    let reference = capture(text, r"(?i)ACTURE\s*N[°ºo]?\s*([0-9.]{6,})", 1)
        .or_else(|| {
            capture(
                text,
                r"(?m)^\s*(20\.[0-9]{4,8}|20[0-9]{5}|[0-9]{12})\s*$",
                1,
            )
        })
        .map(|v| v.replace('.', ""));
    let date = capture(text, r"(?i)\bLE\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})", 1)
        .or_else(|| capture(text, r"(?i)FACT\.?\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})", 1))
        .and_then(|value| iso_date(&value));
    let regex = Regex::new(
        r"(?m)^\s*([0-9]+)\s+([A-ZÀ-ÖØ-Þ].+?)\s+[1-9]\s+[0-9 ]+?\s+([0-9.,]+)\s+([0-9.,]+)(-?)\s*$",
    )
    .map_err(|error| format!("analyse vétérinaire indisponible: {error}"))?;
    let credit = document_sign(text);
    let mut lines = Vec::new();
    for row in regex.captures_iter(text) {
        let (delivery, order, destination) = delivery_context(text, row.get(0).unwrap().start());
        let product = row[2].trim();
        if product.to_uppercase().contains("REMISE") {
            continue;
        }
        let line_sign = if !row[5].is_empty() { -1.0 } else { credit };
        let quantity = number(&row[1]);
        let unit_price = number(&row[3]);
        let amount = number(&row[4]).map(|value| value.abs() * line_sign);
        let label = if line_sign < 0.0 {
            format!("AVOIR — {product}")
        } else {
            product.to_string()
        };
        lines.push(ImportLine {
            kind: "veto".into(),
            date: delivery.clone().or_else(||date.clone()),
            reference: reference.clone(),
            label: label.clone(),
            quantity,
            unit_price,
            amount,
            details: json!({
                "date_commande":order,"date_livraison":delivery,"destination":destination,"source_ligne":row.get(0).unwrap().start(),
                "fournisseur": "Cooperl",
                "produit": label,
                "quantite": quantity,
                "pu_ht": unit_price,
                "montant_ht": amount,
                "num_facture": reference,
            }),
        });
    }
    finish_document("vétérinaire", lines, reference, date)
}

fn is_semence(upper: &str) -> bool {
    [
        "YXIA",
        "LB-CIA",
        "LB CIA",
        "PIETRAIN",
        "PIÉTRAIN",
        "DOSE IA",
        "DANBRED",
        "BLISTER LIFE",
        "SEMENCE",
        "REDEVANCE GEN",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn parse_semence(text: &str) -> Result<ImportDocument, String> {
    let upper = text.to_uppercase();
    let provider = if upper.contains("YXIA") {
        "Yxia"
    } else if upper.contains("LB-CIA")
        || upper.contains("LB CIA")
        || upper.contains("PIETRAIN")
        || upper.contains("PIÉTRAIN")
    {
        "LB-CIA Piétrain"
    } else if upper.contains("DANBRED") {
        "DanBred"
    } else {
        "Semence"
    };
    let reference = capture(text, r"(?i)\b(FAC[0-9A-Z-]+)\b", 1)
        .or_else(|| capture(text, r"(?i)NUM[ÉE]RO\s+DE\s+FACTURE\s+([A-Z0-9-]+)", 1));
    let date = capture(
        text,
        r"(?i)(?:DATE|FACTURE)\s*(?:DE|DU)?\s*[:.]?\s*([0-9]{2}[/.][0-9]{2}[/.][0-9]{2,4})",
        1,
    )
    .or_else(|| capture(text, r"([0-9]{2}[/.][0-9]{2}[/.][0-9]{2,4})", 1))
    .and_then(|value| iso_date(&value));
    let mut ht = labeled_amount(text, &["TOTAL HT"]);
    let mut ttc = labeled_amount(
        text,
        &["TOTAL TTC", "MONTANT TTC", "TTC A PAYER", "TTC À PAYER"],
    );
    let fee = upper.contains("AUTORENOUVELLEMENT");
    if fee {
        for line in text
            .lines()
            .filter(|line| line.to_uppercase().contains("AUTORENOUVELLEMENT"))
        {
            if let Some(value) = last_amount(line) {
                ht = Some(value.abs());
            }
        }
        if ttc.is_none() {
            ttc = capture(text, r"(?i)EUR\s+([0-9][0-9 .\u{202f}]*,[0-9]{2})", 1)
                .and_then(|value| number(&value));
        }
    }
    // Exception autorisée uniquement lorsque la facture indique elle-même
    // l'absence de TVA (micro-entreprise / franchise en base).
    if ht.is_none()
        && (upper.contains("MICRO-ENTREPRISE")
            || upper.contains("MICRO ENTREPRISE")
            || upper.contains("TVA NON APPLICABLE")
            || upper.contains("293 B"))
    {
        ht = ttc;
    }
    let mut doses = 0_i64;
    for line in text.lines() {
        let line_upper = line.to_uppercase();
        if line_upper.contains("BLISTER") {
            doses += capture(&line_upper, r"([0-9]+)\s*BLISTER", 1)
                .and_then(|value| integer(&value))
                .unwrap_or_default();
        } else if line_upper.contains("DOSE IA") {
            doses += capture(&line_upper, r"([0-9]+),00\b", 1)
                .and_then(|value| integer(&value))
                .unwrap_or_default();
        }
    }
    let mut label = if fee {
        "Redevance autorenouvellement (DanBred)".to_string()
    } else if upper.contains("DANBRED") {
        "Semence DanBred".to_string()
    } else if upper.contains("PIETRAIN") || upper.contains("PIÉTRAIN") || upper.contains("DOSE IA")
    {
        "Doses Piétrain (PN3)".to_string()
    } else {
        "Semence / doses IA".to_string()
    };
    if doses > 0 {
        label.push_str(&format!(" ({doses} doses)"));
    }
    let sign = document_sign(text);
    ht = ht.map(|value| value.abs() * sign);
    ttc = ttc.map(|value| value.abs() * sign);
    if sign < 0.0 {
        label = format!("AVOIR — {label}");
    }
    let line = ImportLine {
        kind: "semence".into(),
        date: date.clone(),
        reference: reference.clone(),
        label: label.clone(),
        quantity: (doses > 0).then_some(doses as f64),
        unit_price: None,
        amount: ht,
        details: json!({
            "fournisseur": provider,
            "designation": label,
            "nb_doses": (doses > 0).then_some(doses),
            "montant_ht": ht,
            "montant_ttc": ttc,
            "num_facture": reference,
        }),
    };
    finish_document("semence", vec![line], reference, date)
}

fn parse_genetique(text: &str) -> Result<ImportDocument, String> {
    let compact: String = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    // Sur le modèle « duplicata » réellement produit par Cooperl (le même
    // que pour les apports/factures d'aliment), les libellés « FACTURE N° »,
    // « NET A PAYER » et « BASE H.T. » font partie du fond de page (image),
    // pas du texte extrait : aucun ne s'y trouve jamais. Le seul repère
    // fiable est le numéro de document lui-même (« 14.41649 »), qui suit
    // « Semaine N° JJ.SS » sur la même ligne. Sans ce correctif, la
    // référence restait toujours vide (confondant toutes les factures et
    // tous les avoirs entre eux à l'import) et les montants n'étaient
    // jamais trouvés (l'import génétique échouait purement et simplement
    // sur un vrai document — vérifié sur onze bordereaux réels).
    let reference = capture(
        text,
        r"(?i)Semaine\s*N[°ºo]?\s*[0-9.]+\s+([0-9]{1,2}\.[0-9]{4,6})",
        1,
    )
    .or_else(|| capture(text, r"\b([0-9]{1,2}\.[0-9]{4,6})\b", 1))
    .or_else(|| capture(text, r"(?m)^\s*(14[0-9]{5,10})\s*$", 1))
    .map(|value| value.replace('.', ""));
    // Un avoir renvoie vers la facture qu'il annule ; on le garde dans le
    // libellé (la table `achatgenetique` n'a pas de colonne dédiée, et
    // ajouter une migration pour une seule information d'affichage aurait
    // été disproportionné) pour que facture et avoir restent traçables l'un
    // à l'autre même si l'éleveur ne les importe pas le même jour.
    let avoir_facture = capture(&compact, r"(?i)AVOIRSURFACTURE\s*N[Oo°]?\s*([0-9]+)", 1);
    let date = capture(text, r"(?i)\bLE\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})", 1)
        .or_else(|| {
            capture(
                &compact,
                r"(?i)LIVRAISONDU([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
                1,
            )
        })
        .and_then(|value| iso_date(&value));
    // `-?` après le nombre d'animaux et après le poids : sur un avoir, les
    // quantités sont elles-mêmes écrites en négatif dans la colonne
    // (« 27-   COCHETTE SERENIS ... 3255,000- »). Sans ce `-?`, la ligne ne
    // commençait pas par des chiffres suivis d'un espace et n'était jamais
    // reconnue : un avoir donnait 0 animal et 0 kg, quelle que soit sa
    // taille réelle.
    let animals =
        Regex::new(r"(?m)^[ \t]*-?[ \t]*([0-9]+)[ \t]*-?\s+(COCHETTE|VERRAT)\s+.+?\s+[0-9]+\s+-?[ \t]*([0-9.,]+)[ \t]*-?")
            .map_err(|error| format!("analyse génétique indisponible: {error}"))?;
    let mut count = 0_i64;
    let mut weight = 0.0;
    let mut boars = 0_i64;
    for row in animals.captures_iter(text) {
        count += integer(&row[1]).unwrap_or_default();
        weight += number(&row[3]).unwrap_or_default();
        if &row[2] == "VERRAT" {
            boars += integer(&row[1]).unwrap_or_default();
        }
    }
    let average = capture(
        text,
        r"(?i)Prix\s*moyen\s*reproducteurs[^0-9]*([0-9.,]+)",
        1,
    )
    .and_then(|value| number(&value));
    // La ligne de synthèse TVA (« 10400,65   2 5,5%   572,04   10972,69 »,
    // ou sa variante négative sur un avoir) est la seule source fiable de
    // BASE H.T. et TOTAL T.T.C. sur ce modèle sans libellés. Le signe des
    // montants (tiret final sur chaque nombre côté avoir) donne directement
    // le sens du document — plus besoin d'une liste de références codées en
    // dur comme avant, qui ne fonctionnait de toute façon plus depuis que
    // la référence n'était jamais extraite.
    let recap_regex = Regex::new(
        r"(?m)^[ \t]*(-?[ \t]*[0-9][0-9.,]*(?:[ \t]*-)?)\s+[0-9]\s+[0-9]+[,.][0-9]+%\s+(-?[ \t]*[0-9][0-9.,]*(?:[ \t]*-)?)\s+(-?[ \t]*[0-9][0-9.,]*(?:[ \t]*-)?)",
    )
    .map_err(|e| e.to_string())?;
    let recaps: Vec<_> = recap_regex.captures_iter(text).collect();
    let ht = if recaps.is_empty() {
        capture(
            text,
            r"(?i)(?:TOTAL\s+HORS\s+TAXES|TOTAL\s+H\.?\s*T\.?)\s*:?\s*(-?[ \t]*[0-9][0-9.,]*(?:[ \t]*-)?)",
            1,
        )
        .and_then(|v| number(&v))
    } else {
        Some(recaps.iter().filter_map(|r| number(&r[1])).sum::<f64>())
    };
    let ttc =
        (!recaps.is_empty()).then(|| recaps.iter().filter_map(|r| number(&r[3])).sum::<f64>());
    let is_avoir = ht.is_some_and(|value| value < 0.0)
        || ttc.is_some_and(|value| value < 0.0)
        || document_sign(text) < 0.0
        || avoir_facture.is_some();
    let sign = if is_avoir { -1.0 } else { 1.0 };
    // Les calculs économiques utilisent exclusivement la base HT. Le TTC
    // reste conservé dans les détails pour contrôle de facture, jamais comme
    // solution de repli silencieuse.
    let amount = ht.map(|value| value.abs() * sign);
    let base_label = if boars > 0 && boars == count {
        format!("{count} verrat(s)")
    } else if count > 0 {
        format!("{count} cochettes")
    } else {
        "Cochettes / reproducteurs".into()
    };
    let label = match (sign < 0.0, avoir_facture.as_deref()) {
        (true, Some(facture)) => format!("AVOIR — {base_label} (sur facture {facture})"),
        (true, None) => format!("AVOIR — {base_label}"),
        (false, _) => base_label,
    };
    let line = ImportLine {
        kind: "genetique".into(),
        date: date.clone(),
        reference: reference.clone(),
        label: label.clone(),
        quantity: (count > 0).then_some(count as f64 * sign),
        unit_price: average,
        amount,
        details: json!({
            "fournisseur": "Cooperl",
            "designation": label,
            "nb_animaux": (count > 0).then_some(count * sign as i64),
            "toutes_bandes": boars > 0 && boars == count,
            "poids_total": (weight != 0.0).then_some((weight.abs() * 10.0).round() / 10.0 * sign),
            "prix_moyen": average,
            "montant_ht": amount,
            "montant_net": ttc.map(|value| value.abs() * sign),
            "num_facture": reference,
            "avoir": sign < 0.0,
            "facture_liee": avoir_facture,
        }),
    };
    finish_document("génétique", vec![line], reference, date)
}

#[derive(Debug)]
struct ApportLot {
    bon: Option<String>,
    reference: Option<String>,
    pigs: Option<i64>,
    weight: Option<f64>,
    gross: Option<f64>,
    muscle_range: Option<f64>,
    muscle_lot: Option<f64>,
    technical_value: Option<f64>,
}

fn parse_apport(text: &str) -> Result<ImportDocument, String> {
    let reference = apport_document_reference(text);
    let linked_apport = capture(text, r"(?i)AVOIR\s+SUR\s+APPORT\s+N[Oo°º]?\s*([0-9.]+)", 1)
        .map(|value| value.replace('.', ""));
    let date = capture(
        text,
        r"(?i)ENLEVEMENT\s*DU\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})",
        1,
    )
    // Sur le modèle Cooperl réellement utilisé (« duplicata » comme
    // « bordereau simplifié »), le libellé « ENLEVEMENT DU » fait partie du
    // fond de page (image), pas du texte extrait : seule la date reste,
    // seule sur sa ligne, juste avant le profil d'élevage
    // (NAISSEUR/ENGRAISSEUR...). La date « LE JJ/MM/AA » du même bloc est
    // la date de facturation, pas la date d'enlèvement — donc à ne prendre
    // qu'en dernier recours.
    .or_else(|| {
        capture(
            text,
            r"(?im)^[ \t]*([0-9]{1,2}/[0-9]{2}/[0-9]{2,4})[ \t]*$",
            1,
        )
    })
    .or_else(|| capture(text, r"(?i)\bLE\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})", 1))
    .and_then(|value| iso_date(&value));
    let week = capture(text, r"(?i)Semaine\s*N[°ºo]?\s*([0-9./]+)", 1);
    let total_net = captures_all(text, r"(?i)NET\s*A\s*PAYER\s*E?\s*([0-9.,]+)\s*E?", 1)
        .last()
        .and_then(|value| number(value));
    let global_price =
        capture(text, r"(?i)Prix moyen porc\s*:?\s*([0-9.,]+)", 1).and_then(|value| number(&value));
    let global_value = capture(text, r"(?i)Plus.?value\s*/?\s*Base\s*:?\s*([0-9.,]+)", 1)
        .and_then(|value| number(&value));
    let lots = split_lots(text)
        .into_iter()
        .filter_map(|(bon, body)| parse_lot(bon, &body))
        .collect::<Vec<_>>();
    let lots = if lots.is_empty() {
        vec![ApportLot {
            bon: None,
            reference: capture(text, r"\b([A-Z0-9]{4,5})\b", 1),
            pigs: capture(text, r"([0-9]+)\s*:\s*NOMBRE\s+D.ANIMAUX", 1)
                .and_then(|value| integer(&value)),
            weight: capture(text, r"(?i)POIDS\s*TOTAL\s*:\s*([0-9.,]+)", 1)
                .and_then(|value| number(&value)),
            // Sans total de lot explicitement identifié, le net global ne
            // doit jamais être promu silencieusement en montant HT.
            gross: None,
            muscle_range: None,
            muscle_lot: None,
            technical_value: None,
        }]
    } else {
        lots
    };
    let gross_total: f64 = lots.iter().filter_map(|lot| lot.gross).sum();
    // Une pièce sans animaux est une régularisation financière autonome
    // (participation oubliée, pénalité ou avoir). Son Total bon HT suffit :
    // recréer en plus le détail comme valorisation/retenue doublerait la
    // correction dans les écrans économiques.
    let financial_adjustment = lots.iter().all(|lot| lot.pigs.is_none());
    let economic_lines = if financial_adjustment {
        Vec::new()
    } else {
        parse_economic_lines(text, reference.as_deref(), date.as_deref())
    };
    let retention_total: f64 = economic_lines
        .iter()
        .filter(|line| line.kind == "retenue")
        .filter_map(|line| line.amount)
        .map(f64::abs)
        .sum();
    let lots_json = lots
        .iter()
        .map(|lot| {
            json!({
                "bon": lot.bon,
                "ref": lot.reference,
                "nb_porcs": lot.pigs,
                "poids": lot.weight,
                "montant_ht": lot.gross,
                "muscle_gamme": lot.muscle_range,
                "muscle_lot": lot.muscle_lot,
                "value_technique": lot.technical_value,
            })
        })
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    for (index, lot) in lots.iter().enumerate() {
        let net_amount = match (total_net, lot.gross) {
            (Some(net), Some(gross)) if gross_total.abs() > f64::EPSILON => {
                Some((net * gross / gross_total * 100.0).round() / 100.0)
            }
            (_, gross) => gross,
        };
        // `gross` est le montant HT explicite du lot. C'est l'unique montant
        // autorisé pour les prix moyens et résultats économiques.
        let amount = lot.gross;
        let average_weight = match (lot.weight, lot.pigs) {
            (Some(weight), Some(pigs)) if pigs > 0 => {
                Some((weight / pigs as f64 * 100.0).round() / 100.0)
            }
            _ => None,
        };
        let is_avoir = is_apport_avoir(text);
        let label = match (&lot.reference, &lot.bon) {
            _ if linked_apport.is_some() => format!(
                "Avoir sur apport {}",
                linked_apport.as_deref().unwrap_or_default()
            ),
            (Some(reference), _) if is_avoir => format!("Avoir - lot {reference}"),
            (None, Some(bon)) if is_avoir => format!("Avoir - bon {bon}"),
            (Some(reference), _) => format!("Lot {reference}"),
            (None, Some(bon)) => format!("Bon {bon}"),
            _ => "Vente de porcs".into(),
        };
        lines.push(ImportLine {
            kind: "vente".into(),
            date: date.clone(),
            reference: reference.clone(),
            label,
            quantity: lot.pigs.map(|value| value as f64),
            unit_price: match (amount, lot.weight) {
                (Some(amount), Some(weight)) if weight > 0.0 => Some(amount / weight),
                _ => None,
            },
            amount,
            details: json!({
                "num_apport": reference,
                "date": date,
                "semaine": week,
                "frappe": lot.reference,
                "nb_porcs": lot.pigs,
                "poids_total": lot.weight,
                "poids_moyen": average_weight,
                "prix_moyen": global_price,
                "plus_value": global_value,
                "montant_ht": amount,
                "montant_net": net_amount,
                "tmp": lot.muscle_lot,
                "muscle_gamme": lot.muscle_range,
                "muscle_lot": lot.muscle_lot,
                "total_retenues": (index == 0 && retention_total > 0.0).then_some(retention_total),
                "lots_json": lots_json,
                "avoir": is_avoir,
                "regularisation": financial_adjustment,
                "apport_lie": linked_apport,
            }),
        });
    }
    lines.extend(economic_lines);
    finish_document("apport", lines, reference, date)
}

fn is_apport_avoir(text: &str) -> bool {
    let compact: String = text
        .to_uppercase()
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    compact.contains("AVOIR")
}

/// Numéro propre du document. Sur un avoir, « AVOIR SUR APPORT N° ... » est
/// la référence corrigée et non celle du nouveau document : elle ne doit
/// jamais servir de clé d'import, sinon l'avoir écraserait la vente initiale.
fn apport_document_reference(text: &str) -> Option<String> {
    let linked = capture(text, r"(?i)AVOIR\s+SUR\s+APPORT\s+N[Oo°º]?\s*([0-9.]+)", 1)
        .map(|value| value.replace('.', ""));
    if let Some(value) = captures_all(text, r"(?i)APPORT\s*N[°ºo]?\s*([0-9.]{6,})", 1)
        .into_iter()
        .map(|value| value.replace('.', ""))
        .find(|value| Some(value.as_str()) != linked.as_deref())
    {
        return Some(value);
    }
    // Dans les duplicatas, le numéro est soit seul juste après DUPLICATA,
    // soit sur la ligne « Semaine N° » (anciens documents 12.46644).
    capture(text, r"(?im)^\s*([0-9]{6,}|[0-9]{1,3}\.[0-9]{4,})\s*$", 1)
        .or_else(|| {
            capture(
                text,
                r"(?im)^\s*Semaine\s+N[°ºo]?\s+[0-9./]+\s+([0-9.]{6,})\s*$",
                1,
            )
        })
        .map(|value| value.replace('.', ""))
}

fn split_lots(text: &str) -> Vec<(Option<String>, String)> {
    let Ok(regex) = Regex::new(r"(?i)Bon\s*n[°ºo]?\s*([0-9]+)") else {
        return vec![(None, text.to_string())];
    };
    let matches: Vec<_> = regex.captures_iter(text).collect();
    if matches.is_empty() {
        return vec![(None, text.to_string())];
    }
    let mut lots = Vec::<(Option<String>, String)>::new();
    for (index, captures) in matches.iter().enumerate() {
        let Some(whole_match) = captures.get(0) else {
            continue;
        };
        let body_start = whole_match.end();
        let body_end = matches
            .get(index + 1)
            .and_then(|next| next.get(0))
            .map_or(text.len(), |value| value.start());
        let bon = captures.get(1).map(|value| value.as_str().to_string());
        let body = text[body_start..body_end].to_string();
        if let Some((_, known_body)) = lots.iter_mut().find(|(known, _)| known == &bon) {
            known_body.push('\n');
            known_body.push_str(&body);
        } else {
            lots.push((bon, body));
        }
    }
    lots
}

fn parse_lot(bon: Option<String>, body: &str) -> Option<ApportLot> {
    let reference = lot_reference(body);
    let total_line = body
        .lines()
        .rev()
        .find(|line| line.to_uppercase().contains("TOTAL BON"));
    let amounts = total_line
        .map(|line| {
            Regex::new(r"[0-9]+(?:[.,][0-9]+)*(?:-)?")
                .ok()
                .map(|regex| {
                    regex
                        .find_iter(line)
                        .filter_map(|value| number(value.as_str()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let gross = amounts.last().copied();
    let weight = (amounts.len() >= 2).then(|| amounts[amounts.len() - 2].abs());
    let animal_regex = Regex::new(
        r"(?im)^\s*([0-9]+)\s+(SAISI|CREVE|CREVÉ|CREVEE|PORC|LEGER|LÉGER|LOURD|COEUR|TRUIE)",
    )
    .ok()?;
    let pigs: i64 = animal_regex
        .captures_iter(body)
        .filter_map(|row| row.get(1))
        .filter_map(|value| integer(value.as_str()))
        .sum();
    let muscle = Regex::new(r"(?i)muscle\s*:\s*de la gamme\s*([0-9.,]+)\s*du lot\s*([0-9.,]+)")
        .ok()?
        .captures(body);
    let muscle_range = muscle
        .as_ref()
        .and_then(|row| row.get(1))
        .and_then(|value| number(value.as_str()));
    let muscle_lot = muscle
        .as_ref()
        .and_then(|row| row.get(2))
        .and_then(|value| number(value.as_str()));
    let technical_value = capture(body, r"(?i)Value\s+Technique\s*:?\s*([0-9.,]+)\s*cts", 1)
        .and_then(|value| number(&value));
    if reference.is_none() && weight.is_none() && gross.is_none() && pigs == 0 {
        None
    } else {
        Some(ApportLot {
            bon,
            reference,
            pigs: (pigs > 0).then_some(pigs),
            weight,
            gross,
            muscle_range,
            muscle_lot,
            technical_value,
        })
    }
}

fn lot_reference(text: &str) -> Option<String> {
    let regex = Regex::new(r"\b[A-Z0-9]{3,5}\b").ok()?;
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for value in regex.find_iter(text).map(|value| value.as_str()) {
        if !value.ends_with("KG")
            && value
                .chars()
                .any(|character| character.is_ascii_alphabetic())
            && value.chars().any(|character| character.is_ascii_digit())
        {
            *occurrences.entry(value.to_string()).or_default() += 1;
        }
    }
    occurrences
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(value, _)| value)
}

fn parse_economic_lines(
    text: &str,
    reference: Option<&str>,
    date: Option<&str>,
) -> Vec<ImportLine> {
    let Ok(keyword) = Regex::new(
        r"(?i)(\+\s*VALUE|PRIME\s+SOLIDARITE|COMPLEMENT|PARTICIPATION|FRAIS\s+DE\s+(?:GROUPEMENT|RAMASSAGE)|SERVICE\s+PUBLIC|EQUARRISSAGE|ÉQUARRISSAGE|CVEE|CONTRIBUTION\s+SANITAIRE|COTISATION)",
    ) else {
        return Vec::new();
    };
    let apport_credit =
        Regex::new(r"(?i)AVOIR\s+SUR\s+APPORT").is_ok_and(|regex| regex.is_match(text));
    let mut ordered = Vec::<(String, String, f64)>::new();
    for raw in text.lines() {
        if !keyword.is_match(raw)
            || raw.to_uppercase().contains("VALUE TECHNIQUE")
            || raw.to_uppercase().contains("PLUS VALUE")
            || (raw.to_uppercase().contains("SOIT") && raw.to_uppercase().contains("CUMUL"))
        {
            continue;
        }
        let Some(signed) = last_amount(raw) else {
            continue;
        };
        let label = canonical_label(raw);
        let forced = is_forced_retention(&label) && !apport_credit;
        let kind = if forced || signed < 0.0 || (document_sign(text) < 0.0 && !apport_credit) {
            "retenue"
        } else {
            "valorisation"
        };
        let stored = if kind == "retenue" {
            -signed.abs()
        } else {
            signed.abs()
        };
        if let Some((_, _, current)) = ordered
            .iter_mut()
            .find(|(known_kind, known_label, _)| known_kind == kind && known_label == &label)
        {
            *current += stored;
        } else {
            ordered.push((kind.into(), label, stored));
        }
    }
    ordered
        .into_iter()
        .map(|(kind, label, amount)| ImportLine {
            kind: kind.clone(),
            date: date.map(str::to_string),
            reference: reference.map(str::to_string),
            label: label.clone(),
            quantity: None,
            unit_price: None,
            amount: Some((amount * 100.0).round() / 100.0),
            details: json!({
                "num_apport": reference,
                "date": date,
                "libelle": label,
                "montant": (amount * 100.0).round() / 100.0,
                "categorie": kind,
            }),
        })
        .collect()
}

/// Remplace les lettres françaises accentuées par leur équivalent ASCII, afin
/// que la comparaison par sous-chaîne de `canonical_label` fusionne les
/// variantes accentuées et non accentuées d'un même libellé (ex. « CHARTE
/// QUALITE REGI » et « CHARTE QUALITÉ RÉGIONALE ») plutôt que d'en faire
/// deux postes distincts dans le relevé — trouvé en écrivant le test associé.
fn strip_french_accents(input: &str) -> String {
    input
        .chars()
        .map(|character| match character {
            'À' | 'Â' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Î' | 'Ï' => 'I',
            'Ô' | 'Ö' => 'O',
            'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            other => other,
        })
        .collect()
}

fn canonical_label(raw: &str) -> String {
    let upper = strip_french_accents(&raw.to_uppercase());
    let compact: String = upper
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    let mappings = [
        ("COTISATIONAUJESKY", "Cotisation Aujeszky"),
        ("SERVICECOCHETTE", "Service cochette"),
        ("PRIMECOCHETTESERENIS", "Prime cochette Serenis"),
        ("QUEUELONGUE", "Queue longue (RSE)"),
        ("CHARTEQUALITEREGI", "Charte Qualité Régionale"),
        ("COOPERLLPF", "Cooperl LPF"),
        ("QUALIVIANDEPBE", "Qualiviande PBE"),
        ("QUALIVIANDE", "Qualiviande"),
        ("SANSANTIBIOTI", "Porc sans antibiotique"),
        ("SOLIDARITEJEUNE", "Prime Solidarité Jeune"),
        ("COCHONDUDIMANC", "Complément Cochon du Dimanche"),
        ("COUTRFID", "Participation coût RFID"),
        ("PARTICIPATIONPSA", "Participation PSA"),
        ("FRAISDERAMASSAGE", "Frais de ramassage"),
        ("FRAISDEGROUPEMENT", "Frais de groupement"),
        ("SERVICEPUBLICEQUARRISSAGE", "Service public équarrissage"),
        ("EQUARRISSAGE", "Service public équarrissage"),
        ("CVEE", "CVEE étendue"),
        ("CONTRIBUTIONSANITAIRE", "Contribution sanitaire CVS"),
        ("COTISATION", "Cotisation actions techniques"),
        ("RSE", "RSE"),
        ("SANSOGM", "Sans OGM"),
        ("BIENETRE", "Bien-être"),
        ("WELFARE", "Welfare"),
    ];
    for (key, label) in mappings {
        if compact.contains(key) {
            return label.into();
        }
    }
    let before_amount = Regex::new(r"\s+[0-9.,]+-?\s*$")
        .map(|regex| regex.replace(raw, "").into_owned())
        .unwrap_or_else(|_| raw.to_string());
    let without_quantity = Regex::new(r"^\s*[0-9]+\s+")
        .map(|regex| regex.replace(&before_amount, "").into_owned())
        .unwrap_or(before_amount);
    without_quantity
        .replace("+ VALUE", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_forced_retention(label: &str) -> bool {
    let upper = label.to_uppercase();
    [
        "ÉQUARRISSAGE",
        "EQUARRISSAGE",
        "GROUPEMENT",
        "CVEE",
        "CONTRIBUTION SANITAIRE",
        "COTISATION",
        "PRÉLÈVEMENT",
        "PRELEVEMENT",
        "FRAIS",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn last_amount(line: &str) -> Option<f64> {
    capture(
        line,
        r"(?i)(-?\s*\(?[0-9][0-9., \u{00a0}\u{202f}]*\)?-?)\s*$",
        1,
    )
    .and_then(|value| number(&value))
}

fn labeled_amount(text: &str, labels: &[&str]) -> Option<f64> {
    labels.iter().find_map(|label| {
        let pattern = format!(
            r"(?i){}[^\n0-9-]{{0,14}}(-?[0-9][0-9 .\u{{00a0}}\u{{202f}}]*,[0-9]{{2}}-?)",
            regex::escape(label)
        );
        capture(text, &pattern, 1).and_then(|value| number(&value))
    })
}

fn parse_synthese(text: &str) -> Result<ImportDocument, String> {
    let frappe = capture(text, r"\b(DA[0-9]{3})\b", 1);
    let date = capture(
        text,
        r"(?i)Date d.abattage[^0-9]*([0-9]{2}/[0-9]{2}/[0-9]{2,4})",
        1,
    )
    .and_then(|value| iso_date(&value));
    let range_rate =
        capture(text, r"(?i)([0-9]+)%\s*dans la gamme", 1).and_then(|value| number(&value));
    let mut pigs = None;
    let mut average_weight = None;
    let mut tmp = None;
    let mut plus_value = None;
    if let Some(reference) = frappe.as_deref() {
        for line in text.lines().filter(|line| line.contains(reference)) {
            let values: Vec<_> = line.split_whitespace().collect();
            if let Some(index) = values.iter().position(|value| *value == reference) {
                let sequence = &values[index + 1..];
                if sequence.len() >= 8 {
                    pigs = integer(sequence[0]);
                    average_weight = number(sequence[3]);
                    tmp = number(sequence[4]);
                    plus_value = sequence.last().and_then(|value| number(value));
                    break;
                }
            }
        }
    }
    let line = ImportLine {
        kind: "synthese".into(),
        date: date.clone(),
        reference: frappe.clone(),
        label: format!(
            "Synthèse Uniporc {}",
            frappe.as_deref().unwrap_or("sans frappe")
        ),
        quantity: pigs.map(|value| value as f64),
        unit_price: None,
        amount: None,
        details: json!({
            "frappe": frappe,
            "date": date,
            "nb_porcs": pigs,
            "poids_moyen": average_weight,
            "tmp": tmp,
            "plus_value": plus_value,
            "tx_qualification": range_rate,
        }),
    };
    finish_document("synthèse Uniporc", vec![line], frappe, date)
}

fn finish_document(
    document_type: &str,
    lines: Vec<ImportLine>,
    reference: Option<String>,
    date: Option<String>,
) -> Result<ImportDocument, String> {
    if lines.is_empty() {
        return Err(format!(
            "Le document {document_type} est reconnu, mais aucune ligne exploitable n'a été trouvée"
        ));
    }
    if lines.iter().all(|line| line.amount.is_none()) && document_type != "synthèse Uniporc" {
        return Err(format!(
            "Le document {document_type} est reconnu, mais aucun montant fiable n'a été trouvé"
        ));
    }
    let mut warnings = Vec::new();
    if reference.is_none() {
        warnings
            .push("Numéro de facture ou d'apport non détecté : la confirmation est bloquée".into());
    }
    if date.is_none() {
        warnings.push("Date non détectée : vérifie le document avant confirmation".into());
    }
    Ok(ImportDocument {
        document_type: document_type.into(),
        lines,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construit un mini-PDF valide à la main (table xref classique), pour
    /// reproduire précisément l'offset `startxref` sans dépendre de l'API
    /// d'écriture de lopdf.
    fn build_minimal_pdf(body_text: &str) -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut offsets = Vec::new();
        buffer.extend_from_slice(b"%PDF-1.4\n");
        let objects = [
            "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n".to_string(),
            "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n".to_string(),
            "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>endobj\n".to_string(),
            {
                let content = format!("BT /F1 24 Tf 10 100 Td ({body_text}) Tj ET\n");
                format!(
                    "4 0 obj<</Length {}>>stream\n{content}endstream\nendobj\n",
                    content.len()
                )
            },
            "5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj\n".to_string(),
        ];
        for object in &objects {
            offsets.push(buffer.len());
            buffer.extend_from_slice(object.as_bytes());
        }
        let xref_pos = buffer.len();
        buffer.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for offset in &offsets {
            buffer.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        buffer.extend_from_slice(b"trailer<</Size 6/Root 1 0 R>>\nstartxref\n");
        buffer.extend_from_slice(format!("{xref_pos}\n%%EOF").as_bytes());
        buffer
    }

    #[test]
    fn repare_un_startxref_decale_comme_le_produit_ldprx() {
        let good = build_minimal_pdf("Bon n 1");
        assert!(
            lopdf::Document::load_mem(&good).is_ok(),
            "le PDF de référence doit être valide"
        );

        // Décale l'offset déclaré de +31 octets, exactement le décalage
        // observé sur un vrai bordereau Cooperl généré par LDPRX 4.54.
        let marker = b"startxref\n";
        let start = good
            .windows(marker.len())
            .rposition(|w| w == marker)
            .unwrap()
            + marker.len();
        let end = good[start..]
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap()
            + start;
        let real_offset: usize = std::str::from_utf8(&good[start..end])
            .unwrap()
            .parse()
            .unwrap();
        let mut broken = good.clone();
        broken.splice(start..end, (real_offset + 31).to_string().into_bytes());

        assert!(
            lopdf::Document::load_mem(&broken).is_err(),
            "ce cas doit reproduire l'échec observé (offset startxref invalide)"
        );

        let patched =
            repair_startxref_offset(&broken).expect("la réparation doit trouver le vrai xref");
        let repaired_document =
            lopdf::Document::load_mem(&patched).expect("le PDF réparé doit se charger");
        let pages: Vec<u32> = repaired_document.get_pages().keys().copied().collect();
        let text = repaired_document
            .extract_text_with_limit(&pages, MAX_DECOMPRESSED_PAGE)
            .unwrap();
        assert!(
            text.contains("Bon n 1"),
            "le texte du PDF réparé doit rester lisible, obtenu: {text:?}"
        );
    }

    #[test]
    fn ne_touche_pas_un_startxref_deja_correct() {
        let good = build_minimal_pdf("Bon n 1");
        assert!(repair_startxref_offset(&good).is_none());
    }

    #[test]
    fn veterinaire_dates_bons_tva_et_lignes_repetées() {
        let text="PRODUITS VETERINAIRES\n20.18089\nLE 14/02/26\nBon n° 1113 du 9/02/26\nLivré chez SITE A\n 1 SANIBLANC SAC 3 01113 11,18 11,18\n 1 ECO-CONTRIBUTION TVA 20% 4 01113 0,04 0,04\nBon n° 28761 du 12/02/26\nLivré chez SITE B\n 1 ECO-CONTRIBUTION TVA 20% 4 28761 0,24 0,24";
        let doc = parse_document(text).unwrap();
        assert_eq!(doc.lines.len(), 3);
        assert_eq!(doc.lines[0].reference.as_deref(), Some("2018089"));
        assert_eq!(doc.lines[0].date.as_deref(), Some("2026-02-09"));
        assert_eq!(doc.lines[2].date.as_deref(), Some("2026-02-12"));
        assert_eq!(doc.lines[2].details["destination"], "Livré chez SITE B");
        assert!((doc.lines.iter().filter_map(|x| x.amount).sum::<f64>() - 11.46).abs() < 0.001);
    }
    #[test]
    fn aliment_chaque_bon_garde_date_et_destination() {
        let text="FACTURE N° 123456 ALIMENTS\nLIVRE CHEZ SITE A\nBon de livraison n° 10 du 1/08/26\nPORC GR 01 1,000 200,00 210,00 2 210,00\nLIVRE CHEZ SITE B\nBon de livraison n° 11 du 2/08/26\nPORC GR 01 2,000 200,00 210,00 2 420,00";
        let doc = parse_document(text).unwrap();
        assert_eq!(doc.lines.len(), 2);
        assert_eq!(doc.lines[0].date.as_deref(), Some("2026-08-01"));
        assert_eq!(doc.lines[1].date.as_deref(), Some("2026-08-02"));
        assert_eq!(doc.lines[1].details["destination"], "LIVRE CHEZ SITE B");
    }
    #[test]
    fn genetique_totalise_les_bases_ht_pas_les_ttc() {
        let doc=parse_document("ANIMAUX REPRODUCTEURS\nSemaine N° 26.01 14.12345\nLE 1/01/26\n100,00 2 5,5% 5,50 105,50\n20,00 4 20,0% 4,00 24,00").unwrap();
        assert_eq!(doc.lines[0].amount, Some(120.0));
        assert_eq!(doc.lines[0].details["montant_ht"], 120.0);
        assert_eq!(doc.lines[0].details["montant_net"], 129.5);
    }
    #[test]
    #[ignore]
    fn audit_dossier_pdf_local() {
        let root = std::env::var("CHECK_PDF_DIR").expect("CHECK_PDF_DIR");
        let mut report = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|x| x.to_str()) != Some("pdf") {
                continue;
            }
            let result = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|b| extract_pdf_text(&b))
                .and_then(|t| parse_document(&t));
            report.push(match result {Ok(d)=>json!({"file":path,"type":d.document_type,"lines":d.lines,"warnings":d.warnings}),Err(e)=>json!({"file":path,"error":e})});
        }
        std::fs::write(
            std::env::var("CHECK_PDF_REPORT").expect("CHECK_PDF_REPORT"),
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{} PDF examinés", report.len());
    }

    #[test]
    #[ignore]
    fn dump_pdf_temporaire() {
        let path = std::env::var("DUMP_PDF").expect("DUMP_PDF non défini");
        let bytes = std::fs::read(&path).expect("lecture PDF");
        let text = extract_pdf_text(&bytes).expect("extraction texte");
        println!("=====8<===== {path}\n{text}\n=====>8=====");
        match parse_document(&text) {
            Ok(document) => println!("PARSED: {document:#?}"),
            Err(error) => println!("PARSE ERROR: {error}"),
        }
    }

    #[test]
    fn date_a_deux_chiffres_prend_le_21e_siecle() {
        // chrono seul renvoie l'an 26, pas 2026, pour "%d/%m/%y" — vérifié
        // en écrivant ce test après l'avoir constaté sur de vrais bordereaux.
        assert_eq!(iso_date("27/07/26"), Some("2026-07-27".to_string()));
        assert_eq!(iso_date("3/07/26"), Some("2026-07-03".to_string()));
        assert_eq!(iso_date("16/08/2026"), Some("2026-08-16".to_string()));
    }

    #[test]
    fn nombres_francais_et_avoirs() {
        assert_eq!(number("1 234,56"), Some(1234.56));
        assert_eq!(number("1.234,56-"), Some(-1234.56));
        assert_eq!(number(""), None);
        assert_eq!(number("NaN"), None);
        assert_eq!(number("inf"), None);
        assert_eq!(integer("999999999999999999999999999999"), None);
        assert_eq!(document_sign("NOTE DE CRÉDIT fournisseur"), -1.0);
    }

    #[test]
    fn aliment_conserve_la_presentation_dans_la_cle() {
        let text = "FACTURE N° 123.456\nALIMENTS SILOS DÉSIGNATION PRODUIT\nBon de livraison du 16/08/2026\nPORC CROISSANCE GR 12 5,50 100 320,00 20 1760,00\nPORC CROISSANCE FE 12 4,00 100 300,00 20 1200,00";
        let Ok(parsed) = parse_document(text) else {
            panic!("le document aliment doit être analysé");
        };
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].label, "PORC CROISSANCE GR");
        assert_eq!(parsed.lines[1].label, "PORC CROISSANCE FE");
        assert_eq!(parsed.lines[0].reference.as_deref(), Some("123456"));
    }

    #[test]
    fn aliment_reconnu_meme_sans_entete_de_tableau() {
        // Texte réel (extrait via extract_pdf_text) d'une facture aliment
        // Cooperl où lopdf ne restitue pas la ligne d'en-tête « Désignation
        // produit / Silos » du tableau, ni le mot complet « FACTURE » (seule
        // la forme abrégée « Fact.N° » survit) — probablement du texte
        // positionné hors flux principal — alors que les lignes produit
        // sont intactes.
        let text = "ALIMENTS 33 LA MELTIERE\nGESTA PLUS               FE    04             3,050 *    287,50   290,50   2       886,02\nMULTI BE CROISSANCE C    FE    02             4,970 *    289,00   292,00   2     1.451,24\nFact.N°\n326070852454\nDATE : 24/07/26";
        let Ok(parsed) = parse_document(text) else {
            panic!("le document aliment sans en-tête doit tout de même être reconnu");
        };
        assert!(
            parsed.warnings.is_empty(),
            "aucun avertissement attendu: {:?}",
            parsed.warnings
        );
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].label, "GESTA PLUS FE");
        assert_eq!(parsed.lines[0].reference.as_deref(), Some("326070852454"));
    }

    #[test]
    fn apport_classe_les_frais_en_retenues() {
        let text = "APPORT N° 123456 CHARCUTIERS\nENLEVEMENT DU 16/08/2026\nBon n° 42\nDA915 DA915\n63 PORC\nTotal Bon..... 5700,00 10000,00\nmuscle : de la gamme 62,1 du lot 61,3\nFRAIS DE GROUPEMENT 2 ABC 120,00\nPRIME SOLIDARITE JEUNE 2 ABC 80,00\nNET A PAYER 9960,00";
        let Ok(parsed) = parse_document(text) else {
            panic!("le document d'apport doit être analysé");
        };
        assert!(parsed.lines.iter().any(|line| line.kind == "vente"));
        let Some(retention) = parsed.lines.iter().find(|line| line.kind == "retenue") else {
            panic!("la retenue doit être analysée");
        };
        assert_eq!(retention.label, "Frais de groupement");
        assert_eq!(retention.amount, Some(-120.0));
        assert!(parsed.lines.iter().any(|line| line.kind == "valorisation"));
    }

    #[test]
    fn apport_repartit_deux_bons_sur_la_meme_facture() {
        // Chiffres réels d'un bordereau Cooperl à deux lots (APPORT N°
        // 226081270686, ORY EMMANUEL, deux Bon n° 35776/35777 dans une même
        // facture) — reproduit dans la mise en forme que produit
        // extract_pdf_text sur ce type de document (labels d'en-tête absents
        // du texte extrait, dates isolées sur leur ligne).
        let text = "APPORT N°   226081270686\nAPPORT DE PORCS CHARCUTIERS\nSemaine N° 26.31\n16961             LE 4/08/26\nN° TVA:\n27/07/26\nV/ID :FR06510329899\nNAISSEUR/ENGRAISSEUR\nDestination : COOPERL ARC ATL MONTFORT   Bon n° 35776\n 1 SAISI 2 DA915 107,100\n 29 PORC (COEUR DE GAMME) 2 DA915 2542,700 1,625 4.133,12\n 2 PORC SAISIE PARTIELLE 2 DA915 169,300 1,408 238,42\n 1 LOURD P4 106,1 A 107 KG 2 DA915 103,800 1,566 162,55\n 9 LEGER P2 73 A 77,9 KG 2 DA915 666,900 1,449 966,21\n 16 LEGER P3 78 A 82,9 KG 2 DA915 1252,800 1,557 1.950,97\n 2 LEGER P3 78 A 82,9 KG CS 2 DA915 149,900 1,379 206,64\n 6 LEGER P1 45,0 A 72,9 KG 2 DA915 404,900 1,300 526,52\n 2 LEGER P1 45,0 A 72,9 KG CS 2 DA915 119,900 1,035 124,09\n Total Bon..... 5517,300 8.855,10\n % de muscle: de la gamme 62,7 du lot 62,8 + Value Technique: 10,07 cts\nDestination : COOPERL ARC ATL MONTFORT   Bon n° 35777\n 1 CREVE RESP. PARTAGEE 2 G6KL 43,900 1,585 69,58\n 45 PORC (COEUR DE GAMME) 2 G6KL 4036,800 1,572 6.345,90\n 2 PORC SAISIE PARTIELLE 2 G6KL 169,300 1,446 244,88\n 1 PORC 103,1 A 105 KG 2 G6KL 100,700 1,495 150,54\n 1 LOURD P4 106,1 A 107 KG 2 G6KL 103,800 1,525 158,29\n 1 LOURD P4 109,1 A 110 KG 2 G6KL 106,900 1,435 153,40\n 1 LOURD P4 111,1 A 112 KG 2 G6KL 108,800 1,395 151,77\n 1 LOURD P4 112,1 A 113 KG 2 G6KL 109,200 1,245 135,95\n 3 LEGER P2 73 A 77,9 KG 2 G6KL 219,900 1,379 303,32\n 9 LEGER P3 78 A 82,9 KG 2 G6KL 700,300 1,519 1.063,46\n 1 LEGER P1 45,0 A 72,9 KG CS 2 G6KL 60,400 1,035 62,51\n Total Bon..... 5760,000 9.607,76\n % de muscle: de la gamme 59,9 du lot 59,9 + Value Technique: 9,93 cts\n134 : NOMBRE D'ANIMAUX POIDS TOTAL : 11277,300\nNET A PAYER E 19.465,50 E";
        let Ok(parsed) = parse_document(text) else {
            panic!("le bordereau à deux lots doit être analysé");
        };
        let ventes: Vec<_> = parsed
            .lines
            .iter()
            .filter(|line| line.kind == "vente")
            .collect();
        assert_eq!(
            ventes.len(),
            2,
            "les deux Bon n° doivent produire deux lignes de vente distinctes, obtenu {ventes:?}"
        );

        // Date : "27/07/26", seule sur sa ligne (l'enlèvement), pas "4/08/26"
        // qui suit "LE" (la facturation).
        assert_eq!(ventes[0].date.as_deref(), Some("2026-07-27"));

        assert_eq!(ventes[0].quantity, Some(68.0));
        assert_eq!(ventes[1].quantity, Some(66.0));
        assert_eq!(
            ventes[0].details.get("poids_total").and_then(Value::as_f64),
            Some(5517.3)
        );
        assert_eq!(
            ventes[1].details.get("poids_total").and_then(Value::as_f64),
            Some(5760.0)
        );

        // Le montant de calcul reste le HT explicite de chaque lot. Le net
        // global est seulement conservé dans les détails pour contrôle.
        assert_eq!(ventes[0].amount, Some(8855.10));
        assert_eq!(ventes[1].amount, Some(9607.76));
        assert_eq!(
            ventes[0].details.get("montant_net").and_then(Value::as_f64),
            Some(9335.98)
        );
        assert_eq!(
            ventes[1].details.get("montant_net").and_then(Value::as_f64),
            Some(10129.52)
        );

        assert_eq!(
            ventes[0].details.get("muscle_lot").and_then(Value::as_f64),
            Some(62.8)
        );
        assert_eq!(
            ventes[1].details.get("muscle_lot").and_then(Value::as_f64),
            Some(59.9)
        );
    }

    #[test]
    fn apport_prend_la_date_denlevement_pas_la_date_de_facturation() {
        // Sur le modèle « duplicata » réellement utilisé, seule la date
        // reste dans le texte extrait (le libellé ENLEVEMENT DU fait partie
        // du fond de page) : "LE 11/07/26" est la date de facturation,
        // "3/07/26" seule sur sa ligne est la vraie date d'enlèvement.
        let text = "APPORT N° 226071267848\nAPPORT DE PORCS CHARCUTIERS\nSemaine N° 26.27\n16961             LE 11/07/26\nN° TVA :       V/ID : FR06510329899\n             3/07/26\n                     NAISSEUR/ENGRAISSEUR\n Destination =    COOPERL LAMBALLE          Bon n°  10831\n 1 PORC (COEUR DE GAMME) 60 % 2 DA915 94,500 1,580 149,31\n Total bon..... 6334,700 10.569,44\nNET A PAYER 11.145,80";
        let Ok(parsed) = parse_document(text) else {
            panic!("le document d'apport doit être analysé");
        };
        let vente = parsed
            .lines
            .iter()
            .find(|line| line.kind == "vente")
            .expect("une ligne de vente");
        assert_eq!(vente.date.as_deref(), Some("2026-07-03"));
    }

    #[test]
    fn apport_duplicata_sans_libelle_apport_est_reconnu() {
        let text = "D U P L I C A T A\n226071267848\nSemaine N° 26.27\nPORCS CHARCUTIERS\n16961 LE 11/07/26\n3/07/26\nNAISSEUR/ENGRAISSEUR\nBon n° 10831\n67 PORC 2 DA915 6334,700\nTotal bon..... 6334,700 10.569,44";
        let parsed = parse_document(text).expect("duplicata Cooperl reconnu");
        let vente = parsed
            .lines
            .iter()
            .find(|line| line.kind == "vente")
            .expect("vente extraite");
        assert_eq!(vente.reference.as_deref(), Some("226071267848"));
        assert_eq!(vente.date.as_deref(), Some("2026-07-03"));
        assert_eq!(vente.quantity, Some(67.0));
        assert_eq!(vente.amount, Some(10_569.44));
    }

    #[test]
    fn apport_reforme_ancien_numero_et_frappe_courte() {
        let text = "D U P L I C A T A\nSemaine N° 26.06 12.46644\nPORCS CHARCUTIERS\n3/02/26\nNAISSEUR/ENGRAISSEUR\nBon n° 25809\n3 TRUIE VIANDE 0-125KG 2 DA9 308,000 0,698 214,98\nTotal bon..... 308,000 212,38";
        let parsed = parse_document(text).expect("réformes reconnues");
        let vente = &parsed.lines[0];
        assert_eq!(vente.reference.as_deref(), Some("1246644"));
        assert_eq!(vente.quantity, Some(3.0));
        assert_eq!(import_test_detail(vente, "frappe"), Some("DA9"));
        assert_eq!(vente.amount, Some(212.38));
    }

    #[test]
    fn apport_avoir_garde_son_numero_et_ne_sort_aucun_porc() {
        let text = "D U P L I C A T A\n226081272085\nSemaine N° 26.32\n**** A V O I R ****\nAPPORT DE PORCS CHARCUTIERS\n3/08/26\nAVOIR SUR APPORT NO 1230826\nBon n° 36190\nFRAIS DE RAMASSAGE CHARCUTIERS 4908,800- 0,010- 49,08\nTotal bon..... 49,08";
        let parsed = parse_document(text).expect("avoir d'apport reconnu");
        assert_eq!(parsed.lines.len(), 1);
        let avoir = &parsed.lines[0];
        assert_eq!(avoir.reference.as_deref(), Some("226081272085"));
        assert_eq!(avoir.quantity, None);
        assert_eq!(avoir.amount, Some(49.08));
        assert_eq!(import_test_detail(avoir, "apport_lie"), Some("1230826"));
        assert_eq!(
            avoir.details.get("regularisation").and_then(Value::as_bool),
            Some(true)
        );
    }

    fn import_test_detail<'a>(line: &'a ImportLine, key: &str) -> Option<&'a str> {
        line.details.get(key).and_then(Value::as_str)
    }

    #[test]
    fn semence_avoir_force_des_montants_negatifs() {
        let text = "YXIA SEMENCE AVOIR FAC2026001\nDATE : 16/08/2026\n20 BLISTER LIFE\nTOTAL HT 1 392,50\nTOTAL TTC 1 671,00";
        let Ok(parsed) = parse_document(text) else {
            panic!("l'avoir de semence doit être analysé");
        };
        assert_eq!(parsed.lines[0].amount, Some(-1392.5));
        assert!(parsed.lines[0].label.starts_with("AVOIR"));
    }

    #[test]
    fn genetique_moins_avant_apres_et_espaces_sans_mot_avoir() {
        for (ht, tva, ttc) in [
            ("-100,00", "-5,50", "-105,50"),
            ("100,00-", "5,50-", "105,50-"),
            ("- 100,00", "- 5,50", "- 105,50"),
            ("100,00 -", "5,50 -", "105,50 -"),
            ("−100,00", "−5,50", "−105,50"),
        ] {
            let text=format!("ANIMAUX REPRODUCTEURS\nSemaine N° 26.01 14.99999\nLE 2/01/26\n-1 VERRAT TEST 2 -100,000 1,00 -100,00\n{ht} 2 5,5% {tva} {ttc}");
            let doc = parse_document(&text).unwrap();
            let line = &doc.lines[0];
            assert_eq!(line.amount, Some(-100.0), "{text}");
            assert_eq!(line.quantity, Some(-1.0));
            assert_eq!(line.details["montant_net"], -105.5);
            assert_eq!(line.details["poids_total"], -100.0);
            assert_eq!(line.details["avoir"], true);
            let fallback=format!("ANIMAUX REPRODUCTEURS\nSemaine N° 26.01 14.99999\nLE 2/01/26\nTOTAL HORS TAXES {ht}");
            assert_eq!(
                parse_document(&fallback).unwrap().lines[0].amount,
                Some(-100.0)
            );
        }
    }
    #[test]
    fn genetique_une_remise_ne_change_pas_le_signe_du_total() {
        let text="ANIMAUX REPRODUCTEURS\nSemaine N° 26.01 14.99999\nLE 2/01/26\nREMISE -20,00\n100,00 2 5,5% 5,50 105,50";
        let doc = parse_document(text).unwrap();
        assert_eq!(doc.lines[0].amount, Some(100.0));
        assert_eq!(doc.lines[0].details["avoir"], false);
    }

    #[test]
    fn verrat_souffleur_et_prime_ne_comptent_qu_un_animal() {
        let doc=parse_document("ANIMAUX REPRODUCTEURS\nSemaine N° 25.27 14.41384\nLE 8/07/25\n1 VERRAT VIGOR SOUFFLEUR 2 132,000 1,42 187,44\n1 PRIME VERRAT VIGOR SOUFFLEUR 2 450,00 450,00\n1 SERVICE VERRAT 2 60,00 60,00\n697,44 2 5,5% 38,36 735,80").unwrap();
        let l = &doc.lines[0];
        assert_eq!(l.amount, Some(697.44));
        assert_eq!(l.quantity, Some(1.0));
        assert_eq!(l.details["poids_total"], 132.0);
        assert_eq!(l.details["toutes_bandes"], true);
        assert!(l.label.contains("verrat"));
    }
    #[test]
    fn genetique_reconnait_une_vraie_facture() {
        // Texte réel (extrait via extract_pdf_text) d'une facture Cooperl
        // « ANIMAUX REPRODUCTEURS » — aucun libellé « FACTURE N° »,
        // « NET A PAYER » ni « BASE H.T. » n'existe sur ce modèle (fond de
        // page image) : seuls la ligne « Semaine N° » et le tableau de TVA
        // sans en-tête portent l'information exploitable.
        let text = "D  U  P  L  I  C  A  T  A\nSemaine N°  25.30                    14.41649\n                                                        EI  ORY EMMANUEL\n          ANIMAUX REPRODUCTEURS                         33 LA MELTIERE\n                                                        CHAPELLE-ERBREE (LA)\n           16961             LE 29/07/25\n V/ID : FR06510329899                                   35500  CHAPELLE-ERBREE (LA)\n             21/07/25                 897\n   Livré chez     ELFA ORY EMMANUEL         302 LA BASSE CHEVRIE\n      27    COCHETTE SERENIS                     2            3255,000      1,43     4.654,65\n      26    PRIME COCHETTE SERENIS               2                        197,00     5.122,00\n      26    SERVICE COCHETTE                     2                         24,00       624,00\n       1    COCHETTE SERENIS                     2             120,000\n  ***  Prix moyen reproducteurs hors transport        349,16\n      28                                                      3375,000\n      10400,65    2 5,5%        572,04    10972,69\n        16961            14.41649    MODE DE REGLEMENT =\n        29/07/25                     -------------------\n       10.400,65\n          572,04\n       10.972,69\n       10.972,69\n        11/08/25        10.972,69    VALEUR EN NOTRE TRAITE AU  11/08/25";
        let parsed = parse_document(text).expect("la facture génétique réelle doit être reconnue");
        let line = &parsed.lines[0];
        assert_eq!(line.reference.as_deref(), Some("1441649"));
        assert_eq!(line.date.as_deref(), Some("2025-07-29"));
        assert_eq!(line.quantity, Some(28.0));
        assert_eq!(line.amount, Some(10400.65));
        assert_eq!(
            line.details.get("montant_ht").and_then(Value::as_f64),
            Some(10400.65)
        );
        assert_eq!(
            line.details.get("avoir").and_then(Value::as_bool),
            Some(false)
        );
        assert!(!line.label.starts_with("AVOIR"));
    }

    #[test]
    fn genetique_reconnait_un_vrai_avoir_et_le_relie_a_sa_facture() {
        // Texte réel d'un avoir Cooperl sur la même facture que le test
        // précédent (14.41649 → 1441649). Les quantités et montants sont
        // déjà négatifs dans le texte extrait (tiret final sur chaque
        // nombre) ; la bannière « **** A V O I R ****» a ses lettres
        // espacées par le générateur du PDF et ne matche donc jamais
        // `\bAVOIR\b`, contrairement à « AVOIR SUR FACTURE » qui, lui, est
        // en toutes lettres.
        let text = "D  U  P  L  I  C  A  T  A\nSemaine N°  25.30                    14.41836\n                         **** A V O I R ****            EI  ORY EMMANUEL\n          ANIMAUX REPRODUCTEURS                         33 LA MELTIERE\n                                                        CHAPELLE-ERBREE (LA)\n           16961             LE 12/08/25\n V/ID : FR06510329899                                   35500  CHAPELLE-ERBREE (LA)\n             21/07/25               10750\n AVOIR SUR FACTURE NO 1441649\n ----------------------------\n      26-   PRODUIT COTISATION AUJESKY           2                          3,05        79,30-\n      27-   COCHETTE SERENIS                     2            3255,000-     1,43     4.654,65-\n       1-   COCHETTE SERENIS                     2             120,000-     1,43       171,60-\n      26-   SERVICE COCHETTE                     2                         24,00       624,00-\n      26-   PRIME COCHETTE SERENIS               2                        193,95     5.042,70-\n  ***  Prix moyen reproducteurs hors transport        352,46\n      28-                                                     3375,000-\n      10572,25-   2 5,5%        581,47-   11153,72-\n        16961            14.41836\n        12/08/25       --AVOIR--\n       10.572,25-\n          581,47-\n       11.153,72-\n       11.153,72-\n                        11.153,72-                          **** A V O I R ****";
        let parsed = parse_document(text).expect("l'avoir génétique réel doit être reconnu");
        let line = &parsed.lines[0];
        assert_eq!(line.reference.as_deref(), Some("1441836"));
        assert_eq!(line.quantity, Some(-28.0));
        assert_eq!(line.amount, Some(-10572.25));
        assert_eq!(
            line.details.get("montant_ht").and_then(Value::as_f64),
            Some(-10572.25)
        );
        assert_eq!(
            line.details.get("avoir").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            line.details.get("facture_liee").and_then(Value::as_str),
            Some("1441649")
        );
        assert_eq!(line.label, "AVOIR — 28 cochettes (sur facture 1441649)");
    }

    #[test]
    fn les_postes_demandes_gardent_des_libelles_distincts() {
        assert_eq!(
            canonical_label("PRODUIT COTISATION AUJESKY 12,00"),
            "Cotisation Aujeszky"
        );
        assert_eq!(
            canonical_label("SERVICE COCHETTE 20,00"),
            "Service cochette"
        );
        assert_eq!(
            canonical_label("PRIME COCHETTE SERENIS 30,00"),
            "Prime cochette Serenis"
        );
    }

    /// Les 10 libellés de plus-value demandés en §3 : vérifie qu'ils sont
    /// tous reconnus (`canonical_label` ne retombe pas sur le texte brut
    /// nettoyé, seul indice qu'aucune correspondance de `mappings` n'a joué).
    #[test]
    fn les_10_libelles_de_plus_value_demandes_sont_reconnus() {
        for raw in [
            "PARTICIPATION P.S.A. 0J",
            "+ VALUE R.S.E.",
            "PRIME SOLIDARITE JEUNE 5 CT",
            "+ VALUE QUALIVIANDE PBE",
            "COMPLEMENT COCHON DU DIMANC",
            "+ VALUE CHARTE QUALITE REGI",
            "+ VALUE COOPERL LPF",
            "+ VALUE PORC SANS ANTIBIOTI",
            "+ VALUE QUEUE LONGUE (RSE)",
            "PARTICIPATION COUT RFID",
        ] {
            let label = canonical_label(raw);
            assert_ne!(
                label, raw,
                "« {raw} » n'a été reconnu par aucune entrée de `mappings` (canonicalisé tel quel)"
            );
        }
        // Deux formes du même poste (avec/sans accent, abrégée/complète)
        // doivent fusionner sous le même libellé canonique — sinon
        // apport_ordonne_les_lignes (le regroupement par label) les compte
        // comme deux lignes distinctes au lieu d'une seule cumulée.
        assert_eq!(
            canonical_label("+ VALUE CHARTE QUALITE REGI"),
            canonical_label("+ VALUE CHARTE QUALITÉ RÉGIONALE")
        );
    }
}
