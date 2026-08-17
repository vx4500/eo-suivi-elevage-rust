use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_PDF_PAGES: usize = 30;
const MAX_DECOMPRESSED_PAGE: usize = 2 * 1024 * 1024;

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

pub fn parse_document(text: &str) -> Result<ImportDocument, String> {
    let normalized = text
        .replace(['\u{2212}', '\u{2013}', '\u{2014}'], "-")
        .replace('\u{00a0}', " ");
    let upper = normalized.to_uppercase();
    if upper.contains("AUTORENOUVELLEMENT") || is_semence(&upper) {
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
    let compact: String = upper.chars().filter(|character| !character.is_whitespace()).collect();
    if compact.contains("ANIMAUXREPRODUCTEURS")
        || (compact.contains("COCHETTE") && compact.contains("REPRODUCTEURS"))
    {
        return parse_genetique(&normalized);
    }
    if upper.contains("APPORT") && upper.contains("CHARCUTIERS") {
        return parse_apport(&normalized);
    }
    if upper.contains("PRODUITS VETERINAIRES") || upper.contains("PRODUITS VÉTÉRINAIRES") {
        return parse_veto(&normalized);
    }
    if upper.contains("ALIMENTS")
        && (upper.contains("SILOS")
            || upper.contains("DÉSIGNATION PRODUIT")
            || upper.contains("DESIGNATION PRODUIT"))
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
        .replace(['\u{00a0}', '\u{202f}', ' '], "")
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
    for format in ["%d/%m/%Y", "%d/%m/%y"] {
        if let Ok(date) = NaiveDate::parse_from_str(&normalized, format) {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
    None
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

fn parse_aliment(text: &str) -> Result<ImportDocument, String> {
    let reference = capture(
        text,
        r"(?i)ACTURE\s*N[°ºo:]*\s*([0-9][0-9. ]{4,})",
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
    let regex = Regex::new(
        r"(?m)^(.+?)\s+(MI|FE|GR)\s+([0-9]+)\s+([0-9.,]+)(-?)\s*\*?\s+[0-9.,]+\s+([0-9.,]+)\s+[0-9]+\s+([0-9.,]+)(-?)\s*$",
    )
    .map_err(|error| format!("analyse aliment indisponible: {error}"))?;
    let credit = document_sign(text);
    let mut lines = Vec::new();
    for row in regex.captures_iter(text) {
        let product = format!("{} {}", row[1].split_whitespace().collect::<Vec<_>>().join(" "), &row[2]);
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
            date: date.clone(),
            reference: reference.clone(),
            label: product.clone(),
            quantity: tonnage,
            unit_price,
            amount,
            details: json!({
                "fournisseur": "Cooperl Nutrition",
                "produit": product,
                "silo": row[3].to_string(),
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
    let reference = capture(text, r"(?i)ACTURE\s*N[°ºo]?\s*([0-9]{6,})", 1);
    let date = capture(text, r"(?i)\bLE\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})", 1)
        .or_else(|| {
            capture(
                text,
                r"(?i)FACT\.?\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})",
                1,
            )
        })
        .and_then(|value| iso_date(&value));
    let regex = Regex::new(
        r"(?m)^([0-9]+)\s+([A-ZÀ-ÖØ-Þ].+?)\s+4\s+[0-9 ]+?\s+([0-9.,]+)\s+([0-9.,]+)(-?)\s*$",
    )
    .map_err(|error| format!("analyse vétérinaire indisponible: {error}"))?;
    let credit = document_sign(text);
    let mut lines = Vec::new();
    for row in regex.captures_iter(text) {
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
            date: date.clone(),
            reference: reference.clone(),
            label: label.clone(),
            quantity,
            unit_price,
            amount,
            details: json!({
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
    let reference = capture(text, r"(?i)\b(FAC[0-9A-Z-]+)\b", 1).or_else(|| {
        capture(
            text,
            r"(?i)NUM[ÉE]RO\s+DE\s+FACTURE\s+([A-Z0-9-]+)",
            1,
        )
    });
    let date = capture(
        text,
        r"(?i)(?:DATE|FACTURE)\s*(?:DE|DU)?\s*[:.]?\s*([0-9]{2}[/.][0-9]{2}[/.][0-9]{2,4})",
        1,
    )
    .or_else(|| capture(text, r"([0-9]{2}[/.][0-9]{2}[/.][0-9]{2,4})", 1))
    .and_then(|value| iso_date(&value));
    let mut ht = labeled_amount(text, &["TOTAL HT"]);
    let mut ttc = labeled_amount(text, &["TOTAL TTC", "MONTANT TTC", "TTC A PAYER", "TTC À PAYER"]);
    let fee = upper.contains("AUTORENOUVELLEMENT");
    if fee {
        for line in text.lines().filter(|line| line.to_uppercase().contains("AUTORENOUVELLEMENT")) {
            if let Some(value) = last_amount(line) {
                ht = Some(value.abs());
            }
        }
        if ttc.is_none() {
            ttc = capture(text, r"(?i)EUR\s+([0-9][0-9 .\u{202f}]*,[0-9]{2})", 1)
                .and_then(|value| number(&value));
        }
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
    } else if upper.contains("PIETRAIN") || upper.contains("PIÉTRAIN") || upper.contains("DOSE IA") {
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
    let compact: String = text.chars().filter(|character| !character.is_whitespace()).collect();
    let reference = capture(
        &compact,
        r"(?i)FACTUREN[°ºo]*([0-9.]+)",
        1,
    )
    .map(|value| value.replace('.', ""));
    let date = capture(
        text,
        r"(?i)\bLE\s*([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
        1,
    )
    .or_else(|| {
        capture(
            &compact,
            r"(?i)LIVRAISONDU([0-9]{1,2}/[0-9]{1,2}/[0-9]{2,4})",
            1,
        )
    })
    .and_then(|value| iso_date(&value));
    let animals = Regex::new(r"(?m)^([0-9]+)\s+COCHETTE\s+\w+\s+[0-9]+\s+([0-9.,]+)")
        .map_err(|error| format!("analyse génétique indisponible: {error}"))?;
    let mut count = 0_i64;
    let mut weight = 0.0;
    for row in animals.captures_iter(text) {
        count += integer(&row[1]).unwrap_or_default();
        weight += number(&row[2]).unwrap_or_default();
    }
    let average = capture(
        text,
        r"(?i)Prix\s*moyen\s*reproducteurs[^0-9]*([0-9.,]+)",
        1,
    )
    .and_then(|value| number(&value));
    let net = capture(&compact, r"(?i)NETAPAYER([0-9.,]+)(-?)", 1)
        .or_else(|| capture(&compact, r"(?i)TOTALT\.?T\.?C\.?([0-9.,]+)(-?)", 1))
        .and_then(|value| number(&value));
    let ht = capture(&compact, r"(?i)BASEH\.?T\.?([0-9.,]+)(-?)", 1)
        .and_then(|value| number(&value));
    let sign = document_sign(text);
    let amount = net.or(ht).map(|value| value.abs() * sign);
    let label = format!(
        "{}{}",
        if sign < 0.0 { "AVOIR — " } else { "" },
        if count > 0 {
            format!("{count} cochettes")
        } else {
            "Cochettes / reproducteurs".into()
        }
    );
    let line = ImportLine {
        kind: "genetique".into(),
        date: date.clone(),
        reference: reference.clone(),
        label: label.clone(),
        quantity: (count > 0).then_some(count as f64),
        unit_price: average,
        amount,
        details: json!({
            "fournisseur": "Cooperl",
            "designation": label,
            "nb_animaux": (count > 0).then_some(count),
            "poids_total": (weight > 0.0).then_some((weight * 10.0).round() / 10.0),
            "prix_moyen": average,
            "montant_ht": ht.map(|value| value.abs() * sign),
            "montant_net": net.map(|value| value.abs() * sign),
            "num_facture": reference,
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
    let reference = capture(text, r"(?i)APPORT\s*N[°ºo]?\s*([0-9]{6,})", 1);
    let date = capture(
        text,
        r"(?i)ENLEVEMENT\s*DU\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})",
        1,
    )
    .or_else(|| capture(text, r"(?i)D\s*U\s+([0-9]{2}/[0-9]{2}/[0-9]{2,4})", 1))
    .or_else(|| capture(text, r"(?i)\bLE\s*([0-9]{2}/[0-9]{2}/[0-9]{2,4})", 1))
    .and_then(|value| iso_date(&value));
    let week = capture(text, r"(?i)Semaine\s*N[°ºo]?\s*([0-9./]+)", 1);
    let total_net = captures_all(text, r"(?i)NET\s*A\s*PAYER\s*E?\s*([0-9.,]+)\s*E?", 1)
        .last()
        .and_then(|value| number(value));
    let global_price = capture(text, r"(?i)Prix moyen porc\s*:?\s*([0-9.,]+)", 1)
        .and_then(|value| number(&value));
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
            gross: total_net,
            muscle_range: None,
            muscle_lot: None,
            technical_value: None,
        }]
    } else {
        lots
    };
    let gross_total: f64 = lots.iter().filter_map(|lot| lot.gross).sum();
    let economic_lines = parse_economic_lines(text, reference.as_deref(), date.as_deref());
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
        let amount = match (total_net, lot.gross) {
            (Some(net), Some(gross)) if gross_total.abs() > f64::EPSILON => {
                Some((net * gross / gross_total * 100.0).round() / 100.0)
            }
            (_, gross) => gross,
        };
        let average_weight = match (lot.weight, lot.pigs) {
            (Some(weight), Some(pigs)) if pigs > 0 => Some((weight / pigs as f64 * 100.0).round() / 100.0),
            _ => None,
        };
        let label = match (&lot.reference, &lot.bon) {
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
                "montant_net": amount,
                "tmp": lot.muscle_lot,
                "muscle_gamme": lot.muscle_range,
                "muscle_lot": lot.muscle_lot,
                "total_retenues": (index == 0 && retention_total > 0.0).then_some(retention_total),
                "lots_json": lots_json,
            }),
        });
    }
    lines.extend(economic_lines);
    finish_document("apport", lines, reference, date)
}

fn split_lots(text: &str) -> Vec<(Option<String>, String)> {
    let Ok(regex) = Regex::new(r"(?i)Bon\s*n[°ºo]?\s*([0-9]+)") else {
        return vec![(None, text.to_string())];
    };
    let matches: Vec<_> = regex.captures_iter(text).collect();
    if matches.is_empty() {
        return vec![(None, text.to_string())];
    }
    matches
        .iter()
        .enumerate()
        .filter_map(|(index, captures)| {
            let body_start = captures.get(0)?.end();
            let body_end = matches
                .get(index + 1)
                .and_then(|next| next.get(0))
                .map_or(text.len(), |value| value.start());
            Some((
                captures.get(1).map(|value| value.as_str().to_string()),
                text[body_start..body_end].to_string(),
            ))
        })
        .collect()
}

fn parse_lot(bon: Option<String>, body: &str) -> Option<ApportLot> {
    let reference = lot_reference(body);
    let total = Regex::new(r"(?i)Total\s*Bon\.+\s*([0-9.,]+)\s+([0-9.,]+)")
        .ok()?
        .captures(body);
    let weight = total
        .as_ref()
        .and_then(|row| row.get(1))
        .and_then(|value| number(value.as_str()));
    let gross = total
        .as_ref()
        .and_then(|row| row.get(2))
        .and_then(|value| number(value.as_str()));
    let animal_regex = Regex::new(
        r"(?im)^\s*([0-9]+)\s+(SAISI|CREVE|CREVÉ|CREVEE|PORC|LEGER|LÉGER|LOURD|COEUR)",
    )
    .ok()?;
    let pigs: i64 = animal_regex
        .captures_iter(body)
        .filter_map(|row| row.get(1))
        .filter_map(|value| integer(value.as_str()))
        .sum();
    let muscle = Regex::new(
        r"(?i)muscle\s*:\s*de la gamme\s*([0-9.,]+)\s*du lot\s*([0-9.,]+)",
    )
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
    let technical_value = capture(
        body,
        r"(?i)Value\s+Technique\s*:?\s*([0-9.,]+)\s*cts",
        1,
    )
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
    let regex = Regex::new(r"\b[A-Z0-9]{4,5}\b").ok()?;
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for value in regex.find_iter(text).map(|value| value.as_str()) {
        if value.chars().any(|character| character.is_ascii_alphabetic())
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

fn parse_economic_lines(text: &str, reference: Option<&str>, date: Option<&str>) -> Vec<ImportLine> {
    let Ok(keyword) = Regex::new(
        r"(?i)(\+\s*VALUE|PRIME\s+SOLIDARITE|COMPLEMENT|PARTICIPATION|FRAIS\s+DE\s+GROUPEMENT|SERVICE\s+PUBLIC|EQUARRISSAGE|ÉQUARRISSAGE|CVEE|CONTRIBUTION\s+SANITAIRE|COTISATION)",
    ) else {
        return Vec::new();
    };
    let mut ordered = Vec::<(String, String, f64)>::new();
    for raw in text.lines() {
        if !keyword.is_match(raw)
            || raw.to_uppercase().contains("VALUE TECHNIQUE")
            || raw.to_uppercase().contains("PLUS VALUE")
        {
            continue;
        }
        let Some(signed) = last_amount(raw) else {
            continue;
        };
        let label = canonical_label(raw);
        let forced = is_forced_retention(&label);
        let kind = if forced || signed < 0.0 || document_sign(text) < 0.0 {
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

fn canonical_label(raw: &str) -> String {
    let upper = raw.to_uppercase();
    let compact: String = upper.chars().filter(|character| character.is_alphabetic()).collect();
    let mappings = [
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
    let range_rate = capture(text, r"(?i)([0-9]+)%\s*dans la gamme", 1)
        .and_then(|value| number(&value));
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
        label: format!("Synthèse Uniporc {}", frappe.as_deref().unwrap_or("sans frappe")),
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
        warnings.push("Numéro de facture ou d'apport non détecté : la confirmation est bloquée".into());
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
    fn apport_classe_les_frais_en_retenues() {
        let text = "APPORT N° 123456 CHARCUTIERS\nENLEVEMENT DU 16/08/2026\nBon n° 42\nDA915 DA915\n63 PORC\nTotal Bon..... 5700,00 10000,00\nmuscle : de la gamme 62,1 du lot 61,3\nFRAIS DE GROUPEMENT 2 ABC 120,00\nPRIME SOLIDARITE JEUNE 2 ABC 80,00\nNET A PAYER 9960,00";
        let Ok(parsed) = parse_document(text) else {
            panic!("le document d'apport doit être analysé");
        };
        assert!(parsed.lines.iter().any(|line| line.kind == "vente"));
        let Some(retention) = parsed
            .lines
            .iter()
            .find(|line| line.kind == "retenue") else {
                panic!("la retenue doit être analysée");
            };
        assert_eq!(retention.label, "Frais de groupement");
        assert_eq!(retention.amount, Some(-120.0));
        assert!(parsed.lines.iter().any(|line| line.kind == "valorisation"));
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
}
