# Mise à jour du serveur Debian 13

Le serveur de recette écoute sur le port `8080`. La base reste dans
`/var/lib/eo-suivi/elevage.db` et ne doit jamais être placée dans le
dossier du code.

Ce chemin historique est volontairement conservé pendant les mises à jour afin
de ne jamais démarrer par erreur sur une seconde base vide.

## 1. Décompresser et compiler

```bash
cd /opt
unzip -o /root/eo-suivi-elevage-v2.0-rust-portage.zip
cd /opt/eo-suivi-elevage-v2.0-rust
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Si `clippy` ou les tests échouent, ne remplace pas le binaire qui fonctionne.

## 2. Installer le binaire validé

```bash
systemctl stop eo-suivi-rust
cp target/release/eo-suivi-elevage /opt/eo-suivi-rust/eo-suivi-elevage
cp -a static /opt/eo-suivi-rust/
cp eo-suivi-rust.service /etc/systemd/system/eo-suivi-rust.service
chown -R eo-suivi:eo-suivi /opt/eo-suivi-rust /var/lib/eo-suivi
systemctl daemon-reload
systemctl enable --now eo-suivi-rust
```

## 3. Vérifier

```bash
systemctl status eo-suivi-rust --no-pager
journalctl -u eo-suivi-rust -n 80 --no-pager
curl -I http://127.0.0.1:8080/login
```

Puis ouvrir `https://rust-elevage.basse-chevrie.ovh`.

La version Python et la version Rust ne doivent jamais écrire en même temps
dans le même fichier SQLite.
