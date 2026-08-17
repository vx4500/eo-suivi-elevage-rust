# Mise à jour du serveur Debian 13

La version 2.2.0 fournit une mise à jour automatisée et réversible depuis le
dépôt GitHub privé. Elle conserve les chemins de production :

- sources : `/opt/eo-suivi-rust-src` ;
- application : `/opt/eo-suivi-rust` ;
- base : `/var/lib/eo-suivi/elevage.db` ;
- sauvegardes : `/var/backups/eo-suivi-rust` ;
- service : `eo-suivi-rust`.

La base ne doit jamais être copiée dans le dépôt source. Les versions Python et
Rust ne doivent jamais écrire simultanément dans le même fichier SQLite.

## Mise à jour recommandée

Sur le serveur, en `root` :

```bash
cd /opt/eo-suivi-rust-src
GIT_SSH_COMMAND='ssh -i /root/.ssh/eo-suivi-rust-github -o IdentitiesOnly=yes' \
git pull --ff-only origin main
./scripts/mettre-a-jour-debian13.sh
```

Les prochains déploiements pourront ensuite se faire avec la seule commande :

```bash
/opt/eo-suivi-rust-src/scripts/mettre-a-jour-debian13.sh
```

Le script :

1. empêche deux mises à jour simultanées avec un verrou système ;
2. refuse les modifications locales suivies par Git ;
3. récupère `main` uniquement en avance rapide ;
4. exécute `cargo check`, Clippy, les tests et la compilation release sans
   interrompre le service ;
5. crée une sauvegarde SQLite vérifiée par `PRAGMA quick_check` ;
6. conserve l'ancien binaire et l'ancienne unité systemd ;
7. remplace le binaire atomiquement, redémarre le service et contrôle à la fois
   la page de connexion et le numéro de version ;
8. restaure automatiquement l'ancienne version si l'installation ou le
   contrôle HTTP échoue.

Un échec de compilation laisse donc l'application en service. Un échec après
l'arrêt déclenche le retour arrière ; la sauvegarde de la base est conservée.

## Vérification manuelle

```bash
systemctl status eo-suivi-rust --no-pager -l
journalctl -u eo-suivi-rust -n 80 --no-pager -l -o cat
curl -fsS http://127.0.0.1:8080/login | grep -F "Version Rust 2.2.0"
```

Puis ouvrir `https://rust-elevage.basse-chevrie.ovh`.
