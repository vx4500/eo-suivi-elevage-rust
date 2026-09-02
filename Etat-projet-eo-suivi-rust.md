# EO-Suivi Élevage Rust — État du projet

Version actuelle : **2.2.57** — Dernière mise à jour de ce document : 2 septembre 2026.

### Version 2.2.57 — historique GTE conservé

- Les bandes archivées restent visibles dans la GTE dès qu’elles possèdent des ventes, des charges d’aliment, des achats ou des effectifs.
- Le filtre de période continue de s’appliquer aux chiffres historiques sans masquer les lots clôturés.

### Version 2.2.56 — périodes GTE, accès et mortalité fiable

- Résultats GTE filtrables entre deux dates, avec raccourcis sur les 3, 12, 24 et 36 derniers mois.
- Présentation clarifiée des rôles et pages visibles de chaque utilisateur.
- La mortalité sous la mère reste indéterminée avant un sevrage réellement enregistré, au lieu d’afficher 100 % à tort.

### Version 2.2.55 — découverte du serveur sur le réseau local

Le serveur s'annonce en mDNS/DNS-SD (`_eosuivi._tcp.local.`) avec son port, sa
version et le nom de l'élevage. Prépare l'application mobile : beaucoup
d'élevages n'ont aucun accès internet — ni nom de domaine, ni certificat, ni
DNS — et le téléphone doit trouver le serveur seul sur le réseau. L'annonce
n'est jamais bloquante (multicast filtré ou interface absente n'empêchent pas le
démarrage) et se coupe avec `EO_MDNS=0`.

### Version 2.2.54 — progression des bandes et commandes clients

- Statistiques de mise bas par bande : truies ayant mis bas, porcelets nés et moyenne par truie.
- Flèche d’avancement, stades réels de Gestation, Lactation, Post-sevrage et Engraissement, avec bâtiment et salle des animaux.
- Rappel visible du lien et du code permettant au client de modifier sa commande de vente directe.

### Version 2.2.53 — accès utilisateurs, bandes et ventilation économique

- Choix du rôle et des pages visibles pour chaque utilisateur.
- Distinction entre bandes configurées et cycles ouverts sur le tableau de bord.
- Import des factures d’aliment bloqué tant que le site manque ou que le stade ne peut pas être reconnu.

### Version 2.2.52 — coût d'achat en GTE, lignées et import génétique

- GTE : le coût d'achat des animaux reçus (`receptionachat`) est imputé au lot
  comme charge d'entrée. La MSA, qui ne retient que l'aliment, surestimait la
  marge des profils post-sevreur et engraisseur ; la marge brute par truie
  repose désormais sur la marge après cette charge. Sans réception d'achat,
  rien ne change pour un naisseur ou un naisseur-engraisseur.
- Truies rattachables à une lignée du catalogue génétique (`truie.lignee_id`,
  colonne additive) au lieu de la seule race en texte libre, qui est conservée.
  Une lignée encore rattachée à des truies ne peut plus être supprimée.
- Import CSV en masse des factures génétiques : modèle téléchargeable, aperçu
  ligne à ligne et application transactionnelle, comme les imports existants.
- Correction de deux erreurs `clippy` (`nonminimal_bool`) qui faisaient échouer
  l'intégration continue sur Rust stable, donc bloquaient toute fusion.

### Version 2.2.51 — identité visuelle et sécurité des accès

- Identité EI ORY Emmanuel intégrée à la connexion, à l’icône du site et à la barre de navigation, avec adaptation mobile.
- Sessions révoquées après désactivation, changement de droits ou changement de mot de passe.
- Commandes publiques empêchées d’écraser une fiche client à partir du seul numéro de téléphone.
- Exposition HTTP de Docker Compose limitée à la machine locale en l’absence de proxy HTTPS.

### Version 2.2.50 — surfaces des cases et fiabilité des cycles

- Dimensions intérieures ou surface utile par case, secteur, mode de logement, poids de contrôle, objectif et diagnostic documenté de charge.
- Effectifs de portée recalculés par cycle réel, historique de maternité continu et contrôles renforcés sur pertes, adoptions et sevrages.
- Affectations économiques, factures et ventilation des ventes consolidées.

### Version 2.2.49 — économie fictive sur cinq ans

Le portail de démonstration complète son historique pour couvrir cinq ans de
ventes et conserve les sept bandes actives espacées de 21 jours. Il ajoute les
ventes, aliments par stade, frais sanitaires, semences et génétique avec leurs
affectations. Les données restent fictives et ne représentent pas un coût de
production complet. Ajout transactionnel unique, sans réinitialiser les comptes
ni remplacer les saisies existantes ; les bandes déjà renseignées sont exclues.

### Version 2.2.43 — ergonomie et gestion quotidienne

- Modification des mises-bas dans la fiche truie, pertes associées synchronisées et effectifs contrôlés.
- Six critères de sélection, tétines/splayleg, mères choisies surlignées et quatre niveaux de réforme.
- Tâches, réparations et entretien réunis ; notes modifiables et supprimables.
- Choix de semence depuis les factures importées ; relevés d’eau limités au site et bandes modifiables.
- Inventaire CSV des produits et silos avec import atomique et contrôle du stock de référence.
- Page Effectifs retirée ; bandes fantômes v1.14 archivées avant nettoyage (liens d’adoption protégés).
- Plan sanitaire, inséminations et pilotage technique réorganisés.

### Version 2.2.42 — maternité et nourrice artificielle

Adoptions atomiques vers une truie ou une case de nourrice, suivi des lots avec pertes
et sevrage vers le post-sevrage, compteurs de vivants et décès séparés des mort-nés et
momifiés. Les cases peuvent être renommées et leur plafond de porcelets est retiré.
Nouvelles tables additives `adoptionporcelet` et `sortienourrice`, vues de calcul
`portee_effectif` et `nourrice_effectif`. Les références d'historique sont protégées
contre la suppression. Voir la notice utilisateur pour la saisie.


Ce fichier fusionne les anciens audits, listings et listes de modifications.
Les informations devenues obsolètes (anciens comptages de routes, tâches déjà
livrées et doublons) ont été retirées. L'historique destiné aux utilisateurs
est affiché dans l'application sur `/correctifs`; l'historique technique
complet reste disponible dans les commits et demandes de fusion GitHub.

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

- Saisie rapide (bouton « + ») : ELD, poids, NEC, IA, écho, chaleur, mises-bas,
  pertes de porcs et porcelets, sorties, sevrages et mouvements de porcs.
- Centre de pilotage professionnel : huit étapes techniques datées par bande,
  stade actif, progression, prochaine intervention avec délai, effectif et
  site. La feuille de style est versionnée pour empêcher les anciens visuels
  conservés par le navigateur ou Cloudflare.
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
- Vente directe : classement filtrable des produits vendus par quantité, kg,
  chiffre d'affaires, prix moyen ou nombre de commandes ; fermeture immédiate
  du formulaire public avec bandeau de fin de vente.
- Tri croissant/décroissant sur les tableaux (classe `.rust-sortable`).

*(Détail des versions : voir la page `/correctifs` de l'application.)*

---

## 3. Reste à faire — priorités

### Fiabilité des données
- [x] Fiabiliser les effectifs réels (anciens inventaires incohérents,
      mortalités avec un stade erroné) — première étape : détection, sans
      correction automatique (l'éleveur reste seul juge de la correction).
      Quatre contrôles ajoutés à l'écran existant `/etat-donnees` (même
      principe que ses contrôles actuels : une valeur à zéro signifie que le
      contrôle est conforme), plutôt qu'un nouvel écran séparé :
      « Cases avec effectif calculé négatif » (mortalités/sorties
      supérieures aux entrées connues depuis le dernier inventaire),
      « Cases dépassant leur capacité déclarée », « Déclarations de
      mortalité sans stade renseigné » et « Porcs charcutiers sans bande
      d'origine ». Vérifié par un test dédié (`etat_donnees_detecte_les_incoherences_deffectif`
      dans `tests/schema.rs`) qui fabrique les quatre situations dans une
      base SQLite en mémoire et vérifie que chaque contrôle les détecte.
- [x] Corriger l'affectation du stade lors des déclarations de mortalité et
      détecter les effectifs incohérents. Les deux points de saisie
      (`/declaration` et la saisie rapide « perte ») laissaient l'utilisateur
      choisir un stade dans une liste fixe indépendante de la case
      sélectionnée, et seul `/declaration` vérifiait l'effectif présent dans
      la case avant d'enregistrer une perte. Le stade est désormais déduit
      automatiquement du type de la salle de la case choisie (mêmes motifs
      que les capacités par étape du §8/Phase 5 — `stade_pour_type_salle`,
      fonction pure testée unitairement), et la saisie rapide applique le
      même contrôle d'effectif insuffisant que `/declaration`. La saisie
      manuelle de « stade » reste utilisée telle quelle quand aucune case
      n'est renseignée (perte sous la mère, cas « Autre »).
- [x] Rattacher les porcs charcutiers à leur bande d'origine (au lieu d'un
      total générique d'engraissement). La donnée existait déjà par bande
      (`total_band_pigs`, utilisé sur la fiche bande), mais l'écran
      Prestataire/Engraissement (`/engraissement`) n'affichait que le
      formulaire de déclaration et le journal de mortalité, sans effectif par
      bande : un prestataire suivant plusieurs bandes ne pouvait pas savoir
      combien de porcs étaient réellement présents pour chacune. Ajout d'un
      tableau « Porcs présents par bande » en tête de cet écran, avec le même
      calcul que la fiche bande (limité aux bandes actives confiées à
      l'engraisseur pour ce rôle, comme le reste de l'écran).

### Conduite d'élevage et productivité
- [x] Produire une GTE complète en complément de la GTTT — déjà livré par le
      chantier §8/Phase 3 (écran `/gte`) ; entrée laissée par erreur dans
      cette liste lors de la clôture du chantier §8, corrigée ici.
- [x] Afficher les ELD dans les fiches truies **sans** garder la moyenne ELD
      par bande. L'historique ELD par truie était déjà affiché sur la fiche
      truie (`/truie/{id}`, section « Mesures ELD, poids et état
      corporel ») ; en revanche `eld_bandes` (moyenne ELD par bande) était
      calculé à chaque chargement de `/productivite` sans être utilisé par
      aucun modèle — code mort supprimé (la moyenne globale `eld_resume`,
      elle, reste affichée et est conservée).
- [x] Afficher TMP et données techniques d'abattage par bande — déjà livré :
      tableau « technique » de `/productivite` (TMP, muscle, plus-value, prix
      net, montant net par bande). Entrée laissée par erreur dans cette
      liste, corrigée ici.
- [x] Afficher stade, emplacement actuel et effectif réel des porcs.
      L'effectif réel était déjà affiché (`suivi_porcs.presents`), mais la
      fiche bande n'avait qu'un journal brut des mouvements (« Derniers
      emplacements enregistrés »), sans vue consolidée. Nouvelle section
      « Emplacement actuel » : une ligne par case où la bande a été
      affectée, avec le stade déduit (même logique que
      `stade_pour_type_salle`, §3 précédent) et l'effectif réellement
      présent dans la case aujourd'hui (`case_pig_count`, temps réel).
      Limite assumée et documentée dans le code : cet effectif compte tous
      les porcs de la case, pas seulement ceux de cette bande, si plusieurs
      bandes y ont été mêlées (le schéma ne trace pas la bande porc par
      porc au-delà du premier mouvement).
- [x] Finaliser la fiche de mise-bas au format A4 définitif — déjà livré par
      le chantier §8/Phase 7 (`/bande/{id}/fiche-mise-bas`). Entrée laissée
      par erreur dans cette liste, corrigée ici.
- [x] Permettre d'ordonner librement les salles dans l'implantation — déjà
      livré : boutons Monter/Descendre sur `/structure`
      (`structure_salle_ordre`, échange transactionnel de l'ordre avec la
      salle voisine du même site). Entrée laissée par erreur dans cette
      liste, corrigée ici.

### Sanitaire
- [x] Rappels sanitaires calculés séparément pour truies, cochettes,
      porcelets et verrats (avec historique). Truies/cochettes/porcelets
      étaient déjà couverts (§8/Phase 6, colonne « Catégorie » = `cible` sur
      `/sanitaire`) ; les verrats restaient un trou documenté (« un verrat
      n'appartient pas à une bande », `acterealise.bande_id` étant
      `NOT NULL`). Fermé par une table additive `acterealiseverrat`
      (mêmes colonnes qu'`acterealise`, sans toucher à sa contrainte
      existante — recréer la table pour l'assouplir aurait été un risque
      inutile sur une base de production) et une route dédiée
      `/sanitaire/fait-verrat`. L'historique « Actes réalisés » réunit
      maintenant bande et verrat par `UNION ALL`. *Limite assumée,
      inchangée :* pas d'échéance calculée pour les rappels verrats (aucune
      date de référence par verrat en base, comme documenté en Phase 6 pour
      les références autres que mise-bas) — seul l'historique était visé ici.
      Un vrai bug SQLite trouvé en écrivant le test dédié
      (`historique_sanitaire_reunit_bandes_et_verrats` dans
      `tests/schema.rs`) : sans alias explicite sur `id`/`date_realise`,
      SQLite refuse l'`ORDER BY` d'un `UNION ALL` (« ORDER BY term does not
      match any column in the result set ») — corrigé avant d'atteindre la
      production.

### Aliment et stock
- [x] Prévision de consommation et de commande d'aliment par bande avant
      rechargement. `/aliment-previsions` calculait déjà « jours avant
      rupture » par silo (§8/Phase 6) mais ne suggérait aucune quantité à
      commander, et n'avait aucune vue par bande. Ajout de deux fonctions
      pures testées (`quantite_a_commander` : tonnage pour ramener le silo à
      sa capacité déclarée ; `commande_urgente` : compare les jours avant
      rupture à un délai de livraison réglable,
      `aliment_delai_commande_jours`, 5 jours par défaut) affichées en badge
      sur chaque silo, et d'une section « Consommation aliment par bande
      (90 derniers jours) » à partir des livraisons déjà rattachées à une
      bande. *Limite assumée :* pas de projection prédictive par bande
      (poids cible/effectif restant varient trop pour un chiffre fiable
      sans intervention de l'éleveur) — visibilité historique par bande,
      volontairement pas une estimation inventée.
- [x] Importer les consommations des machines à soupe. Livré une fois le
      vrai format obtenu (machine Asserva, éleveur ORY EMMANUEL, 5 fichiers
      réels fournis). Trois formats CSV coexistent sur ces machines :
      `Histo_dis` (routage formule → vannes, sans quantité), `Histo_Modif`
      (journal de réglages par vanne) et `Histo_fab` (fabrication —
      quantité consigne/reçue d'eau et de chaque produit nommé par gâchée).
      Seul `Histo_fab` porte de vraies quantités consommées ; c'est le seul
      traité (`src/machine_soupe.rs`). Un cinquième fichier fourni (`Sv_7`,
      sans extension) s'est révélé être un instantané interne au format
      binaire propriétaire (chaînes préfixées par leur longueur, confirmé
      par une lecture hexadécimale) — pas un CSV, délibérément pas géré.
      Deux points techniques réels rencontrés en analysant le vrai fichier :
      encodage Windows-1252/Latin-1 (accents sur un octet, ex. 0xE9 pour
      « é »), pas UTF-8 — un décodage direct aurait échoué ou tronqué le
      fichier ; et le même bug de siècle sur les dates à 2 chiffres que
      les imports PDF Cooperl (`chrono` ne préfixe pas l'année de 2000).
      Import en deux étapes comme les truies (`importjournal`/`importligne`) :
      aperçu listant les produits réels distincts trouvés (les 32 slots
      placeholder `Produit_N` jamais configurés sont filtrés), puis écran de
      correspondance manuelle produit → silo — nouveau silo créé à la volée
      si besoin, mais rien n'est deviné automatiquement, comme demandé.
      Nouvelle table additive `consommationsoupe` (garde le nom exact du
      produit machine pour traçabilité). Affichée en complément
      (pas en remplacement) du bilan de matière existant sur
      `/aliment-previsions`, avec un avertissement sur l'unité (litres ou kg
      selon la machine, jamais convertie au hasard) — l'intégrer au calcul
      de « jours avant rupture » est un incrément naturel séparé si le
      besoin se confirme. Testé contre le vrai fichier `Histo_fab01-26.csv`
      (62 gâchées, 2 produits réels identifiés, dates et quantités
      vérifiées une à une) et par 4 tests unitaires dédiés.

### Économie et imports (demandes en attente)
- [x] Permettre deux lots sur une même facture d'apport Cooperl. Le
      découpage par « Bon n° » (`split_lots`/`parse_lot`) gérait déjà
      plusieurs lots en théorie, mais n'avait jamais été exercé par un test
      ni vérifié contre un vrai PDF. Vérifié et corrigé contre neuf vrais
      bordereaux Cooperl (ORY EMMANUEL) fournis par l'éleveur, dont un à deux
      lots (APPORT N° 226081270686, Bon n° 35776 et 35777) : les 134 porcs et
      les deux poids/montants par lot se retrouvent exactement, et le net à
      payer global se répartit correctement au prorata entre les deux
      lignes de vente. Trois vrais bugs trouvés et corrigés au passage :
      1) un bordereau Cooperl généré par « LDPRX 4.54 » a un octet
      `startxref` qui pointe à côté du mot-clé `xref` (décalage constaté de
      31 octets) — lopdf refusait alors le PDF entier
      (`ParseError::InvalidTrailer`, « Le PDF est illisible, chiffré ou
      endommagé ») alors que le contenu était intact ; `extract_pdf_text`
      retente maintenant avec l'offset réel du mot-clé `xref` quand le
      premier chargement échoue (`repair_startxref_offset`) ; 2) sur le
      modèle de bordereau réellement produit par Cooperl (« duplicata »
      comme « bordereau simplifié »), le libellé « ENLEVEMENT DU » fait
      partie du fond de page (image), pas du texte extrait : la date
      d'enlèvement se retrouvait donc être systématiquement la date de
      facturation (qui suit « LE », elle bien présente comme texte) — la
      date d'enlèvement, elle, apparaît seule sur sa ligne juste avant le
      profil d'élevage (NAISSEUR/ENGRAISSEUR...) ; ajout d'un repli qui la
      capture avant de retomber sur « LE » ; 3) `iso_date` acceptait une
      année à deux chiffres via `%d/%m/%Y` (chrono n'exige pas 4 chiffres à
      la lecture) sans lui ajouter 2000 : chaque date d'un vrai bordereau
      Cooperl (toujours en JJ/MM/AA) était importée avec l'année 0026 au
      lieu de 2026. Les trois corrections sont couvertes par des tests
      dédiés (`repare_un_startxref_decale_comme_le_produit_ldprx`,
      `apport_repartit_deux_bons_sur_la_meme_facture`,
      `apport_prend_la_date_denlevement_pas_la_date_de_facturation`,
      `date_a_deux_chiffres_prend_le_21e_siecle`) construits à partir des
      vrais chiffres et de la vraie mise en forme observée.
- [x] Facture génétique : bien différencier facture et avoir (même
      fournisseur, même modèle de document). En creusant la demande contre
      onze vrais bordereaux Cooperl « ANIMAUX REPRODUCTEURS » fournis par
      l'éleveur (7 factures distinctes + 2 avoirs), constat plus grave que
      prévu : **l'import génétique ne fonctionnait sur aucun vrai
      document**, avoir ou facture — `parse_genetique` cherchait les
      libellés « FACTURE N° », « NET A PAYER » et « BASE H.T. », qui
      n'existent nulle part dans le texte extrait de ce modèle (les libellés
      appartiennent au fond de page image, seules les valeurs sont du vrai
      texte, comme découvert sur les bordereaux d'apport). Résultat vérifié
      avant correction : erreur « aucun montant fiable n'a été trouvé » sur
      la facture réelle la plus simple. Un ancien correctif reclassait à la
      main deux avoirs par leur référence (« 1443203 », « 1441836 ») plutôt
      que de corriger l'extraction — mais cette référence n'était elle-même
      jamais extraite (même défaut de libellé absent), rendant ce correctif
      mort depuis son écriture. Réécrit intégralement sans dépendre d'aucun
      libellé : la référence du document se lit après « Semaine N° » (seul
      repère fiable, ex. « 14.41649 » → 1441649) ; la base H.T. et le total
      T.T.C. se lisent sur l'unique ligne de synthèse TVA sans en-tête
      (« 10400,65   2 5,5%   572,04   10972,69 ») ; le signe (facture ou
      avoir) vient directement du tiret final déjà présent sur chaque nombre
      côté avoir, plus robuste que chercher le mot « AVOIR » — dont
      l'affichage en bannière a ses lettres espacées par le générateur
      (« **** A V O I R **** ») et ne correspond donc jamais à un simple
      mot ; la référence de la facture créditée (« AVOIR SUR FACTURE NO
      1441649 »), elle en toutes lettres, est récupérée et ajoutée au
      libellé (« AVOIR — 28 cochettes (sur facture 1441649) ») pour que
      facture et avoir restent visuellement et durablement reliés sans
      migration de schéma. Bug additionnel corrigé au passage : le comptage
      des animaux (`^([0-9]+)\s+COCHETTE...`) ne reconnaissait aucune ligne
      d'un avoir, où la quantité est elle-même suivie d'un tiret
      (« 27-   COCHETTE... ») — un avoir donnait donc 0 animal quelle que
      soit sa taille réelle. Vérifié contre les onze bordereaux réels (tous
      s'importent désormais, factures et avoirs) ; deux tests dédiés
      (`genetique_reconnait_une_vraie_facture`,
      `genetique_reconnait_un_vrai_avoir_et_le_relie_a_sa_facture`)
      remplacent l'ancien test qui vérifiait le mécanisme mort plutôt que le
      comportement réel.
- [x] Fusionner les variantes de « Charte Qualité Régionale » et vérifier les
      doublons similaires sur toutes les pages. Vrai bug trouvé en écrivant
      le test : `canonical_label` comparait par sous-chaîne sur un texte
      seulement mis en majuscules, donc « CHARTE QUALITE REGI » (sans
      accent) et « CHARTE QUALITÉ RÉGIONALE » (avec accents) ne
      fusionnaient pas — deux lignes distinctes au lieu d'une seule cumulée
      sur *toutes* les pages qui utilisent cette fonction partagée (imports
      PDF Cooperl/Uniporc et saisies manuelles). Corrigé par
      `strip_french_accents`, appliquée avant la comparaison. Vérifié par
      `economic_import::tests::les_10_libelles_de_plus_value_demandes_sont_reconnus`.
- [x] Reconnaître les libellés de plus-value suivants :
      `PARTICIPATION P.S.A. 0J`, `+ VALUE R.S.E.`,
      `PRIME SOLIDARITE JEUNE 5 CT`, `+ VALUE QUALIVIANDE PBE`,
      `COMPLEMENT COCHON DU DIMANC`, `+ VALUE CHARTE QUALITE REGI`,
      `+ VALUE COOPERL LPF`, `+ VALUE PORC SANS ANTIBIOTI`,
      `+ VALUE QUEUE LONGUE (RSE)`, `PARTICIPATION COUT RFID`. Déjà
      reconnus par les entrées existantes de `canonical_label` (vérifié un
      par un, pas supposé) ; ajout du test dédié ci-dessus pour éviter une
      régression silencieuse si `mappings` est modifié plus tard.
- [x] Intégrer le tableau « Cahiers des charges — valorisations » dans
      Économie et retirer sa page séparée. La page `/cahiers` n'apparaissait
      déjà plus dans la navigation (`base.html`) mais restait accessible et
      autonome par URL directe. Son contenu (paramètres des cahiers +
      valorisations réelles importées) est désormais une section de
      `/economique#cahiers` ; `/cahiers` et les actions
      `/cahiers/{id}/maj`/`supprimer` redirigent vers cette ancre pour tout
      favori existant. `templates/cahiers.html` supprimé.
- [x] Supprimer l'estimation prévisionnelle (obsolète) — déjà fait : aucune
      trace de cette fonctionnalité dans le code actuel (recherché sans
      résultat). Entrée laissée par erreur dans cette liste, corrigée ici.
- [x] Regrouper « Lier automatiquement » avec l'import Cooperl et renommer
      l'action « Importer des documents PDF ». Déjà regroupé en une seule
      action (aucun bouton « Lier automatiquement » séparé dans aucun
      template — seules des routes historiques `/economique/auto-lier` et
      `/economique/rattacher-auto` restent enregistrées sans point d'entrée
      dans l'UI, gardées pour la parité des routes Python) ; bouton renommé
      « Importer des documents PDF » (au lieu de « Analyser et lier
      automatiquement ») pour correspondre exactement à la demande.
- [x] Ajouter un modèle d'import des factures génétiques, téléchargeable.
      Traité en 2.2.52 avec le pipeline complet qui manquait — un modèle CSV
      seul, sans point d'entrée pour le consommer, aurait laissé croire à un
      import possible. `/economique/genetique/modele.csv` (en-tête construit
      à partir de la même liste de colonnes que l'analyse, donc impossible à
      désynchroniser), aperçu ligne à ligne sans écriture
      (`/economique/genetique/import`) puis application transactionnelle
      (`.../confirmer`), avec les mêmes garde-fous que les imports existants :
      empreinte SHA-256 du fichier, doublon interne, facture déjà en base,
      contrôle rejoué au moment d'écrire, affectation automatique par bande
      ensuite. Une bande inconnue n'invalide pas la ligne : la facture entre
      non affectée, comme pour un import PDF. Six tests dans
      `src/routes/genetique_import.rs`.

### Présentation et navigation
- [x] Retirer l'ancien texte générique de mise à jour (« remplacer le dossier
      de l'application ») — déjà fait : `templates/maj.html` ne contient
      plus que le texte spécifique au service Debian (dépôt d'archive
      contrôlée, script `mettre-a-jour-debian13.sh`). Recherché sans
      résultat, entrée laissée par erreur dans cette liste.
- [x] Continuer à simplifier la navigation et regrouper les pages proches
      dans des sous-menus (voir structure proposée en §5) — déjà largement
      fait : le menu (`templates/base.html`) est organisé en cinq groupes
      déroulants (Reproduction, Élevage, Pilotage, Aide, Administration)
      plutôt qu'une liste plate, avec un regroupement proche de celui
      suggéré en §5 (ex. Administration ≈ « Paramètres (utilisateurs,
      archives, journal) »). Pas de nouveau regroupement forcé pour
      coller mot à mot à la structure de référence : les pages sont déjà
      correctement classées et un remaniement cosmétique sans besoin
      identifié aurait été du bikeshedding.
- [x] Corriger l'affichage de `/* Capacity badge styles */` en toutes
      lettres sur chaque page (dont la page de connexion). Vrai bug trouvé
      dans `templates/base.html` : ce commentaire CSS était placé entre
      deux balises `<style>`, donc en dehors de tout bloc de style — le
      navigateur l'affichait comme texte brut au lieu de l'interpréter
      comme un commentaire. Déplacé à l'intérieur du bloc `<style>`
      suivant, avec lequel il va (styles des badges de capacité). Vérifié
      qu'aucune autre fuite du même genre n'existe dans les templates.
- [x] Saisie rapide : renommer « Perte porcelet » en « Perte porc » sur le
      bouton d'accès (`templates/base.html`) — changement de libellé
      uniquement, aucun changement de comportement demandé.
- [x] Page `/bandes` : édition « à la volée » de mise-bas, n° de marquage
      et site directement dans le tableau, sans ouvrir la fiche bande
      complète. Chaque ligne a son propre petit formulaire (associé via
      l'attribut HTML `form=` plutôt que des `<form>` imbriqués, invalides
      en HTML) et un bouton « Enregistrer » dédié. Stade et effectif
      restent affichés mais non éditables ici : ce sont des valeurs
      calculées (planning + truies actives), pas des colonnes de `bande` —
      les rendre éditables casserait la cohérence avec la fiche bande, qui
      les recalcule toujours à partir des mêmes données sous-jacentes.
- [x] Eau/électricité : redistribuer la consommation aux bandes selon leur
      présence, avec un tarif au relevé (daté). Le rattachement des bandes
      présentes à un relevé existait déjà (`bandes` sur `releve_compteur`,
      calculé par `present_bands` à la saisie) mais ne servait qu'à
      l'affichage, jamais à répartir un coût. Ajout d'un champ
      `prix_unitaire` par relevé (saisie manuelle et import CSV en masse) ;
      le coût de chaque période (conso × prix du relevé qui la clôture) se
      répartit à parts égales entre les bandes présentes sur cette
      période — à parts égales et non au prorata d'un effectif, car l'eau/
      l'électricité d'un site (lavage, ventilation, chauffage communs) ne
      dépend pas linéairement du nombre de têtes par bande, contrairement à
      l'aliment qui se répartit déjà naturellement par les livraisons
      rattachées à une bande. Nouvelle section « Coût redistribué aux
      bandes » sur `/energie`, cumulée par bande et par type de compteur.
      Deux fonctions pures testées (`cout_consommation`,
      `repartir_cout_par_bande`).
- [x] Mode démo : générateur rejouable produisant plus de 850 truies
      actives, engraissement mixte sur place/extérieur et 5 ans
      d'historique. Nouveau module `src/demo.rs`, appelé par le même bouton
      bascule qu'avant (`/parametres#demo`) — remplace le geste symbolique
      précédent (une bande, une truie, un événement) sans changer son
      fonctionnement pour l'éleveur : toujours un seul bouton, toujours
      retiré proprement via `demoobjet`. Choix pour rester exécutable en
      une seule transaction (~10 s en pratique, vérifié par un test bout en
      bout sur une vraie base SQLite) sans générer des dizaines de milliers
      de lignes inutiles : seules les ~7 bandes les plus récentes (actives,
      un peu plus d'un cycle de reproduction) ont de vraies truies et des
      porcs charcutiers individuels (7×125 = 875 truies actives) ; les
      bandes plus anciennes, jusqu'à 5 ans en arrière (~86 bandes au
      total), n'ont qu'une ligne `bande` avec ses agrégats de production
      (`cs_truies_mb`, `cs_nt_portee`, etc.) déjà remplis — exactement ce
      que lisent les écrans GTTT/productivité historiques, vérifié dans le
      code existant avant d'écrire le générateur plutôt que supposé.
      Engraissement : un tiers des bandes actives est confié à un
      prestataire de démonstration (`bande.engraisseur_id`, même mécanisme
      que les vrais prestataires), le reste reste sur place ; les porcs
      charcutiers portent une `destination` cohérente avec leur bande.
      Nouvelles tables tracées dans `demoobjet` (`site`, `utilisateur`,
      `porccharcutier`) en plus des trois existantes ; suppression
      symétrique mise à jour dans le bon ordre (enfants avant parents, pour
      respecter les clés étrangères). Un vrai piège de compilation trouvé
      et corrigé : `rand::thread_rng()` n'est pas `Send` et ne peut donc
      pas être conservé à travers les nombreux `.await` d'un handler axum —
      remplacé par `StdRng::from_entropy()`. Vérifié contre une vraie base
      SQLite (pas seulement en théorie) : volumes, répartition sur/hors
      site, traçabilité complète dans `demoobjet`, et suppression totale
      sans violation de contrainte.

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
curl -fsS http://127.0.0.1:8080/login | grep -F "Version Rust 2.2.57"
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
      marge_brute_par_truie, taux_renouvellement_pct). *Complété en 2.2.52 :*
      le coût d'achat des animaux entrants (`receptionachat`, Phase 2) est
      désormais imputé au lot comme charge d'entrée. La MSA garde sa
      définition (recettes moins le seul aliment) ; une colonne « Marge après
      achat » la corrige pour les profils acheteurs et sert de base à la marge
      brute par truie. Sans réception d'achat, le coût vaut 0 et rien ne
      change pour un naisseur ou un naisseur-engraisseur. Requête isolée en
      constante `GTE_LOTS_SQL`, rejouée telle quelle par un test sur base
      SQLite en mémoire. *Reste hors périmètre :* l'indice de consommation est
      calculé sur le poids vendu, pas sur le gain de poids depuis l'entrée —
      il reste donc optimiste pour un engraisseur qui achète des animaux déjà
      lourds ; à traiter avec le poids d'entrée par lot.
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
      soumission successive des deux formulaires. *Complété en 2.2.52 :* une
      truie se rattache à une lignée du catalogue par la colonne additive
      `truie.lignee_id` (`POST /truie/{id}/lignee`, sélecteur visible
      seulement module actif). `truie.race`, texte libre historique, est
      conservé tel quel : aucune donnée réécrite, aucun élevage obligé
      d'utiliser le catalogue. Piège vérifié en conditions réelles :
      `truie.lignee_id` étant une clé étrangère, supprimer une lignée encore
      utilisée renvoyait une erreur technique — la suppression est désormais
      refusée avec le nombre de truies concernées, et `/genetique` affiche
      cette colonne.
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
      mise-bas) ; corrigé en `printf('%+d day', jour)`. *Limite levée depuis :*
      l'historique des rappels verrats, annoncé ici comme non couvert, l'est
      par la table additive `acterealiseverrat` et la route
      `/sanitaire/fait-verrat` (voir §3, Sanitaire). Seule subsiste l'absence
      d'échéance calculée pour ces rappels, faute de date de référence par
      verrat en base.
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
