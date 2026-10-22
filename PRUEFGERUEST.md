# Prüfgerüst: privater PackageKit ohne root

Ziel: schreibende Transaktionen (Installieren, Entfernen, Aktualisieren, Abbrechen,
Simulation) prüfen, **ohne** ein echtes Paket anzufassen und ohne sudo.

## Aufbau

```text
dbus-daemon (eigene Konfiguration, <type>system</type>, Socket $QS_PK_DIR/sock)
 ├─ qs-pk-attrappe polkit     → org.freedesktop.PolicyKit1 (erlauben | verweigern | verzoegern:MS)
 └─ packagekitd --backend=test_* --disable-timer --keep-environment   (DBUS_SYSTEM_BUS_ADDRESS=…)
    oder qs-pk-attrappe paketkit → org.freedesktop.PackageKit (Attrappe mit Paketbestand)
qs_paketkit ← QS_PK_BUS=unix:path=$QS_PK_DIR/sock
```

```bash
pruefgeruest/pk-pruefbus.sh start attrappe                  # oder test_thread | test_fail | …
eval "$(pruefgeruest/pk-pruefbus.sh env)"                   # setzt QS_PK_BUS
cargo run --offline --example pk-suche -- querencia
pruefgeruest/pk-pruefbus.sh stop
```

Optionen: `--polkit erlauben|verweigern|verzoegern:MS|keiner` (Standard `erlauben`),
Ordner `QS_PK_DIR` (Standard `/tmp/qs-pk-$USER`), Attrappe `QS_PK_ATTRAPPE` (Standard
`target/debug/qs-pk-attrappe`, wird bei Bedarf gebaut), Daemon `QS_PKD`. Das Skript lehnt
jedes Backend außer `test_*` und `attrappe` ab. Die Integrationstests (`tests/pruefbus.rs`)
bauen sich je Test einen eigenen Bus unter `/tmp/qs-pk-t<pid>-<n>/` und räumen ihn ab.

## Befunde: packagekitd als Benutzer (PackageKit 1.3.4, Ubuntu 26.04)

| Frage | Ergebnis |
|---|---|
| Startet `packagekitd` ohne root? | **Ja**, mit `--backend=test_* --disable-timer --keep-environment` und `DBUS_SYSTEM_BUS_ADDRESS` auf den privaten Bus. Warnungen ohne Folgen: logind fehlt, `transactions.db` nur lesbar, Download-Cache nicht löschbar. |
| Ohne `--keep-environment`? | packagekitd verwirft die Umgebung und nimmt den echten Systembus; dort darf nur root den Namen besitzen. **Immer** `--keep-environment`. |
| Braucht es polkit? | Lesende Rollen nicht. Schreibende fragen `org.freedesktop.PolicyKit1`; fehlt der Dienst, endet die Transaktion mit `not-authorized` („ServiceUnknown“). Mit der polkit-Attrappe (erlauben) laufen sie durch. Kein `PACKAGEKIT_NO_POLKIT` nötig. |
| Socketpfad | Unix-Sockets ≤ 107 Zeichen: unter dem Scratchpad zu lang („Socket name too long“), daher `/tmp/qs-pk-…`. |
| GUID in der Adresse | Das Skript lässt `,guid=…` weg, sonst scheitert eine alte `QS_PK_BUS` nach Neustart am Handshake. |

### Was die mitgelieferten Test-Backends können

| Backend | Lesen | Schreiben | Einsatz in den Tests |
|---|---|---|---|
| `test_thread` | `SearchNames`: 2 Pakete (glib2, gtk2), Percentage 0→100 in ~1 s, Cancel geht | nichts (NotSupported) | Suche, Cancel, `GetTransactionList`, Daemon-Abbruch während Suche |
| `test_fail` | Suche → `internal-error` „Error number 1“; Details/Updates → sofort `Finished(success)` ohne Daten | Install/Remove/Update → nach polkit sofort `Finished(success)`, ohne Fortschritt | Fehlerweg, polkit-Wege (fehlend → not-authorized, verweigert, erlaubt), Flaggen/Simulation |
| `test_succeed` | **defekt:** `pk_backend_job_finished: assertion 'PK_IS_BACKEND_JOB (job)' failed`, Transaktion endet nie | – | nur Nachweis, dass die Lesefrist greift |
| `test_spawn` | **defekt:** sucht `/usr/share/PackageKit/helpers/test/search-name.sh`, das Paket liefert `helpers/test_spawn/` → `internal-error` | – | nur Defekt-Nachweis; ohne root nicht behebbar |
| `test_nop` | alles NotSupported | – | ungenutzt |

Kein Test-Backend liefert eine Installation mit Fortschritt 0→100. Deshalb gibt es
**`qs-pk-attrappe paketkit`** (`pruefgeruest/pk-attrappe.rs`): dieselbe D-Bus-Schnittstelle
(Teilmenge, die `qs_paketkit` nutzt), PackageKit-Verhalten nachgebildet (Aufrufe nur von der
anlegenden Verbindung, `Destroy` nach `Finished`, `PropertiesChanged`), fester Bestand:

| Paket | Zweck |
|---|---|
| `querencia-bilder` 0.3.0 (installiert) / 0.4.0 | `paket_nach_name` bevorzugt installiert; Update „Fehlerbehebung“, Neustart der Anwendung |
| `querencia-editor` → braucht `libquerencia-gemein` | Simulation: zusätzliche Abhängigkeit, Download-Bytes |
| `libquerencia-gemein` | Entfernen-Simulation mit `allow_deps=false` → `dep-resolution-failed` |
| `powertop` (installiert), `gedit` | Entfernen, Details |
| `libssl3t64` (Sicherheit, Neustart System), `tzdata` | Aktualisierungen |
| `langsam` (5 s), `gesperrt` (cannot-get-lock), `kaputt` (dep-resolution-failed), `neustart` (RequireRestart) | Abbruch, Sperre, Fehler, Neustart |

Eingebaute Prüfregeln: schreibende Aufrufe ohne die drei Pflicht-Hinweise und `RemovePackages`
mit `allow_deps`/`autoremove = true` enden mit `internal-error`. Umgebungsvariable
`QS_ATTRAPPE_POLKIT=verweigern` → `not-authorized`. Die Attrappe weigert sich, auf
`/run/dbus/system_bus_socket` zu laufen.

## Tests

| Satz | Befehl | Inhalt |
|---|---|---|
| Unit | `cargo test --offline --workspace --lib` | Enum-/Fehler-/Status-Abbildung, Paket-ID, Agentenname, Kategorien |
| Prüfbus (23) | `cargo test --offline --test pruefbus` | 9 gegen echten `packagekitd` (test_thread/test_fail/test_succeed/test_spawn), 14 gegen die Attrappe |
| Echt, lesend (3) | `pruefgeruest/echt-lesend.sh` | `suchen("querencia")`/gedit/bash, `details(bash)`, `aktualisierungen`, `laufende_transaktionen` – `#[ignore]` |

Fehlen `dbus-daemon` oder `/usr/libexec/packagekitd`, melden die betroffenen Tests
`BLOCKED: …` auf stderr und kehren ohne Prüfung zurück.

## Nicht im Gerüst prüfbar

- Echte polkit-Dialoge (Agent `querencia-polkit-agent`) – nur mit echter Transaktion auf dem Systembus, verboten.
- Unterscheidung `KeinAgent`/`NichtBerechtigt` gegen echten polkitd: Heuristik, siehe `src/agent.rs`.
- apt-spezifische Fortschrittsfolgen (Download-Größen, `cannot-get-lock` durch echtes apt).
