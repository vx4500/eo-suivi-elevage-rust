# EO-Suivi Élevage Rust — État du projet

Version actuelle : **2.2.6** — Dernière mise à jour de ce document : 19 août 2026.

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
test de parité (`tests/route_parity.rs`). Le chantier de mise en conformité
avec la spécification complète (types d'élevage, réception d'achats, GTE,
modules optionnels, capacités par étape, prévision d'aliment, rappels
sanitaires, fiche de mise-bas A4 — voir §8) a couvert ses 7 phases fin
août 2026 ; la priorité actuelle porte sur la fiabilité des effectifs
(anciens inventaires incohérents) et les demandes en attente du §3.

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
- [ ] Corriger l'affectation du stade lors des déclarations de mortalité et
      détecter les effectifs incohérents.
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
curl -fsS http://127.0.0.1:8080/login | grep -F "Version Rust 2.2.6"
```

Puis ouvrir `https://rust-elevage.basse-chevrie.ovh`.

---

## 8. Chantier — mise en conformité avec la spécification « naisseur-engraisseur évolutif »

Suivi de l'audit du 19/08/2026 comparant l'application à la spécification
complète (petit élevage familial → structure multi-sites, principe « la
complexité s'active, elle ne s'impose pas »). Chaque phase est une branche et
une PR séparée, testée (`cargo build`/`clippy`/`test`) avant fusion — pas de
gros commit unique sur une application en production.

- [x] **Phase 1 — Type d'élevage (§0bis).** Réglage `parametre.type_elevage`
      (5 profils : naisseur-engraisseur, naisseur, post-sevreur seul,
      post-sevreur-engraisseur, engraisseur seul), écran dans Paramètres,
      propagé dans la session (`SessionData::type_elevage`, rafraîchi en
      direct pour les sessions déjà ouvertes) et utilisé pour masquer les
      menus Reproduction/Charcutiers-Prestataire selon le profil actif.
      Prépare les phases suivantes (§1bis, §7 GTE adaptée).
- [x] **Phase 2 — Réception d'animaux achetés (§1bis).** Table
      `receptionachat` (date, fournisseur, n° bon de livraison, lot d'origine
      fournisseur conservé comme traçabilité, effectif, poids moyen/total,
      prix), écran `/reception` avec formulaire + historique, affectation
      directe à une salle/case (insère aussi un transfert), alerte quarantaine
      à l'arrivée (bannière tant que `quarantaine_jusqu` n'est pas dépassé).
      La réception alimente le registre de mouvements existant
      (`mouvementstock`) au lieu d'une valeur figée, et sa suppression annule
      proprement le mouvement et le transfert associés (pas d'effectif
      fantôme). Visible uniquement pour les profils qui reçoivent des achats
      (`session.recoit_achats()`).
- [x] **Phase 3 — GTE complète (§7), adaptée au type d'élevage.** Nouvel
      écran `/gte` : indice de consommation (IC, kg aliment/kg produit) par
      lot, coût alimentaire par porc produit, marge sur coût alimentaire
      (MSA), marge brute par truie et taux de renouvellement du cheptel (12
      mois glissants). Les colonnes et la section « Renouvellement » propres
      aux truies ne s'affichent que pour les profils qui en conduisent
      (`session.a_truies()`, Phase 1) — vérifié en conditions réelles en
      basculant un lot de naisseur-engraisseur à post-sevreur. 5 fonctions de
      calcul pures et testées unitairement (indice_consommation,
      cout_alimentaire_par_porc, marge_sur_cout_alimentaire,
      marge_brute_par_truie, taux_renouvellement_pct). *Reste hors périmètre
      de cette phase : l'imputation explicite du coût d'achat d'un animal
      entrant comme charge d'entrée pour les profils achats (Phase 2), qui
      affinera le MSA de ces profils dans un prochain incrément.*
- [x] **Phase 4 — Modules optionnels (§0, §2, §4).** Cases à cocher
      « Génétique avancée » (décochée par défaut) et « Prestataires externes »
      (cochée par défaut, préserve le comportement des bases existantes) dans
      Paramètres. Catalogue de lignées (`/genetique` : nom, fournisseur,
      index prolificité/croissance/IC, contrat de renouvellement) visible
      uniquement module actif. Lien « Prestataire » masqué quand le module
      est désactivé (Charcutiers/engraissement restent disponibles : seule la
      sous-traitance externe est masquée). Piège vérifié en conditions
      réelles : la page Paramètres a deux formulaires distincts postant vers
      `/parametres/maj` (type d'élevage+modules, et informations générales) —
      soumettre l'un ne doit pas réinitialiser les cases à cocher de l'autre ;
      un garde-fou (`form.contains_key("type_elevage")`) l'empêche, testé par
      soumission successive des deux formulaires. *Reste hors périmètre : le
      rattachement d'une truie à une lignée du catalogue (actuellement
      `truie.race` en texte libre uniquement) — prochain incrément naturel.*
- [x] **Phase 5 — Capacités par étape (§0, §3).** Trois nouveaux réglages
      (`capacite_maternite`=60, `capacite_postsevrage`=300,
      `capacite_engraissement`=800, éditables comme `capacite_verraterie`
      existant dans Réglages de conduite) et une carte « Capacités par
      étape » sur le tableau de bord : occupation recalculée à partir des
      truies/porcs réellement présents dans les salles correspondantes
      (`salle.type`), jamais une valeur figée. Limitée aux phases réellement
      actives pour le type d'élevage (verraterie/maternité seulement si
      `a_truies()`, engraissement seulement si `engraisse()`) — vérifié en
      conditions réelles en plaçant une truie en maternité (occupation passe
      à 1/60) puis en basculant vers un profil post-sevreur (seul le
      post-sevrage reste affiché). *Conduite en continu / hors bande : déjà
      fonctionnellement supportée (bande_code nullable, écran « Sans bande
      active » existant) — non repris ici faute de gain identifié au-delà de
      l'existant ; à documenter comme mode nommé si un besoin utilisateur
      concret apparaît.*
- [x] **Phase 6 — Prévision d'aliment et rappels sanitaires par catégorie.**
      *Aliment* : écran `/aliment-previsions`, silos + relevés de niveau
      manuels (même principe que `compteur_energie`/`releve_compteur`
      existant) ; la consommation quotidienne se déduit par bilan de matière
      entre deux relevés et les livraisons reçues entre-temps, avec
      estimation du nombre de jours avant rupture. *Sanitaire* : les colonnes
      `cible`/`reference`/`jour`/`rappel` d'`acteprotocole` existaient déjà en
      base mais n'étaient exploitées par aucun calcul ; une section
      « Rappels » sur `/sanitaire` liste maintenant, par catégorie, les actes
      marqués « rappel » dont l'échéance (mise-bas + décalage en jours) est
      atteinte pour une bande active et pas encore réalisée, avec repère
      visuel « en retard ». Un bug réel a été trouvé et corrigé en testant en
      conditions réelles : `printf('+%d day', jour)` produisait un modificateur
      SQLite invalide (`+-14 day`) pour un décalage négatif (ex. vaccin avant
      mise-bas) ; corrigé en `printf('%+d day', jour)`. *Limite connue :* les
      rappels verrats ne sont pas couverts (`acterealise.bande_id` est
      obligatoire, un verrat n'appartient pas à une bande) — resterait à
      traiter dans un incrément dédié si le besoin se confirme.
- [x] **Phase 7 — Fiche de mise-bas au format A4 définitif (§9).** Nouvel
      écran `/bande/{id}/fiche-mise-bas`, mise en page dédiée (`@page { size:
      A4; margin: 15mm; }`) distincte du dump générique `impression.html` :
      en-tête avec nom d'élevage (paramètre `nom_elevage`), code de bande,
      date de mise-bas et site, ligne de totaux (truies, nés totaux/vifs,
      mort-nés, momifiés, sevrés), tableau détaillé par truie (même requête
      que l'export CSV existant, pour rester cohérent entre les deux formats)
      et zone de signature éleveur/vétérinaire pour l'archivage papier du
      registre d'élevage. Lien ajouté sur la fiche bande, à côté de l'export
      CSV existant. Vérifié en conditions réelles avec et sans données.

Chaque case cochée référence sa PR dans l'historique Git de ce dépôt.
