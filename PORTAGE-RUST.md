# État du portage depuis EO-Suivi 1.65

La 1.65 d’origine contient 215 routes HTTP et environ 8 200 lignes dans son
fichier principal. Le changement Python → Rust est une réécriture du moteur,
pas une conversion automatique. La version 2.1.4 relie explicitement plus de 120
chemins Rust ; certaines déclarations gèrent plusieurs verbes HTTP.

## Fonctions opérationnelles en Rust 2.1.4

| Domaine | État |
| --- | --- |
| SQLite et migrations | Schéma historique complet, migrations additives |
| Authentification | Rôles, ancien PBKDF2, CSRF, verrouillage |
| Reproduction | Bandes, truies, chaleurs, IA groupées, échos, mises-bas, sevrages, traitements, mesures et pertes |
| Transferts et effectifs | Porcs/truies en écriture, contrôles source/capacité, annulation, inventaires |
| Tableau de bord | Bandes, tâches, IA, ventes annuelles, mortalité, prix annuel et courbe des dernières ventes |
| Économie | Aliment, véto, semence, génétique, ventes et résultat par bande |
| Abattoir | Apports, synthèse par bande et saisies sanitaires |
| Vente directe | Produits, inventaires, commandes modifiables/imprimables, préparation, sessions, coûts et charges |
| Eau et électricité | Compteurs, relevés, alertes, remplacement et import CSV |
| Exports | CSV mise-bas, modèle/import truies, calendrier ICS et fiches imprimables |
| Exploitation | Utilisateurs, structure, tâches, sauvegarde, entretien, quotidien, cahiers |

## Modules secondaires restant à porter pour une parité totale

- imports historiques PDF/OCR et imports Excel spécialisés ;
- génération PDF serveur et QR codes (les fiches HTML sont imprimables) ;
- calculs GTTT/IFIP les plus avancés et écrans de personnalisation historiques ;
- fiches charcutier complètes, pharmacie et protocoles sanitaires en écriture ;
- contrôles quotidiens salle par salle et écrans secondaires de l’engraisseur ;
- newsletter Brevo, SMS et historique complet des communications ;
- base de démonstration, activation 45 jours, installateur Inno Setup et mise à
  jour ZIP/Git depuis l’interface ;
- reprise à l’identique des URL secondaires encore servies par la couche de
  compatibilité.

Une écriture non portée renvoie volontairement `501 Not Implemented`. Cela
évite qu’une action partielle modifie silencieusement la base réelle.

## Validation avec la sauvegarde du 16 août 2026

- intégrité SQLite : conforme ;
- clés étrangères cassées : aucune ;
- 51 tables Rust comparées à la base réelle ;
- requêtes transferts/effectifs, économie, abattoir, vente directe et
  impressions exécutées sur une copie ;
- aucune donnée personnelle ni copie de la base n’est incluse dans l’archive ;
- la 2.0.1 a compilé et démarré avec Rust 1.97.1 sur Debian 13 ;
- la 2.1.4 doit être validée par `cargo fmt --check`, `cargo clippy`,
  `cargo test` et `cargo build --release` sur le serveur de recette.
