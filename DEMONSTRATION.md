# EO-Suivi Élevage — démonstration 2.2.49

## Historique économique fictif

Au premier démarrage de cette version, le portail ajoute, en une transaction,
un historique couvrant au moins cinq ans de ventes et de dépenses fictives.
La conduite conserve sept bandes actives et un intervalle de 21 jours entre
cycles ; les cycles anciens sont archivés et ne sont pas des bandes actives
supplémentaires. L'historique des mises-bas remonte plus loin pour couvrir le
délai avant les premières ventes.

Les ventes d'abattoir, aliments (gestation, lactation, post-sevrage, croissance,
finition), frais sanitaires, semences et achats de génétique sont affectés aux
bandes. Les opérations futures ne sont pas créées. Les comptes, mots de passe,
suggestions et saisies existants ne sont pas réinitialisés. Une bande possédant
déjà des données économiques est laissée intacte, donc son historique peut
rester incomplet. Le complément ne se répète pas au redémarrage.

Les tarifs sont des hypothèses illustratives, pas des références de marché.
La marge affichée porte uniquement sur les postes suivis par l'écran économique :
elle n'inclut pas l'ensemble des charges (travail, amortissements, énergie,
financement, etc.) et ne doit pas être présentée comme un bénéfice net.
Le tableau mensuel du portail couvre cinq ans et le mois en cours.

Pour le conteneur OEelevage-demo déjà installé, utiliser après récupération du
correctif : `bash scripts/mettre-a-jour-demo.sh`. Le script teste et compile
séparément du binaire actif, sauvegarde la base à l'arrêt, puis contrôle la page
de connexion. En cas d'échec, il restaure le binaire et la base sauvegardés.

## Démarrage sur Linux x86_64

Décompressez l’archive dans un nouveau dossier, puis lancez :

    bash scripts/lancer-demo.sh

Choisissez un mot de passe administrateur d’au moins 16 caractères. Ouvrez http://127.0.0.1:18444, connectez-vous avec `admin` et ce mot de passe. Le premier lancement génère un élevage fictif ; patientez jusqu’au démarrage du serveur.

Dans **Gérer les accès et suggestions** (bandeau de démonstration), renseignez le nom et l’identifiant de chaque testeur. Le mot de passe aléatoire est affiché une seule fois. Transmettez-le de manière privée. L’accès expire 48 heures après sa création ; une session ouverte est également refusée après expiration. Vous pouvez révoquer un accès immédiatement.

La bulle « Quelle évolution aimeriez-vous ? » est disponible sur toutes les pages après connexion. Les suggestions restent dans cette base locale ; aucun courriel n’est envoyé. L’administrateur les consulte dans la gestion des accès. Les participants partagent les mêmes données fictives : ne saisir aucune donnée réelle ou confidentielle.

## Séparation de l’exploitation

Aucune base, sauvegarde, facture, mot de passe ni configuration de votre exploitation n’est dans l’archive. Le catalogue génétique contient les faits extraits du document fourni, sans le PDF original. Au lancement, le dossier `donnees-demo` est créé indépendamment de votre configuration habituelle. Le programme refuse une base existante sans marqueur de démonstration.

Les mises à jour, restaurations, paramètres système, imports externes et communications réelles sont désactivés dans cette démonstration. Les autres écrans et opérations d’élevage utilisent l’application habituelle. Les comptes et suggestions sont anonymisés ou supprimés après 90 jours (contrôle au démarrage et toutes les heures).

Arrêt : Ctrl+C dans le terminal. Conservez votre mot de passe administrateur : aucun mot de passe universel n’est fourni. Ne supprimez jamais les données de votre vrai serveur pour réinitialiser cette démo.

## Accès depuis d’autres ordinateurs

Par défaut, le service n’écoute que sur cet ordinateur. Pour un accès réseau contrôlé, configurez EO_HOST, un pare-feu et un reverse proxy HTTPS ; avec HTTPS, activez `EO_SECURE_COOKIES=1`. N’exposez pas directement un service HTTP contenant des mots de passe sur Internet. Aucune mise en ligne n’est effectuée par cette archive.

Avant ouverture publique, complétez les mentions légales avec votre adresse professionnelle et celles de l’hébergeur. La page Contact reprend uniquement les informations que vous avez communiquées. Les identifiants professionnels applicables restent à compléter.

## Autres systèmes / compilation

Le binaire inclus est compilé pour Linux x86_64 avec glibc 2.34 ou plus récente (notamment Debian 12/13 ou Ubuntu 22.04/24.04). Si votre système est incompatible, les sources, Cargo.lock et un Dockerfile sont fournis pour reconstruire le serveur. La compilation Docker nécessite une connexion pour récupérer l’image Rust et ses dépendances. Windows/macOS : utilisez Docker ou un serveur Linux ; ce n’est pas un fichier HTML autonome. Le Dockerfile est fourni mais sa construction n’a pas été exécutée ici.

© 2026 Emmanuel ORY, éléments originaux d’EO-Suivi Élevage. Les composants tiers conservent leurs propres licences ; voir `LICENCES-TIERS` et le manifeste fourni dans l’archive. L’accès de démonstration n’accorde aucun droit de redistribution des éléments originaux sans autorisation.
