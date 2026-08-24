#!/usr/bin/env bash
set -Eeuo pipefail

export PATH="/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

SRC_DIR="${EO_SOURCE_DIR:-/opt/eo-suivi-rust-src}"
APP_DIR="${EO_APP_DIR:-/opt/eo-suivi-rust}"
DB_FILE="${EO_DATABASE:-/var/lib/eo-suivi/elevage.db}"
BACKUP_DIR="${EO_BACKUP_DIR:-/var/backups/eo-suivi-rust}"
SERVICE="${EO_SERVICE:-eo-suivi-rust}"
SSH_KEY="${EO_GITHUB_SSH_KEY:-/root/.ssh/eo-suivi-rust-github}"
LOGIN_URL="${EO_LOGIN_URL:-http://127.0.0.1:8080/login}"
STYLE_URL="${EO_STYLE_URL:-http://127.0.0.1:8080/static/style.css}"

die() {
    echo "Erreur : $*" >&2
    exit 1
}

if [[ $(id -u) -ne 0 ]]; then
    die "cette commande doit être lancée avec root."
fi

for command in cargo curl flock git install sqlite3 systemctl; do
    command -v "$command" >/dev/null 2>&1 || die "commande manquante : $command"
done

exec 9>/run/lock/maj-eo-suivi-rust.lock
flock -n 9 || die "une mise à jour EO-Suivi est déjà en cours."

[[ -d "$SRC_DIR/.git" ]] || die "dépôt Git introuvable : $SRC_DIR"
[[ -f "$DB_FILE" ]] || die "base SQLite introuvable : $DB_FILE"
[[ -r "$SRC_DIR/Cargo.toml" ]] || die "Cargo.toml introuvable dans $SRC_DIR"

mkdir -p "$BACKUP_DIR" "$APP_DIR"
cd "$SRC_DIR"

# Cargo.lock peut être produit localement sur ce dépôt historique. Les fichiers
# suivis par Git doivent en revanche rester strictement intacts.
if [[ -n $(git status --porcelain --untracked-files=no) ]]; then
    git status --short
    die "le dépôt contient des modifications locales suivies par Git."
fi

if [[ -f "$SSH_KEY" ]]; then
    export GIT_SSH_COMMAND="ssh -i $SSH_KEY -o IdentitiesOnly=yes"
fi

echo "Recherche de la dernière version sur GitHub..."
git fetch origin main
git merge --ff-only origin/main

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
[[ -n "$version" ]] || die "version Cargo introuvable."

if systemctl is-active --quiet "$SERVICE"; then
    current_page=$(curl -fsS "$LOGIN_URL" 2>/dev/null || true)
    current_style=$(curl -fsS "$STYLE_URL?v=$version" 2>/dev/null || true)
    if grep -Fq "Version Rust $version" <<<"$current_page" \
        && grep -Fq "v$version" <<<"$current_style"; then
        echo "EO-Suivi Rust $version et son interface sont déjà installés."
        exit 0
    fi
fi

echo "Contrôles et compilation de la version $version..."
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release

stamp=$(date +%Y%m%d_%H%M%S)
db_backup="$BACKUP_DIR/elevage_avant_${version}_${stamp}.db"
binary_backup="$BACKUP_DIR/eo-suivi-elevage_${stamp}"
unit_backup="$BACKUP_DIR/eo-suivi-rust.service_${stamp}"
static_backup="$BACKUP_DIR/static_${stamp}"
new_binary="$APP_DIR/.eo-suivi-elevage.new-${stamp}"

echo "Sauvegarde contrôlée de la base : $db_backup"
sqlite3 "$DB_FILE" ".backup '$db_backup'"
[[ $(sqlite3 "$db_backup" "PRAGMA quick_check;") == "ok" ]] || die "la sauvegarde SQLite est invalide."

binary_saved=0
unit_saved=0
static_saved=0
if [[ -f "$APP_DIR/eo-suivi-elevage" ]]; then
    cp -a "$APP_DIR/eo-suivi-elevage" "$binary_backup"
    binary_saved=1
fi
if [[ -f /etc/systemd/system/eo-suivi-rust.service ]]; then
    cp -a /etc/systemd/system/eo-suivi-rust.service "$unit_backup"
    unit_saved=1
fi
if [[ -d "$APP_DIR/static" ]]; then
    cp -a "$APP_DIR/static" "$static_backup"
    static_saved=1
fi

# Préparer le nouveau binaire sur le même système de fichiers permet au mv
# suivant de remplacer l'exécutable de façon atomique.
install -m 0755 target/release/eo-suivi-elevage "$new_binary"
chown eo-suivi:eo-suivi "$new_binary"

rollback_required=0
restore_previous_release() {
    status=$?
    trap - EXIT
    if [[ "$rollback_required" -eq 1 ]]; then
        echo "Échec du déploiement : restauration de la version précédente." >&2
        systemctl stop "$SERVICE" >/dev/null 2>&1 || true
        if [[ "$binary_saved" -eq 1 ]]; then
            install -m 0755 "$binary_backup" "$APP_DIR/eo-suivi-elevage"
            chown eo-suivi:eo-suivi "$APP_DIR/eo-suivi-elevage"
        fi
        if [[ "$unit_saved" -eq 1 ]]; then
            install -m 0644 "$unit_backup" /etc/systemd/system/eo-suivi-rust.service
        fi
        if [[ "$static_saved" -eq 1 ]]; then
            mkdir -p "$APP_DIR/static"
            cp -a "$static_backup/." "$APP_DIR/static/"
        fi
        systemctl daemon-reload || true
        systemctl start "$SERVICE" || true
        echo "La base sauvegardée reste disponible dans $db_backup" >&2
    fi
    exit "$status"
}
trap restore_previous_release EXIT

rollback_required=1
echo "Installation atomique de la version $version..."
systemctl stop "$SERVICE"
mv "$new_binary" "$APP_DIR/eo-suivi-elevage"
mkdir -p "$APP_DIR/static"
cp -a static/. "$APP_DIR/static/"
install -m 0644 eo-suivi-rust.service /etc/systemd/system/eo-suivi-rust.service
install -m 0644 eo-suivi-rust-update.service /etc/systemd/system/eo-suivi-rust-update.service
install -m 0644 eo-suivi-rust-update.path /etc/systemd/system/eo-suivi-rust-update.path
chown -R eo-suivi:eo-suivi "$APP_DIR"
systemctl daemon-reload
systemctl enable "$SERVICE" >/dev/null
systemctl enable --now eo-suivi-rust-update.path >/dev/null
systemctl start "$SERVICE"

healthy=0
for _ in {1..15}; do
    if current_page=$(curl -fsS "$LOGIN_URL" 2>/dev/null) \
        && current_style=$(curl -fsS "$STYLE_URL?v=$version" 2>/dev/null) \
        && grep -Fq "Version Rust $version" <<<"$current_page" \
        && grep -Fq "v$version" <<<"$current_style"; then
        healthy=1
        break
    fi
    sleep 1
done
[[ "$healthy" -eq 1 ]] || die "le contrôle HTTP de la version ou de l'interface a échoué."

rollback_required=0
trap - EXIT

echo "Mise à jour terminée : EO-Suivi Rust $version"
echo "Sauvegarde SQLite : $db_backup"
systemctl --no-pager --full status "$SERVICE" | sed -n '1,12p'
