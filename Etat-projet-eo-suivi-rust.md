# EO-Suivi Élevage Rust — État du projet

Version actuelle : **2.2.4** — Dernière mise à jour de ce document : 18 août 2026.

Ce fichier remplace et fusionne : `AUDIT-PORTAGE-RUST-2.1.2.md`,
`DEMANDES-MODIFICATIONS-EOELEVAGE.md`, `LISTING-APPLICATION-EO-SUIVI.md`,
`MISES-A-JOUR-RESTANTES.md` et `PORTAGE-RUST.md`. Les informations désormais
obsolètes (anciens comptages de routes, tâches déjà livrées, doublons entre
documents) ont été retirées. `VERSIONS-RUST.md` (historique détaillé version
par version) et `MISE-A-JOUR-DEBIAN13.md` (procédure serveur) restent des
fichiers séparés et ne sont pas dupliqués ici au-delà d'un résumé.

---

## 1. Résumé

Le portage Python 1.65 → Rust est fonctionnellement avancé : le socle
(authentification, SQLite, sécurité, sauvegarde/mise à jour réversible), la
reproduction, les transferts/effectifs, l'économie avec imports PDF, la
productivité (GTTT centralisé), la vente directe, l'eau/électricité et la
saisie rapide sont opérationnels. Les 68 routes historiques encore absentes
après l'audit du 16/08/2026 ont été reliées en 2.2.0 et sont couvertes par un
test de parité (`tests/route_parity.rs`). Les priorités actuelles portent sur
la fiabilité des effectifs, la prévision d'aliment, la GTE, les rappels
sanitaires et la fiche de mise-bas A4.

---

## 2. Fonctions livrées

- Saisie rapide (bouton « + ») : ELD, poids, NEC, IA, écho, chaleur, pertes,
  sorties et mouvements de porcs.
- Transferts par bande ou unité, avec bâtiment/salle/case de destination.
- Suivi de bande : dates clés, effectif, emplacements, marquage, avance/retard
  sur le départ ou la vente.
- Productivité : GTTT centralisé, objectifs, résultats par bande, données
  techniques d'abattage (TMP, poids, plus-value).
- Économie : coûts par bande, avoirs, valorisations/retenues, variantes
  aliment GR/FE/MI, apports à plusieurs lots.
- Import PDF : aperçu, détection des doublons, transaction globale,
  annulation, journal, import simultané de 1 à 5 PDF (2.2.3).
- Reclassement automatique des avoirs génétiques 1443203 et 1441836, et des
  montants à signe moins final.
- File IA/cochettes après sevrage, affectation groupée à une bande, capacité
  de verraterie paramétrable (31 places par défaut) — 2.2.4.
- Plan sanitaire, pharmacie, liste des porcs traités (hors vaccins).
- Prestataire, RFID, inventaires par case, structure ordonnable,
  eau/électricité, calendrier ICS.
- Sauvegarde SQLite vérifiée, restauration contrôlée, mise à jour Debian
  réversible avec rollback automatique.
- Tri croissant/décroissant sur les tableaux (classe `.rust-sortable`).

*(Détail version par version : voir `VERSIONS-RUST.md`.)*

---

## 3. Reste à faire — priorités

### Fiabilité des données
- [ ] Fiabiliser les effectifs réels (anciens inventaires incohérents,
      mortalités avec un stade erroné).
- [x] Corriger l'affectation du stade lors des déclarations de mortalité et
      détecter les effectifs incohérents. *(Le stade n'est plus saisi
      librement : il est recalculé côté serveur à partir de l'âge réel de la
      bande à la date déclarée, avec le même calendrier que les fiches bande.
      L'effectif déclaré est désormais vérifié contre l'effectif total suivi
      de la bande même sans case précisée.)*
- [ ] Rattacher les porcs charcutiers à leur bande d'origine (au lieu d'un
      total générique d'engraissement).

### Conduite d'élevage et productivité
- [ ] Produire une GTE complète en complément de la GTTT.
- [ ] Afficher les ELD dans les fiches truies **sans** garder la moyenne ELD
      par bande.
- [ ] Afficher TMP et données techniques d'abattage par bande.
- [ ] Afficher stade, emplacement actuel et effectif réel des porcs.
- [ ] Finaliser la fiche de mise-bas au format A4 définitif.
- [ ] Permettre d'ordonner librement les salles dans l'implantation.

### Sanitaire
- [ ] Rappels sanitaires calculés séparément pour truies, cochettes,
      porcelets et verrats (avec historique).

### Aliment et stock
- [ ] Prévision de consommation et de commande d'aliment par bande avant
      rechargement.
- [ ] Importer les consommations des machines à soupe.

### Économie et imports (demandes en attente)
- [ ] Permettre deux lots sur une même facture d'apport Cooperl.
- [ ] Fusionner les variantes de « Charte Qualité Régionale » et vérifier les
      doublons similaires sur toutes les pages.
- [ ] Reconnaître les libellés de plus-value suivants :
      `PARTICIPATION P.S.A. 0J`, `+ VALUE R.S.E.`,
      `PRIME SOLIDARITE JEUNE 5 CT`, `+ VALUE QUALIVIANDE PBE`,
      `COMPLEMENT COCHON DU DIMANC`, `+ VALUE CHARTE QUALITE REGI`,
      `+ VALUE COOPERL LPF`, `+ VALUE PORC SANS ANTIBIOTI`,
      `+ VALUE QUEUE LONGUE (RSE)`, `PARTICIPATION COUT RFID`.
- [ ] Intégrer le tableau « Cahiers des charges — valorisations » dans
      Économie et retirer sa page séparée.
- [ ] Supprimer l'estimation prévisionnelle (obsolète).
- [ ] Regrouper « Lier automatiquement » avec l'import Cooperl et renommer
      l'action « Importer des documents PDF ».
- [ ] Ajouter un modèle d'import des factures génétiques, téléchargeable.

### Présentation et navigation
- [ ] Retirer l'ancien texte générique de mise à jour (« remplacer le dossier
      de l'application »).
- [ ] Continuer à simplifier la navigation et regrouper les pages proches
      dans des sous-menus (voir structure proposée en §5).

### Avec service externe (nécessite configuration explicite de l'éleveur)
- [ ] Sauvegardes automatiques vers NAS, stockage en ligne ou messagerie.
- [ ] Synchronisation/export des échéances vers Google Calendar et Outlook.

---

## 4. Feuille de route (versions futures)

| Version | Contenu prévu |
|---|---|
| 2.3.0 | Reproduction, cycles, GTTT/GTE complets |
| 2.4.0 | Effectifs, stock, transferts et bâtiments |
| 2.5.0 | Économie, abattoir et vente directe |
| 2.6.0 | Administration, sauvegarde, mise à jour, finition des écrans |
| Hors calendrier | OCR des scans/photos, imports Excel spécialisés, imports automatiques de référentiels techniques externes, synchronisation directe Gmail/Outlook et stockage cloud (identifiants propres à chaque fournisseur), installateur graphique Windows |

---

## 5. Navigation cible (référence)

Structure proposée pour réduire le nombre de pages visibles (fonctions
secondaires dans des sous-menus) :

Accueil · Productivité · Planning · Bandes · Truies (IA/cochettes, réformes,
attente et réaffectation) · Charcutiers · Comparaison · Plan sanitaire ·
Stocks · Économie · Implantation · Entretien · Paramètres (utilisateurs,
archives, journal) · Contact.

---

## 6. État du portage Python 1.65 → Rust

- La base Python 1.65 comptait 215 routes HTTP pour environ 8 200 lignes de
  code dans son fichier principal. Le portage est une réécriture du moteur,
  pas une conversion automatique.
- Les 68 chemins historiques restés absents après l'audit du 16/08/2026 ont
  été reliés en 2.2.0 et sont couverts par `tests/route_parity.rs`. Le garde-
  fou `501 Not Implemented` ne concerne plus que les URL réellement
  inconnues.
- Dernière validation sur la sauvegarde du 16 août 2026 : intégrité SQLite
  conforme, aucune clé étrangère cassée, 51 tables comparées à la base
  réelle, requêtes principales exécutées sur une copie, aucune donnée
  personnelle incluse dans l'archive de contrôle.

### Règle de validation avant tout déploiement

1. Copie horodatée de la base réelle.
2. Migration appliquée uniquement sur la copie.
3. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
4. Tests HTTP des routes concernées avec les quatre rôles.
5. Vérification des totaux avant/après import.
6. Sauvegarde puis déploiement atomique avec rollback possible.

La base de production `/var/lib/eo-suivi/elevage.db` ne doit jamais être
utilisée pour développer ou tester une migration.

---

## 7. Déploiement et mise à jour du serveur (Debian 13)

Chemins de production (ne changent pas) :
- sources : `/opt/eo-suivi-rust-src`
- application : `/opt/eo-suivi-rust`
- base : `/var/lib/eo-suivi/elevage.db`
- sauvegardes : `/var/backups/eo-suivi-rust`
- service : `eo-suivi-rust`

La base ne doit jamais être copiée dans le dépôt source. Les versions Python
et Rust ne doivent jamais écrire simultanément dans le même fichier SQLite.

**Mise à jour recommandée** (en `root`) :

```bash
cd /opt/eo-suivi-rust-src
GIT_SSH_COMMAND='ssh -i /root/.ssh/eo-suivi-rust-github -o IdentitiesOnly=yes' \
git pull --ff-only origin main
./scripts/mettre-a-jour-debian13.sh
```

Déploiements suivants, en une seule commande :

```bash
/opt/eo-suivi-rust-src/scripts/mettre-a-jour-debian13.sh
```

Le script :
1. empêche deux mises à jour simultanées (verrou système) ;
2. refuse les modifications locales suivies par Git ;
3. récupère `main` uniquement en avance rapide ;
4. exécute `cargo check`, Clippy, les tests et la compilation release sans
   interrompre le service ;
5. crée une sauvegarde SQLite vérifiée par `PRAGMA quick_check` ;
6. conserve l'ancien binaire et l'ancienne unité systemd ;
7. remplace le binaire atomiquement, redémarre le service et contrôle la page
   de connexion ainsi que le numéro de version ;
8. restaure automatiquement l'ancienne version si l'installation ou le
   contrôle HTTP échoue.

Un échec de compilation laisse l'application en service. Un échec après
l'arrêt déclenche le retour arrière ; la sauvegarde de la base est conservée.

**Vérification manuelle :**

```bash
systemctl status eo-suivi-rust --no-pager -l
journalctl -u eo-suivi-rust -n 80 --no-pager -l -o cat
curl -fsS http://127.0.0.1:8080/login | grep -F "Version Rust 2.2.4"
```

Puis ouvrir `https://rust-elevage.basse-chevrie.ovh`.
