#!/usr/bin/env bash
# Updates the translation template and catalogs with fastframe-i18n's script,
# from the fastframe checkout Cargo already has. Pass --check to change nothing
# and fail if the template is out of date. Requires GNU gettext tools with Rust
# support; normal Cargo builds do not.
set -euo pipefail
cd "$(dirname "$0")/../.."
crate=$(cargo metadata --format-version 1 --locked |
    grep -o '"manifest_path":"[^"]*fastframe-i18n/Cargo.toml"' | head -n1 |
    sed 's/^"manifest_path":"//; s/Cargo.toml"$//')
exec "$crate/scripts/update-translations.sh" --package ZapFast --domain zapfast \
    --bugs 'https://github.com/crmne/zapfast/issues/new?template=translation.yml' \
    --keyword translated:2 --fuzzy-matching "$@"
