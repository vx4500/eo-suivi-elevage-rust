# Application Android EO-Suivi

Coquille Android autour de l'application web servie par le serveur de
l'élevage. Tous les écrans et toute la logique métier restent côté serveur :
**mettre à jour le serveur met à jour l'application sur tous les téléphones**,
sans passer par un store et sans réinstaller quoi que ce soit.

L'APK n'a besoin d'être remplacé que si la coquille elle-même change
(découverte, gestion des adresses, niveau d'API imposé par Google).

## Ce que fait l'application

- **Trouve le serveur toute seule** sur le réseau local, par mDNS
  (`_eosuivi._tcp`, publié par `src/mdns.rs`). Indispensable dans un élevage
  sans accès internet : ni domaine, ni DNS, ni certificat.
- **Saisie manuelle en repli**, toujours disponible : des points d'accès Wi-Fi
  filtrent le multicast et le mDNS ne traverse pas les VLAN.
- **Deux adresses mémorisées**, interne et externe. Au lancement, l'adresse
  interne est testée par une connexion TCP d'une seconde ; si elle ne répond
  pas, l'externe prend le relais. Contrairement à la bascule par SSID de
  l'application Home Assistant, cela ne demande aucune permission de
  localisation et fonctionne avec plusieurs bornes Wi-Fi.
- **Imports et exports** : sélecteur de fichier pour les imports CSV/PDF,
  téléchargement des exports vers le dossier Téléchargements.
- Bouton retour suivant l'historique du site, tirer pour rafraîchir, menu
  « Changer de serveur ».

## Compiler

Nécessite un JDK 17+ et le SDK Android (plateforme 34, build-tools 34).

```bash
cd android
echo "sdk.dir=$HOME/Android/Sdk" > local.properties
./gradlew :app:assembleDebug          # APK de test, signé avec la clé de debug
./gradlew :app:testDebugUnitTest      # tests unitaires (normalisation d'adresse)
```

L'APK est produit dans `app/build/outputs/apk/debug/app-debug.apk`.

Pour une version distribuable, il faut une clé de signature :

```bash
keytool -genkey -v -keystore eo-suivi.keystore -alias eo-suivi \
        -keyalg RSA -keysize 2048 -validity 10000
./gradlew :app:assembleRelease
```

Le fichier `.keystore` ne doit jamais être versionné : sans lui, plus aucune
mise à jour de l'APK déjà installé n'est possible.

## Installer sur un téléphone

Copier l'APK sur le téléphone, l'ouvrir depuis le gestionnaire de fichiers, et
autoriser l'installation depuis cette application quand Android le demande.

## Point de sécurité assumé

Le HTTP en clair est autorisé (`res/xml/network_security_config.xml`), parce
qu'un élevage sans internet ne peut pas obtenir de certificat. Sur ces
installations, le cookie de session circule en clair sur le réseau local, et
le serveur doit tourner avec `EO_SECURE_COOKIES=false` — sinon le navigateur
refuse de renvoyer le cookie et la connexion boucle. Un élevage disposant d'un
domaine et d'un certificat utilise l'adresse externe en HTTPS et remet
`EO_SECURE_COOKIES=true`.
