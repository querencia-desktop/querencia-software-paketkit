//! `qs-pk-attrappe` – Attrappen für den privaten Prüfbus (nur Tests, nie auf dem Systembus).
//!
//! ```text
//! qs-pk-attrappe paketkit --adresse unix:path=/tmp/qs-pk-$USER/sock
//! qs-pk-attrappe polkit   --adresse … [--modus erlauben|verweigern|verzoegern:MS]
//! ```
//!
//! **paketkit**: spricht die Teilmenge von `org.freedesktop.PackageKit` und
//! `org.freedesktop.PackageKit.Transaction`, die `qs_paketkit` nutzt, mit einem festen
//! Paketbestand (siehe `BESTAND`). Sie existiert, weil die mitgelieferten Test-Backends
//! von PackageKit 1.3.4 Installation mit Fortschritt nicht abdecken (`test_succeed` bringt
//! den Daemon zum Absturz, `test_spawn` findet sein Hilfsskript nicht, `test_thread` kann
//! nur suchen) – siehe PRUEFGERUEST.md. Verhalten wie PackageKit: Methodenaufrufe auf einer
//! Transaktion nur von der anlegenden Verbindung; Signale `Package`, `Details`,
//! `UpdateDetail`, `ItemProgress`, `ErrorCode`, `RequireRestart`, `Finished`, `Destroy`;
//! `PropertiesChanged` für `Status`/`Percentage`/`AllowCancel`/`DownloadSizeRemaining`.
//! Zusätzliche Prüfregeln: Schreibende Aufrufe ohne die drei Pflicht-Hinweise aus
//! SCHNITTSTELLEN.md und `RemovePackages` mit `allow_deps=true` oder `autoremove=true`
//! enden mit `internal-error`.
//! Sonderpakete: `langsam` (5 s), `gesperrt` (cannot-get-lock), `kaputt`
//! (dep-resolution-failed), `neustart` (RequireRestart system).
//! `QS_ATTRAPPE_POLKIT=verweigern` → schreibende Vorgänge enden mit `not-authorized`.
//!
//! **polkit**: `org.freedesktop.PolicyKit1.Authority` für einen echten `packagekitd` auf
//! dem Prüfbus; erlaubt alles (oder verweigert bzw. verzögert).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::message::Header;
use zbus::object_server::ObjectServer;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{fdo, interface, Connection};

const DIENST: &str = "org.freedesktop.PackageKit";
const TX_IFACE: &str = "org.freedesktop.PackageKit.Transaction";

// PkInfoEnum / PkStatusEnum / PkExitEnum / PkErrorEnum / PkRoleEnum (Auszug).
const INFO_INSTALLIERT: u32 = 1;
const INFO_VERFUEGBAR: u32 = 2;
const INFO_NORMAL: u32 = 5;
const INFO_FEHLERBEHEBUNG: u32 = 6;
const INFO_SICHERHEIT: u32 = 8;
const INFO_AKTUALISIERT: u32 = 11;
const INFO_INSTALLIERT_WIRD: u32 = 12;
const INFO_ENTFERNT_WIRD: u32 = 13;
const ST_EINRICHTEN: u32 = 2;
const ST_ABFRAGE: u32 = 4;
const ST_ENTFERNEN: u32 = 6;
const ST_QUELLEN: u32 = 7;
const ST_LADEN: u32 = 8;
const ST_INSTALLIEREN: u32 = 9;
const ST_AKTUALISIEREN: u32 = 10;
const ST_ABH: u32 = 13;
const ST_FERTIG: u32 = 18;
const ST_ABBRECHEN: u32 = 19;
const ST_SPERRE: u32 = 30;
const ST_BERECHTIGUNG: u32 = 31;
const EXIT_ERFOLG: u32 = 1;
const EXIT_FEHLER: u32 = 2;
const EXIT_ABGEBROCHEN: u32 = 3;
const E_INTERN: u32 = 4;
const E_NICHT_GEFUNDEN: u32 = 8;
const E_ABH: u32 = 13;
const E_ABGEBROCHEN: u32 = 17;
const E_SPERRE: u32 = 26;
const E_NICHT_BERECHTIGT: u32 = 48;
const ROLLE_DETAILS: u32 = 3;
const ROLLE_UPDATE_DETAIL: u32 = 8;
const ROLLE_UPDATES: u32 = 9;
const ROLLE_INSTALL: u32 = 11;
const ROLLE_REFRESH: u32 = 13;
const ROLLE_REMOVE: u32 = 14;
const ROLLE_RESOLVE: u32 = 17;
const ROLLE_SEARCH_DETAILS: u32 = 18;
const ROLLE_SEARCH_NAME: u32 = 21;
const ROLLE_UPDATE: u32 = 22;
const FLAG_SIMULIEREN: u64 = 1 << 2;
const FILTER_INSTALLIERT: u64 = 1 << 2;
const FILTER_NICHT_INSTALLIERT: u64 = 1 << 3;

/// (Paket-ID, Zusammenfassung, installiert, Gruppe, Größe)
const BESTAND: &[(&str, &str, bool, u32, u64)] = &[
    (
        "querencia-bilder;0.3.0-1;amd64;installed",
        "Bildbetrachter für Querencia",
        true,
        12,
        812_000,
    ),
    (
        "querencia-bilder;0.4.0-1;amd64;attrappe",
        "Bildbetrachter für Querencia",
        false,
        12,
        845_000,
    ),
    (
        "querencia-editor;0.2.0-1;amd64;attrappe",
        "Texteditor für Querencia",
        false,
        2,
        1_200_000,
    ),
    (
        "libquerencia-gemein;0.1.0-1;amd64;attrappe",
        "Gemeinsame Bibliothek der Querencia-Programme",
        false,
        22,
        300_000,
    ),
    (
        "powertop;2.15-1;amd64;installed",
        "Diagnosewerkzeug für den Stromverbrauch",
        true,
        21,
        450_000,
    ),
    (
        "gedit;48.1-1;amd64;attrappe",
        "Texteditor der GNOME-Umgebung",
        false,
        5,
        1_374_208,
    ),
    (
        "libssl3t64;3.5.0-2;amd64;attrappe-security",
        "Secure Sockets Layer – Laufzeitbibliothek",
        false,
        27,
        2_100_000,
    ),
    (
        "tzdata;2026e-1;all;attrappe-updates",
        "Zeitzonen-Daten",
        false,
        27,
        350_000,
    ),
    (
        "langsam;1.0-1;amd64;attrappe",
        "Prüfpaket: Installation dauert 5 s",
        false,
        20,
        10_000,
    ),
    (
        "gesperrt;1.0-1;amd64;attrappe",
        "Prüfpaket: Paketsperre belegt",
        false,
        20,
        10_000,
    ),
    (
        "kaputt;1.0-1;amd64;attrappe",
        "Prüfpaket: Abhängigkeiten unlösbar",
        false,
        20,
        10_000,
    ),
    (
        "neustart;1.0-1;amd64;attrappe",
        "Prüfpaket: verlangt Neustart",
        false,
        20,
        10_000,
    ),
];

/// Aktualisierungen: (Paket-ID, info, restart)
const AKTUALISIERUNGEN: &[(&str, u32, u32)] = &[
    (
        "libssl3t64;3.5.0-2;amd64;attrappe-security",
        INFO_SICHERHEIT,
        4,
    ),
    ("tzdata;2026e-1;all;attrappe-updates", INFO_NORMAL, 1),
    (
        "querencia-bilder;0.4.0-1;amd64;attrappe",
        INFO_FEHLERBEHEBUNG,
        2,
    ),
];

fn name_von(id: &str) -> &str {
    id.split(';').next().unwrap_or_default()
}

fn eintrag(id: &str) -> Option<&'static (&'static str, &'static str, bool, u32, u64)> {
    BESTAND.iter().find(|e| e.0 == id)
}

fn info_von(e: &(&str, &str, bool, u32, u64)) -> u32 {
    if e.2 {
        INFO_INSTALLIERT
    } else {
        INFO_VERFUEGBAR
    }
}

// ---------------------------------------------------------------- Transaktion

#[derive(Default)]
struct TxZustand {
    rolle: u32,
    status: u32,
    prozent: u32,
    abbruch_erlaubt: bool,
    download_rest: u64,
    hinweise: Vec<String>,
    laeuft: bool,
    beendet: bool,
}

type Geteilt = Arc<Mutex<TxZustand>>;

struct Transaktion {
    pfad: String,
    absender: String,
    zustand: Geteilt,
    abbruch: Arc<AtomicBool>,
    liste: Arc<Mutex<Vec<String>>>,
}

/// Sendet Signale und Eigenschaftsänderungen aus einem Arbeits-Thread.
#[derive(Clone)]
struct Sender {
    conn: Connection,
    pfad: String,
    zustand: Geteilt,
    abbruch: Arc<AtomicBool>,
    liste: Arc<Mutex<Vec<String>>>,
}

impl Sender {
    fn signal<B>(&self, name: &str, body: &B)
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let _ = zbus::block_on(self.conn.emit_signal(
            None::<zbus::names::BusName<'_>>,
            self.pfad.as_str(),
            TX_IFACE,
            name,
            body,
        ));
    }

    fn eigenschaften(&self, werte: Vec<(&str, Value<'_>)>) {
        let map: HashMap<&str, Value<'_>> = werte.into_iter().collect();
        let leer: Vec<&str> = Vec::new();
        let _ = zbus::block_on(self.conn.emit_signal(
            None::<zbus::names::BusName<'_>>,
            self.pfad.as_str(),
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(TX_IFACE, map, leer),
        ));
    }

    fn status(&self, s: u32) {
        self.zustand.lock().unwrap().status = s;
        self.eigenschaften(vec![("Status", Value::U32(s))]);
    }

    fn prozent(&self, p: u32) {
        self.zustand.lock().unwrap().prozent = p;
        self.eigenschaften(vec![("Percentage", Value::U32(p))]);
    }

    fn paket(&self, info: u32, id: &str, zusammenfassung: &str) {
        self.signal("Package", &(info, id, zusammenfassung));
    }

    fn fehler(&self, code: u32, text: &str) {
        self.signal("ErrorCode", &(code, text));
    }

    fn fertig(&self, exit: u32, start: Instant) {
        {
            let mut z = self.zustand.lock().unwrap();
            z.laeuft = false;
            z.beendet = true;
            z.status = ST_FERTIG;
        }
        self.liste.lock().unwrap().retain(|p| p != &self.pfad);
        self.eigenschaften(vec![("Status", Value::U32(ST_FERTIG))]);
        self.signal("Finished", &(exit, start.elapsed().as_millis() as u32));
        // Wie PackageKit: das Objekt verschwindet kurz danach.
        let s = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            s.signal("Destroy", &());
            let _ = zbus::block_on(
                s.conn
                    .object_server()
                    .remove::<Transaktion, _>(s.pfad.as_str()),
            );
        });
    }

    fn abgebrochen(&self) -> bool {
        self.abbruch.load(Ordering::SeqCst)
    }
}

impl Transaktion {
    fn pruefen(&self, hdr: &Header<'_>) -> fdo::Result<()> {
        let absender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        if absender != self.absender {
            return Err(fdo::Error::AccessDenied(format!(
                "sender does not match ({absender} vs {})",
                self.absender
            )));
        }
        let mut z = self.zustand.lock().unwrap();
        if z.laeuft || z.beendet {
            return Err(fdo::Error::Failed("transaction already committed".into()));
        }
        z.laeuft = true;
        Ok(())
    }

    fn sender(&self, conn: &Connection) -> Sender {
        Sender {
            conn: conn.clone(),
            pfad: self.pfad.clone(),
            zustand: self.zustand.clone(),
            abbruch: self.abbruch.clone(),
            liste: self.liste.clone(),
        }
    }

    /// Startet `arbeit` im Hintergrund, nachdem die Methode geantwortet hat.
    fn starten<F>(&self, conn: &Connection, rolle: u32, arbeit: F)
    where
        F: FnOnce(&Sender, Instant) + Send + 'static,
    {
        self.zustand.lock().unwrap().rolle = rolle;
        self.liste.lock().unwrap().push(self.pfad.clone());
        let s = self.sender(conn);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            let start = Instant::now();
            s.eigenschaften(vec![("Role", Value::U32(rolle))]);
            s.status(ST_EINRICHTEN);
            arbeit(&s, start);
        });
    }

    fn hinweise_ok(&self) -> bool {
        let h = &self.zustand.lock().unwrap().hinweise;
        ["locale=de_DE.UTF-8", "interactive=true", "background=false"]
            .iter()
            .all(|p| h.iter().any(|x| x == p))
    }
}

/// Gemeinsamer Ablauf von Installieren/Aktualisieren/Entfernen/Auffrischen.
fn schreibvorgang(
    s: &Sender,
    start: Instant,
    ids: Vec<String>,
    haupt_status: u32,
    hinweise_ok: bool,
) {
    if !hinweise_ok {
        s.fehler(
            E_INTERN,
            "attrappe: SetHints mit locale/interactive/background fehlt",
        );
        return s.fertig(EXIT_FEHLER, start);
    }
    s.status(ST_BERECHTIGUNG);
    std::thread::sleep(Duration::from_millis(30));
    if std::env::var("QS_ATTRAPPE_POLKIT").as_deref() == Ok("verweigern") {
        s.fehler(E_NICHT_BERECHTIGT, "attrappe: polkit hat abgelehnt");
        return s.fertig(EXIT_FEHLER, start);
    }
    for id in &ids {
        if haupt_status != ST_QUELLEN && eintrag(id).is_none() {
            s.fehler(E_NICHT_GEFUNDEN, &format!("{id} nicht gefunden"));
            return s.fertig(EXIT_FEHLER, start);
        }
    }
    let namen: Vec<&str> = ids.iter().map(|i| name_von(i)).collect();
    if namen.contains(&"gesperrt") {
        s.status(ST_SPERRE);
        std::thread::sleep(Duration::from_millis(100));
        s.fehler(
            E_SPERRE,
            "Die Paketverwaltung wird von apt (PID 4711) benutzt",
        );
        return s.fertig(EXIT_FEHLER, start);
    }
    if namen.contains(&"kaputt") {
        s.status(ST_ABH);
        s.fehler(E_ABH, "kaputt hängt von nicht-vorhanden (>= 2) ab");
        return s.fertig(EXIT_FEHLER, start);
    }
    let langsam = namen.contains(&"langsam");
    let schritt = Duration::from_millis(if langsam { 250 } else { 15 });
    {
        let mut z = s.zustand.lock().unwrap();
        z.abbruch_erlaubt = true;
    }
    s.eigenschaften(vec![("AllowCancel", Value::Bool(true))]);
    s.status(ST_ABH);
    s.prozent(0);
    let zwei_phasen = haupt_status != ST_ENTFERNEN && haupt_status != ST_QUELLEN;
    for p in (0..=100u32).step_by(5) {
        if s.abgebrochen() {
            s.status(ST_ABBRECHEN);
            s.fehler(E_ABGEBROCHEN, "Die Transaktion wurde abgebrochen");
            return s.fertig(EXIT_ABGEBROCHEN, start);
        }
        let phase = if zwei_phasen && p < 50 {
            ST_LADEN
        } else {
            haupt_status
        };
        if p == 0 || (zwei_phasen && p == 50) {
            s.status(phase);
        }
        s.prozent(p);
        let rest = if phase == ST_LADEN {
            (50 - p.min(50)) as u64 * 10_000
        } else {
            0
        };
        s.eigenschaften(vec![("DownloadSizeRemaining", Value::U64(rest))]);
        for id in &ids {
            let item = if zwei_phasen { (p % 50) * 2 } else { p };
            s.signal("ItemProgress", &(id.as_str(), phase, item));
        }
        std::thread::sleep(schritt);
    }
    for id in &ids {
        let info = match haupt_status {
            ST_ENTFERNEN => INFO_ENTFERNT_WIRD,
            ST_AKTUALISIEREN => INFO_AKTUALISIERT,
            _ => INFO_INSTALLIERT_WIRD,
        };
        let z = eintrag(id).map(|e| e.1).unwrap_or_default();
        s.paket(info, id, z);
    }
    if namen.contains(&"neustart") || namen.contains(&"libssl3t64") {
        let id = ids.first().cloned().unwrap_or_default();
        s.signal("RequireRestart", &(4u32, id.as_str()));
    }
    s.fertig(EXIT_ERFOLG, start)
}

/// Simulation: meldet, was geschehen würde.
fn simulation(s: &Sender, start: Instant, ids: Vec<String>, rolle: u32) {
    s.status(ST_ABH);
    for id in &ids {
        if eintrag(id).is_none() {
            s.fehler(E_NICHT_GEFUNDEN, &format!("{id} nicht gefunden"));
            return s.fertig(EXIT_FEHLER, start);
        }
    }
    let mut bytes = 0u64;
    for id in &ids {
        let e = eintrag(id).expect("geprüft");
        match rolle {
            ROLLE_INSTALL => {
                s.paket(INFO_INSTALLIERT_WIRD, id, e.1);
                bytes += e.4;
                if name_von(id) == "querencia-editor" {
                    let dep = BESTAND[3];
                    s.paket(INFO_INSTALLIERT_WIRD, dep.0, dep.1);
                    bytes += dep.4;
                }
            }
            ROLLE_REMOVE => {
                if name_von(id) == "libquerencia-gemein" {
                    s.fehler(
                        E_ABH,
                        "querencia-editor hängt von libquerencia-gemein ab (allow_deps=false)",
                    );
                    return s.fertig(EXIT_FEHLER, start);
                }
                s.paket(INFO_ENTFERNT_WIRD, id, e.1);
            }
            _ => {
                s.paket(INFO_AKTUALISIERT, id, e.1);
                bytes += e.4;
                if name_von(id) == "libssl3t64" {
                    let dep = BESTAND[3];
                    s.paket(INFO_INSTALLIERT_WIRD, dep.0, dep.1);
                    bytes += dep.4;
                }
            }
        }
    }
    s.zustand.lock().unwrap().download_rest = bytes;
    s.eigenschaften(vec![("DownloadSizeRemaining", Value::U64(bytes))]);
    s.fertig(EXIT_ERFOLG, start)
}

fn suche(s: &Sender, start: Instant, filter: u64, werte: Vec<String>, details: bool) {
    s.status(ST_ABFRAGE);
    for e in BESTAND {
        if filter & FILTER_INSTALLIERT != 0 && !e.2 {
            continue;
        }
        if filter & FILTER_NICHT_INSTALLIERT != 0 && e.2 {
            continue;
        }
        let name = name_von(e.0);
        let treffer = werte.iter().any(|w| {
            let w = w.to_lowercase();
            name.contains(&w) || (details && e.1.to_lowercase().contains(&w))
        });
        if treffer {
            s.paket(info_von(e), e.0, e.1);
        }
    }
    s.fertig(EXIT_ERFOLG, start)
}

#[interface(name = "org.freedesktop.PackageKit.Transaction")]
impl Transaktion {
    fn set_hints(&self, hints: Vec<String>, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let absender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        if absender != self.absender {
            return Err(fdo::Error::AccessDenied("sender does not match".into()));
        }
        self.zustand.lock().unwrap().hinweise = hints;
        Ok(())
    }

    fn search_names(
        &self,
        filter: u64,
        values: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_SEARCH_NAME, move |s, t| {
            suche(s, t, filter, values, false)
        });
        Ok(())
    }

    fn search_details(
        &self,
        filter: u64,
        values: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_SEARCH_DETAILS, move |s, t| {
            suche(s, t, filter, values, true)
        });
        Ok(())
    }

    fn resolve(
        &self,
        filter: u64,
        packages: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_RESOLVE, move |s, t| {
            s.status(ST_ABFRAGE);
            for e in BESTAND {
                if filter & FILTER_INSTALLIERT != 0 && !e.2 {
                    continue;
                }
                if packages.iter().any(|p| p == name_von(e.0)) {
                    s.paket(info_von(e), e.0, e.1);
                }
            }
            s.fertig(EXIT_ERFOLG, t)
        });
        Ok(())
    }

    fn get_details(
        &self,
        package_ids: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_DETAILS, move |s, t| {
            s.status(ST_ABFRAGE);
            for id in &package_ids {
                let Some(e) = eintrag(id) else {
                    s.fehler(E_NICHT_GEFUNDEN, &format!("{id} nicht gefunden"));
                    return s.fertig(EXIT_FEHLER, t);
                };
                let mut m: HashMap<&str, Value<'_>> = HashMap::new();
                m.insert("package-id", Value::from(e.0));
                m.insert("summary", Value::from(e.1));
                m.insert(
                    "description",
                    Value::from(format!(
                        "{}.\n\nPaket aus der PackageKit-Attrappe von qs_paketkit.",
                        e.1
                    )),
                );
                m.insert("license", Value::from("GPL-3.0-or-later"));
                m.insert("group", Value::U32(e.3));
                m.insert("size", Value::U64(e.4));
                m.insert(
                    "url",
                    Value::from(format!("https://querencia.example/{}", name_von(e.0))),
                );
                s.signal("Details", &m);
            }
            s.fertig(EXIT_ERFOLG, t)
        });
        Ok(())
    }

    fn get_updates(
        &self,
        _filter: u64,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_UPDATES, move |s, t| {
            s.status(ST_ABFRAGE);
            for (id, info, _) in AKTUALISIERUNGEN {
                s.paket(*info, id, eintrag(id).map(|e| e.1).unwrap_or_default());
            }
            s.fertig(EXIT_ERFOLG, t)
        });
        Ok(())
    }

    fn get_update_detail(
        &self,
        package_ids: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        self.starten(conn, ROLLE_UPDATE_DETAIL, move |s, t| {
            s.status(ST_ABFRAGE);
            for id in &package_ids {
                let restart = AKTUALISIERUNGEN
                    .iter()
                    .find(|a| a.0 == id)
                    .map(|a| a.2)
                    .unwrap_or(1);
                let leer: Vec<&str> = Vec::new();
                s.signal(
                    "UpdateDetail",
                    &(
                        id.as_str(),
                        leer.clone(),
                        leer.clone(),
                        leer.clone(),
                        leer.clone(),
                        leer.clone(),
                        restart,
                        "Aktualisierung aus der Attrappe",
                        "",
                        1u32,
                        "2026-10-18T00:00:00Z",
                        "",
                    ),
                );
            }
            s.fertig(EXIT_ERFOLG, t)
        });
        Ok(())
    }

    fn install_packages(
        &self,
        transaction_flags: u64,
        package_ids: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        let ok = self.hinweise_ok();
        self.starten(conn, ROLLE_INSTALL, move |s, t| {
            if transaction_flags & FLAG_SIMULIEREN != 0 {
                simulation(s, t, package_ids, ROLLE_INSTALL)
            } else {
                schreibvorgang(s, t, package_ids, ST_INSTALLIEREN, ok)
            }
        });
        Ok(())
    }

    fn update_packages(
        &self,
        transaction_flags: u64,
        package_ids: Vec<String>,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        let ok = self.hinweise_ok();
        self.starten(conn, ROLLE_UPDATE, move |s, t| {
            if transaction_flags & FLAG_SIMULIEREN != 0 {
                simulation(s, t, package_ids, ROLLE_UPDATE)
            } else {
                schreibvorgang(s, t, package_ids, ST_AKTUALISIEREN, ok)
            }
        });
        Ok(())
    }

    fn remove_packages(
        &self,
        transaction_flags: u64,
        package_ids: Vec<String>,
        allow_deps: bool,
        autoremove: bool,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        let ok = self.hinweise_ok();
        self.starten(conn, ROLLE_REMOVE, move |s, t| {
            if allow_deps || autoremove {
                s.fehler(
                    E_INTERN,
                    "attrappe: allow_deps und autoremove müssen false sein",
                );
                return s.fertig(EXIT_FEHLER, t);
            }
            if transaction_flags & FLAG_SIMULIEREN != 0 {
                simulation(s, t, package_ids, ROLLE_REMOVE)
            } else {
                schreibvorgang(s, t, package_ids, ST_ENTFERNEN, ok)
            }
        });
        Ok(())
    }

    fn refresh_cache(
        &self,
        _force: bool,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> fdo::Result<()> {
        self.pruefen(&hdr)?;
        let ok = self.hinweise_ok();
        self.starten(conn, ROLLE_REFRESH, move |s, t| {
            schreibvorgang(s, t, Vec::new(), ST_QUELLEN, ok)
        });
        Ok(())
    }

    fn cancel(&self, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        let absender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        if absender != self.absender {
            return Err(fdo::Error::AccessDenied("sender does not match".into()));
        }
        let z = self.zustand.lock().unwrap();
        if !z.laeuft {
            return Err(fdo::Error::Failed("transaction not running".into()));
        }
        self.abbruch.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[zbus(property)]
    fn role(&self) -> u32 {
        self.zustand.lock().unwrap().rolle
    }
    #[zbus(property)]
    fn status(&self) -> u32 {
        self.zustand.lock().unwrap().status
    }
    #[zbus(property)]
    fn percentage(&self) -> u32 {
        self.zustand.lock().unwrap().prozent
    }
    #[zbus(property)]
    fn allow_cancel(&self) -> bool {
        self.zustand.lock().unwrap().abbruch_erlaubt
    }
    #[zbus(property)]
    fn download_size_remaining(&self) -> u64 {
        self.zustand.lock().unwrap().download_rest
    }
}

// ---------------------------------------------------------------- Daemon

struct Daemon {
    zaehler: AtomicU32,
    liste: Arc<Mutex<Vec<String>>>,
}

#[interface(name = "org.freedesktop.PackageKit")]
impl Daemon {
    async fn create_transaction(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        let n = self.zaehler.fetch_add(1, Ordering::SeqCst) + 1;
        let pfad = format!("/{n}_attrappe");
        let tx = Transaktion {
            pfad: pfad.clone(),
            absender: hdr.sender().map(|s| s.to_string()).unwrap_or_default(),
            zustand: Arc::default(),
            abbruch: Arc::default(),
            liste: self.liste.clone(),
        };
        server.at(pfad.as_str(), tx).await?;
        OwnedObjectPath::try_from(pfad).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    fn get_transaction_list(&self) -> Vec<OwnedObjectPath> {
        self.liste
            .lock()
            .unwrap()
            .iter()
            .filter_map(|p| OwnedObjectPath::try_from(p.as_str()).ok())
            .collect()
    }

    fn can_authorize(&self, _action_id: String) -> u32 {
        1
    }

    #[zbus(property)]
    fn backend_name(&self) -> String {
        "attrappe".into()
    }
    #[zbus(property)]
    fn distro_id(&self) -> String {
        "querencia;0.1;x86_64".into()
    }
    #[zbus(property)]
    fn roles(&self) -> u64 {
        (1u64 << ROLLE_DETAILS)
            | (1 << ROLLE_UPDATES)
            | (1 << ROLLE_INSTALL)
            | (1 << ROLLE_REMOVE)
            | (1 << ROLLE_SEARCH_NAME)
            | (1 << ROLLE_UPDATE)
            | (1 << ROLLE_RESOLVE)
            | (1 << ROLLE_REFRESH)
    }
}

// ---------------------------------------------------------------- polkit

struct Polkit {
    modus: String,
}

type Subjekt = (String, HashMap<String, OwnedValue>);

/// Rückgabe von `CheckAuthorization`: **ein** Wert vom Typ `(bba{ss})`. zbus löst eine
/// zurückgegebene Struktur (oder ein Tupel) in einzelne Werte auf; erst `(Struktur,)` ergibt
/// den einen Strukturwert, den polkit liefert. Sonst stürzt packagekitd ab.
#[derive(serde::Serialize, zbus::zvariant::Type)]
struct PolkitErgebnis {
    autorisiert: bool,
    rueckfrage: bool,
    details: HashMap<String, String>,
}

#[interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl Polkit {
    async fn check_authorization(
        &self,
        _subject: Subjekt,
        action_id: String,
        _details: HashMap<String, String>,
        _flags: u32,
        _cancellation_id: String,
    ) -> (PolkitErgebnis,) {
        if let Some(ms) = self.modus.strip_prefix("verzoegern:") {
            let ms: u64 = ms.parse().unwrap_or(3000);
            // Nicht den Executor blockieren: eigener Thread + Kanal.
            let (tx, rx) = async_channel::bounded::<()>(1);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(ms));
                let _ = tx.send_blocking(());
            });
            let _ = rx.recv().await;
        }
        let erlaubt = self.modus != "verweigern";
        eprintln!(
            "polkit-attrappe: {action_id} -> {}",
            if erlaubt { "erlaubt" } else { "verweigert" }
        );
        (PolkitErgebnis {
            autorisiert: erlaubt,
            rueckfrage: !erlaubt,
            details: HashMap::new(),
        },)
    }

    fn cancel_check_authorization(&self, _cancellation_id: String) {}

    fn enumerate_temporary_authorizations(
        &self,
        _subject: Subjekt,
    ) -> Vec<(String, String, Subjekt, u64, u64)> {
        Vec::new()
    }

    #[zbus(property)]
    fn backend_name(&self) -> String {
        "qs-attrappe".into()
    }
    #[zbus(property)]
    fn backend_version(&self) -> String {
        "0".into()
    }
    #[zbus(property)]
    fn backend_features(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------- main

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let art = args.first().cloned().unwrap_or_default();
    let wert = |schalter: &str| {
        args.iter()
            .position(|a| a == schalter)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let adresse = wert("--adresse")
        .or_else(|| std::env::var("QS_PK_BUS").ok())
        .unwrap_or_default();
    if adresse.is_empty() || !(art == "paketkit" || art == "polkit") {
        eprintln!("Nutzung: qs-pk-attrappe paketkit|polkit --adresse unix:path=… [--modus erlauben|verweigern|verzoegern:MS]");
        std::process::exit(2);
    }
    // Schutz: nie auf dem echten Systembus antreten.
    if adresse.contains("/run/dbus/system_bus_socket") || adresse.contains("/var/run/dbus/") {
        eprintln!("qs-pk-attrappe: weigere mich, auf dem Systembus zu laufen");
        std::process::exit(3);
    }
    let ergebnis: zbus::Result<Connection> = zbus::block_on(async {
        let b = zbus::connection::Builder::address(adresse.as_str())?;
        if art == "paketkit" {
            b.name(DIENST)?
                .serve_at(
                    "/org/freedesktop/PackageKit",
                    Daemon {
                        zaehler: AtomicU32::new(0),
                        liste: Arc::default(),
                    },
                )?
                .build()
                .await
        } else {
            let modus = wert("--modus").unwrap_or_else(|| "erlauben".into());
            b.name("org.freedesktop.PolicyKit1")?
                .serve_at("/org/freedesktop/PolicyKit1/Authority", Polkit { modus })?
                .build()
                .await
        }
    });
    match ergebnis {
        Ok(_conn) => {
            println!("qs-pk-attrappe {art} bereit auf {adresse}");
            loop {
                std::thread::park();
            }
        }
        Err(e) => {
            eprintln!("qs-pk-attrappe: {e}");
            std::process::exit(1);
        }
    }
}
