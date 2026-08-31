#!/usr/bin/env bash
# Application normale, données exclusivement fictives, ancienne base conservée.
set -Eeuo pipefail
app=/opt/eo-suivi-rust-demo
service=eo-suivi-demo
source_db="$app/donnees-demo/elevage.db"
override=/etc/systemd/system/eo-suivi-demo.service.d/90-mode-normal.conf

[[ $(id -u) == 0 ]] || { echo 'Exécuter dans le conteneur avec root.'; exit 1; }
for cmd in sqlite3 systemctl curl flock; do
    command -v "$cmd" >/dev/null || { echo "Commande manquante : $cmd"; exit 1; }
done
exec 9>/run/lock/eo-demo-update.lock
flock -n 9 || { echo 'Une autre opération est en cours.'; exit 1; }
cd "$app"
[[ -s "$source_db" ]]
[[ $(systemctl show "$service" -p WorkingDirectory --value) == "$app" ]]
[[ $(systemctl show "$service" -p User --value) == root ]]
environment=$(systemctl show "$service" -p Environment --value)
[[ " $environment " == *" EO_DEMO_PORTAL=1 "* ]] || {
    echo 'Le service doit encore être en mode démonstration. Aucun changement effectué.'; exit 1;
}
[[ " $environment " == *" ELEVAGE_DATA=$app/donnees-demo "* ]]
[[ " $environment " == *" EO_PORT=8080 "* ]]
[[ ! -e "$override" ]] || { echo "Configuration déjà présente : $override"; exit 1; }
[[ $(sqlite3 -readonly "$source_db" "SELECT valeur FROM parametre WHERE cle='demo_portal';") == 1 ]]
[[ $(sqlite3 -readonly "$source_db" "SELECT valeur FROM parametre WHERE cle='demo_economie_v1';") == 1 ]] || {
    echo 'Installer d’abord les cinq ans fictifs : bash scripts/mettre-a-jour-demo.sh'; exit 1;
}
systemctl is-active --quiet "$service"
backup=$(mktemp -d /var/backups/eo-demo-mode-normal.XXXXXX)
destination=$(mktemp -d "$app/donnees-test-complet.XXXXXX")
systemctl cat "$service" > "$backup/service-avant.conf"
printf '%s\n' "$source_db" > "$backup/base-originale.txt"
printf '%s\n' "$destination" > "$backup/dossier-test.txt"
created=0
rollback() {
    trap - ERR INT TERM
    echo "Échec : retour au portail initial. Sauvegarde : $backup"
    systemctl stop "$service" || exit 1
    if [[ $created == 1 && -e "$override" ]]; then
        mv "$override" "$backup/90-mode-normal-annule.conf" || exit 1
    fi
    systemctl daemon-reload
    systemctl start "$service"
    echo 'La base originale est intacte ; la copie de test est conservée.'
    exit 1
}
trap rollback ERR INT TERM
systemctl stop "$service"
sqlite3 "$source_db" ".backup '$backup/elevage.db'"
[[ $(sqlite3 -readonly "$backup/elevage.db" 'PRAGMA quick_check;') == ok ]]
cp -a "$backup/elevage.db" "$destination/elevage.db"
# Activer les modules métier ; ne changer aucun rôle, compte ou mot de passe.
sqlite3 -bail "$destination/elevage.db" <<'SQL'
BEGIN IMMEDIATE;
INSERT INTO parametre(cle,valeur) VALUES
 ('type_elevage','naisseur_engraisseur'),
 ('module_genetique','1'),
 ('module_prestataires','1'),
 ('module_charcutiers_rfid','1'),
 ('module_vente_directe','1')
ON CONFLICT(cle) DO UPDATE SET valeur=excluded.valeur;
COMMIT;
SQL
mkdir -p /etc/systemd/system/eo-suivi-demo.service.d
created=1
printf '[Service]\nEnvironment=EO_DEMO_PORTAL=0\nEnvironment=ELEVAGE_DATA=%s\nEnvironment=EO_SECURE_COOKIES=1\n' "$destination" > "$override"
systemctl daemon-reload
systemctl start "$service"
ok=0
for tentative in {1..30}; do
    if curl --max-time 3 -fsS http://127.0.0.1:8080/login -o "$backup/login.html" \
        && grep -Fq 'Version Rust' "$backup/login.html" \
        && ! grep -Fq 'mot de passe de 48 h' "$backup/login.html"; then
        ok=1
        break
    fi
    sleep 2
done
[[ $ok == 1 ]]
systemctl is-active --quiet "$service"
environment=$(systemctl show "$service" -p Environment --value)
[[ " $environment " == *" EO_DEMO_PORTAL=0 "* ]]
[[ " $environment " == *" ELEVAGE_DATA=$destination "* ]]
trap - ERR INT TERM
echo 'Mode NORMAL actif avec les données fictives. Reconnectez-vous sur votre adresse HTTPS habituelle.'
echo "Base de test : $destination/elevage.db"
echo "Sauvegarde et configuration initiale : $backup"
echo 'Comptes, mots de passe et rôles inchangés ; expiration de démonstration désactivée.'
echo 'Les fonctions normales, dont imports, restauration et communications, sont maintenant accessibles selon le rôle.'
echo 'Ne configurez pas de vrais destinataires ni de clé d’envoi sur ce site de test.'
