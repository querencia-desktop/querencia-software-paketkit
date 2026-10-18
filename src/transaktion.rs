//! Signalstrom einer PackageKit-Transaktion und Abbildung auf [`Ereignis`] bzw.
//! [`Fortschritt`].
//!
//! Alle Signale eines Transaktionspfads (`Package`, `Details`, `UpdateDetail`,
//! `ItemProgress`, `ErrorCode`, `RequireRestart`, `Finished`, `Destroy` sowie
//! `PropertiesChanged` für `Percentage`/`Status`/`DownloadSizeRemaining`) kommen über
//! einen [`MessageStream`] mit Pfadregel; ein zweiter Strom beobachtet
//! `NameOwnerChanged` für `org.freedesktop.PackageKit`, damit ein Daemon-Neustart als
//! [`Fehler::DaemonWeg`] erkannt wird.

use std::collections::{HashMap, VecDeque};

use futures_lite::StreamExt;
use qs_typen::{Fehler, Fortschritt, Vorgang};
use zbus::message::{Message, Type};
use zbus::zvariant::{ObjectPath, OwnedValue};
use zbus::{Connection, MatchRule, MessageStream};

use crate::enums::{self, exit, fehlercode, status};
use crate::proxies::{DIENST, TRANSAKTION};

/// Ein Ereignis einer Transaktion (noch PackageKit-nah).
#[derive(Clone, Debug, PartialEq)]
pub enum Ereignis {
    /// `Package(info, package_id, summary)`.
    Paket {
        /// `PkInfoEnum`.
        info: u32,
        /// Paket-ID.
        id: String,
        /// Zusammenfassung.
        zusammenfassung: String,
    },
    /// `Details(a{sv})`.
    Details(HashMap<String, OwnedValue>),
    /// `UpdateDetail(...)`, gekürzt auf das, was wir brauchen.
    UpdateDetail {
        /// Paket-ID.
        id: String,
        /// `PkRestartEnum`.
        neustart: u32,
        /// `PkUpdateStateEnum`.
        zustand: u32,
        /// Beschreibung der Aktualisierung.
        text: String,
    },
    /// `ItemProgress(id, status, percentage)`.
    PaketFortschritt {
        /// Paket-ID.
        id: String,
        /// `PkStatusEnum`.
        status: u32,
        /// Prozent.
        prozent: u32,
    },
    /// `ErrorCode(code, details)`.
    Fehlercode {
        /// `PkErrorEnum`.
        code: u32,
        /// Details.
        details: String,
    },
    /// `RequireRestart(type, package_id)`.
    Neustart {
        /// `PkRestartEnum`.
        art: u32,
        /// Paket-ID.
        id: String,
    },
    /// `Finished(exit, runtime)`.
    Fertig {
        /// `PkExitEnum`.
        exit: u32,
        /// Laufzeit in Millisekunden.
        laufzeit_ms: u32,
    },
    /// `Destroy`.
    Zerstoert,
    /// Eigenschaft `Percentage`.
    Prozent(u32),
    /// Eigenschaft `Status`.
    Status(u32),
    /// Eigenschaft `DownloadSizeRemaining`.
    DownloadBytes(u64),
    /// Eigenschaft `AllowCancel`.
    AbbruchErlaubt(bool),
    /// Der Daemon hat den Bus verlassen (oder unsere Verbindung ist weg).
    DaemonWeg,
}

/// Hört auf alle Signale eines Transaktionspfads und auf das Verschwinden des Daemons.
pub struct Lauscher {
    signale: MessageStream,
    namen: MessageStream,
    puffer: VecDeque<Ereignis>,
    zu_ende: bool,
}

impl Lauscher {
    /// Registriert die Match-Regeln beim Bus. Muss **vor** dem Methodenaufruf geschehen,
    /// der die Transaktion startet, sonst gehen frühe Signale verloren.
    pub async fn neu(conn: &Connection, pfad: &ObjectPath<'_>) -> Result<Self, Fehler> {
        let regel = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(DIENST)
            .map_err(fehler_aus_zbus)?
            .path(pfad.clone())
            .map_err(fehler_aus_zbus)?
            .build();
        // PackageKit kann bei großen Suchen tausende `Package`-Signale schicken.
        let signale = MessageStream::for_match_rule(regel, conn, Some(16384))
            .await
            .map_err(fehler_aus_zbus)?;
        let regel_namen = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")
            .map_err(fehler_aus_zbus)?
            .interface("org.freedesktop.DBus")
            .map_err(fehler_aus_zbus)?
            .member("NameOwnerChanged")
            .map_err(fehler_aus_zbus)?
            .arg(0, DIENST)
            .map_err(fehler_aus_zbus)?
            .build();
        let namen = MessageStream::for_match_rule(regel_namen, conn, Some(16))
            .await
            .map_err(fehler_aus_zbus)?;
        Ok(Lauscher {
            signale,
            namen,
            puffer: VecDeque::new(),
            zu_ende: false,
        })
    }

    /// Nächstes Ereignis. Nach [`Ereignis::DaemonWeg`] kommt nichts mehr.
    pub async fn naechstes(&mut self) -> Ereignis {
        loop {
            if let Some(e) = self.puffer.pop_front() {
                return e;
            }
            if self.zu_ende {
                return Ereignis::DaemonWeg;
            }
            let (signale, namen) = (&mut self.signale, &mut self.namen);
            let nachricht = futures_lite::future::or(async { signale.next().await }, async {
                namen.next().await
            })
            .await;
            match nachricht {
                Some(Ok(msg)) => self.puffer.extend(ereignisse_aus(&msg)),
                Some(Err(_)) => {}
                None => {
                    // Verbindung geschlossen: Bus oder eigene Verbindung weg.
                    self.zu_ende = true;
                    self.puffer.push_back(Ereignis::DaemonWeg);
                }
            }
        }
    }
}

/// Zerlegt eine D-Bus-Nachricht in null oder mehr Ereignisse.
pub fn ereignisse_aus(msg: &Message) -> Vec<Ereignis> {
    let kopf = msg.header();
    let (Some(iface), Some(member)) = (kopf.interface(), kopf.member()) else {
        return Vec::new();
    };
    let body = msg.body();
    let mut aus = Vec::new();
    match (iface.as_str(), member.as_str()) {
        (TRANSAKTION, "Package") => {
            if let Ok((info, id, zusammenfassung)) = body.deserialize::<(u32, String, String)>() {
                aus.push(Ereignis::Paket {
                    info,
                    id,
                    zusammenfassung,
                });
            }
        }
        (TRANSAKTION, "Details") => {
            if let Ok(map) = body.deserialize::<HashMap<String, OwnedValue>>() {
                aus.push(Ereignis::Details(map));
            }
        }
        (TRANSAKTION, "UpdateDetail") => {
            type Roh = (
                String,
                Vec<String>,
                Vec<String>,
                Vec<String>,
                Vec<String>,
                Vec<String>,
                u32,
                String,
                String,
                u32,
                String,
                String,
            );
            if let Ok(r) = body.deserialize::<Roh>() {
                aus.push(Ereignis::UpdateDetail {
                    id: r.0,
                    neustart: r.6,
                    zustand: r.9,
                    text: r.7,
                });
            }
        }
        (TRANSAKTION, "ItemProgress") => {
            if let Ok((id, status, prozent)) = body.deserialize::<(String, u32, u32)>() {
                aus.push(Ereignis::PaketFortschritt {
                    id,
                    status,
                    prozent,
                });
            }
        }
        (TRANSAKTION, "ErrorCode") => {
            if let Ok((code, details)) = body.deserialize::<(u32, String)>() {
                aus.push(Ereignis::Fehlercode { code, details });
            }
        }
        (TRANSAKTION, "RequireRestart") => {
            if let Ok((art, id)) = body.deserialize::<(u32, String)>() {
                aus.push(Ereignis::Neustart { art, id });
            }
        }
        (TRANSAKTION, "Finished") => {
            if let Ok((exit, laufzeit_ms)) = body.deserialize::<(u32, u32)>() {
                aus.push(Ereignis::Fertig { exit, laufzeit_ms });
            }
        }
        (TRANSAKTION, "Destroy") => aus.push(Ereignis::Zerstoert),
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
            type Roh = (String, HashMap<String, OwnedValue>, Vec<String>);
            if let Ok((iface, geaendert, _)) = body.deserialize::<Roh>() {
                if iface == TRANSAKTION {
                    // Status vor Percentage, damit „Lade herunter … 0 %“ stimmig ist.
                    if let Some(v) = geaendert.get("Status").and_then(als_u64) {
                        aus.push(Ereignis::Status(v as u32));
                    }
                    if let Some(v) = geaendert.get("Percentage").and_then(als_u64) {
                        aus.push(Ereignis::Prozent(v as u32));
                    }
                    if let Some(v) = geaendert.get("DownloadSizeRemaining").and_then(als_u64) {
                        aus.push(Ereignis::DownloadBytes(v));
                    }
                    if let Some(v) = geaendert.get("AllowCancel") {
                        if let Ok(b) = bool::try_from(v.clone()) {
                            aus.push(Ereignis::AbbruchErlaubt(b));
                        }
                    }
                }
            }
        }
        ("org.freedesktop.DBus", "NameOwnerChanged") => {
            if let Ok((name, _alt, neu)) = body.deserialize::<(String, String, String)>() {
                if name == DIENST && neu.is_empty() {
                    aus.push(Ereignis::DaemonWeg);
                }
            }
        }
        _ => {}
    }
    aus
}

/// Liest einen Zahlenwert (u32/u64/i32/i64/u8/u16) aus einem Variant.
pub fn als_u64(v: &OwnedValue) -> Option<u64> {
    use zbus::zvariant::Value;
    match &**v {
        Value::U8(x) => Some(*x as u64),
        Value::U16(x) => Some(*x as u64),
        Value::U32(x) => Some(*x as u64),
        Value::U64(x) => Some(*x),
        Value::I16(x) => u64::try_from(*x).ok(),
        Value::I32(x) => u64::try_from(*x).ok(),
        Value::I64(x) => u64::try_from(*x).ok(),
        _ => None,
    }
}

/// Liest eine Zeichenkette aus einem Variant.
pub fn als_text(v: &OwnedValue) -> Option<String> {
    use zbus::zvariant::Value;
    match &**v {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Englische Namen der `PkExitEnum`-Werte.
pub fn exitname(exit: u32) -> &'static str {
    match exit {
        0 => "unknown",
        exit::ERFOLG => "success",
        exit::FEHLGESCHLAGEN => "failed",
        exit::ABGEBROCHEN => "cancelled",
        exit::SCHLUESSEL_NOETIG => "key-required",
        exit::EULA_NOETIG => "eula-required",
        exit::GETOETET => "killed",
        exit::MEDIENWECHSEL => "media-change-required",
        exit::UNVERTRAUT_NOETIG => "need-untrusted",
        exit::ABGEBROCHEN_PRIORITAET => "cancelled-priority",
        10 => "skip-transaction",
        11 => "repair-required",
        _ => "exit-unbekannt",
    }
}

/// Fehler aus `Finished(exit)` und dem zuletzt gemeldeten `ErrorCode`.
pub fn fehler_aus_exit(exit: u32, letzter: Option<Fehler>) -> Option<Fehler> {
    match exit {
        exit::ERFOLG => None,
        exit::ABGEBROCHEN | exit::ABGEBROCHEN_PRIORITAET => Some(Fehler::Abgebrochen),
        _ => Some(letzter.unwrap_or_else(|| Fehler::Sonstiges {
            code: format!("exit-{}", exitname(exit)),
            text: String::new(),
        })),
    }
}

/// Übersetzt zbus-Fehler in [`Fehler`].
pub fn fehler_aus_zbus(e: zbus::Error) -> Fehler {
    match &e {
        zbus::Error::MethodError(name, text, _) => {
            let n = name.as_str();
            if n.ends_with(".ServiceUnknown")
                || n.ends_with(".NameHasNoOwner")
                || n.ends_with(".NoReply")
                || n.ends_with(".Disconnected")
            {
                Fehler::DaemonWeg
            } else if n.ends_with(".AccessDenied") || n.ends_with(".RefusedByPolicy") {
                Fehler::NichtBerechtigt
            } else {
                Fehler::Sonstiges {
                    code: n.to_string(),
                    text: text.clone().unwrap_or_default(),
                }
            }
        }
        zbus::Error::InputOutput(_) | zbus::Error::Address(_) => Fehler::Sonstiges {
            code: "dbus-verbindung".into(),
            text: e.to_string(),
        },
        _ => Fehler::Sonstiges {
            code: "dbus".into(),
            text: e.to_string(),
        },
    }
}

/// Wandelt Ereignisse laufend in [`Fortschritt`] um; merkt sich den letzten Fehlercode.
#[derive(Debug, Default)]
pub struct Abbildung {
    letzter_fehler: Option<Fehler>,
    fertig: bool,
    agent_vorhanden: bool,
}

impl Abbildung {
    /// Neue Abbildung; `agent_vorhanden` steuert `not-authorized` → KeinAgent/NichtBerechtigt.
    pub fn neu(agent_vorhanden: bool) -> Self {
        Abbildung {
            letzter_fehler: None,
            fertig: false,
            agent_vorhanden,
        }
    }

    /// `true`, sobald `Fertig`, `Abgebrochen` oder `Fehler` gemeldet wurde.
    pub fn ist_fertig(&self) -> bool {
        self.fertig
    }

    /// Bildet ein Ereignis auf null oder mehr Fortschrittsmeldungen ab.
    pub fn abbilden(&mut self, e: Ereignis) -> Vec<Fortschritt> {
        if self.fertig {
            return Vec::new();
        }
        match e {
            Ereignis::Prozent(p) => vec![Fortschritt::Prozent(p.min(101) as u8)],
            Ereignis::Status(s) => {
                let mut aus = vec![Fortschritt::Status(enums::statustext(s).to_string())];
                if s == status::WARTET_AUF_SPERRE {
                    aus.push(Fortschritt::WartetAufSperre);
                }
                aus
            }
            Ereignis::PaketFortschritt { id, prozent, .. } => vec![Fortschritt::PaketFortschritt {
                id,
                prozent: prozent.min(101) as u8,
            }],
            Ereignis::Fehlercode { code, details } => {
                let f = enums::fehler_aus_code(code, &details, self.agent_vorhanden);
                self.letzter_fehler = Some(f);
                if code == fehlercode::KEINE_SPERRE || code == fehlercode::SPERRE_NOETIG {
                    vec![Fortschritt::WartetAufSperre]
                } else {
                    Vec::new()
                }
            }
            Ereignis::Neustart { .. } => vec![Fortschritt::BrauchtNeustart],
            Ereignis::Fertig { exit, .. } => {
                self.fertig = true;
                match fehler_aus_exit(exit, self.letzter_fehler.take()) {
                    None => vec![Fortschritt::Fertig],
                    Some(Fehler::Abgebrochen) => vec![Fortschritt::Abgebrochen],
                    Some(f) => vec![Fortschritt::Fehler(f)],
                }
            }
            Ereignis::Zerstoert | Ereignis::DaemonWeg => {
                // `Destroy` vor `Finished` oder Daemon weg: Transaktion ist verloren.
                self.fertig = true;
                vec![Fortschritt::Fehler(Fehler::DaemonWeg)]
            }
            Ereignis::Paket { .. }
            | Ereignis::Details(_)
            | Ereignis::UpdateDetail { .. }
            | Ereignis::DownloadBytes(_)
            | Ereignis::AbbruchErlaubt(_) => Vec::new(),
        }
    }
}

/// `PkRoleEnum` → [`Vorgang`] (für die Wiederanbindung an laufende Transaktionen).
pub fn vorgang_aus_rolle(rolle: u32) -> Option<Vorgang> {
    match rolle {
        11 => Some(Vorgang::Installieren),
        14 => Some(Vorgang::Entfernen),
        22 => Some(Vorgang::Aktualisieren),
        13 => Some(Vorgang::QuellenAuffrischen),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbildung_erfolg() {
        let mut a = Abbildung::neu(true);
        assert_eq!(
            a.abbilden(Ereignis::Status(8)),
            vec![Fortschritt::Status("Lade herunter …".into())]
        );
        assert_eq!(
            a.abbilden(Ereignis::Prozent(50)),
            vec![Fortschritt::Prozent(50)]
        );
        assert_eq!(
            a.abbilden(Ereignis::Prozent(101)),
            vec![Fortschritt::Prozent(101)]
        );
        assert_eq!(
            a.abbilden(Ereignis::Prozent(300)),
            vec![Fortschritt::Prozent(101)]
        );
        assert_eq!(
            a.abbilden(Ereignis::PaketFortschritt {
                id: "a;1;x;y".into(),
                status: 9,
                prozent: 30
            }),
            vec![Fortschritt::PaketFortschritt {
                id: "a;1;x;y".into(),
                prozent: 30
            }]
        );
        assert_eq!(
            a.abbilden(Ereignis::Neustart {
                art: 4,
                id: String::new()
            }),
            vec![Fortschritt::BrauchtNeustart]
        );
        assert_eq!(
            a.abbilden(Ereignis::Paket {
                info: 1,
                id: "x".into(),
                zusammenfassung: String::new()
            }),
            vec![]
        );
        assert!(!a.ist_fertig());
        assert_eq!(
            a.abbilden(Ereignis::Fertig {
                exit: 1,
                laufzeit_ms: 5
            }),
            vec![Fortschritt::Fertig]
        );
        assert!(a.ist_fertig());
        assert_eq!(a.abbilden(Ereignis::Zerstoert), vec![]);
    }

    #[test]
    fn abbildung_fehler_und_abbruch() {
        let mut a = Abbildung::neu(false);
        assert_eq!(
            a.abbilden(Ereignis::Fehlercode {
                code: 48,
                details: "x".into()
            }),
            vec![]
        );
        assert_eq!(
            a.abbilden(Ereignis::Fertig {
                exit: 2,
                laufzeit_ms: 0
            }),
            vec![Fortschritt::Fehler(Fehler::KeinAgent)]
        );

        let mut b = Abbildung::neu(true);
        assert_eq!(
            b.abbilden(Ereignis::Fertig {
                exit: 3,
                laufzeit_ms: 0
            }),
            vec![Fortschritt::Abgebrochen]
        );

        let mut c = Abbildung::neu(true);
        assert_eq!(
            c.abbilden(Ereignis::Fehlercode {
                code: 26,
                details: "apt".into()
            }),
            vec![Fortschritt::WartetAufSperre]
        );
        assert_eq!(
            c.abbilden(Ereignis::Fertig {
                exit: 2,
                laufzeit_ms: 0
            }),
            vec![Fortschritt::Fehler(Fehler::SperreBelegt("apt".into()))]
        );

        let mut d = Abbildung::neu(true);
        assert_eq!(
            d.abbilden(Ereignis::Status(30)),
            vec![
                Fortschritt::Status("Warte auf die Paketsperre …".into()),
                Fortschritt::WartetAufSperre
            ]
        );
        assert_eq!(
            d.abbilden(Ereignis::DaemonWeg),
            vec![Fortschritt::Fehler(Fehler::DaemonWeg)]
        );

        let mut e = Abbildung::neu(true);
        assert_eq!(
            e.abbilden(Ereignis::Fertig {
                exit: 6,
                laufzeit_ms: 0
            }),
            vec![Fortschritt::Fehler(Fehler::Sonstiges {
                code: "exit-killed".into(),
                text: String::new()
            })]
        );
    }

    #[test]
    fn rollen() {
        assert_eq!(vorgang_aus_rolle(11), Some(Vorgang::Installieren));
        assert_eq!(vorgang_aus_rolle(14), Some(Vorgang::Entfernen));
        assert_eq!(vorgang_aus_rolle(22), Some(Vorgang::Aktualisieren));
        assert_eq!(vorgang_aus_rolle(13), Some(Vorgang::QuellenAuffrischen));
        assert_eq!(vorgang_aus_rolle(21), None);
        assert_eq!(exitname(2), "failed");
    }
}
