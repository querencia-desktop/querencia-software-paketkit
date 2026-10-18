# qs_paketkit – PackageKit-Schicht für „Querencia Software“

Zwei Crates nach `~/projects/querencia-software/SCHNITTSTELLEN.md`:

| Crate | Ordner | Inhalt |
|---|---|---|
| `qs_typen` | `qs_typen/` | gemeinsame Typen und Traits aller SW-Teilprojekte (nur `async-trait`) |
| `qs_paketkit` | `.` | `PaketKit`: `qs_typen::Paketquelle` über zbus 5.19 gegen `org.freedesktop.PackageKit` |

Dazu das Prüfgerüst (`pruefgeruest/`, siehe [PRUEFGERUEST.md](PRUEFGERUEST.md)) und das
Beispiel `beispiele/pk-suche.rs`.

## Einbinden

```toml
[dependencies]
qs_typen = { path = "../querencia-software-paketkit/qs_typen" }
qs_paketkit = { path = "../querencia-software-paketkit" }   # nur wer PackageKit wirklich spricht
```

## Nutzung

```rust
use qs_typen::{Fortschritt, Paketquelle, Vorgang};

// Systembus – oder der private Prüfbus, wenn QS_PK_BUS gesetzt ist.
let pk = qs_paketkit::PaketKit::neu().await?;          // oder neu_blockierend()
let treffer = pk.suchen("gedit", false).await?;         // SearchNames, Filter arch+newest
let paket = pk.paket_nach_name("gedit").await?;         // Resolve, installierte Version zuerst
let details = pk.details(&paket.unwrap().id).await?;    // GetDetails
let updates = pk.aktualisierungen().await?;             // GetUpdates + GetUpdateDetail
let sim = pk.simulieren(Vorgang::Entfernen, ids.clone()).await?; // nur zusätzliche Pakete

// Schreibend: melden läuft auf dem Thread, der ausfuehren ruft (GTK-Hauptthread).
let griff = pk.ausfuehren(Vorgang::Installieren, ids, Box::new(|f| match f {
    Fortschritt::Prozent(p) if p <= 100 => { /* Balken */ }
    Fortschritt::Prozent(_) => { /* 101 = unbekannt: pulsieren */ }
    Fortschritt::Status(text) => { /* „Lade herunter …“ */ }
    Fortschritt::Fertig | Fortschritt::Abgebrochen | Fortschritt::Fehler(_) => { /* Ende */ }
    _ => {}
}));
griff.abbrechen(); // jederzeit
```

Ohne glib-Hauptschleife (Kommandozeile, SW-4 `qs-cli`): `ausfuehren_kanal()` liefert
`(FortschrittEmpfaenger, AbbruchGriff)`; `beobachten_kanal(pfad)` bindet sich an eine
laufende Transaktion aus `laufende_transaktionen()` an.

### Verhalten in Kürze

- Jede Transaktion setzt `SetHints(["locale=de_DE.UTF-8","interactive=true","background=false"])`.
- Entfernen immer mit `allow_deps=false`, `autoremove=false`; Installieren/Aktualisieren mit `ONLY_TRUSTED`.
- `Simulation` enthält nur, was **zusätzlich** zu den angefragten Paketen geschähe.
- Meldungsfolge von `ausfuehren`: `Gestartet`, dann `Status`/`Prozent`/`PaketFortschritt`/
  `WartetAufSperre`/`BrauchtNeustart`, zuletzt genau eins von `Fertig`, `Abgebrochen`, `Fehler(_)`.
- `not-authorized` wird zu `KeinAgent`, wenn kein polkit-Agentenprozess des Benutzers läuft,
  sonst zu `NichtBerechtigt` (Heuristik in `src/agent.rs`, abschaltbar mit `mit_agentpruefung`).
- Verschwindet `org.freedesktop.PackageKit` vom Bus (Absturz, Neustart) oder kommt `Destroy`
  vor `Finished`: `Fehler::DaemonWeg`.
- Lesende Transaktionen und Simulationen haben eine Frist (Standard 300 s, `mit_frist`), danach
  `Cancel` und `Fehler::Sonstiges { code: "zeitueberschreitung", .. }`. Schreibende nicht.

## Beispiel (nur lesend)

```bash
cargo run --offline --example pk-suche -- gedit
cargo run --offline --example pk-suche -- --details gedit
cargo run --offline --example pk-suche -- --aktualisierungen
```

## Prüfen

```bash
nice cargo test -j2 --offline --workspace              # Unit + 23 Prüfbus-Tests (privater Bus)
./pruefgeruest/echt-lesend.sh                          # 3 lesende Tests gegen den echten Daemon
nice cargo clippy -j2 --offline --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" nice cargo doc -j2 --offline --workspace --no-deps
```

**Niemals** schreibende Vorgänge gegen den Systembus prüfen. Schreibtests laufen nur gegen
`packagekitd --backend=test_*` oder `qs-pk-attrappe` auf dem privaten Prüfbus.
