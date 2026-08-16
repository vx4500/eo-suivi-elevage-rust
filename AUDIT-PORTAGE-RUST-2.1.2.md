# Audit du portage EO-Suivi Python 1.65 vers Rust 2.1.2

Date de l'audit : 16 août 2026

## Conclusion

La version Rust 2.1.2 n'est pas encore un remplacement fonctionnel complet de la
version Python 1.65. Elle conserve la base SQLite et couvre déjà plusieurs parcours
importants, mais une partie des pages est une vue générique et de nombreuses actions
métier ne sont pas encore reliées.

L'écart mesuré automatiquement est le suivant :

| Élément comparé | Python 1.65 | Rust 2.1.2 | Écart |
|---|---:|---:|---:|
| Routes déclarées | 203 | 115 | 101 routes Python absentes en Rust |
| Modèles HTML dédiés | 51 | 23 | 34 modèles Python absents en Rust |
| Fichiers de tests métier | 10 | 1 | couverture métier très incomplète |
| Tests Rust actuels | — | 3 avant audit | authentification et schéma seulement |

Les nombres de routes ne s'additionnent pas directement : le Rust possède aussi
13 routes nouvelles ou renommées, notamment le module Eau & électricité. Ils donnent
néanmoins une mesure fiable du travail restant.

## Avancement réalisé dans Rust 2.1.3

Le lot 2.1.3 corrige les points les plus risqués trouvés par cet audit : droits
interdits par défaut, import des truies avec aperçu et transaction, GTTT unique,
stock initial par case, Prestataire, Structure, Sanitaire, Pharmacie, Charcutiers,
IFIP, Productivité, Réformes, Cochettes, signes comptables et retenues économiques,
ainsi que l'import transactionnel Eau & électricité. La liste exacte est tenue dans
`VERSIONS-RUST.md`.

Les fonctions qui restent absentes ne renvoient plus une page vide laissant croire
à une réussite : elles répondent explicitement « non encore portée » sans modifier
la base. Les prochains lots restent nécessaires pour reprendre les imports PDF, les
documents PDF/QR, la restauration depuis l'interface et les communications externes.

## Ce qui est déjà réellement présent en Rust

### Socle

- Serveur Axum, modèles MiniJinja et accès SQLite avec SQLx.
- Lecture de la base Python existante et migration prudente du schéma.
- Authentification compatible avec les mots de passe PBKDF2 Python.
- Changement obligatoire du mot de passe initial, blocage après échecs et CSRF sur
  les formulaires Rust reliés.
- Version affichée automatiquement dans le pied de page.
- Service systemd, Docker, sauvegarde et procédure de mise à jour GitHub.

### Élevage et reproduction

- Tableaux de bord, listes et fiches de bandes et de truies.
- Ajout de bandes et truies, affectation à une bande, réforme, mesures et pertes.
- Inséminations groupées et événements génériques.
- Transferts de porcs et de truies avec transaction SQLite.
- Refus d'un transfert supérieur à l'effectif disponible.
- Refus d'un transfert dépassant la capacité de la case de destination.
- Inventaires d'effectifs et page d'état des données simplifiée.

### Autres modules partiellement opérationnels

- Économie : saisies simples aliment, vétérinaire, vente, semence et génétique.
- Vente directe : produits, commandes, sessions, charges, coûts et stocks simples.
- Eau & électricité : compteurs, relevés, rappels, remplacement et import CSV.
- Abattoir, cahiers des charges, quotidien, entretien, structure et tâches : fonctions
  de base présentes.

## Fonctions incomplètes ou seulement génériques

Les pages suivantes existent comme route Rust, mais ne reprennent pas encore toute
l'interface et tous les calculs Python :

| Domaine | État Rust 2.1.2 | Travail restant principal |
|---|---|---|
| GTTT | Incomplet | filtrage période/bande/cycle, calcul portée par portée, vues bandes/cheptel/rang |
| Productivité | Incomplet | croisement imports et événements réels, objectifs, vues complètes |
| IFIP | Incomplet | modification des références et comparaisons complètes |
| Réformes/cochettes | Incomplet | critères et seuils modifiables, génétique cochette |
| Sanitaire | Incomplet | protocoles, actes, traitements, fait/non fait, rappels |
| Pharmacie | Lecture générique | mouvements, réglages, seuils et écritures |
| Stock | Lecture générique | doses, stock initial par case, prévisions aliment |
| Structure | Incomplet | modification, suppression, RFID et ordre des salles |
| Planning | Lecture générique | présentation métier et actions associées |
| Charcutiers | Lecture générique | fiche individuelle, traitements et suppressions |
| Engraissement | Incomplet | déclarations prestataire, mortalité, consignes et suivi |
| Journal | Lecture générique | journalisation systématique et suppression contrôlée |
| Paramètres | Incomplet | objectifs, aliments, démonstration et réglages complets |
| Vente directe | Incomplet | fiche session, modification commande, impressions et communications |
| Sauvegarde | Incomplet | restauration contrôlée et tests de sauvegarde/restauration |

## Routes Python prioritaires absentes

La comparaison complète détecte 101 chemins manquants. Les familles prioritaires
sont regroupées ci-dessous.

### Imports et économie

- `/import` et `/import-pdf`.
- Rattachement automatique ou manuel des achats et ventes aux bandes.
- Répartition d'un aliment ou achat vétérinaire sur plusieurs bandes/sites.
- Affectation génétique et semence.
- Import factures/avoirs, signes négatifs et reclassement des retenues.

### Reproduction et sanitaire

- Actions dédiées chaleur, IA, échographie, mise-bas, sevrage et traitement.
- Portée d'une truie dans une bande et inventaire de bande.
- Causes de pertes et protocoles sanitaires.
- Actes sanitaires : ajouter, modifier, supprimer et marquer comme fait.

### Administration et structure

- Modification des sections et mots de passe des utilisateurs.
- Mise à jour depuis l'interface et installation ZIP.
- Restauration de sauvegarde.
- Modification/suppression/ordre/RFID des sites, salles et cases.

### Vente directe

- Détail et modification d'une session.
- Modification et impression d'une commande.
- Impression de la préparation.
- Communications email/SMS, consentements et désinscription.

### Documents et outils

- PDF mise-bas et registre.
- QR code truie, scanner RFID, fichier modèle historique.
- Pages À propos, Contact et Correctifs.

## Modèles HTML Python non repris

Les 34 écrans dédiés absents du dossier Rust sont : À propos, archives, attente,
fiche/listes charcutiers, cochettes, commande publique, contact, correctifs,
eau-électricité historique, engraissement, état des données, GTTT, IFIP, journal,
mise à jour, paramètres, pharmacie, planning, productivité, recherche, réformes,
sanitaire, scan, stock, structure, tâches, impression truie, communications vente
directe, deux impressions vente directe, détail de session et liste des sessions.

Cela ne signifie pas que toutes ces URL sont absentes : plusieurs utilisent
actuellement `liste.html`, qui ne restitue pas les formulaires ni le fonctionnement
métier de l'écran Python.

## Corrections Python à préserver pendant la reprise

### Versions 1.11 à 1.17

- Relations de cases et intégrité des transferts/mortalités.
- Mise à jour Git/ZIP avec sauvegarde, contrôle, rollback et journal.
- Avoirs négatifs et reconnaissance des différents signes comptables.
- Retenues : équarrissage, groupement, CVEE, contribution sanitaire et cotisations.
- Import sans doublons sémantiques et reconstruction des valorisations.
- Permissions interdites par défaut et opérations puissantes réservées.
- GTTT filtré sur la bande de la portée, sans mélanger le cycle courant.
- Une vraie date de mise-bas ne doit pas être écrasée par une date déduite.
- Capacité de case et effectif disponible contrôlés avant transfert.

### Versions 1.18 à 1.37

- Ordre chronologique cohérent des bandes.
- Navigation regroupée et fiches truie/bande ventilées en rubriques.
- Fiche truie personnalisable.
- Quotidien simplifié et CSRF corrigé.
- Saisie rapide des chaleurs, IA prévue puis inséminations groupées.
- Affectation groupée des truies à une bande et affichage correct dans la bande.

### Versions 1.55 à 1.65

- Démonstration complète et nettoyage par manifeste.
- Pilotage technique unifié : bandes, cheptel et rang de portée.
- Eau & électricité : compteurs, import Excel/CSV, doublons, remplacement sans
  fausse consommation et reconnaissance Berue/Berrue.
- Réunion du socle Python 1.55 et du module énergie sans régression.

## Corrections métier demandées à intégrer

- Taux de mort-nés = mort-nés / nés totaux.
- Mortalité sous la mère tenant compte des adoptions et retraits.
- Calcul GTTT centralisé et unique pour toutes les pages.
- Portée liée à une bande/cycle, calculée portée par portée.
- Stock initial par case.
- Import en deux étapes : aperçu puis validation.
- Détection claire des doublons et anomalies avant écriture.
- Une transaction globale par import avec annulation complète en cas d'erreur.
- Conservation de la source et d'un identifiant d'import.
- Tests prioritaires : imports, reproduction, effectifs, transferts, mortalité,
  sevrage, économie et permissions.

## Ordre de reprise proposé

### Rust 2.1.3 — sécurité et traçabilité

- Permissions salarié interdites par défaut et sections personnalisées.
- Tests de permissions, CSRF et accès aux opérations puissantes.
- Suppression du faux succès des routes de compatibilité : une fonction non reliée
  doit être clairement signalée comme indisponible.
- Inventaire des routes dans un test de parité.

### Rust 2.2.0 — imports fiables

- Import CSV des truies avec aperçu, anomalies, doublons et validation.
- Import économique/PDF repris de Python : aliments, vétérinaire, semence,
  génétique, ventes abattoir et avoirs.
- Transaction globale, journal d'import et possibilité d'annuler un import.
- Tests sur une copie de la sauvegarde réelle.

### Rust 2.3.0 — reproduction et GTTT

- Actions chaleur, IA, écho, mise-bas, sevrage et traitement dédiées.
- Cycle de reproduction et portée liée à la bande historique.
- Calcul unique GTTT, taux de mort-nés et mortalité sous mère corrigés.
- Pilotage bandes/cheptel/rang et comparaisons IFIP.

### Rust 2.4.0 — effectifs, stock et bâtiments

- Stock initial par case, inventaires, mortalités et transferts complets.
- Structure modifiable, ordre des salles, RFID et contrôles quotidiens par salle.
- Engraissement renommé Prestataire avec session et consignes.

### Rust 2.5.0 — économie, abattoir et vente directe

- Rattachements et répartitions par bande/site.
- Avoirs négatifs, retenues, TMP, saisies et cahiers des charges.
- Sessions vente directe, impressions, communications et consentements.

### Rust 2.6.0 — administration et finition

- Sauvegarde/restauration, mise à jour GitHub et rollback depuis l'interface.
- Paramètres, démonstration complète, scanner, QR et documents PDF.
- Remplacement des dernières pages génériques et tri des tableaux.

## Règle de validation

Chaque version doit être vérifiée dans cet ordre :

1. copie horodatée de la base réelle ;
2. migration de la copie seulement ;
3. `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ;
4. tests HTTP des routes concernées avec les quatre rôles ;
5. vérification des totaux avant/après import ;
6. sauvegarde puis déploiement atomique avec possibilité de rollback.

La base de production `/var/lib/eo-suivi/elevage.db` ne doit jamais être utilisée
pour développer ou tester une migration.
