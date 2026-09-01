# EO-Suivi Élevage — portage Rust 2.2.50

Cette archive reprend la base EO-Suivi 1.65 sous forme d'un serveur Rust. Elle
n'utilise plus FastAPI, SQLModel, Uvicorn ni Python pour les fonctions déjà
portées. La base reste au format SQLite `elevage.db` afin de conserver la
compatibilité avec les sauvegardes 1.55 à 1.65.

## Ce qui est déjà natif Rust

- serveur HTTP Axum et accès SQLite asynchrone SQLx ;
- création et migration additive des tables historiques, plus les tables
  eau/électricité ;
- connexion, rôles, limitation des tentatives, CSRF sur les formulaires et
  lecture des mots de passe PBKDF2 créés par la version Python ;
- centre de pilotage annuel avec parcours professionnel des huit étapes de
  chaque bande, stade actif, prochaine intervention, délai, dates clés,
  effectif de truies et événements ;
- chaleurs, inséminations groupées, échographies, mises-bas, sevrages,
  mesures ELD/poids/NEC et pertes de porcelets ;
- tableau professionnel de maternité par bande, avec état de chaque truie,
  mise à jour progressive, pertes jusqu'à J+28 et sevrage vers une case/vanne
  de post-sevrage créant le mouvement d'effectif ;
- adoptions entre truies et vers une nourrice artificielle, lots rattachés à leur
  bande, pertes et sevrages tracés sans double comptage ;
- fiche truie organisée en onglets, mesures et historique modifiables, liaison
  case/vanne, soins de portée et analyse de l'IA probablement fécondante ;
- file IA/cochettes après sevrage : cochettes, sorties de maternité et
  retours d'IA sans bande regroupés, affectation groupée, capacité de
  verraterie réglable (31 places par défaut) ;
- compteurs d'eau/électricité, relevés, consommation entre deux index,
  remplacement de compteur, rattachement site/bandes et import CSV ;
- transferts contrôlés des porcs et truies, capacité des cases, inventaires
  remplaçables par date et effectifs recalculés après le dernier comptage ;
- mouvement des porcs par bande ou unité depuis le bouton « + », suivi des
  emplacements, effectif et numéro de marquage ;
- saisie rapide des mises-bas et des pertes de porcs ou porcelets ;
- saisies économiques et résultats par bande (coût/porc, marge, prix net/kg) ;
- import PDF économique Cooperl/Uniporc : aperçu, doublons, sélection
  simultanée de 1 à 5 documents, transaction globale et historique ;
- reclassement automatique des avoirs génétiques et des montants à signe
  moins final ;
- résultats abattoir par bande et saisies sanitaires par morceau ;
- produits, inventaires, commandes modifiables et imprimables, classement des
  ventes par quantité/kg, prix ou chiffre d'affaires, fermeture de la page
  client, feuilles de préparation, sessions et charges de vente directe ;
- utilisateurs, journal, structure, tâches, téléchargement et restauration
  contrôlée de la base ;
- vues de consultation sanitaire, pharmacie, planning et stocks ;
- GTTT centralisé et filtrable par période/bande, productivité annuelle
  réelle, objectifs pondérés modifiables, productivité par bande/cheptel/rang,
  ELD, résultats techniques d'abattage, réformes, cochettes, repères IFIP,
  charcutiers, effectifs et contrôle d'état des données ;
- entretien, saisies abattoir, cahiers des charges et notes quotidiennes en
  lecture/écriture ;
- suivi engraissement, export CSV mise-bas, import CSV truies, fiches
  imprimables et calendrier ICS ;
- parité des 68 anciennes URL historiquement absentes : saisie rapide,
  attente, scanner/QR, PDF serveur, réglages et plans d'aliment, restauration
  contrôlée, rattachement économique, communications Brevo et détail des
  sessions.

Le détail de la correspondance fonctionnelle avec la 1.65 se trouve dans
`Etat-projet-eo-suivi-rust.md`. L'OCR des images reste volontairement un
module externe : les imports PDF texte sont natifs et les scans sans couche
texte sont refusés.

## Démarrage sur Linux

Installer Rust avec <https://rustup.rs>, puis :

```bash
cargo build --release
ELEVAGE_DATA="$PWD/data" ./target/release/eo-suivi-elevage
```

Ouvrir ensuite <http://localhost:8080>.

Sur le serveur Debian 13 déjà relié au dépôt GitHub, la mise à jour contrôlée
se fait avec `/opt/eo-suivi-rust-src/scripts/mettre-a-jour-debian13.sh`. Elle
teste et sauvegarde avant l'arrêt, puis restaure l'ancien binaire si le
contrôle de santé échoue.

Sur une base neuve, le compte temporaire est `admin` / `admin`. Le logiciel
oblige à choisir immédiatement un mot de passe d'au moins huit caractères.

## Réutiliser une base 1.65

1. arrêter l'ancienne application ;
2. conserver une copie de sauvegarde ;
3. placer `elevage.db` dans le dossier défini par `ELEVAGE_DATA` ;
4. démarrer la version Rust.

Les migrations sont additives : aucune table ni colonne historique n'est
supprimée. Il ne faut toutefois pas faire fonctionner les versions Python et
Rust en même temps sur la même base.

## Démonstration fictive

Le portail `EO_DEMO_PORTAL=1` dispose d'un historique économique d'au moins
cinq ans pour une conduite en sept bandes actives. Les anciennes démos sont
complétées une seule fois, sans réinitialiser les comptes ni remplacer les
bandes déjà renseignées économiquement. Voir `DEMONSTRATION.md` pour le détail
des données et `scripts/mettre-a-jour-demo.sh` pour le conteneur de démonstration.

## Docker

```bash
docker compose up --build -d
```

La base est conservée dans le volume `eo_data`.

## Variables

| Variable | Valeur par défaut | Usage |
| --- | --- | --- |
| `ELEVAGE_DATA` | `data` | dossier de `elevage.db` |
| `EO_HOST` | `0.0.0.0` | adresse d'écoute |
| `EO_PORT` | `8080` | port HTTP |
| `EO_SECURE_COOKIES` | `false` | mettre `true` lorsque le site est servi en HTTPS |
| `RUST_LOG` | `info` | niveau du journal |

## Vérifications

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Pour produire un binaire Linux x86-64 statique, installable sans dépendre de
la version de glibc du serveur :

```bash
sudo apt-get install musl-tools
./scripts/build-linux-musl.sh
```

Le schéma SQLite de l'archive a été exécuté sur une base vierge, puis sur une
copie de la sauvegarde réelle du 16 août 2026. Les tables attendues et
leurs colonnes correspondent ; la base réelle contient en plus une ancienne
table énergie vide, laissée intacte. Les contrôles SQLite `quick_check` et
clés étrangères sont conformes. Chaque mise à jour doit repasser les trois
commandes Cargo ci-dessus sur le serveur avant remplacement du binaire.
