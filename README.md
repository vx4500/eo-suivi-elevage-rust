# EO-Suivi Élevage — portage Rust 2.1.7

Cette archive reprend la base EO-Suivi 1.65 sous forme d’un serveur Rust. Elle
n’utilise plus FastAPI, SQLModel, Uvicorn ni Python pour les fonctions déjà
portées. La base reste au format SQLite `elevage.db` afin de conserver la
compatibilité avec les sauvegardes 1.55 à 1.65.

## Ce qui est déjà natif Rust

- serveur HTTP Axum et accès SQLite asynchrone SQLx ;
- création et migration additive des 49 tables historiques, plus les tables
  eau/électricité ;
- connexion, rôles, limitation des tentatives, CSRF sur les formulaires et
  lecture des mots de passe PBKDF2 créés par la version Python ;
- tableau de bord annuel, bandes, dates clés, truies et événements ;
- chaleurs, inséminations groupées, échographies, mises-bas, sevrages,
  mesures ELD/poids/NEC et pertes de porcelets ;
- compteurs d’eau/électricité, relevés, consommation entre deux index,
  remplacement de compteur, rattachement site/bandes et import CSV ;
- transferts contrôlés des porcs et truies, capacité des cases, inventaires
  remplaçables par date et effectifs recalculés après le dernier comptage ;
- saisies économiques et résultats par bande (coût/porc, marge, prix net/kg) ;
- import PDF économique Cooperl/Uniporc avec aperçu, doublons, mise à jour
  transactionnelle et historique ;
- résultats abattoir par bande et saisies sanitaires par morceau ;
- produits, inventaires, commandes modifiables et imprimables, feuilles de
  préparation, sessions et charges de vente directe ;
- utilisateurs, journal, structure, tâches et sauvegarde de la base ;
- vues de consultation sanitaire, pharmacie, planning et stocks ;
- GTTT centralisé et filtrable par période/bande, productivité annuelle
  réelle, objectifs pondérés modifiables, productivité par bande/cheptel/rang,
  ELD, résultats techniques d'abattage, réformes, cochettes, repères IFIP, charcutiers,
  effectifs et contrôle d’état des données ;
- entretien, saisies abattoir, cahiers des charges et notes quotidiennes en
  lecture/écriture ;
- suivi engraissement, export CSV mise-bas, import CSV truies, fiches
  imprimables et calendrier ICS.

Le détail des modules secondaires restant à réécrire pour atteindre une parité
totale avec les 215 routes de la 1.65 se trouve dans `PORTAGE-RUST.md`.

## Démarrage sur Linux

Installer Rust avec <https://rustup.rs>, puis :

```bash
cargo build --release
ELEVAGE_DATA="$PWD/data" ./target/release/eo-suivi-elevage
```

Ouvrir ensuite <http://localhost:8080>.

Sur une base neuve, le compte temporaire est `admin` / `admin`. Le logiciel
oblige à choisir immédiatement un mot de passe d’au moins huit caractères.

## Réutiliser une base 1.65

1. arrêter l’ancienne application ;
2. conserver une copie de sauvegarde ;
3. placer `elevage.db` dans le dossier défini par `ELEVAGE_DATA` ;
4. démarrer la version Rust.

Les migrations sont additives : aucune table ni colonne historique n’est
supprimée. Il ne faut toutefois pas faire fonctionner les versions Python et
Rust en même temps sur la même base.

## Windows

Double-cliquer sur `build-windows.bat` après avoir installé Rust. Le dossier
`dist-rust` contiendra l’exécutable, le fichier de lancement et les ressources.

## Docker

```bash
docker compose up --build -d
```

La base est conservée dans le volume `eo_data`.

## Variables

| Variable | Valeur par défaut | Usage |
| --- | --- | --- |
| `ELEVAGE_DATA` | `data` | dossier de `elevage.db` |
| `EO_HOST` | `0.0.0.0` | adresse d’écoute |
| `EO_PORT` | `8080` | port HTTP |
| `RUST_LOG` | `info` | niveau du journal |

## Vérifications

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Le schéma SQLite de l’archive a été exécuté sur une base vierge, puis sur une
copie de la sauvegarde réelle du 16 août 2026. Les 51 tables attendues et leurs
colonnes correspondent ; la base réelle contient en plus une ancienne table
énergie vide, laissée intacte. Les contrôles SQLite `quick_check` et clés
étrangères sont conformes. La version 2.0.1 a compilé et démarré sur Debian 13
avec Rust 1.97.1 ; la 2.1.7 doit repasser les trois commandes Cargo ci-dessus
sur le serveur avant remplacement du binaire.
