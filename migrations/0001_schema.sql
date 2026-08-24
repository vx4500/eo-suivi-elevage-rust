PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS utilisateur (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    identifiant TEXT NOT NULL UNIQUE,
    nom TEXT,
    prenom TEXT,
    hash_mdp TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'salarie',
    parent_id INTEGER REFERENCES utilisateur(id),
    actif INTEGER NOT NULL DEFAULT 1,
    sections TEXT,
    doit_changer_mdp INTEGER NOT NULL DEFAULT 0,
    tentatives_echec INTEGER NOT NULL DEFAULT 0,
    bloque_jusqu TEXT
);

CREATE TABLE IF NOT EXISTS site (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL,
    nom TEXT
);

CREATE TABLE IF NOT EXISTS salle (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dernier_lavage TEXT,
    site_id INTEGER NOT NULL REFERENCES site(id),
    nom TEXT NOT NULL,
    type TEXT,
    rfid TEXT,
    nb_cases INTEGER NOT NULL DEFAULT 0,
    ordre INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS casesalle (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    salle_id INTEGER NOT NULL REFERENCES salle(id),
    nom TEXT NOT NULL,
    rfid TEXT,
    nb_max_porcs INTEGER,
    num_vanne TEXT
);

CREATE TABLE IF NOT EXISTS verrat (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL,
    race TEXT,
    note TEXT,
    actif INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS truie (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT,
    num_travail TEXT NOT NULL,
    num_national TEXT,
    rfid TEXT,
    race TEXT,
    date_entree TEXT,
    statut TEXT NOT NULL DEFAULT 'active',
    note TEXT,
    rang INTEGER NOT NULL DEFAULT 0,
    date_naissance TEXT,
    issf REAL,
    perf_nt REAL,
    perf_nv REAL,
    perf_mn REAL,
    perf_mo REAL,
    perf_sevres REAL,
    perf_adoptes REAL,
    perf_retires REAL,
    perf_tx_perte REAL,
    tx_chetifs REAL,
    nb_retours INTEGER,
    pere_national TEXT,
    mere_national TEXT,
    reformee INTEGER NOT NULL DEFAULT 0,
    date_reforme TEXT,
    motif_sortie TEXT,
    prix_sortie REAL,
    num_apport_sortie TEXT,
    mere_cochette INTEGER NOT NULL DEFAULT 0,
    bande_code TEXT,
    salle_id INTEGER REFERENCES salle(id),
    case_id INTEGER REFERENCES casesalle(id),
    source_import_id TEXT
);

CREATE INDEX IF NOT EXISTS ix_truie_num_travail ON truie(num_travail);
CREATE INDEX IF NOT EXISTS ix_truie_rfid ON truie(rfid);

CREATE TABLE IF NOT EXISTS bande (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT,
    code TEXT NOT NULL,
    num_officiel TEXT,
    date_mb TEXT,
    site TEXT,
    note TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    engraisseur_id INTEGER REFERENCES utilisateur(id),
    poids_cible REAL,
    instructions TEXT,
    cs_truies_saillies INTEGER,
    cs_pleines INTEGER,
    cs_truies_mb INTEGER,
    cs_nt_portee REAL,
    cs_nv_portee REAL,
    cs_mn_portee REAL,
    cs_sevres_portee REAL,
    cs_total_sevres INTEGER,
    cs_tx_pertes_nv REAL,
    cs_adoptes INTEGER,
    cs_retires INTEGER,
    cs_poids_sevrage REAL,
    cs_gmq_ps REAL,
    cs_gmq_engr REAL,
    cs_gmq_nv REAL
);

CREATE INDEX IF NOT EXISTS ix_bande_code ON bande(code);

CREATE TABLE IF NOT EXISTS evenement (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    type TEXT NOT NULL,
    date TEXT NOT NULL,
    truie_id INTEGER REFERENCES truie(id),
    bande_id INTEGER REFERENCES bande(id),
    verrat_id INTEGER REFERENCES verrat(id),
    nes_totaux INTEGER,
    nes_vifs INTEGER,
    mort_nes INTEGER,
    momifies INTEGER,
    chetifs INTEGER,
    ecrases INTEGER,
    tues_truie INTEGER,
    heure_debut TEXT,
    heure_fin TEXT,
    suivi_actif INTEGER NOT NULL DEFAULT 0,
    delivrance_ok INTEGER,
    nb_sevres INTEGER,
    poids_moyen REAL,
    adoptes INTEGER,
    retires INTEGER,
    eld_entree REAL,
    eld_sortie REAL,
    de_lieu TEXT,
    vers_lieu TEXT,
    nb_animaux INTEGER,
    produit TEXT,
    motif TEXT,
    delai_attente INTEGER,
    note TEXT,
    resultat TEXT,
    nb_doses INTEGER
);

CREATE INDEX IF NOT EXISTS ix_evenement_date ON evenement(date);
CREATE INDEX IF NOT EXISTS ix_evenement_truie ON evenement(truie_id);
CREATE INDEX IF NOT EXISTS ix_evenement_bande ON evenement(bande_id);

CREATE TABLE IF NOT EXISTS acteprotocole (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    libelle TEXT NOT NULL,
    cible TEXT NOT NULL,
    reference TEXT NOT NULL,
    jour INTEGER NOT NULL,
    produit TEXT,
    note TEXT,
    actif INTEGER NOT NULL DEFAULT 1,
    rappel INTEGER NOT NULL DEFAULT 0,
    rappel_ecart_j INTEGER,
    categorie TEXT,
    dose TEXT,
    unite TEXT,
    voie TEXT,
    duree_j INTEGER,
    delai_attente INTEGER,
    aiguille TEXT,
    preconisations TEXT
);

CREATE TABLE IF NOT EXISTS acterealise (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    acte_id INTEGER NOT NULL REFERENCES acteprotocole(id),
    bande_id INTEGER NOT NULL REFERENCES bande(id),
    date_realise TEXT NOT NULL,
    note TEXT
);

-- Table séparée plutôt qu'un bande_id rendu nullable sur acterealise : un
-- verrat n'appartient pas à une bande, mais acterealise.bande_id est
-- NOT NULL et SQLite ne permet pas de retirer une contrainte NOT NULL sans
-- recréer la table (risque écarté pour une base de production). Mêmes
-- colonnes qu'acterealise, réunies avec elle par UNION ALL pour
-- l'historique (§3 « Rappels sanitaires… avec historique »).
CREATE TABLE IF NOT EXISTS acterealiseverrat (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    acte_id INTEGER NOT NULL REFERENCES acteprotocole(id),
    verrat_id INTEGER NOT NULL REFERENCES verrat(id),
    date_realise TEXT NOT NULL,
    note TEXT
);

CREATE TABLE IF NOT EXISTS reglage (
    cle TEXT PRIMARY KEY,
    valeur INTEGER NOT NULL,
    libelle TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS parametre (
    cle TEXT PRIMARY KEY,
    valeur TEXT
);

CREATE TABLE IF NOT EXISTS planaliment (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    categorie TEXT NOT NULL,
    jour_debut INTEGER NOT NULL DEFAULT 0,
    jour_fin INTEGER NOT NULL DEFAULT 0,
    aliment TEXT,
    quantite REAL,
    unite TEXT DEFAULT 'kg/j',
    note TEXT,
    ordre INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS causeperte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    libelle TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS perteporcelet (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    truie_id INTEGER REFERENCES truie(id),
    bande_id INTEGER REFERENCES bande(id),
    age_j INTEGER,
    nb INTEGER NOT NULL DEFAULT 1,
    cause TEXT,
    date TEXT,
    evenement_id INTEGER REFERENCES evenement(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS porccharcutier (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rfid TEXT,
    date_naissance TEXT,
    bande_code TEXT,
    cahier_charges TEXT,
    sexe TEXT,
    mere_bio TEXT,
    mere_courante TEXT,
    structure TEXT,
    poids1 REAL,
    poids2 REAL,
    poids3 REAL,
    date_mort TEXT,
    cause_mort TEXT,
    type_perte TEXT,
    destination TEXT,
    note TEXT
);

CREATE TABLE IF NOT EXISTS transfert (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    espece TEXT NOT NULL DEFAULT 'porc',
    bande_id INTEGER REFERENCES bande(id),
    salle_source_id INTEGER REFERENCES salle(id),
    salle_dest_id INTEGER REFERENCES salle(id),
    case_source_id INTEGER REFERENCES casesalle(id),
    case_dest_id INTEGER REFERENCES casesalle(id),
    nombre INTEGER,
    truie_id INTEGER REFERENCES truie(id),
    vente_apport_id INTEGER REFERENCES venteapport(id),
    note TEXT
);

CREATE TABLE IF NOT EXISTS livraisonaliment (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    fournisseur TEXT,
    produit TEXT,
    silo TEXT,
    tonnage REAL,
    pu_ht REAL,
    montant_ht REAL,
    num_facture TEXT,
    site TEXT,
    bande_id INTEGER REFERENCES bande(id),
    bandes TEXT
);

CREATE TABLE IF NOT EXISTS achatveto (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    produit TEXT,
    quantite REAL,
    pu_ht REAL,
    montant_ht REAL,
    num_facture TEXT,
    delai_attente INTEGER,
    fournisseur TEXT,
    doses_unite INTEGER,
    site TEXT,
    bande_id INTEGER REFERENCES bande(id),
    bandes TEXT
);

CREATE TABLE IF NOT EXISTS venteapport (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    num_apport TEXT,
    bande_id INTEGER REFERENCES bande(id),
    frappe TEXT,
    nb_porcs INTEGER,
    poids_total REAL,
    poids_moyen REAL,
    prix_moyen REAL,
    plus_value REAL,
    montant_net REAL,
    tmp REAL,
    tx_qualification REAL,
    nb_hors_poids INTEGER,
    nb_tmp_bas INTEGER,
    nb_g2 INTEGER,
    nb_tatouage INTEGER,
    nb_qualifies INTEGER,
    nb_livres INTEGER,
    muscle_gamme REAL,
    muscle_lot REAL,
    total_retenues REAL,
    semaine TEXT,
    lots_json TEXT
);

CREATE TABLE IF NOT EXISTS achatsemence (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    num_facture TEXT,
    fournisseur TEXT,
    designation TEXT,
    nb_doses INTEGER,
    montant_ht REAL,
    montant_ttc REAL,
    bande_id INTEGER REFERENCES bande(id),
    note TEXT
);

CREATE TABLE IF NOT EXISTS achatgenetique (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    num_facture TEXT,
    fournisseur TEXT DEFAULT 'Cooperl',
    designation TEXT,
    nb_animaux INTEGER,
    poids_total REAL,
    prix_moyen REAL,
    montant_ht REAL,
    montant_net REAL,
    bande_code TEXT,
    note TEXT
);

CREATE TABLE IF NOT EXISTS objectif (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cle TEXT NOT NULL,
    libelle TEXT NOT NULL,
    valeur REAL,
    sens TEXT NOT NULL DEFAULT 'haut',
    decimales INTEGER NOT NULL DEFAULT 1,
    ordre INTEGER NOT NULL DEFAULT 0,
    actif INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS mesuretruie (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    truie_id INTEGER NOT NULL REFERENCES truie(id),
    date TEXT NOT NULL,
    periode TEXT,
    eld REAL,
    poids REAL,
    nec REAL,
    note TEXT
);

CREATE TABLE IF NOT EXISTS traitementcharcutier (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    charcutier_id INTEGER REFERENCES porccharcutier(id),
    bande_code TEXT,
    date TEXT,
    produit TEXT,
    dose TEXT,
    motif TEXT,
    delai_attente INTEGER,
    note TEXT
);

CREATE TABLE IF NOT EXISTS referenceifip (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cle TEXT NOT NULL,
    libelle TEXT NOT NULL,
    moyenne REAL,
    tiers_sup REAL,
    sens TEXT NOT NULL DEFAULT 'haut',
    decimales INTEGER NOT NULL DEFAULT 1,
    annee TEXT,
    ordre INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS mouvementstock (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code_ifip TEXT,
    date TEXT,
    bande_code TEXT,
    nombre REAL,
    poids REAL,
    montant REAL,
    age INTEGER,
    libelle TEXT,
    destination TEXT,
    type_saisie TEXT,
    est_stock INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    horodatage TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    utilisateur TEXT,
    action TEXT,
    objet TEXT,
    detail TEXT,
    chemin TEXT
);

CREATE TABLE IF NOT EXISTS entretien (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    type TEXT,
    site TEXT,
    derniere_date TEXT,
    frequence_jours INTEGER NOT NULL DEFAULT 365,
    note TEXT
);

CREATE TABLE IF NOT EXISTS declarationmort (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    horodatage TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    bande_code TEXT,
    date TEXT,
    stade TEXT,
    case_id INTEGER REFERENCES casesalle(id),
    cause TEXT,
    poids REAL,
    nombre INTEGER NOT NULL DEFAULT 1,
    declare_par TEXT,
    note TEXT
);

CREATE TABLE IF NOT EXISTS saisieabattoir (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    bande_code TEXT,
    num_apport TEXT,
    morceau TEXT NOT NULL,
    nombre INTEGER NOT NULL DEFAULT 1,
    motif TEXT,
    note TEXT
);

CREATE TABLE IF NOT EXISTS produitventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    prix REAL NOT NULL DEFAULT 0,
    unite TEXT NOT NULL DEFAULT 'kg',
    actif INTEGER NOT NULL DEFAULT 1,
    ordre INTEGER NOT NULL DEFAULT 0,
    quantite_disponible REAL,
    image_data BLOB,
    image_mime TEXT
);

CREATE TABLE IF NOT EXISTS reglageventedirecte (
    id INTEGER PRIMARY KEY,
    date_livraison TEXT,
    texte_livraison TEXT,
    commandes_ouvertes INTEGER NOT NULL DEFAULT 1,
    message_fermeture TEXT
);

CREATE TABLE IF NOT EXISTS sessionventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    date_creation TEXT NOT NULL DEFAULT (date('now')),
    date_livraison TEXT,
    nb_porcs INTEGER NOT NULL DEFAULT 0,
    bande_reference TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    notes TEXT,
    date_cloture TEXT,
    date_limite_commandes TEXT
);

CREATE TABLE IF NOT EXISTS coutelevageventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_vente_id INTEGER NOT NULL UNIQUE REFERENCES sessionventedirecte(id),
    semence REAL NOT NULL DEFAULT 0,
    gestation REAL NOT NULL DEFAULT 0,
    maternite REAL NOT NULL DEFAULT 0,
    post_sevrage REAL NOT NULL DEFAULT 0,
    engraissement REAL NOT NULL DEFAULT 0,
    veto_autres REAL NOT NULL DEFAULT 0,
    bande_id INTEGER REFERENCES bande(id),
    nb_porcs_calcules INTEGER,
    poids_moyen_kg REAL,
    cout_par_porc REAL,
    cout_par_kg REAL,
    calcule_le TEXT
);

CREATE TABLE IF NOT EXISTS chargeventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_vente_id INTEGER NOT NULL REFERENCES sessionventedirecte(id),
    categorie TEXT NOT NULL DEFAULT 'autre',
    libelle TEXT NOT NULL,
    montant REAL NOT NULL DEFAULT 0,
    note TEXT
);

CREATE TABLE IF NOT EXISTS clientventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    email TEXT,
    telephone TEXT,
    newsletter_email INTEGER NOT NULL DEFAULT 0,
    newsletter_sms INTEGER NOT NULL DEFAULT 0,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    token_desinscription TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS commandeventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    client_id INTEGER REFERENCES clientventedirecte(id),
    suivi_email INTEGER NOT NULL DEFAULT 1,
    suivi_sms INTEGER NOT NULL DEFAULT 0,
    session_vente_id INTEGER REFERENCES sessionventedirecte(id),
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    nom_client TEXT NOT NULL,
    telephone TEXT NOT NULL,
    email TEXT,
    notes TEXT,
    statut TEXT NOT NULL DEFAULT 'nouvelle',
    total REAL NOT NULL DEFAULT 0,
    token_modification TEXT,
    code_modification TEXT,
    recap_envoye_le TEXT
);

CREATE TABLE IF NOT EXISTS reglagecommunicationventedirecte (
    id INTEGER PRIMARY KEY,
    brevo_api_key TEXT,
    sender_email TEXT,
    sender_name TEXT NOT NULL DEFAULT 'EI ORY EMMANUEL',
    sms_sender TEXT NOT NULL DEFAULT 'ORYEMMANUEL',
    email_list_id INTEGER,
    sms_list_id INTEGER
);

CREATE TABLE IF NOT EXISTS messageventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    commande_id INTEGER REFERENCES commandeventedirecte(id),
    client_id INTEGER REFERENCES clientventedirecte(id),
    canal TEXT NOT NULL,
    type_message TEXT NOT NULL,
    destinataire TEXT NOT NULL,
    contenu TEXT NOT NULL,
    succes INTEGER NOT NULL DEFAULT 0,
    detail TEXT
);

CREATE TABLE IF NOT EXISTS lignecommandeventedirecte (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    commande_id INTEGER NOT NULL REFERENCES commandeventedirecte(id) ON DELETE CASCADE,
    produit_id INTEGER REFERENCES produitventedirecte(id),
    nom_produit TEXT NOT NULL,
    prix_unitaire REAL NOT NULL,
    unite TEXT NOT NULL,
    quantite REAL NOT NULL,
    total_ligne REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS produitpharmacie (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    produit TEXT NOT NULL,
    stock_actuel REAL DEFAULT 0,
    unite TEXT DEFAULT 'doses',
    seuil_alerte REAL,
    note TEXT,
    maj TEXT
);

CREATE TABLE IF NOT EXISTS mouvementpharmacie (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    produit TEXT NOT NULL,
    date TEXT,
    type TEXT,
    quantite REAL,
    note TEXT,
    bande_code TEXT
);

CREATE TABLE IF NOT EXISTS controlequotidien (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL DEFAULT (date('now')),
    horodatage TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    categorie TEXT NOT NULL,
    salle_id INTEGER REFERENCES salle(id),
    salle_nom TEXT,
    element TEXT,
    statut TEXT NOT NULL DEFAULT 'ok',
    note TEXT,
    utilisateur TEXT
);

CREATE TABLE IF NOT EXISTS tache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    titre TEXT NOT NULL,
    type TEXT,
    bande_code TEXT,
    salle TEXT,
    echeance TEXT,
    fait INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS consosoupe (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    no_formule TEXT,
    produit TEXT NOT NULL,
    quantite REAL,
    cle TEXT UNIQUE
);

CREATE TABLE IF NOT EXISTS porteerang (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    num_travail TEXT NOT NULL,
    rang INTEGER NOT NULL,
    bande TEXT,
    duree_gest REAL,
    nv REAL,
    mn REAL,
    tx_mn_nt REAL,
    sev REAL,
    ad REAL,
    re REAL,
    tx_pertes REAL,
    eld1 REAL,
    eld2 REAL
);

CREATE TABLE IF NOT EXISTS cahiercharges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    valeur_par_porc REAL,
    actif INTEGER NOT NULL DEFAULT 1,
    note TEXT
);

CREATE TABLE IF NOT EXISTS valorisationapport (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    num_apport TEXT,
    date TEXT,
    libelle TEXT NOT NULL,
    montant REAL,
    categorie TEXT DEFAULT 'valorisation'
);

CREATE TABLE IF NOT EXISTS compteur_energie (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    type TEXT NOT NULL,
    site_id INTEGER REFERENCES site(id),
    unite TEXT NOT NULL DEFAULT 'm3',
    rappel_jours INTEGER,
    actif INTEGER NOT NULL DEFAULT 1,
    note TEXT
);

CREATE TABLE IF NOT EXISTS releve_compteur (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    compteur_id INTEGER NOT NULL REFERENCES compteur_energie(id) ON DELETE CASCADE,
    date_releve TEXT NOT NULL,
    valeur_index REAL NOT NULL,
    bandes TEXT,
    note TEXT,
    remplacement_compteur INTEGER NOT NULL DEFAULT 0,
    prix_unitaire REAL
);

CREATE INDEX IF NOT EXISTS ix_releve_compteur_compteur ON releve_compteur(compteur_id);
CREATE INDEX IF NOT EXISTS ix_releve_compteur_date ON releve_compteur(date_releve);

CREATE TABLE IF NOT EXISTS importjournal (
    token TEXT PRIMARY KEY,
    type_import TEXT NOT NULL,
    nom_fichier TEXT,
    statut TEXT NOT NULL DEFAULT 'apercu',
    cree_par INTEGER REFERENCES utilisateur(id),
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resume TEXT,
    applique_le TEXT
);

CREATE TABLE IF NOT EXISTS importligne (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token TEXT NOT NULL REFERENCES importjournal(token) ON DELETE CASCADE,
    numero_ligne INTEGER NOT NULL,
    action TEXT NOT NULL,
    anomalie TEXT,
    donnees_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_importligne_token ON importligne(token);

CREATE TABLE IF NOT EXISTS inventairecase (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    case_id INTEGER NOT NULL REFERENCES casesalle(id),
    date TEXT NOT NULL,
    nombre INTEGER NOT NULL,
    note TEXT,
    cree_par TEXT,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_inventairecase_case_date ON inventairecase(case_id,date);

-- Réception d'animaux achetés (§1bis de la spécification) : post-sevreur,
-- post-sevreur-engraisseur et engraisseur seul démarrent leur cycle ici plutôt
-- qu'à la mise-bas. Le lot d'origine du fournisseur est conservé comme
-- traçabilité même si la mère n'est pas dans la base.
CREATE TABLE IF NOT EXISTS receptionachat (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    fournisseur TEXT,
    num_bon_livraison TEXT,
    lot_origine_fournisseur TEXT,
    bande_code TEXT,
    case_id INTEGER REFERENCES casesalle(id),
    effectif INTEGER NOT NULL,
    poids_moyen REAL,
    poids_total REAL,
    prix_total REAL,
    quarantaine_jusqu TEXT,
    note TEXT,
    cree_par TEXT,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    -- Références vers les mouvements générés par cette réception (registre
    -- d'effectifs et transfert éventuel), pour pouvoir les annuler proprement
    -- si la réception est supprimée sans laisser d'effectif fantôme.
    mouvementstock_id INTEGER REFERENCES mouvementstock(id),
    transfert_id INTEGER REFERENCES transfert(id)
);

CREATE INDEX IF NOT EXISTS ix_receptionachat_date ON receptionachat(date);
CREATE INDEX IF NOT EXISTS ix_receptionachat_bande ON receptionachat(bande_code);

-- Prévision de consommation d'aliment avant rechargement (§5). Même principe
-- que compteur_energie/releve_compteur : relevés manuels périodiques, la
-- consommation moyenne se déduit de deux relevés et des livraisons reçues
-- entre les deux (bilan de matière), jamais une valeur figée.
CREATE TABLE IF NOT EXISTS silo_aliment (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    site_id INTEGER REFERENCES site(id),
    capacite_tonnes REAL,
    actif INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS releve_silo (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    silo_id INTEGER NOT NULL REFERENCES silo_aliment(id) ON DELETE CASCADE,
    date TEXT NOT NULL,
    niveau_tonnes REAL NOT NULL,
    note TEXT
);

CREATE INDEX IF NOT EXISTS ix_releve_silo_silo ON releve_silo(silo_id);
CREATE INDEX IF NOT EXISTS ix_releve_silo_date ON releve_silo(date);

-- Consommations importées depuis les exports « Histo_fab » d'une machine à
-- soupe (§ « machines à soupe » des demandes en attente). Chaque ligne est
-- une gâchée réelle (quantité consigne/reçue par produit). `produit_machine`
-- garde le nom exact tel qu'exporté par la machine, pour traçabilité même
-- après que l'éleveur l'a relié à un silo existant lors de l'import.
CREATE TABLE IF NOT EXISTS consommationsoupe (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT,
    heure_debut TEXT,
    no_formule INTEGER,
    produit_machine TEXT NOT NULL,
    silo_id INTEGER REFERENCES silo_aliment(id),
    quantite_consigne REAL,
    quantite_recue REAL,
    token_import TEXT REFERENCES importjournal(token)
);

CREATE INDEX IF NOT EXISTS ix_consommationsoupe_silo ON consommationsoupe(silo_id);
CREATE INDEX IF NOT EXISTS ix_consommationsoupe_date ON consommationsoupe(date);

-- Catalogue de lignées génétiques (module optionnel « Génétique avancée »,
-- §2 de la spécification). Un petit élevage garde le champ texte libre
-- truie.race ; ce catalogue n'est utile qu'aux élevages en sélection ou
-- multiplication qui suivent un fournisseur, des index et un contrat.
CREATE TABLE IF NOT EXISTS lignee_genetique (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    nom TEXT NOT NULL,
    fournisseur TEXT,
    index_prolificite REAL,
    index_croissance REAL,
    index_ic REAL,
    contrat_renouvellement TEXT,
    note TEXT,
    cree_le TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Les données de démonstration sont supprimées à partir de ce manifeste uniquement.
-- Aucune ligne d'élevage réelle n'est reconnue ou supprimée par son nom.
CREATE TABLE IF NOT EXISTS demoobjet (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    table_name TEXT NOT NULL,
    row_id INTEGER NOT NULL,
    UNIQUE(table_name,row_id)
);

WITH defaults(cle,libelle,valeur,sens,decimales,ordre) AS (VALUES
('cs_truies_saillies','Truies saillies',NULL,'haut',0,1),
('cs_pleines','Pleines à l''écho',NULL,'haut',0,2),
('cs_truies_mb','Truies mises-bas',NULL,'haut',0,3),
('cs_nt_portee','NT / portée',18,'haut',1,4),
('cs_nv_portee','NV / portée',17,'haut',2,5),
('cs_mn_portee','Mort-nés / portée',1,'bas',2,6),
('cs_sevres_portee','Sevrés / portée',14,'haut',2,7),
('cs_tx_pertes_nv','Taux pertes / nés vifs (%)',15,'bas',1,8),
('cs_total_sevres','Total sevrés',300,'haut',0,9),
('cs_poids_sevrage','Poids au sevrage (kg)',NULL,'haut',1,10),
('cs_gmq_ps','GMQ post-sevrage (g/j)',NULL,'haut',0,11),
('cs_gmq_engr','GMQ engraissement (g/j)',NULL,'haut',0,12),
('cs_gmq_nv','GMQ naissance-vente (g/j)',NULL,'haut',0,13),
('sevres_truie_an','Sevrés / truie productive / an',33.5,'haut',1,14),
('portees_truie_an','Portées / truie productive / an',2.35,'haut',2,15))
INSERT INTO objectif(cle,libelle,valeur,sens,decimales,ordre,actif)
SELECT d.cle,d.libelle,d.valeur,d.sens,d.decimales,d.ordre,1 FROM defaults d
WHERE NOT EXISTS(SELECT 1 FROM objectif o WHERE o.cle=d.cle);

WITH defaults(cle,libelle,moyenne,tiers_sup,sens,decimales,annee,ordre) AS (VALUES
('sevres_truie_an','Sevrés / truie productive / an',33.5,35.8,'haut',1,'GTTT 2024',1),
('nes_vifs','Nés vivants / portée',15.8,NULL,'haut',1,'GTTT 2024',2),
('sevres_portee','Sevrés / portée',13.4,NULL,'haut',1,'GTTT 2024',3),
('tx_pertes_allait','Taux pertes / nés vivants (%)',14.8,NULL,'bas',1,'GTTT 2024',4),
('imb','Intervalle mise-bas (j)',146.5,NULL,'bas',1,'GTTT 2024',5),
('age_sevrage','Âge au sevrage (j)',23.5,NULL,'bas',1,'GTTT 2024',6),
('tx_fecondation','Fécondation 1re saillie (%)',91.8,NULL,'haut',1,'GTTT 2024',7),
('tx_renouvellement','Renouvellement (%)',45,NULL,'bas',0,'GTTT 2024',8),
('porcs_truie_an','Porcs produits / truie présente / an',25.3,NULL,'haut',1,'GTE 2023',9),
('tx_pertes_engr','Pertes sevrage → vente (%)',6.4,NULL,'bas',1,'GTE 2023',10))
INSERT INTO referenceifip(cle,libelle,moyenne,tiers_sup,sens,decimales,annee,ordre)
SELECT d.cle,d.libelle,d.moyenne,d.tiers_sup,d.sens,d.decimales,d.annee,d.ordre FROM defaults d
WHERE NOT EXISTS(SELECT 1 FROM referenceifip r WHERE r.cle=d.cle);

INSERT OR IGNORE INTO reglage(cle,valeur,libelle) VALUES
('sevrage',28,'Sevrage'),
('aliment_2e_age',42,'Aliment 2e âge'),
('transfert_engr',71,'Transfert PS → engraissement'),
('aliment_finition',140,'Aliment finition'),
('depart',215,'Départ abattoir'),
('fenetre_alerte',3,'Fenêtre d’alerte (jours)'),
('gestation',115,'Durée gestation'),
('chaleur_j',1,'1re chaleur (j avant IA)'),
('echo_j',28,'Échographie (j après IA)'),
('passage_gestante_j',20,'Passage gestante (j après écho)'),
('passage_maternite_j',5,'Passage maternité (j avant mise-bas)'),
('aliment_1er_age_j',10,'Aliment porcelet 1er âge (j après mise-bas)'),
('retour_j',21,'Retour en chaleur à surveiller (j après IA)'),
('chaleur_post_sevrage_j',5,'Chaleur après sevrage (j)'),
('capacite_verraterie',31,'Capacité de verraterie (places)'),
('capacite_maternite',60,'Capacité de maternité (places)'),
('capacite_postsevrage',300,'Capacité de post-sevrage (places)'),
('capacite_engraissement',800,'Capacité d’engraissement (places)');

INSERT OR IGNORE INTO reglageventedirecte(id) VALUES (1);

WITH defaults(libelle) AS (VALUES
('Écrasement'),('Chétif / non conforme'),('Tué par la truie'),('Diarrhée'),
('Respiratoire'),('Mort subite'),('Boiterie'),('Autre'))
INSERT INTO causeperte(libelle)
SELECT d.libelle FROM defaults d
WHERE NOT EXISTS(SELECT 1 FROM causeperte c WHERE lower(c.libelle)=lower(d.libelle));
