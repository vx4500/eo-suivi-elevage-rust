#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
export EO_DEMO_PORTAL=1
# Ce lanceur ignore volontairement ELEVAGE_DATA hérité de votre serveur réel.
export ELEVAGE_DATA="$PWD/donnees-demo"
export EO_HOST="${EO_HOST:-127.0.0.1}"
export EO_PORT="${EO_PORT:-18444}"
if [[ ! -f "$ELEVAGE_DATA/elevage.db" ]]; then
  password="${EO_DEMO_ADMIN_PASSWORD:-}"
  if [[ ${#password} -lt 16 ]]; then
    read -r -s -p 'Choisissez le mot de passe administrateur (16 caractères minimum) : ' EO_DEMO_ADMIN_PASSWORD
    printf '\n'
    export EO_DEMO_ADMIN_PASSWORD
  fi
fi
printf 'Démonstration : http://%s:%s — administrateur : admin\n' "$EO_HOST" "$EO_PORT"
exec ./eo-suivi-elevage
