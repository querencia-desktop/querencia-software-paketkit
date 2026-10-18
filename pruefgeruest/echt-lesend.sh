#!/bin/bash
# Lesende Integrationstests gegen den ECHTEN PackageKit auf dem Systembus.
# Nur SearchNames, Resolve, GetDetails, GetUpdates, GetUpdateDetail, GetTransactionList.
# Nichts wird installiert, entfernt oder aktualisiert.
set -euo pipefail
cd "$(dirname "$0")/.."
unset QS_PK_BUS
nice cargo test -j2 --offline --test echt -- --ignored --test-threads=1 "$@"
