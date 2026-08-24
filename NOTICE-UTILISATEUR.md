# Notice utilisateur — EO‑Suivi Élevage

Version de la notice : 2.2.19 — mise à jour le 24 août 2026.

Cette notice doit être relue à chaque livraison. Toute fonction ajoutée ou
modifiée doit être inscrite dans la section « Nouveautés de la version » avant
la publication, puis le numéro et la date ci-dessus doivent être actualisés.

## Mise-bas

1. Ouvrir la saisie rapide avec le bouton **+**, puis choisir **Mise-bas**.
2. Sans recherche, la liste contient seulement les truies dont la date prévue
   de mise-bas est comprise entre J-10 et J+10 et qui n'ont pas encore de
   mise-bas enregistrée pour cette bande.
3. Pour retrouver exceptionnellement une autre truie, saisir son numéro de
   travail, son RFID ou sa bande dans le champ de recherche manuelle.
4. Renseigner les nés vifs, mort-nés, momifiés et les autres observations.
5. Choisir **Délivrance OK** ou **Délivrance NOK**. Cocher **Suivi de mise-bas
   nécessaire** lorsque la truie doit rester sous surveillance.
6. Valider. Les informations sont visibles dans l'historique de la fiche truie.

Une mise-bas peut aussi être saisie directement depuis la fiche de la truie.
La recherche manuelle constitue une dérogation volontaire : vérifier le numéro
de la truie avant validation.

## Apports abattoir et effectifs des cases

À la création ou à l'import d'un apport abattoir, EO‑Suivi crée automatiquement
un mouvement de sortie lié à l'apport. Le logiciel retire d'abord les porcs des
cases où la présence de la bande est tracée. Si aucune case d'origine ne peut
être déterminée, il conserve une sortie avec la mention **case d'origine non
renseignée** afin que l'écart puisse être corrigé dans le registre des
mouvements.

Un nouvel import du même apport remplace ses mouvements automatiques : il ne
doit donc pas compter deux fois la sortie.

## Contrôles conseillés

- Après un apport, ouvrir les transferts et vérifier le numéro d'apport, la
  bande, la quantité et les cases d'origine.
- Contrôler régulièrement les alertes d'effectif négatif et les inventaires par
  case.
- Pour une délivrance NOK, consulter la fiche truie et traiter les lignes
  marquées **À suivre** selon le protocole de l'élevage.

## Nouveautés de la version 2.2.19

- Assistant de résolution des principaux problèmes sanitaires et techniques,
  avec questionnaire d'ambiance et signaux d'urgence.
- Calculateur de quantité à partir du poids, du nombre d'animaux, de la dose
  prescrite et de la concentration du produit.
- Lancement sécurisé d'une mise à jour GitHub depuis Administration → Mise à
  jour.
- Démonstration enrichie avec structure, pharmacie, tâches et causes de perte.
- Liste de mise-bas limitée à la période prévue, avec recherche manuelle.
- Suivi de mise-bas et état de délivrance OK/NOK.
- Mouvements automatiques de sortie lors des apports abattoir.
- Création de cette notice utilisateur versionnée.
- Écran unique pour préparer les cochettes, les truies vides et enregistrer
  toutes les inséminations cochées en une seule validation.
- Suppression de l'ancien écran Scanner.
- Transfert du calendrier ICS depuis Paramètres.
- Affichage conditionnel des charcutiers RFID et des prestataires.
- Centre unique « Importer un document ».
- Cahiers des charges et valorisations regroupés dans Économie.

## Règle de maintenance de la notice

Pour chaque mise à jour du logiciel :

1. modifier la version et la date en tête de fichier ;
2. ajouter les changements visibles par l'utilisateur dans « Nouveautés » ;
3. corriger les procédures concernées, pas seulement la liste des nouveautés ;
4. faire relire la notice pendant la revue de code ;
5. conserver dans Git la notice avec le code de la même version.
