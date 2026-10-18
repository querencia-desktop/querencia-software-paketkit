//! [`PaketKit`]: die [`Paketquelle`] über D-Bus.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qs_typen::{
    Abbruch, Aktualisierung, Fehler, Fortschritt, Paket, PaketId, Paketdetails, Paketquelle,
    Simulation, Vorgang,
};
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zbus::Connection;

use crate::agent::{self, Agentpruefung};
use crate::enums::{self, filter, flagge, info};
use crate::proxies::{PackageKitProxy, TransactionProxy};
use crate::transaktion::{self, als_text, als_u64, fehler_aus_zbus, Abbildung, Ereignis, Lauscher};

/// Umgebungsvariable mit der Adresse eines privaten Busses (Prüfgerüst). Leer oder
/// nicht gesetzt = Systembus.
pub const UMGEBUNG_BUS: &str = "QS_PK_BUS";

/// Hinweise, die jede Transaktion setzt (SCHNITTSTELLEN.md).
pub const HINWEISE: [&str; 3] = ["locale=de_DE.UTF-8", "interactive=true", "background=false"];

/// Standardfrist für lesende Transaktionen und Simulationen (siehe [`PaketKit::mit_frist`]).
pub const FRIST_LESEN: Duration = Duration::from_secs(300);

/// Fehlercode in [`Fehler::Sonstiges`] bei überschrittener Frist.
pub const CODE_ZEIT: &str = "zeitueberschreitung";

/// Verbindung zu PackageKit (Systembus oder privater Bus aus `QS_PK_BUS`).
///
/// `PaketKit` ist `Clone + Send + Sync`; alle Klone teilen eine D-Bus-Verbindung.
/// PackageKit erlaubt Methodenaufrufe auf einer Transaktion nur der Verbindung, die sie
/// angelegt hat – deshalb laufen auch `abbrechen()` und die Hintergrund-Threads von
/// [`PaketKit::ausfuehren_kanal`] über dieselbe Verbindung.
#[derive(Clone, Debug)]
pub struct PaketKit {
    verbindung: Connection,
    agentpruefung: Agentpruefung,
    frist: Option<Duration>,
}

/// Empfängt die Fortschrittsmeldungen einer Transaktion aus
/// [`PaketKit::ausfuehren_kanal`] oder [`PaketKit::beobachten_kanal`].
///
/// Der Kanal endet (liefert `None`) nach `Fertig`, `Abgebrochen` oder `Fehler`.
#[derive(Clone, Debug)]
pub struct FortschrittEmpfaenger {
    rx: async_channel::Receiver<Fortschritt>,
}

impl FortschrittEmpfaenger {
    /// Nächste Meldung, asynchron. `None` = Transaktion beendet.
    pub async fn naechster(&self) -> Option<Fortschritt> {
        self.rx.recv().await.ok()
    }

    /// Nächste Meldung, blockierend (für Kommandozeilenwerkzeuge ohne Hauptschleife).
    pub fn naechster_blockierend(&self) -> Option<Fortschritt> {
        self.rx.recv_blocking().ok()
    }

    /// Nächste Meldung ohne Warten.
    pub fn versuchen(&self) -> Abruf {
        match self.rx.try_recv() {
            Ok(f) => Abruf::Meldung(f),
            Err(async_channel::TryRecvError::Empty) => Abruf::Leer,
            Err(async_channel::TryRecvError::Closed) => Abruf::Beendet,
        }
    }

    /// Sammelt alle Meldungen bis zum Ende (blockierend), höchstens `frist` lang.
    pub fn alle_blockierend(&self, frist: Duration) -> Vec<Fortschritt> {
        let ende = std::time::Instant::now() + frist;
        let mut aus = Vec::new();
        while std::time::Instant::now() < ende {
            match self.versuchen() {
                Abruf::Meldung(f) => aus.push(f),
                Abruf::Leer => std::thread::sleep(Duration::from_millis(10)),
                Abruf::Beendet => break,
            }
        }
        aus
    }
}

/// Ergebnis von [`FortschrittEmpfaenger::versuchen`].
#[derive(Clone, Debug, PartialEq)]
pub enum Abruf {
    /// Eine Meldung lag vor.
    Meldung(Fortschritt),
    /// Gerade keine Meldung, die Transaktion läuft noch.
    Leer,
    /// Kanal beendet, es kommt nichts mehr.
    Beendet,
}

/// Abbruchgriff einer Transaktion aus [`PaketKit::ausfuehren_kanal`].
pub struct AbbruchGriff {
    verbindung: Connection,
    pfad: Arc<Mutex<Option<OwnedObjectPath>>>,
    angefordert: Arc<AtomicBool>,
}

impl AbbruchGriff {
    /// Objektpfad der Transaktion, sobald sie angelegt ist (für Protokoll/Wiederanbindung).
    pub fn pfad(&self) -> Option<String> {
        self.pfad
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|p| p.to_string())
    }
}

impl Abbruch for AbbruchGriff {
    fn abbrechen(&self) {
        self.angefordert.store(true, Ordering::SeqCst);
        let pfad = self.pfad.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(pfad) = pfad else {
            return; // Transaktion noch nicht angelegt; der Arbeits-Thread sieht die Flagge.
        };
        let conn = self.verbindung.clone();
        // Nicht auf dem Aufrufer-Thread blockieren (meist der GTK-Thread).
        std::thread::spawn(move || {
            let _ = zbus::block_on(async move {
                let tx = TransactionProxy::builder(&conn).path(pfad)?.build().await?;
                tx.cancel().await
            });
        });
    }
}

impl PaketKit {
    /// Verbindet sich: mit dem Bus aus `QS_PK_BUS`, sonst mit dem Systembus.
    pub async fn neu() -> Result<Self, Fehler> {
        let verbindung = match std::env::var(UMGEBUNG_BUS) {
            Ok(adresse) if !adresse.trim().is_empty() => Self::verbinden_mit(&adresse).await?,
            _ => Connection::system().await.map_err(fehler_aus_zbus)?,
        };
        Ok(PaketKit {
            verbindung,
            agentpruefung: Agentpruefung::Automatisch,
            frist: Some(FRIST_LESEN),
        })
    }

    /// Wie [`PaketKit::neu`], aber blockierend (für Programme ohne eigene Hauptschleife).
    pub fn neu_blockierend() -> Result<Self, Fehler> {
        zbus::block_on(Self::neu())
    }

    /// Verbindet sich mit einer bestimmten Busadresse (z. B. `unix:path=/tmp/qs-pk/sock`).
    pub async fn mit_adresse(adresse: &str) -> Result<Self, Fehler> {
        Ok(PaketKit {
            verbindung: Self::verbinden_mit(adresse).await?,
            agentpruefung: Agentpruefung::Automatisch,
            frist: Some(FRIST_LESEN),
        })
    }

    /// Nutzt eine bestehende zbus-Verbindung.
    pub fn mit_verbindung(verbindung: Connection) -> Self {
        PaketKit {
            verbindung,
            agentpruefung: Agentpruefung::Automatisch,
            frist: Some(FRIST_LESEN),
        }
    }

    /// Legt fest, wie bei `not-authorized` auf einen fehlenden Agenten geschlossen wird.
    pub fn mit_agentpruefung(mut self, art: Agentpruefung) -> Self {
        self.agentpruefung = art;
        self
    }

    /// Frist für lesende Transaktionen und Simulationen (Standard [`FRIST_LESEN`]);
    /// `None` = unbegrenzt. Läuft sie ab, wird die Transaktion mit `Cancel` beendet und
    /// `Fehler::Sonstiges { code: "zeitueberschreitung", .. }` geliefert. Schreibende
    /// Vorgänge aus `ausfuehren` haben keine Frist; das regelt der Transaktionsmonitor.
    pub fn mit_frist(mut self, frist: Option<Duration>) -> Self {
        self.frist = frist;
        self
    }

    /// Die zugrunde liegende Verbindung.
    pub fn verbindung(&self) -> &Connection {
        &self.verbindung
    }

    async fn verbinden_mit(adresse: &str) -> Result<Connection, Fehler> {
        zbus::connection::Builder::address(adresse)
            .map_err(fehler_aus_zbus)?
            .build()
            .await
            .map_err(fehler_aus_zbus)
    }

    /// Name des Backends (`apt`, `test_thread`, `attrappe`, …).
    pub async fn backend_name(&self) -> Result<String, Fehler> {
        let pk = PackageKitProxy::new(&self.verbindung)
            .await
            .map_err(fehler_aus_zbus)?;
        pk.backend_name().await.map_err(fehler_aus_zbus)
    }

    /// Legt eine Transaktion an, setzt die Hinweise, lauscht und ruft `start` auf.
    /// `melden` bekommt jedes Ereignis bis einschließlich `Fertig`.
    /// Rückgabe: der `exit`-Wert des `Finished`-Signals.
    async fn transaktion<F, Fut, M>(&self, start: F, mut melden: M) -> Result<u32, Fehler>
    where
        F: FnOnce(TransactionProxy<'static>) -> Fut,
        Fut: std::future::Future<Output = zbus::Result<()>>,
        M: FnMut(Ereignis),
    {
        let pfad = self.anlegen().await?;
        let mut lauscher = Lauscher::neu(&self.verbindung, &pfad).await?;
        let tx = self.proxy(pfad.clone()).await?;
        tx.set_hints(&HINWEISE).await.map_err(fehler_aus_zbus)?;
        start(tx.clone()).await.map_err(fehler_aus_zbus)?;
        let Some(frist) = self.frist else {
            return self.bis_fertig(&mut lauscher, &mut melden).await;
        };
        let ergebnis =
            futures_lite::future::or(self.bis_fertig(&mut lauscher, &mut melden), async {
                async_io::Timer::after(frist).await;
                Err(Fehler::Sonstiges {
                    code: CODE_ZEIT.into(),
                    text: format!(
                        "PackageKit hat nach {} s nicht geantwortet",
                        frist.as_secs()
                    ),
                })
            })
            .await;
        if matches!(&ergebnis, Err(Fehler::Sonstiges { code, .. }) if code == CODE_ZEIT) {
            let _ = tx.cancel().await;
        }
        ergebnis
    }

    async fn anlegen(&self) -> Result<OwnedObjectPath, Fehler> {
        let pk = PackageKitProxy::new(&self.verbindung)
            .await
            .map_err(fehler_aus_zbus)?;
        pk.create_transaction().await.map_err(fehler_aus_zbus)
    }

    async fn proxy(&self, pfad: OwnedObjectPath) -> Result<TransactionProxy<'static>, Fehler> {
        TransactionProxy::builder(&self.verbindung)
            .path(pfad)
            .map_err(fehler_aus_zbus)?
            .build()
            .await
            .map_err(fehler_aus_zbus)
    }

    async fn bis_fertig<M: FnMut(Ereignis)>(
        &self,
        lauscher: &mut Lauscher,
        melden: &mut M,
    ) -> Result<u32, Fehler> {
        let mut letzter_fehler = None;
        loop {
            let e = lauscher.naechstes().await;
            match &e {
                Ereignis::Fehlercode { code, details } => {
                    letzter_fehler = Some(enums::fehler_aus_code(
                        *code,
                        details,
                        agent::pruefen(self.agentpruefung),
                    ));
                }
                Ereignis::Fertig { exit, .. } => {
                    let exit = *exit;
                    melden(e);
                    return match transaktion::fehler_aus_exit(exit, letzter_fehler) {
                        None => Ok(exit),
                        Some(f) => Err(f),
                    };
                }
                Ereignis::DaemonWeg | Ereignis::Zerstoert => {
                    melden(e);
                    return Err(Fehler::DaemonWeg);
                }
                _ => {}
            }
            melden(e);
        }
    }

    /// Sammelt alle `Package`-Signale einer lesenden Transaktion.
    async fn pakete_sammeln<F, Fut>(&self, start: F) -> Result<Vec<Paket>, Fehler>
    where
        F: FnOnce(TransactionProxy<'static>) -> Fut,
        Fut: std::future::Future<Output = zbus::Result<()>>,
    {
        let mut pakete = Vec::new();
        self.transaktion(start, |e| {
            if let Ereignis::Paket {
                info,
                id,
                zusammenfassung,
            } = e
            {
                if info == info::INSTALLIERT || info == info::VERFUEGBAR {
                    pakete.push(enums::paket_aus_id(&id, &zusammenfassung, info));
                }
            }
        })
        .await?;
        Ok(pakete)
    }

    /// Suche in Paketbeschreibungen (`SearchDetails`), langsamer als [`Paketquelle::suchen`].
    pub async fn suchen_in_beschreibungen(
        &self,
        text: &str,
        nur_installiert: bool,
    ) -> Result<Vec<Paket>, Fehler> {
        let f = suchfilter(nur_installiert);
        let text = text.to_string();
        self.pakete_sammeln(move |tx| async move { tx.search_details(f, &[text.as_str()]).await })
            .await
    }

    /// Ausführliche Angaben zu mehreren Paketen in einer Transaktion.
    pub async fn details_mehrere(&self, ids: &[PaketId]) -> Result<Vec<Paketdetails>, Fehler> {
        let ids: Vec<String> = ids.to_vec();
        let mut aus = Vec::new();
        self.transaktion(
            move |tx| async move {
                let r: Vec<&str> = ids.iter().map(String::as_str).collect();
                tx.get_details(&r).await
            },
            |e| {
                if let Ereignis::Details(map) = e {
                    aus.push(details_aus_map(&map));
                }
            },
        )
        .await?;
        Ok(aus)
    }

    /// Führt einen schreibenden Vorgang mit den gegebenen Flaggen aus und meldet
    /// Fortschritt über `senden`. Läuft bis `Fertig`/`Fehler`/`Abgebrochen`.
    async fn vorgang_ausfuehren(
        &self,
        vorgang: Vorgang,
        ids: Vec<PaketId>,
        pfad_ablage: Arc<Mutex<Option<OwnedObjectPath>>>,
        abbruch: Arc<AtomicBool>,
        senden: &dyn Fn(Fortschritt),
    ) {
        let mut abbildung = Abbildung::neu(agent::pruefen(self.agentpruefung));
        let ergebnis: Result<(), Fehler> = async {
            let pfad = self.anlegen().await?;
            *pfad_ablage.lock().unwrap_or_else(|e| e.into_inner()) = Some(pfad.clone());
            let mut lauscher = Lauscher::neu(&self.verbindung, &pfad).await?;
            let tx = self.proxy(pfad).await?;
            tx.set_hints(&HINWEISE).await.map_err(fehler_aus_zbus)?;
            if abbruch.load(Ordering::SeqCst) {
                return Err(Fehler::Abgebrochen);
            }
            senden(Fortschritt::Gestartet {
                vorgang: vorgang.clone(),
                pakete: ids.clone(),
            });
            starten(&tx, &vorgang, &ids, flagge::NUR_VERTRAUT).await?;
            loop {
                let e = lauscher.naechstes().await;
                for f in abbildung.abbilden(e) {
                    senden(f);
                }
                if abbildung.ist_fertig() {
                    return Ok(());
                }
            }
        }
        .await;
        if let Err(f) = ergebnis {
            if !abbildung.ist_fertig() {
                senden(match f {
                    Fehler::Abgebrochen => Fortschritt::Abgebrochen,
                    f => Fortschritt::Fehler(f),
                });
            }
        }
    }

    /// Startet eine Transaktion in einem Hintergrund-Thread und liefert den
    /// Fortschritt über einen Kanal – ohne glib, für Werkzeuge ohne Hauptschleife.
    ///
    /// Reihenfolge der Meldungen: `Gestartet`, dann `Status`/`Prozent`/`PaketFortschritt`/
    /// `WartetAufSperre`/`BrauchtNeustart` in Daemon-Reihenfolge, zuletzt genau eine von
    /// `Fertig`, `Abgebrochen`, `Fehler(_)`. Danach schließt der Kanal.
    pub fn ausfuehren_kanal(
        &self,
        vorgang: Vorgang,
        ids: Vec<PaketId>,
    ) -> (FortschrittEmpfaenger, AbbruchGriff) {
        let (tx, rx) = async_channel::unbounded();
        let pfad = Arc::new(Mutex::new(None));
        let abbruch = Arc::new(AtomicBool::new(false));
        let griff = AbbruchGriff {
            verbindung: self.verbindung.clone(),
            pfad: pfad.clone(),
            angefordert: abbruch.clone(),
        };
        let selbst = self.clone();
        std::thread::Builder::new()
            .name("qs-paketkit-transaktion".into())
            .spawn(move || {
                zbus::block_on(
                    selbst.vorgang_ausfuehren(vorgang, ids, pfad, abbruch, &|f| {
                        let _ = tx.try_send(f);
                    }),
                );
            })
            .expect("Thread für PackageKit-Transaktion");
        (FortschrittEmpfaenger { rx }, griff)
    }

    /// Bindet sich an eine bereits laufende Transaktion (Objektpfad aus
    /// [`Paketquelle::laufende_transaktionen`]) und meldet ihren weiteren Fortschritt.
    ///
    /// Die erste Meldung ist `Gestartet` mit dem Vorgang aus der `Role`-Eigenschaft
    /// (Paketliste leer, PackageKit nennt sie nicht), gefolgt vom aktuellen `Status` und
    /// `Prozent`. Abbrechen gelingt nur, wenn die Transaktion von dieser Verbindung stammt
    /// oder PackageKit den Aufrufer dazu ermächtigt (sonst meldet der Daemon einen Fehler,
    /// der ignoriert wird).
    pub fn beobachten_kanal(&self, pfad: &str) -> (FortschrittEmpfaenger, AbbruchGriff) {
        let (tx, rx) = async_channel::unbounded();
        let pfad_ablage = Arc::new(Mutex::new(None));
        let abbruch = Arc::new(AtomicBool::new(false));
        let griff = AbbruchGriff {
            verbindung: self.verbindung.clone(),
            pfad: pfad_ablage.clone(),
            angefordert: abbruch.clone(),
        };
        let selbst = self.clone();
        let pfad = pfad.to_string();
        std::thread::Builder::new()
            .name("qs-paketkit-beobachter".into())
            .spawn(move || {
                zbus::block_on(selbst.beobachten(pfad, pfad_ablage, &|f| {
                    let _ = tx.try_send(f);
                }));
            })
            .expect("Thread für PackageKit-Beobachtung");
        (FortschrittEmpfaenger { rx }, griff)
    }

    async fn beobachten(
        &self,
        pfad: String,
        pfad_ablage: Arc<Mutex<Option<OwnedObjectPath>>>,
        senden: &dyn Fn(Fortschritt),
    ) {
        let mut abbildung = Abbildung::neu(agent::pruefen(self.agentpruefung));
        let ergebnis: Result<(), Fehler> = async {
            let pfad = ObjectPath::try_from(pfad.as_str())
                .map_err(|e| Fehler::Sonstiges {
                    code: "pfad".into(),
                    text: e.to_string(),
                })?
                .into_owned();
            let pfad = OwnedObjectPath::from(pfad);
            *pfad_ablage.lock().unwrap_or_else(|e| e.into_inner()) = Some(pfad.clone());
            let mut lauscher = Lauscher::neu(&self.verbindung, &pfad).await?;
            let tx = self.proxy(pfad).await?;
            let rolle = tx.role().await.map_err(fehler_aus_zbus)?;
            let vorgang = transaktion::vorgang_aus_rolle(rolle).ok_or_else(|| Fehler::Sonstiges {
                code: "rolle".into(),
                text: format!("Transaktion mit Rolle {rolle} ist kein Installations-/Entfernungs-/Aktualisierungsvorgang"),
            })?;
            senden(Fortschritt::Gestartet {
                vorgang,
                pakete: Vec::new(),
            });
            if let Ok(s) = tx.status().await {
                for f in abbildung.abbilden(Ereignis::Status(s)) {
                    senden(f);
                }
            }
            if let Ok(p) = tx.percentage().await {
                for f in abbildung.abbilden(Ereignis::Prozent(p)) {
                    senden(f);
                }
            }
            loop {
                let e = lauscher.naechstes().await;
                for f in abbildung.abbilden(e) {
                    senden(f);
                }
                if abbildung.ist_fertig() {
                    return Ok(());
                }
            }
        }
        .await;
        if let Err(f) = ergebnis {
            if !abbildung.ist_fertig() {
                senden(Fortschritt::Fehler(f));
            }
        }
    }
}

/// Ruft die zum Vorgang passende Methode mit den Flaggen auf.
async fn starten(
    tx: &TransactionProxy<'static>,
    vorgang: &Vorgang,
    ids: &[PaketId],
    flaggen: u64,
) -> Result<(), Fehler> {
    let r: Vec<&str> = ids.iter().map(String::as_str).collect();
    match vorgang {
        Vorgang::Installieren => tx.install_packages(flaggen, &r).await,
        Vorgang::Entfernen => tx.remove_packages(flaggen, &r, false, false).await,
        Vorgang::Aktualisieren => tx.update_packages(flaggen, &r).await,
        Vorgang::QuellenAuffrischen => tx.refresh_cache(false).await,
    }
    .map_err(fehler_aus_zbus)
}

fn suchfilter(nur_installiert: bool) -> u64 {
    let mut f = filter::ARCH | filter::NEUESTE;
    if nur_installiert {
        f |= filter::INSTALLIERT;
    }
    f
}

/// Baut [`Paketdetails`] aus dem `Details`-Signal (`a{sv}`).
pub fn details_aus_map(
    map: &std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
) -> Paketdetails {
    let text = |k: &str| map.get(k).and_then(als_text).unwrap_or_default();
    let zahl = |k: &str| map.get(k).and_then(als_u64);
    let id = text("package-id");
    let zusammenfassung = text("summary");
    let paket = enums::paket_aus_id(&id, &zusammenfassung, 0);
    let groesse = zahl("size")
        .or_else(|| zahl("download-size"))
        .or_else(|| zahl("installed-size"))
        .unwrap_or(0);
    let gruppe = map
        .get("group")
        .and_then(|v| {
            als_u64(v)
                .map(|g| enums::gruppenname(g as u32))
                .or_else(|| als_text(v))
        })
        .unwrap_or_default();
    let url = text("url");
    Paketdetails {
        paket,
        beschreibung: text("description"),
        lizenz: text("license"),
        groesse_bytes: groesse,
        url: if url.is_empty() { None } else { Some(url) },
        gruppe,
    }
}

#[async_trait::async_trait(?Send)]
impl Paketquelle for PaketKit {
    async fn suchen(&self, text: &str, nur_installiert: bool) -> Result<Vec<Paket>, Fehler> {
        let f = suchfilter(nur_installiert);
        let text = text.to_string();
        self.pakete_sammeln(move |tx| async move { tx.search_names(f, &[text.as_str()]).await })
            .await
    }

    async fn paket_nach_name(&self, name: &str) -> Result<Option<Paket>, Fehler> {
        let f = filter::ARCH | filter::NEUESTE;
        let name = name.to_string();
        let mut pakete = self
            .pakete_sammeln(move |tx| async move { tx.resolve(f, &[name.as_str()]).await })
            .await?;
        if let Some(pos) = pakete.iter().position(|p| p.installiert) {
            return Ok(Some(pakete.swap_remove(pos)));
        }
        Ok(pakete.into_iter().next())
    }

    async fn details(&self, id: &PaketId) -> Result<Paketdetails, Fehler> {
        let mut alle = self.details_mehrere(std::slice::from_ref(id)).await?;
        if alle.is_empty() {
            return Err(Fehler::PaketNichtGefunden(id.clone()));
        }
        Ok(alle.swap_remove(0))
    }

    async fn aktualisierungen(&self) -> Result<Vec<Aktualisierung>, Fehler> {
        let mut roh: Vec<(u32, Paket)> = Vec::new();
        self.transaktion(
            |tx| async move { tx.get_updates(filter::KEINER).await },
            |e| {
                if let Ereignis::Paket {
                    info,
                    id,
                    zusammenfassung,
                } = e
                {
                    roh.push((info, enums::paket_aus_id(&id, &zusammenfassung, info)));
                }
            },
        )
        .await?;
        if roh.is_empty() {
            return Ok(Vec::new());
        }
        // Neustart-Hinweise nachladen; wenn das Backend es nicht kann, bleibt es bei `info`.
        let ids: Vec<String> = roh.iter().map(|(_, p)| p.id.clone()).collect();
        let mut neustarts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let _ = self
            .transaktion(
                move |tx| async move {
                    let r: Vec<&str> = ids.iter().map(String::as_str).collect();
                    tx.get_update_detail(&r).await
                },
                |e| {
                    if let Ereignis::UpdateDetail { id, neustart, .. } = e {
                        neustarts.insert(id, neustart);
                    }
                },
            )
            .await;
        Ok(roh
            .into_iter()
            .map(|(info, paket)| {
                let restart = neustarts.get(&paket.id).copied().unwrap_or(0);
                let mut dringlichkeit = enums::dringlichkeit_aus_info(info);
                if matches!(
                    restart,
                    enums::neustart::SICHERHEIT_SITZUNG | enums::neustart::SICHERHEIT_SYSTEM
                ) {
                    dringlichkeit = qs_typen::Dringlichkeit::Sicherheit;
                }
                Aktualisierung {
                    paket,
                    dringlichkeit,
                    neustart_noetig: enums::neustart_noetig(restart),
                }
            })
            .collect())
    }

    fn ausfuehren(
        &self,
        vorgang: Vorgang,
        ids: Vec<PaketId>,
        melden: Box<dyn Fn(Fortschritt)>,
    ) -> Box<dyn Abbruch> {
        let (empfaenger, griff) = self.ausfuehren_kanal(vorgang, ids);
        let kontext =
            glib::MainContext::thread_default().unwrap_or_else(glib::MainContext::default);
        kontext.spawn_local(async move {
            while let Some(f) = empfaenger.naechster().await {
                melden(f);
            }
        });
        Box::new(griff)
    }

    async fn simulieren(&self, vorgang: Vorgang, ids: Vec<PaketId>) -> Result<Simulation, Fehler> {
        if vorgang == Vorgang::QuellenAuffrischen {
            return Ok(Simulation::default());
        }
        let mut sim = Simulation::default();
        let angefragt = ids.clone();
        let vorgang2 = vorgang.clone();
        self.transaktion(
            move |tx| async move {
                starten(
                    &tx,
                    &vorgang2,
                    &ids,
                    flagge::NUR_VERTRAUT | flagge::SIMULIEREN,
                )
                .await
                .map_err(|f| match f {
                    Fehler::Sonstiges { code, text } => {
                        zbus::Error::Failure(format!("{code}: {text}"))
                    }
                    f => zbus::Error::Failure(f.to_string()),
                })
            },
            |e| match e {
                Ereignis::Paket {
                    info,
                    id,
                    zusammenfassung,
                } => {
                    if angefragt
                        .iter()
                        .any(|a| enums::gleiche_paketversion(a, &id))
                    {
                        return;
                    }
                    let p = enums::paket_aus_id(&id, &zusammenfassung, info);
                    match info {
                        info::INSTALLIERT_WIRD | info::NEU_INSTALLIERT => sim.installieren.push(p),
                        info::ENTFERNT_WIRD | info::ERSETZT_WIRD | info::AUFRAEUMEN => {
                            sim.entfernen.push(p)
                        }
                        info::AKTUALISIERT | info::ZURUECKGESTUFT => sim.aktualisieren.push(p),
                        _ => {}
                    }
                }
                Ereignis::DownloadBytes(b) => sim.download_bytes = sim.download_bytes.max(b),
                _ => {}
            },
        )
        .await?;
        Ok(sim)
    }

    async fn laufende_transaktionen(&self) -> Result<Vec<String>, Fehler> {
        let pk = PackageKitProxy::new(&self.verbindung)
            .await
            .map_err(fehler_aus_zbus)?;
        Ok(pk
            .get_transaction_list()
            .await
            .map_err(fehler_aus_zbus)?
            .into_iter()
            .map(|p| p.to_string())
            .collect())
    }
}
