# Historique et feuille de route EO-Suivi Rust

## 2.2.1 — retour de la saisie rapide

- Le bouton flottant « + » est de nouveau affiché en bas à droite sur toutes
  les pages accessibles à un utilisateur autorisé à modifier les données.
- Le panneau permet les saisies ELD/poids/NEC, IA, écho, chaleur, perte de
  porcelet et sortie de truie avec les listes Rust de truies, bandes et cases.
- La chaleur conserve uniquement les trois raccourcis demandés dans
  l’observation : aspect de la vulve, comportement et réflexe d’immobilité.
- Les mesures poids/NEC et le nombre de doses d’IA saisis dans le panneau sont
  maintenant réellement enregistrés par la route Rust.
- Un test empêche une prochaine version de supprimer à nouveau le bouton ou
  ses sources de données.

## 2.2.0 — parité des 68 routes historiques

- Les 68 chemins Python 1.65 encore absents sont maintenant enregistrés et
  vérifiés par un test de parité dédié.
- Actions de reproduction par truie et par bande : chaleur, IA, écho,
  mise-bas, sevrage, traitement, sortie, portée et reclassement verrat.
- Saisie rapide, truies en attente, scanner RFID/numéros, QR, registre PDF et
  export PDF des mises-bas.
- Inventaires et transferts de bande, mortalité, lavage des salles, causes de
  perte, protocole généré hors vaccins et conversion des achats en doses.
- Rattachement économique manuel, multi-bandes, par site et automatique sur
  des fenêtres de cycle explicites, sans inventer les dates absentes.
- Écrans dédiés Planning, Stocks, Journal, Paramètres, Mise à jour, Scanner,
  Communications et détail d’une session de vente.
- Plans d’alimentation, réglages de conduite, démonstration à manifeste,
  sauvegarde préalable et validation SQLite avant restauration.
- Consentements clients, désinscription publique, Brevo courriel/SMS et
  journal de chaque envoi ; les secrets ne sont jamais réaffichés.

## 2.1.9 — consolidation et mises à jour réversibles

- Tous les correctifs fonctionnels, SQLite, imports, sécurité et portabilité de
  la série 2.1.x sont regroupés sous un numéro de version unique.
- Mise à jour Debian 13 automatisée depuis le dépôt GitHub privé avec verrou
  anti-double lancement et refus des modifications Git locales.
- Contrôles Cargo exécutés avant toute interruption du service : `check`,
  Clippy, tests et compilation release.
- Sauvegarde SQLite native contrôlée par `PRAGMA quick_check`, avec conservation
  de l'ancien binaire et de l'unité systemd.
- Remplacement atomique du binaire, contrôle HTTP du numéro installé et retour
  arrière automatique en cas d'échec du démarrage ou du contrôle de santé.
- GitHub Actions limité aux branches utiles et validation de la syntaxe des
  scripts sur Linux, en plus des tests Windows et du binaire statique musl.

## 2.1.8 — robustesse, sécurité et portabilité

- Connexions SQLite configurées en WAL, clés étrangères actives, synchronisation
  normale et attente de 5 secondes lors d'une écriture concurrente.
- Champs SQL facultatifs conservés en `Option<T>` et tests de décodage avec des
  valeurs `NULL`.
- Dates des formulaires et imports validées strictement au format ISO
  `AAAA-MM-JJ`; une valeur incorrecte est refusée avant toute écriture.
- Nombres français, cellules vides et valeurs `NaN`/infinies gérés sans panique;
  les divisions économiques restent conditionnées à un dénominateur positif.
- Hachage et vérification PBKDF2 exécutés dans `spawn_blocking` afin de ne pas
  bloquer les workers HTTP.
- Jetons de session/CSRF générés avec 256 bits aléatoires; cookies de production
  `HttpOnly`, `SameSite=Lax` et `Secure`.
- Rendu des dates absentes ou historiques invalides remplacé par un tiret au
  lieu d'une erreur HTTP 500.
- Scripts Windows forcés en UTF-8 et cible Linux statique musl ajoutée à la CI.

## 2.1.7 — GTTT centralisé et objectifs fiables

- Une seule méthode GTTT alimente la page GTTT, la productivité, les fiches
  de bandes et les repères IFIP.
- Filtres par période et par bande ; une portée sans date n'est jamais
  rattachée artificiellement à une campagne.
- Taux de mort-nés calculé avec les mort-nés sur les nés totaux, sans
  inclure les momifiés dans le numérateur.
- Mortalité sous la mère calculée sur les nés vifs, adoptés et retirés
  avant comparaison avec les sevrés.
- Sevrés et portées par truie productive/an annualisés sur la durée
  réellement couverte par les bandes observées.
- Objectifs techniques modifiables, ajoutables et supprimables, comparés à
  des résultats pondérés plutôt qu'à une moyenne de moyennes.
- Objectifs et repères IFIP initiaux ajoutés sans écraser les réglages
  existants.

## 2.1.6 — conduite, effectifs et productivité

- Étapes des bandes et calendrier ICS calculés depuis les réglages de conduite
  (`gestation`, échographie, maternité, sevrage, transfert, finition et départ).
- Distinction des bandes planifiées, en verraterie, gestantes et en préparation
  maternité, avec tests exacts de toutes les frontières de cycle.
- Inventaire d'une bande ou d'une case remplaçable pour une même date afin
  d'éviter les doubles comptages lors d'une correction.
- Effectif restant calculé uniquement avec les morts, ventes et transferts
  postérieurs au dernier inventaire physique.
- Synthèse des effectifs fondée sur le dernier inventaire de chaque bande, même
  lorsque les comptages n'ont pas tous été réalisés le même jour.
- Productivité en trois vues : bandes/abattage, cheptel/ELD et rangs de portée.
- Moyennes techniques d'abattage pondérées par les porcs : poids, TMP, muscle,
  plus-value, prix net au kilo et montant net.
- Dernière ELD connue par truie, moyenne par bande et répartition du cheptel par
  étape de conduite.

## 2.1.5 — imports économiques PDF contrôlés

- Lecture native des PDF texte avec limites de taille, de pages et de
  décompression pour les documents non fiables.
- Détection des factures Cooperl d'aliment, produits vétérinaires, semence,
  génétique, apports charcutiers et synthèses Uniporc.
- Distinction des présentations d'aliment (miette, farine, granulé), reprise
  des avoirs et montants français, lots, poids, TMP et références de frappe.
- Classement canonique des valorisations ; équarrissage, groupement, CVEE,
  contribution sanitaire et cotisations restent toujours des retenues.
- Aperçu obligatoire sans écriture, détection des doublons et indication des
  ajouts/mises à jour avant confirmation.
- Confirmation en transaction unique, affectation facultative à une bande,
  annulation sans effet et journal durable des imports.
- Import puissant limité aux rôles éleveur et administrateur. Les scans et
  photos sans couche texte sont refusés clairement en attendant l'OCR.

## 2.1.4 — pilotage et vente directe

- Tableau de bord complété avec le prix net/kg annuel, la moyenne pondérée des
  cinq dernières ventes et une courbe des dix derniers apports.
- Inventaire de vente directe séparé des réglages produit pour éviter une
  modification accidentelle du stock.
- Modification transactionnelle des commandes : coordonnées, session, statut,
  produits, quantités, total et stocks sont recalculés ensemble.
- Impression d’une commande client avec lignes, total, livraison et émargement.
- Feuille de préparation par session avec totaux par produit et détail client.
- Modification des sessions de vente et synchronisation de la date de livraison
  lorsqu’une session active est corrigée.
- Journalisation des inventaires et des modifications de commande.

## 2.1.3 — reprise des correctifs historiques

- Audit de parité Python 1.65 / Rust 2.1.2.
- Permissions salarié remises en mode « interdit par défaut ».
- Respect des sections personnalisées du salarié.
- Protection des opérations puissantes réservées à l'éleveur/admin.
- Protection uniforme des zones réservées à l'administrateur.
- Gestion administrateur des sections et réinitialisation sécurisée des mots de passe.
- Import des truies en deux temps : aperçu, anomalies, doublons, confirmation ou annulation.
- Transaction globale d'import et conservation de l'identifiant de source.
- GTTT calculé portée par portée avec mort-nés / nés totaux et adoptions/retraits.
- Inventaires physiques initiaux par case intégrés au calcul des effectifs.
- Contrôles de capacité et d'effectif disponible conservés avant transfert.
- Structure complétée : modification, ordre, RFID, capacité et suppressions protégées.
- Espace Prestataire avec bandes affectées, consignes, poids cible et mortalités.
- Protocoles sanitaires, actes réalisés et pharmacie avec mouvements et seuils.
- Fiches charcutiers et traitements individuels.
- Pilotage technique filtrable, dates réelles, IFIP modifiable, réformes et cochettes configurables.
- Économie : avoirs négatifs, quatre écritures de signe reconnues, retenues obligatoires,
  TMP, informations techniques et suivi mensuel.
- Import Eau & électricité protégé par CSRF et transaction globale ; Berue/Berrue unifié.
- Pages Correctifs, À propos et Diagnostic restaurées.
- Les routes non portées signalent désormais clairement `501` au lieu d'un faux succès.
- Tests étendus : droits, calculs GTTT, signes comptables, schéma, inventaires et retenues.
- Migration validée sur une copie de la sauvegarde réelle : 96 truies, 9 bandes,
  `PRAGMA quick_check = ok` et aucun changement de la base de production.

## 2.1.2

- Déploiement Debian 13 validé avec la base SQLite historique.
- Correction du décodage SQLite `INTEGER`/`REAL` dans les agrégats vides.
- Chemin de base de production corrigé vers `/var/lib/eo-suivi/elevage.db`.
- Version affichée dans le pied de page et sur la connexion.
- Dépôt GitHub privé et procédure de mise à jour serveur préparés.

## 2.1.1

- Extension du portage : transferts, effectifs, état des données, énergie,
  économie, vente directe, structure, sanitaire et entretien.
- Import CSV simple des truies et de l'énergie.
- Contrôles de capacité de case et d'effectif disponible.
- Compatibilité du schéma SQLite portée à 51 tables.

## 2.0.x

- Premier portage Rust/Axum de la base Python.
- Authentification PBKDF2 compatible avec Python.
- Bandes, truies, événements, inséminations et premiers tableaux.
- Service systemd, Docker et documentation Debian 13.

## Prochaines versions

- 2.2.0 : OCR des scans/photos et imports Excel spécialisés.
- 2.3.0 : reproduction, cycles et GTTT complet.
- 2.4.0 : effectifs, stock, transferts et bâtiments.
- 2.5.0 : économie, abattoir et vente directe.
- 2.6.0 : administration, sauvegarde, mise à jour et finition des écrans.

Le détail de chaque lot et la liste des écarts se trouvent dans
`AUDIT-PORTAGE-RUST-2.1.2.md`.
