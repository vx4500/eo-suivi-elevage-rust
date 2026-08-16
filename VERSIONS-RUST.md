# Historique et feuille de route EO-Suivi Rust

## 2.1.3 — en préparation

- Audit de parité Python 1.65 / Rust 2.1.2.
- Permissions salarié remises en mode « interdit par défaut ».
- Respect des sections personnalisées du salarié.
- Protection des opérations puissantes réservées à l'éleveur/admin.
- Protection uniforme des zones réservées à l'administrateur.
- Deux tests unitaires ajoutés pour les droits par défaut et personnalisés.

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

- 2.2.0 : imports fiables et traçables.
- 2.3.0 : reproduction, cycles et GTTT complet.
- 2.4.0 : effectifs, stock, transferts et bâtiments.
- 2.5.0 : économie, abattoir et vente directe.
- 2.6.0 : administration, sauvegarde, mise à jour et finition des écrans.

Le détail de chaque lot et la liste des écarts se trouvent dans
`AUDIT-PORTAGE-RUST-2.1.2.md`.
