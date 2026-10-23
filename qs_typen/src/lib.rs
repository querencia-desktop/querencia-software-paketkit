//! Gemeinsame Typen für „Querencia Software“.
//!
//! Dieses Crate enthält genau die in `schnittstellen.md` festgelegten Typen und
//! Traits, die alle Teilprojekte (`qs_paketkit`, `qs_katalog`, `qs_transaktion`,
//! `querencia-software`) gemeinsam nutzen. Es hat außer `async-trait` keine
//! Abhängigkeiten und bleibt ab dem ersten Commit stabil; Erweiterungen sind nur
//! additiv und werden vorab abgesprochen.
//!
//! Bezeichner sind deutsch. PackageKit-Begriffe (Paket-ID, Status, Fehlercode)
//! sind dort, wo sie durchscheinen, in den Dokumentationskommentaren erklärt.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

/// Paketkennung im PackageKit-Format `"name;version;arch;data"`.
pub type PaketId = String;

/// Ein Paket aus Sicht der Paketverwaltung.
#[derive(Clone, Debug, PartialEq)]
pub struct Paket {
    /// PackageKit-Paket-ID (`name;version;arch;data`).
    pub id: PaketId,
    /// Paketname, z. B. `"querencia-bilder"`.
    pub name: String,
    /// Versionszeichenkette.
    pub version: String,
    /// Architektur, z. B. `"amd64"` oder `"all"`.
    pub arch: String,
    /// Einzeilige Zusammenfassung.
    pub zusammenfassung: String,
    /// `true`, wenn das Paket installiert ist.
    pub installiert: bool,
}

/// Ausführliche Angaben zu einem Paket (`GetDetails`).
#[derive(Clone, Debug, PartialEq)]
pub struct Paketdetails {
    /// Das Paket selbst.
    pub paket: Paket,
    /// Mehrzeilige Beschreibung.
    pub beschreibung: String,
    /// Lizenztext bzw. Lizenzkennung.
    pub lizenz: String,
    /// Download- oder Installationsgröße in Bytes, je nach Backend.
    pub groesse_bytes: u64,
    /// Homepage, falls bekannt.
    pub url: Option<String>,
    /// PackageKit-Gruppe als Text (z. B. `"graphics"`).
    pub gruppe: String,
}

/// Dringlichkeit einer Aktualisierung.
#[derive(Clone, Debug, PartialEq)]
pub enum Dringlichkeit {
    /// Normale Aktualisierung.
    Normal,
    /// Sicherheitsaktualisierung.
    Sicherheit,
    /// Fehlerbehebung.
    Fehlerbehebung,
    /// Erweiterung / neue Funktionen.
    Erweiterung,
}

/// Eine verfügbare Aktualisierung.
#[derive(Clone, Debug, PartialEq)]
pub struct Aktualisierung {
    /// Das neue Paket (verfügbare Version).
    pub paket: Paket,
    /// Dringlichkeit laut PackageKit (`info` bzw. `UpdateDetail`).
    pub dringlichkeit: Dringlichkeit,
    /// `true`, wenn PackageKit einen Neustart (Sitzung oder System) ankündigt.
    pub neustart_noetig: bool,
}

/// Art einer schreibenden Transaktion.
#[derive(Clone, Debug, PartialEq)]
pub enum Vorgang {
    /// `InstallPackages`.
    Installieren,
    /// `RemovePackages` (immer `allow_deps=false`, `autoremove=false`).
    Entfernen,
    /// `UpdatePackages`.
    Aktualisieren,
    /// `RefreshCache`.
    QuellenAuffrischen,
}

/// Fortschrittsmeldung einer laufenden Transaktion.
#[derive(Clone, Debug, PartialEq)]
pub enum Fortschritt {
    /// Transaktion wurde angelegt und abgeschickt.
    Gestartet {
        /// Art des Vorgangs.
        vorgang: Vorgang,
        /// Betroffene Paket-IDs.
        pakete: Vec<PaketId>,
    },
    /// Gesamtfortschritt 0..=100; 101 = unbekannt (PackageKit-Konvention) → als `None` behandeln.
    Prozent(u8),
    /// PackageKit-Status als deutscher Text (z. B. „Lade herunter …“).
    Status(String),
    /// Fortschritt eines einzelnen Pakets (`ItemProgress`).
    PaketFortschritt {
        /// Paket-ID.
        id: PaketId,
        /// Prozent 0..=100.
        prozent: u8,
    },
    /// `cannot-get-lock` bzw. Status `waiting-for-lock` → Oberfläche bietet Warten/Abbrechen.
    WartetAufSperre,
    /// `RequireRestart` wurde gemeldet.
    BrauchtNeustart,
    /// Transaktion erfolgreich beendet.
    Fertig,
    /// Transaktion wurde abgebrochen.
    Abgebrochen,
    /// Transaktion mit Fehler beendet.
    Fehler(Fehler),
}

/// Fehler der Paketverwaltung, bereits auf wenige Fälle abgebildet.
#[derive(Clone, Debug, PartialEq)]
pub enum Fehler {
    /// polkit hat abgelehnt (oder kein Agent, wenn nicht genauer erkennbar).
    NichtBerechtigt,
    /// polkit-Agent fehlt in der Sitzung (G2-I 1.4): eigene Meldung mit Hinweis.
    KeinAgent,
    /// `cannot-get-lock`: ein anderes Programm hält die Paketsperre; Text = Details.
    SperreBelegt(String),
    /// `no-network`.
    NetzFehlt,
    /// `package-not-found` u. ä.; Text = Details.
    PaketNichtGefunden(String),
    /// Abhängigkeits- oder Dateikonflikt; Text = Details.
    AbhaengigkeitenKonflikt(String),
    /// PackageKit wurde während der Transaktion beendet oder neu gestartet.
    DaemonWeg,
    /// Durch den Benutzer abgebrochen.
    Abgebrochen,
    /// Alles andere: PackageKit-Fehlercode als Text plus Details.
    Sonstiges {
        /// PackageKit-ErrorCode, z. B. `"internal-error"`.
        code: String,
        /// Detailtext von PackageKit.
        text: String,
    },
}

impl std::fmt::Display for Fehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fehler::NichtBerechtigt => write!(f, "Keine Berechtigung für diesen Vorgang."),
            Fehler::KeinAgent => write!(
                f,
                "Es fehlt der Berechtigungsdialog der Sitzung (polkit-Agent)."
            ),
            Fehler::SperreBelegt(t) => write!(f, "Die Paketverwaltung ist belegt: {t}"),
            Fehler::NetzFehlt => write!(f, "Keine Netzwerkverbindung."),
            Fehler::PaketNichtGefunden(t) => write!(f, "Paket nicht gefunden: {t}"),
            Fehler::AbhaengigkeitenKonflikt(t) => write!(f, "Abhängigkeitskonflikt: {t}"),
            Fehler::DaemonWeg => write!(f, "Die Paketverwaltung (PackageKit) wurde beendet."),
            Fehler::Abgebrochen => write!(f, "Abgebrochen."),
            Fehler::Sonstiges { code, text } => write!(f, "{code}: {text}"),
        }
    }
}

impl std::error::Error for Fehler {}

/// Eine Anwendung aus dem AppStream-Katalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Anwendung {
    /// Component-ID, z. B. `"org.querencia.Bilder"`.
    pub id: String,
    /// Lokalisierter Name (de, sonst C).
    pub name: String,
    /// Lokalisierte Zusammenfassung.
    pub zusammenfassung: String,
    /// Beschreibung als einfacher Text oder sehr kleines Markup (p, ul/li) – Entscheidung SW-2.
    pub beschreibung: String,
    /// `<pkgname>`; `None` = ohne Paket (z. B. Snap-Komponenten, werden ausgeblendet).
    pub paketname: Option<String>,
    /// Zugeordnete Querencia-Kategorien.
    pub kategorien: Vec<Kategorie>,
    /// Symbol.
    pub symbol: Symbol,
    /// Bildschirmfoto-URLs (v1 zeigt sie nicht an, sammelt sie aber).
    pub bildschirmfotos: Vec<String>,
    /// Projektlizenz.
    pub lizenz: Option<String>,
    /// Homepage.
    pub homepage: Option<String>,
}

/// Symbol einer Anwendung.
#[derive(Clone, Debug, PartialEq)]
pub enum Symbol {
    /// Name im Symbolthema (`stock`).
    Thema(String),
    /// Datei im Cache (`cached`/`local`).
    Datei(PathBuf),
    /// Kein Symbol → Platzhalter.
    Keines,
}

/// Feste Kategorienliste nach GNOME 2 (Reihenfolge = Anzeige in der Seitenleiste).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kategorie {
    /// Zubehör.
    Zubehoer,
    /// Büro.
    Buero,
    /// Grafik.
    Grafik,
    /// Internet.
    Internet,
    /// Unterhaltungsmedien.
    Multimedia,
    /// Spiele.
    Spiele,
    /// Entwicklung.
    Entwicklung,
    /// Bildung.
    Bildung,
    /// Systemwerkzeuge.
    System,
    /// Sonstiges.
    Sonstiges,
}

impl Kategorie {
    /// Anzeigename (deutsch, GNOME-2-Menü).
    pub fn name(self) -> &'static str {
        match self {
            Kategorie::Zubehoer => "Zubehör",
            Kategorie::Buero => "Büro",
            Kategorie::Grafik => "Grafik",
            Kategorie::Internet => "Internet",
            Kategorie::Multimedia => "Unterhaltungsmedien",
            Kategorie::Spiele => "Spiele",
            Kategorie::Entwicklung => "Entwicklung",
            Kategorie::Bildung => "Bildung",
            Kategorie::System => "Systemwerkzeuge",
            Kategorie::Sonstiges => "Sonstiges",
        }
    }

    /// Alle Kategorien in Anzeigereihenfolge.
    pub fn alle() -> &'static [Kategorie] {
        &[
            Kategorie::Zubehoer,
            Kategorie::Buero,
            Kategorie::Grafik,
            Kategorie::Internet,
            Kategorie::Multimedia,
            Kategorie::Spiele,
            Kategorie::Entwicklung,
            Kategorie::Bildung,
            Kategorie::System,
            Kategorie::Sonstiges,
        ]
    }
}

/// Ergebnis einer Simulation (`TRANSACTION_FLAG_SIMULATE`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Simulation {
    /// Würde zusätzlich installiert.
    pub installieren: Vec<Paket>,
    /// Würde entfernt.
    pub entfernen: Vec<Paket>,
    /// Würde aktualisiert.
    pub aktualisieren: Vec<Paket>,
    /// Zu ladende Bytes, soweit das Backend es meldet.
    pub download_bytes: u64,
}

/// Abbruchgriff einer laufenden Transaktion.
pub trait Abbruch {
    /// Bricht die Transaktion ab (`Cancel`); darf jederzeit und mehrfach gerufen werden.
    fn abbrechen(&self);
}

/// Zugang zur Paketverwaltung (liefert SW-1, konsumiert SW-3 und SW-4).
#[async_trait::async_trait(?Send)]
pub trait Paketquelle {
    /// Sucht Pakete nach Name (und Details); `nur_installiert` setzt den Filter `installed`.
    async fn suchen(&self, text: &str, nur_installiert: bool) -> Result<Vec<Paket>, Fehler>;
    /// Löst einen Paketnamen auf; die installierte Version wird bevorzugt.
    async fn paket_nach_name(&self, name: &str) -> Result<Option<Paket>, Fehler>;
    /// Ausführliche Angaben zu einem Paket.
    async fn details(&self, id: &PaketId) -> Result<Paketdetails, Fehler>;
    /// Verfügbare Aktualisierungen mit Dringlichkeit und Neustart-Hinweis.
    async fn aktualisierungen(&self) -> Result<Vec<Aktualisierung>, Fehler>;
    /// Startet eine Transaktion; `melden` wird für jeden Fortschritt aufgerufen (auf dem Aufrufer-Thread).
    /// Rückgabe = Abbruchgriff; `abbrechen()` darf jederzeit gerufen werden.
    fn ausfuehren(
        &self,
        vorgang: Vorgang,
        ids: Vec<PaketId>,
        melden: Box<dyn Fn(Fortschritt)>,
    ) -> Box<dyn Abbruch>;
    /// Nur simulieren: was würde zusätzlich installiert/entfernt? (allow_deps=false, autoremove=false)
    async fn simulieren(&self, vorgang: Vorgang, ids: Vec<PaketId>) -> Result<Simulation, Fehler>;
    /// Objektpfade laufender Transaktionen, für die Wiederanbindung.
    async fn laufende_transaktionen(&self) -> Result<Vec<String>, Fehler>;
}

/// Kennzahlen nach dem Laden des Katalogs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Statistik {
    /// Gelesene Komponenten insgesamt.
    pub komponenten: usize,
    /// Davon mit `<pkgname>`.
    pub mit_paket: usize,
    /// Davon mit Symbol.
    pub mit_symbol: usize,
    /// Davon mit deutschen Texten.
    pub deutsch: usize,
    /// Gelesene Quellen (Pfade).
    pub quellen: Vec<String>,
}

/// AppStream-Katalog (liefert SW-2, konsumiert SW-3).
pub trait Katalog {
    /// Liest alle Quellen und baut den Index; darf 1–2 s dauern, wird im Hintergrund-Thread gerufen.
    fn laden(&mut self) -> Result<Statistik, String>;
    /// Anwendungen einer Kategorie, sortiert nach Name.
    fn kategorie(&self, k: Kategorie) -> Vec<&Anwendung>;
    /// Suche in Name, Zusammenfassung, Schlüsselwörtern, Paketname; Treffer nach Gewicht.
    fn suchen(&self, text: &str) -> Vec<&Anwendung>;
    /// Anwendung zu einem Paketnamen.
    fn nach_paket(&self, paketname: &str) -> Option<&Anwendung>;
    /// Anwendung zu einer Component-ID.
    fn nach_id(&self, id: &str) -> Option<&Anwendung>;
    /// Pfad zur Symboldatei in der Größe 48 oder 64 (Cache unter /var/lib/swcatalog/icons bzw. ~/.cache).
    fn symbol_pfad(&self, a: &Anwendung, groesse: u32) -> Option<PathBuf>;
    /// Alle Anwendungen.
    fn alle(&self) -> &[Anwendung];
}

/// Auftrag an den Transaktionsmonitor.
pub struct Auftrag {
    /// Art des Vorgangs.
    pub vorgang: Vorgang,
    /// Betroffene Pakete.
    pub pakete: Vec<Paket>,
    /// Zugehörige Anwendung, falls aus dem Katalog gestartet.
    pub anwendung: Option<Anwendung>,
}

/// Zustand einer überwachten Transaktion (liefert SW-4 an SW-3).
#[derive(Clone, Debug, PartialEq)]
pub enum Zustand {
    /// Simulation liegt vor, wartet auf Anzeige.
    Simuliert(Simulation),
    /// Wartet auf Bestätigung des Benutzers.
    WartetAufBestaetigung,
    /// Läuft.
    Laeuft {
        /// Gesamtfortschritt, `None` = unbekannt.
        prozent: Option<u8>,
        /// Statustext.
        status: String,
    },
    /// Wartet auf die Paketsperre.
    WartetAufSperre {
        /// Seit wann.
        seit: Instant,
    },
    /// Erfolgreich beendet.
    Fertig,
    /// Fehlgeschlagen.
    Fehlgeschlagen(Fehler),
    /// Abgebrochen.
    Abgebrochen,
}

/// Griff auf eine überwachte Transaktion.
pub struct TransaktionsGriff {
    /// Laufende Nummer des Monitors.
    pub id: u64,
    abbrechen_fn: Option<Rc<dyn Fn(u64)>>,
    bestaetigen_fn: Option<Rc<dyn Fn(u64)>>,
    sperre_fn: Option<Rc<dyn Fn(u64)>>,
}

impl TransaktionsGriff {
    /// Griff ohne Rückrufe (nur Kennung).
    pub fn neu(id: u64) -> Self {
        TransaktionsGriff {
            id,
            abbrechen_fn: None,
            bestaetigen_fn: None,
            sperre_fn: None,
        }
    }

    /// Griff mit Rückrufen des Monitors; jeder Rückruf erhält die `id`.
    pub fn mit_rueckrufen(
        id: u64,
        abbrechen: Rc<dyn Fn(u64)>,
        bestaetigen: Rc<dyn Fn(u64)>,
        sperre_erneut: Rc<dyn Fn(u64)>,
    ) -> Self {
        TransaktionsGriff {
            id,
            abbrechen_fn: Some(abbrechen),
            bestaetigen_fn: Some(bestaetigen),
            sperre_fn: Some(sperre_erneut),
        }
    }

    /// Bricht die Transaktion ab.
    pub fn abbrechen(&self) {
        if let Some(f) = &self.abbrechen_fn {
            f(self.id);
        }
    }

    /// Bestätigt nach der Simulation.
    pub fn bestaetigen(&self) {
        if let Some(f) = &self.bestaetigen_fn {
            f(self.id);
        }
    }

    /// Versucht erneut, die Paketsperre zu bekommen.
    pub fn sperre_erneut_versuchen(&self) {
        if let Some(f) = &self.sperre_fn {
            f(self.id);
        }
    }
}

impl std::fmt::Debug for TransaktionsGriff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransaktionsGriff")
            .field("id", &self.id)
            .finish()
    }
}

/// Transaktionsmonitor (liefert SW-4, konsumiert SW-3).
pub trait Transaktionsmonitor {
    /// Startet einen Auftrag (Simulation, Bestätigung, Ausführung) und meldet Zustände.
    fn starten(
        &self,
        auftrag: Auftrag,
        quelle: Rc<dyn Paketquelle>,
        melden: Box<dyn Fn(Zustand)>,
    ) -> TransaktionsGriff;
    /// Bindet nach einem App-Neustart laufende Transaktionen wieder an.
    fn wieder_anbinden(
        &self,
        quelle: Rc<dyn Paketquelle>,
        melden: Box<dyn Fn(Zustand)>,
    ) -> Vec<TransaktionsGriff>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kategorien_vollstaendig_und_geordnet() {
        let alle = Kategorie::alle();
        assert_eq!(alle.len(), 10);
        assert_eq!(alle[0], Kategorie::Zubehoer);
        assert_eq!(alle[9], Kategorie::Sonstiges);
        for k in alle {
            assert!(!k.name().is_empty());
        }
        assert_eq!(Kategorie::Buero.name(), "Büro");
    }

    #[test]
    fn fehler_anzeige() {
        assert!(Fehler::KeinAgent.to_string().contains("polkit"));
        let s = Fehler::Sonstiges {
            code: "internal-error".into(),
            text: "x".into(),
        };
        assert_eq!(s.to_string(), "internal-error: x");
    }

    #[test]
    fn griff_ohne_rueckrufe_ist_harmlos() {
        let g = TransaktionsGriff::neu(7);
        g.abbrechen();
        g.bestaetigen();
        g.sperre_erneut_versuchen();
        assert_eq!(g.id, 7);
    }
}
