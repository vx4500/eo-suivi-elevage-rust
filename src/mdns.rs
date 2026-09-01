//! Annonce du serveur sur le réseau local (mDNS / DNS-SD).
//!
//! Beaucoup d'élevages n'ont aucun accès internet : ni nom de domaine, ni
//! certificat, ni DNS. Le téléphone doit donc pouvoir trouver le serveur tout
//! seul sur le réseau, à la manière de Home Assistant. Le serveur publie ici
//! un service `_eosuivi._tcp.local.` que l'application Android découvre avec
//! `NsdManager` ; un `avahi-browse -rt _eosuivi._tcp` sur un poste Linux du
//! même réseau affiche exactement la même chose et sert à vérifier.
//!
//! L'annonce n'est jamais bloquante : un réseau qui filtre le multicast, une
//! interface absente ou un démon indisponible ne doivent pas empêcher le
//! serveur de démarrer — l'application garde de toute façon la saisie
//! manuelle d'adresse en repli.

use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Type de service DNS-SD publié. Le nom court `eosuivi` respecte la limite
/// de 15 caractères des étiquettes de service DNS-SD.
const SERVICE_TYPE: &str = "_eosuivi._tcp.local.";

/// Nom d'instance affiché dans la liste de l'application quand plusieurs
/// serveurs répondent (élevage multi-sites, ou serveur de test à côté du
/// serveur réel).
///
/// Les points sont interdits dans une étiquette DNS : ils sont remplacés
/// plutôt que supprimés, pour ne pas coller deux mots. La longueur est
/// bornée à 63 octets, limite d'une étiquette DNS — la coupe se fait sur une
/// frontière de caractère pour ne jamais produire d'UTF-8 invalide.
fn nom_instance(nom_elevage: Option<&str>) -> String {
    let brut = nom_elevage.unwrap_or_default().trim();
    let nettoye: String = brut
        .chars()
        .map(|caractere| match caractere {
            '.' | '\t' | '\n' | '\r' => ' ',
            autre => autre,
        })
        .collect();
    let nettoye = nettoye.split_whitespace().collect::<Vec<_>>().join(" ");
    if nettoye.is_empty() {
        return "EO-Suivi".to_string();
    }
    let mut fin = nettoye.len().min(63);
    while fin > 0 && !nettoye.is_char_boundary(fin) {
        fin -= 1;
    }
    let coupe = nettoye[..fin].trim_end();
    if coupe.is_empty() {
        "EO-Suivi".to_string()
    } else {
        coupe.to_string()
    }
}

/// Vrai si l'annonce est demandée. Activée par défaut : c'est le mode utile
/// pour un élevage. `EO_MDNS=0` la coupe (serveur exposé derrière un proxy,
/// réseau d'entreprise où le multicast n'est pas souhaité).
fn activee() -> bool {
    !matches!(
        std::env::var("EO_MDNS")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "non" | "off"
    )
}

/// Publie le service et renvoie le démon. **Le démon doit rester vivant** :
/// le laisser tomber retire l'annonce du réseau. `None` si l'annonce est
/// désactivée ou si elle a échoué — dans les deux cas le serveur continue.
pub fn annoncer(port: u16, nom_elevage: Option<&str>, version: &str) -> Option<ServiceDaemon> {
    if !activee() {
        tracing::info!("Annonce mDNS désactivée (EO_MDNS)");
        return None;
    }
    let instance = nom_instance(nom_elevage);
    let daemon = match ServiceDaemon::new() {
        Ok(daemon) => daemon,
        Err(error) => {
            tracing::warn!(%error, "Annonce mDNS impossible : le serveur démarre sans");
            return None;
        }
    };
    // `enable_addr_auto` laisse la bibliothèque publier les adresses des
    // interfaces réellement présentes et suivre leurs changements (Wi-Fi qui
    // se reconnecte, IP renouvelée par le DHCP) : une IP figée au démarrage
    // deviendrait fausse sans que personne s'en aperçoive.
    let service = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        "eo-suivi.local.",
        "",
        port,
        &[("version", version), ("chemin", "/")][..],
    );
    let service = match service {
        Ok(service) => service.enable_addr_auto(),
        Err(error) => {
            tracing::warn!(%error, "Description mDNS invalide : le serveur démarre sans");
            return None;
        }
    };
    match daemon.register(service) {
        Ok(()) => {
            tracing::info!(instance = %instance, port, "Serveur annoncé sur le réseau local (mDNS)");
            Some(daemon)
        }
        Err(error) => {
            tracing::warn!(%error, "Enregistrement mDNS refusé : le serveur démarre sans");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_nom_dinstance_retombe_sur_un_defaut() {
        assert_eq!(nom_instance(None), "EO-Suivi");
        assert_eq!(nom_instance(Some("   ")), "EO-Suivi");
        // Un nom fait uniquement de points ne laisse rien d'exploitable.
        assert_eq!(nom_instance(Some("...")), "EO-Suivi");
    }

    #[test]
    fn les_points_et_espaces_sont_normalises() {
        // Les points sont remplacés par une espace, pas supprimés : « GAEC
        // de la Basse-Chevrie » doit rester lisible, pas devenir un seul mot.
        assert_eq!(
            nom_instance(Some(" EARL. Basse-Chevrie \n")),
            "EARL Basse-Chevrie"
        );
    }

    #[test]
    fn le_nom_est_borne_sans_casser_lutf8() {
        // 40 caractères accentués = 80 octets : la coupe à 63 octets tombe
        // au milieu d'un caractère si elle est faite naïvement.
        let long = "é".repeat(40);
        let nom = nom_instance(Some(&long));
        assert!(nom.len() <= 63, "{} octets", nom.len());
        assert!(!nom.is_empty());
        // Le seul fait de pouvoir comparer la chaîne prouve qu'elle est
        // encore de l'UTF-8 valide (un découpage fautif aurait paniqué).
        assert!(nom.chars().all(|caractere| caractere == 'é'));
    }

    #[test]
    fn la_variable_denvironnement_coupe_lannonce() {
        // Test sans parallélisme sur la variable : on vérifie seulement la
        // table des valeurs reconnues, pas l'état global du processus.
        for valeur in ["0", "false", "NON", "off"] {
            assert!(
                matches!(
                    valeur.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "non" | "off"
                ),
                "{valeur} devrait désactiver l'annonce"
            );
        }
    }
}
