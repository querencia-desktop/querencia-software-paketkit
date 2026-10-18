//! PackageKit-Aufzählungen (Zahlenwerte der D-Bus-Schnittstelle, Stand PackageKit 1.3.4,
//! ausgelesen aus `PackageKitGlib-1.0.typelib`) und ihre Abbildung auf deutsche Texte
//! bzw. auf die Typen aus `qs_typen`.

use qs_typen::{Dringlichkeit, Fehler, Paket};

/// PkBitfield: Bit `1 << wert` je Aufzählungswert.
const fn bit(wert: u32) -> u64 {
    1u64 << wert
}

/// Filter (`PkFilterEnum`) als Bitfeld für `SearchNames`, `Resolve`, `GetUpdates`.
pub mod filter {
    use super::bit;
    /// Kein Filter.
    pub const KEINER: u64 = 0;
    /// Nur installierte Pakete.
    pub const INSTALLIERT: u64 = bit(2);
    /// Nur nicht installierte Pakete (`~installed`).
    pub const NICHT_INSTALLIERT: u64 = bit(3);
    /// Nur neueste Version je Paket.
    pub const NEUESTE: u64 = bit(16);
    /// Nur die eigene Architektur.
    pub const ARCH: u64 = bit(18);
}

/// Transaktionsflaggen (`PkTransactionFlagEnum`) als Bitfeld.
pub mod flagge {
    use super::bit;
    /// Keine Flagge.
    pub const KEINE: u64 = 0;
    /// Nur vertrauenswürdige (signierte) Pakete.
    pub const NUR_VERTRAUT: u64 = bit(1);
    /// Nur simulieren.
    pub const SIMULIEREN: u64 = bit(2);
}

/// `PkInfoEnum`.
pub mod info {
    /// Installiert.
    pub const INSTALLIERT: u32 = 1;
    /// Verfügbar.
    pub const VERFUEGBAR: u32 = 2;
    /// Niedrige Dringlichkeit.
    pub const NIEDRIG: u32 = 3;
    /// Erweiterung.
    pub const ERWEITERUNG: u32 = 4;
    /// Normal.
    pub const NORMAL: u32 = 5;
    /// Fehlerbehebung.
    pub const FEHLERBEHEBUNG: u32 = 6;
    /// Wichtig.
    pub const WICHTIG: u32 = 7;
    /// Sicherheit.
    pub const SICHERHEIT: u32 = 8;
    /// Blockiert.
    pub const BLOCKIERT: u32 = 9;
    /// Wird heruntergeladen.
    pub const LAEDT: u32 = 10;
    /// Wird aktualisiert (Simulation: würde aktualisiert).
    pub const AKTUALISIERT: u32 = 11;
    /// Wird installiert (Simulation: würde installiert).
    pub const INSTALLIERT_WIRD: u32 = 12;
    /// Wird entfernt (Simulation: würde entfernt).
    pub const ENTFERNT_WIRD: u32 = 13;
    /// Aufräumen.
    pub const AUFRAEUMEN: u32 = 14;
    /// Wird ersetzt (obsoleting).
    pub const ERSETZT_WIRD: u32 = 15;
    /// Wird erneut installiert.
    pub const NEU_INSTALLIERT: u32 = 19;
    /// Wird zurückgestuft.
    pub const ZURUECKGESTUFT: u32 = 20;
    /// Kritisch.
    pub const KRITISCH: u32 = 26;
}

/// `PkExitEnum`.
pub mod exit {
    /// Erfolg.
    pub const ERFOLG: u32 = 1;
    /// Fehlgeschlagen.
    pub const FEHLGESCHLAGEN: u32 = 2;
    /// Abgebrochen.
    pub const ABGEBROCHEN: u32 = 3;
    /// Schlüssel nötig.
    pub const SCHLUESSEL_NOETIG: u32 = 4;
    /// EULA nötig.
    pub const EULA_NOETIG: u32 = 5;
    /// Getötet.
    pub const GETOETET: u32 = 6;
    /// Medienwechsel nötig.
    pub const MEDIENWECHSEL: u32 = 7;
    /// Nicht vertrauenswürdig nötig.
    pub const UNVERTRAUT_NOETIG: u32 = 8;
    /// Wegen Priorität abgebrochen.
    pub const ABGEBROCHEN_PRIORITAET: u32 = 9;
}

/// `PkStatusEnum`.
pub mod status {
    /// Wartet auf Sperre.
    pub const WARTET_AUF_SPERRE: u32 = 30;
    /// Wartet auf Berechtigung (polkit).
    pub const WARTET_AUF_BERECHTIGUNG: u32 = 31;
    /// Fertig.
    pub const FERTIG: u32 = 18;
}

/// `PkErrorEnum`.
pub mod fehlercode {
    /// Kein Netz.
    pub const KEIN_NETZ: u32 = 2;
    /// Paket nicht installiert.
    pub const PAKET_NICHT_INSTALLIERT: u32 = 7;
    /// Paket nicht gefunden.
    pub const PAKET_NICHT_GEFUNDEN: u32 = 8;
    /// Abhängigkeitsauflösung fehlgeschlagen.
    pub const ABHAENGIGKEITEN: u32 = 13;
    /// Transaktion abgebrochen.
    pub const ABGEBROCHEN: u32 = 17;
    /// Sperre nicht zu bekommen.
    pub const KEINE_SPERRE: u32 = 26;
    /// Dateikonflikt.
    pub const DATEIKONFLIKT: u32 = 35;
    /// Paketkonflikt.
    pub const PAKETKONFLIKT: u32 = 36;
    /// Nicht berechtigt.
    pub const NICHT_BERECHTIGT: u32 = 48;
    /// Aktualisierung nicht gefunden.
    pub const UPDATE_NICHT_GEFUNDEN: u32 = 49;
    /// Keine Mirrors mehr.
    pub const KEINE_MIRRORS: u32 = 43;
    /// Wegen Priorität abgebrochen.
    pub const ABGEBROCHEN_PRIORITAET: u32 = 65;
    /// Sperre nötig.
    pub const SPERRE_NOETIG: u32 = 67;
}

/// `PkRestartEnum`.
pub mod neustart {
    /// Keiner.
    pub const KEINER: u32 = 1;
    /// Anwendung.
    pub const ANWENDUNG: u32 = 2;
    /// Sitzung.
    pub const SITZUNG: u32 = 3;
    /// System.
    pub const SYSTEM: u32 = 4;
    /// Sitzung (Sicherheit).
    pub const SICHERHEIT_SITZUNG: u32 = 5;
    /// System (Sicherheit).
    pub const SICHERHEIT_SYSTEM: u32 = 6;
}

/// Englische PackageKit-Namen der Fehlercodes (`pk_error_enum_to_string`).
const FEHLERNAMEN: [&str; 69] = [
    "unknown",
    "oom",
    "no-network",
    "not-supported",
    "internal-error",
    "gpg-failure",
    "package-id-invalid",
    "package-not-installed",
    "package-not-found",
    "package-already-installed",
    "package-download-failed",
    "group-not-found",
    "group-list-invalid",
    "dep-resolution-failed",
    "filter-invalid",
    "create-thread-failed",
    "transaction-error",
    "transaction-cancelled",
    "no-cache",
    "repo-not-found",
    "cannot-remove-system-package",
    "process-kill",
    "failed-initialization",
    "failed-finalise",
    "failed-config-parsing",
    "cannot-cancel",
    "cannot-get-lock",
    "no-packages-to-update",
    "cannot-write-repo-config",
    "local-install-failed",
    "bad-gpg-signature",
    "missing-gpg-signature",
    "cannot-install-source-package",
    "repo-configuration-error",
    "no-license-agreement",
    "file-conflicts",
    "package-conflicts",
    "repo-not-available",
    "invalid-package-file",
    "package-install-blocked",
    "package-corrupt",
    "all-packages-already-installed",
    "file-not-found",
    "no-more-mirrors-to-try",
    "no-distro-upgrade-data",
    "incompatible-architecture",
    "no-space-on-device",
    "media-change-required",
    "not-authorized",
    "update-not-found",
    "cannot-install-repo-unsigned",
    "cannot-update-repo-unsigned",
    "cannot-get-filelist",
    "cannot-get-requires",
    "cannot-disable-repository",
    "restricted-download",
    "package-failed-to-configure",
    "package-failed-to-build",
    "package-failed-to-install",
    "package-failed-to-remove",
    "update-failed-due-to-running-process",
    "package-database-changed",
    "provide-type-not-supported",
    "install-root-invalid",
    "cannot-fetch-sources",
    "cancelled-priority",
    "unfinished-transaction",
    "lock-required",
    "repo-already-set",
];

/// Englischer PackageKit-Name eines Fehlercodes.
pub fn fehlername(code: u32) -> String {
    FEHLERNAMEN
        .get(code as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("fehlercode-{code}"))
}

/// Englische PackageKit-Namen der Gruppen (`pk_group_enum_to_string`).
const GRUPPENNAMEN: [&str; 36] = [
    "unknown",
    "accessibility",
    "accessories",
    "admin-tools",
    "communication",
    "desktop-gnome",
    "desktop-kde",
    "desktop-other",
    "desktop-xfce",
    "education",
    "fonts",
    "games",
    "graphics",
    "internet",
    "legacy",
    "localization",
    "maps",
    "multimedia",
    "network",
    "office",
    "other",
    "power-management",
    "programming",
    "publishing",
    "repos",
    "security",
    "servers",
    "system",
    "virtualization",
    "science",
    "documentation",
    "electronics",
    "collections",
    "vendor",
    "newest",
    "desktop-dde",
];

/// Englischer PackageKit-Name einer Gruppe.
pub fn gruppenname(gruppe: u32) -> String {
    GRUPPENNAMEN
        .get(gruppe as usize)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("gruppe-{gruppe}"))
}

/// Deutscher Text zu einem `PkStatusEnum`-Wert (für `Fortschritt::Status`).
pub fn statustext(status: u32) -> &'static str {
    match status {
        0 => "Unbekannt",
        1 => "Warte …",
        2 => "Bereite vor …",
        3 => "Läuft …",
        4 => "Frage ab …",
        5 => "Hole Informationen …",
        6 => "Entferne …",
        7 => "Aktualisiere Paketquellen …",
        8 => "Lade herunter …",
        9 => "Installiere …",
        10 => "Aktualisiere …",
        11 => "Räume auf …",
        12 => "Ersetze veraltete Pakete …",
        13 => "Löse Abhängigkeiten auf …",
        14 => "Prüfe Signaturen …",
        15 => "Prüfe Änderungen …",
        16 => "Übernehme Änderungen …",
        17 => "Frage an …",
        18 => "Fertig",
        19 => "Breche ab …",
        20 => "Lade Paketquelle herunter …",
        21 => "Lade Paketliste herunter …",
        22 => "Lade Dateiliste herunter …",
        23 => "Lade Änderungsprotokoll herunter …",
        24 => "Lade Gruppen herunter …",
        25 => "Lade Aktualisierungsinformationen herunter …",
        26 => "Packe neu …",
        27 => "Lade Zwischenspeicher …",
        28 => "Suche Anwendungen …",
        29 => "Erzeuge Paketliste …",
        30 => "Warte auf die Paketsperre …",
        31 => "Warte auf Berechtigung …",
        32 => "Prüfe laufende Prozesse …",
        33 => "Prüfe ausführbare Dateien …",
        34 => "Prüfe Bibliotheken …",
        35 => "Kopiere Dateien …",
        36 => "Führe Skripte aus …",
        _ => "Arbeite …",
    }
}

/// Bildet einen PackageKit-`ErrorCode` auf [`Fehler`] ab.
///
/// `agent_vorhanden` entscheidet bei `not-authorized` zwischen [`Fehler::NichtBerechtigt`]
/// und [`Fehler::KeinAgent`] (siehe [`crate::agent`]).
pub fn fehler_aus_code(code: u32, details: &str, agent_vorhanden: bool) -> Fehler {
    match code {
        fehlercode::NICHT_BERECHTIGT => {
            if agent_vorhanden {
                Fehler::NichtBerechtigt
            } else {
                Fehler::KeinAgent
            }
        }
        fehlercode::KEINE_SPERRE | fehlercode::SPERRE_NOETIG => {
            Fehler::SperreBelegt(details.to_string())
        }
        fehlercode::KEIN_NETZ | fehlercode::KEINE_MIRRORS => Fehler::NetzFehlt,
        fehlercode::PAKET_NICHT_GEFUNDEN
        | fehlercode::PAKET_NICHT_INSTALLIERT
        | fehlercode::UPDATE_NICHT_GEFUNDEN => Fehler::PaketNichtGefunden(details.to_string()),
        fehlercode::ABHAENGIGKEITEN | fehlercode::DATEIKONFLIKT | fehlercode::PAKETKONFLIKT => {
            Fehler::AbhaengigkeitenKonflikt(details.to_string())
        }
        fehlercode::ABGEBROCHEN | fehlercode::ABGEBROCHEN_PRIORITAET => Fehler::Abgebrochen,
        _ => Fehler::Sonstiges {
            code: fehlername(code),
            text: details.to_string(),
        },
    }
}

/// Dringlichkeit aus dem `info`-Wert eines `Package`-Signals von `GetUpdates`.
pub fn dringlichkeit_aus_info(info: u32) -> Dringlichkeit {
    match info {
        info::SICHERHEIT | info::KRITISCH => Dringlichkeit::Sicherheit,
        info::FEHLERBEHEBUNG => Dringlichkeit::Fehlerbehebung,
        info::ERWEITERUNG => Dringlichkeit::Erweiterung,
        _ => Dringlichkeit::Normal,
    }
}

/// `true`, wenn ein `PkRestartEnum`-Wert einen Neustart von Sitzung oder System bedeutet.
pub fn neustart_noetig(restart: u32) -> bool {
    matches!(
        restart,
        neustart::SITZUNG
            | neustart::SYSTEM
            | neustart::SICHERHEIT_SITZUNG
            | neustart::SICHERHEIT_SYSTEM
    )
}

/// Zerlegt eine PackageKit-Paket-ID `name;version;arch;data` in ein [`Paket`].
///
/// `installiert` ist wahr, wenn `info == installed` oder das `data`-Feld mit `installed`
/// beginnt (apt-Backend: `installed` bzw. `installed:<quelle>`).
pub fn paket_aus_id(id: &str, zusammenfassung: &str, info: u32) -> Paket {
    let mut teile = id.splitn(4, ';');
    let name = teile.next().unwrap_or_default().to_string();
    let version = teile.next().unwrap_or_default().to_string();
    let arch = teile.next().unwrap_or_default().to_string();
    let data = teile.next().unwrap_or_default();
    Paket {
        id: id.to_string(),
        name,
        version,
        arch,
        zusammenfassung: zusammenfassung.to_string(),
        installiert: info == info::INSTALLIERT || data.starts_with("installed"),
    }
}

/// Vergleicht zwei Paket-IDs ohne das `data`-Feld (`name;version;arch`).
pub fn gleiche_paketversion(a: &str, b: &str) -> bool {
    let kern = |s: &str| s.splitn(4, ';').take(3).collect::<Vec<_>>().join(";");
    kern(a) == kern(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_und_flaggen_bits() {
        assert_eq!(filter::INSTALLIERT, 4);
        assert_eq!(filter::NICHT_INSTALLIERT, 8);
        assert_eq!(filter::NEUESTE, 1 << 16);
        assert_eq!(filter::ARCH, 1 << 18);
        assert_eq!(flagge::NUR_VERTRAUT, 2);
        assert_eq!(flagge::SIMULIEREN, 4);
    }

    #[test]
    fn fehlercodes_abbilden() {
        assert_eq!(fehler_aus_code(48, "", true), Fehler::NichtBerechtigt);
        assert_eq!(fehler_aus_code(48, "", false), Fehler::KeinAgent);
        assert_eq!(
            fehler_aus_code(26, "apt läuft", true),
            Fehler::SperreBelegt("apt läuft".into())
        );
        assert_eq!(fehler_aus_code(2, "", true), Fehler::NetzFehlt);
        assert_eq!(
            fehler_aus_code(8, "foo", true),
            Fehler::PaketNichtGefunden("foo".into())
        );
        assert_eq!(
            fehler_aus_code(13, "x", true),
            Fehler::AbhaengigkeitenKonflikt("x".into())
        );
        assert_eq!(fehler_aus_code(17, "", true), Fehler::Abgebrochen);
        assert_eq!(
            fehler_aus_code(4, "Error number 1", true),
            Fehler::Sonstiges {
                code: "internal-error".into(),
                text: "Error number 1".into()
            }
        );
        assert_eq!(fehlername(69), "fehlercode-69");
    }

    #[test]
    fn statustexte_deutsch() {
        assert_eq!(statustext(8), "Lade herunter …");
        assert_eq!(
            statustext(status::WARTET_AUF_SPERRE),
            "Warte auf die Paketsperre …"
        );
        assert_eq!(statustext(999), "Arbeite …");
    }

    #[test]
    fn paket_id_zerlegen() {
        let p = paket_aus_id(
            "gedit;46.1-1;amd64;installed:noble",
            "Editor",
            info::VERFUEGBAR,
        );
        assert_eq!(p.name, "gedit");
        assert_eq!(p.version, "46.1-1");
        assert_eq!(p.arch, "amd64");
        assert!(p.installiert);
        let q = paket_aus_id(
            "gedit;46.2-1;amd64;noble-security",
            "Editor",
            info::VERFUEGBAR,
        );
        assert!(!q.installiert);
        let r = paket_aus_id("glib2;2.14.0;i386;fedora", "", info::INSTALLIERT);
        assert!(r.installiert);
        let kurz = paket_aus_id("nurname", "", 0);
        assert_eq!(kurz.name, "nurname");
        assert_eq!(kurz.version, "");
        assert!(gleiche_paketversion(
            "a;1;amd64;installed",
            "a;1;amd64;noble"
        ));
        assert!(!gleiche_paketversion("a;1;amd64;x", "a;2;amd64;x"));
    }

    #[test]
    fn dringlichkeit_und_neustart() {
        assert_eq!(
            dringlichkeit_aus_info(info::SICHERHEIT),
            Dringlichkeit::Sicherheit
        );
        assert_eq!(
            dringlichkeit_aus_info(info::FEHLERBEHEBUNG),
            Dringlichkeit::Fehlerbehebung
        );
        assert_eq!(
            dringlichkeit_aus_info(info::ERWEITERUNG),
            Dringlichkeit::Erweiterung
        );
        assert_eq!(dringlichkeit_aus_info(info::NORMAL), Dringlichkeit::Normal);
        assert!(neustart_noetig(neustart::SYSTEM));
        assert!(neustart_noetig(neustart::SICHERHEIT_SITZUNG));
        assert!(!neustart_noetig(neustart::ANWENDUNG));
        assert!(!neustart_noetig(neustart::KEINER));
        assert_eq!(gruppenname(12), "graphics");
    }
}
