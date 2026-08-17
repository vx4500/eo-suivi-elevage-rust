# État du portage depuis EO-Suivi 1.65

La 1.65 d’origine contient 215 routes HTTP et environ 8 200 lignes dans son
fichier principal. Le changement Python → Rust est une réécriture du moteur,
pas une conversion automatique. La version 2.2.0 relie explicitement les 68
chemins qui manquaient encore après normalisation des paramètres d’URL.

## Fonctions opérationnelles en Rust 2.2.0

| Domaine | État |
| --- | --- |
| SQLite et migrations | Schéma historique complet, migrations additives |
| Authentification | Rôles, ancien PBKDF2, CSRF, verrouillage |
| Reproduction | Bandes, truies, chaleurs, IA groupées, échos, mises-bas, sevrages, traitements, mesures et pertes |
| Transferts et effectifs | Porcs/truies en écriture, contrôles source/capacité, annulation, inventaires datés sans doublon et calcul après comptage |
| Tableau de bord | Bandes, tâches, IA, ventes annuelles, mortalité, prix annuel et courbe des dernières ventes |
| Économie | Aliment, véto, semence, génétique, ventes, résultat par bande et import PDF avec aperçu |
| Abattoir | Apports, synthèse par bande et saisies sanitaires |
| Productivité | GTTT centralisé par période/bande, sevrés par truie productive/an, objectifs pondérés, ELD, rangs et abattage |
| Vente directe | Produits, inventaires, commandes, préparation, sessions, coûts, charges, consentements, Brevo courriel/SMS et historique |
| Eau et électricité | Compteurs, relevés, alertes, remplacement et import CSV |
| Exports | CSV/PDF mise-bas, registre PDF, QR, modèle/import truies, calendrier ICS et fiches imprimables |
| Exploitation | Utilisateurs, structure, tâches, sauvegarde/restauration contrôlée, paramètres, mise à jour en attente, entretien, quotidien, cahiers |

## Extensions qui ne font pas partie des 68 routes

- OCR des scans/photos et imports Excel spécialisés ;
- imports automatiques de nouveaux référentiels techniques externes ;
- synchronisation directe Gmail/Outlook et stockage cloud, qui exigent des
  identifiants propres à chaque fournisseur ;
- installateur graphique Windows Inno Setup.

La couche `501 Not Implemented` reste un garde-fou uniquement pour une URL
inconnue, et non pour l’une des 68 routes historiques couvertes par le test
`tests/route_parity.rs`.

## Validation avec la sauvegarde du 16 août 2026

- intégrité SQLite : conforme ;
- clés étrangères cassées : aucune ;
- 51 tables Rust comparées à la base réelle ;
- requêtes transferts/effectifs, économie, abattoir, vente directe et
  impressions exécutées sur une copie ;
- aucune donnée personnelle ni copie de la base n’est incluse dans l’archive ;
- la 2.0.1 a compilé et démarré avec Rust 1.97.1 sur Debian 13 ;
- la 2.2.0 doit être validée par `cargo fmt --check`, `cargo clippy`,
  `cargo test` et `cargo build --release` sur le serveur de recette.
