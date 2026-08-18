# Point des mises à jour EO-Suivi Élevage

État établi pour la version 2.2.4. La priorité reste la fiabilité des données, des imports et des sauvegardes avant les fonctions externes.

## Livré

- [x] Saisie rapide avec bouton « + » : ELD, poids, NEC, IA, écho, chaleur, pertes, sorties et mouvements.
- [x] Transferts par bande ou unité avec bâtiment, salle et case de destination.
- [x] Suivi des bandes : dates clés, effectif, emplacements, marquage et avance/retard.
- [x] Productivité : GTTT, objectifs, résultats par bande et données techniques d’abattage.
- [x] Économie : coûts par bande, avoirs, valorisations/retenues, variantes GR/FE/MI et apports comportant plusieurs lots.
- [x] Import PDF avec aperçu, détection des doublons, transaction globale, annulation et journal.
- [x] Import simultané de plusieurs PDF économiques en 2.2.3.
- [x] Reclassement des avoirs génétiques 1443203 et 1441836 et des montants avec signe moins final.
- [x] File IA/cochettes après sevrage, affectation groupée et capacité de verraterie paramétrable (31 places par défaut) en 2.2.4.
- [x] Plan sanitaire, pharmacie et liste des porcs traités hors vaccins.
- [x] Prestataire, RFID, inventaires par case, structure ordonnable, eau/électricité et calendrier ICS.
- [x] Sauvegarde SQLite contrôlée, restauration vérifiée et mise à jour Debian réversible.

## Restant prioritaire

- [ ] Contrôler les effectifs réels avec les données de production, notamment les anciens inventaires incohérents et les mortalités dont le stade est erroné.
- [ ] Ajouter la prévision de consommation et de commande d’aliment par bande avant rechargement.
- [ ] Importer les consommations de la machine à soupe.
- [ ] Produire une GTE complète en complément de la GTTT.
- [ ] Ajouter les rappels sanitaires calculés séparément pour truies, cochettes, porcelets et verrats.
- [ ] Générer la fiche de mise-bas A4 définitive.

## Restant avec service externe

- [ ] Sauvegardes automatiques vers NAS, stockage en ligne ou messagerie après configuration explicite de l’éleveur.

## Améliorations secondaires

- [ ] Simplifier encore la navigation et réduire le nombre de pages visibles.
- [ ] Ajouter le modèle d’import génétique téléchargeable.
