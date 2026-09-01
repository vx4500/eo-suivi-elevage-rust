#!/usr/bin/env bash
# Mise à jour du conteneur de démonstration installé dans ce parcours, y
# compris après son passage contrôlé en mode normal.
# Ne télécharge rien depuis Git : récupérer d'abord le correctif validé.
set -Eeuo pipefail
export PATH="/root/.cargo/bin:$PATH"
app=/opt/eo-suivi-rust-demo
service=eo-suivi-demo
binary="$app/target/release/eo-suivi-elevage"

[[ $(id -u) == 0 ]] || { echo 'Exécuter dans le conteneur avec root.'; exit 1; }
for cmd in cargo sqlite3 curl systemctl flock; do
    command -v "$cmd" >/dev/null || { echo "Commande manquante : $cmd"; exit 1; }
done
exec 9>/run/lock/eo-demo-update.lock
flock -n 9 || { echo 'Une mise à jour est déjà en cours.'; exit 1; }
cd "$app"
[[ $(realpath "$(dirname "$0")/..") == "$app" ]]
[[ -x "$binary" ]]
[[ $(systemctl show "$service" -p WorkingDirectory --value) == "$app" ]]
systemctl show "$service" -p ExecStart --value | grep -Fq "path=$binary ;"
environment=$(systemctl show "$service" -p Environment --value)
data_dir=$(sed -n 's/.* ELEVAGE_DATA=\([^ ]*\).*/\1/p' <<<" $environment ")
[[ -n "$data_dir" && "$data_dir" == "$app"/* ]]
db="$data_dir/elevage.db"
[[ -s "$db" ]]
demo_portal=0
if [[ " $environment " == *" EO_DEMO_PORTAL=1 "* ]]; then
    demo_portal=1
fi
[[ " $environment " == *" EO_PORT=8080 "* ]]
if [[ $demo_portal == 1 ]]; then
    [[ $(sqlite3 -readonly "$db" "SELECT valeur FROM parametre WHERE cle='demo_portal';") == 1 ]]
fi
systemctl is-active --quiet "$service"

echo 'Tests et compilation séparés ; la démo reste accessible.'
cargo test --locked -j 2
cargo build --locked --release -j 2 --target-dir target/demo-update
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
[[ -n "$version" ]]
backup=$(mktemp -d /var/backups/eo-demo-economie.XXXXXX)
cp -a "$binary" "$backup/eo-suivi-elevage"
cp -a /etc/systemd/system/eo-suivi-demo.service "$backup/"
saved=0
rollback() {
    trap - ERR INT TERM
    echo "Échec : restauration. Sauvegarde : $backup"
    # Ne jamais restaurer la base si l'arrêt n'a pas réussi.
    systemctl stop "$service" || exit 1
    if [[ $saved == 1 ]]; then
        sqlite3 "$db" ".restore '$backup/elevage.db'" || exit 1
    fi
    install -m 755 "$backup/eo-suivi-elevage" "$binary" || exit 1
    systemctl start "$service"
    exit 1
}
trap rollback ERR INT TERM
systemctl stop "$service"
sqlite3 "$db" ".backup '$backup/elevage.db'"
[[ $(sqlite3 -readonly "$backup/elevage.db" 'PRAGMA quick_check;') == ok ]]
saved=1
install -m 755 target/demo-update/release/eo-suivi-elevage "$binary"
systemctl start "$service"
ok=0
for tentative in {1..60}; do
    if curl --max-time 3 -fsS http://127.0.0.1:8080/login -o "$backup/login.html" \
        && grep -Fq "Version Rust $version" "$backup/login.html"; then
        ok=1
        break
    fi
    sleep 2
done
[[ $ok == 1 ]]
systemctl is-active --quiet "$service"
if [[ $demo_portal == 1 ]]; then
    [[ $(sqlite3 -readonly "$db" "SELECT valeur FROM parametre WHERE cle='demo_economie_v1';") == 1 ]]
fi
trap - ERR INT TERM
echo "Version Rust $version active."
echo "Sauvegarde conservée : $backup"
if [[ $demo_portal == 1 ]]; then
    echo 'Les bandes qui avaient déjà des saisies économiques ont été laissées intactes.'
else
    echo 'Instance issue de la démo et passée en mode normal : configuration conservée.'
fi
