//! Prüfbus für Integrationstests: eigener dbus-daemon, optional polkit-Attrappe, dann
//! `packagekitd --backend=test_*` oder die PackageKit-Attrappe. Alles wird beim Drop
//! beendet und der Ordner entfernt. Nie der echte Systembus.
#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use qs_paketkit::{Agentpruefung, PaketKit};

pub const PKD: &str = "/usr/libexec/packagekitd";
static ZAEHLER: AtomicU32 = AtomicU32::new(0);

pub struct Pruefbus {
    pub ordner: PathBuf,
    pub adresse: String,
    bus: Child,
    polkit: Option<Child>,
    daemon: Option<Child>,
}

/// `Some(grund)`, wenn die Werkzeuge fehlen → Test meldet BLOCKED.
pub fn blockiert(backend: &str) -> Option<String> {
    if Command::new("dbus-daemon")
        .arg("--version")
        .output()
        .is_err()
    {
        return Some("dbus-daemon fehlt".into());
    }
    if backend != "attrappe" && !std::path::Path::new(PKD).exists() {
        return Some(format!("{PKD} fehlt"));
    }
    None
}

impl Pruefbus {
    /// `backend`: `attrappe` oder `test_*`; `polkit`: `None` = keine polkit-Attrappe.
    pub fn neu(backend: &str, polkit: Option<&str>) -> Pruefbus {
        assert!(
            backend == "attrappe" || backend.starts_with("test_"),
            "nur Test-Backends"
        );
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let ordner = PathBuf::from(format!("/tmp/qs-pk-t{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let konf = ordner.join("bus.conf");
        std::fs::write(
            &konf,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>system</type><keep_umask/><listen>unix:path={}/sock</listen><auth>EXTERNAL</auth>
<policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/></policy></busconfig>"#,
                ordner.display()
            ),
        )
        .unwrap();
        let mut bus = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", konf.display()))
            .arg("--nofork")
            .arg("--print-address=1")
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon");
        let mut zeile = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut zeile)
            .unwrap();
        let adresse = zeile.trim().split(",guid=").next().unwrap().to_string();
        let mut pb = Pruefbus {
            ordner,
            adresse,
            bus,
            polkit: None,
            daemon: None,
        };
        if let Some(modus) = polkit {
            pb.polkit = Some(
                Command::new(env!("CARGO_BIN_EXE_qs-pk-attrappe"))
                    .args(["polkit", "--adresse", &pb.adresse, "--modus", modus])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            pb.warte_auf("org.freedesktop.PolicyKit1");
        }
        pb.daemon_starten(backend, polkit == Some("verweigern"));
        pb
    }

    pub fn daemon_starten(&mut self, backend: &str, verweigern: bool) {
        let log = std::fs::File::create(self.ordner.join("daemon.log")).unwrap();
        let kind = if backend == "attrappe" {
            Command::new(env!("CARGO_BIN_EXE_qs-pk-attrappe"))
                .args(["paketkit", "--adresse", &self.adresse])
                .env(
                    "QS_ATTRAPPE_POLKIT",
                    if verweigern { "verweigern" } else { "erlauben" },
                )
                .stdout(Stdio::null())
                .stderr(log)
                .spawn()
                .unwrap()
        } else {
            Command::new(PKD)
                .arg(format!("--backend={backend}"))
                .args(["--disable-timer", "--keep-environment"])
                .env("DBUS_SYSTEM_BUS_ADDRESS", &self.adresse)
                .stdout(Stdio::null())
                .stderr(log)
                .spawn()
                .unwrap()
        };
        self.daemon = Some(kind);
        self.warte_auf("org.freedesktop.PackageKit");
    }

    /// Beendet den Daemon hart (SIGKILL) – simuliert Absturz/Neustart.
    pub fn daemon_beenden(&mut self) {
        if let Some(mut d) = self.daemon.take() {
            let _ = d.kill();
            let _ = d.wait();
        }
    }

    fn warte_auf(&self, name: &str) {
        let ende = Instant::now() + Duration::from_secs(10);
        while Instant::now() < ende {
            let ok = Command::new("busctl")
                .arg(format!("--address={}", self.adresse))
                .args(["status", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("{name} erscheint nicht auf dem Prüfbus {}", self.adresse);
    }

    /// Bibliothek, verbunden mit dem Prüfbus.
    pub fn paketkit(&self, agent: bool) -> PaketKit {
        let pk =
            zbus::block_on(PaketKit::mit_adresse(&self.adresse)).expect("Verbindung zum Prüfbus");
        // Schutz: Schreibtests nur gegen Test-Backends.
        let name = zbus::block_on(pk.backend_name()).unwrap();
        assert!(
            name == "attrappe" || name.starts_with("test_"),
            "unerwartetes Backend {name}"
        );
        pk.mit_agentpruefung(Agentpruefung::Fest(agent))
    }

    pub fn daemon_log(&self) -> String {
        std::fs::read_to_string(self.ordner.join("daemon.log")).unwrap_or_default()
    }
}

impl Drop for Pruefbus {
    fn drop(&mut self) {
        for k in [
            self.daemon.as_mut(),
            self.polkit.as_mut(),
            Some(&mut self.bus),
        ]
        .into_iter()
        .flatten()
        {
            let _ = k.kill();
            let _ = k.wait();
        }
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Führt `ausfuehren` über die Trait-Methode mit eigenem glib-Kontext aus und sammelt
/// alle Meldungen; prüft, dass `melden` auf dem Aufrufer-Thread läuft.
pub fn ausfuehren_glib(
    pk: &PaketKit,
    vorgang: qs_typen::Vorgang,
    ids: Vec<String>,
    abbrechen_nach: Option<Duration>,
    frist: Duration,
) -> Vec<qs_typen::Fortschritt> {
    use qs_typen::{Fortschritt, Paketquelle};
    use std::cell::RefCell;
    use std::rc::Rc;
    let kontext = glib::MainContext::new();
    kontext
        .with_thread_default(|| {
            let gesammelt: Rc<RefCell<Vec<Fortschritt>>> = Rc::default();
            let g2 = gesammelt.clone();
            let faden = std::thread::current().id();
            let griff = pk.ausfuehren(
                vorgang,
                ids,
                Box::new(move |f| {
                    assert_eq!(
                        std::thread::current().id(),
                        faden,
                        "melden auf fremdem Thread"
                    );
                    g2.borrow_mut().push(f);
                }),
            );
            let start = Instant::now();
            let mut abgebrochen = false;
            loop {
                while kontext.iteration(false) {}
                if let Some(nach) = abbrechen_nach {
                    if !abgebrochen && start.elapsed() >= nach {
                        griff.abbrechen();
                        abgebrochen = true;
                    }
                }
                let fertig = gesammelt.borrow().last().is_some_and(|f| {
                    matches!(
                        f,
                        Fortschritt::Fertig | Fortschritt::Abgebrochen | Fortschritt::Fehler(_)
                    )
                });
                if fertig || start.elapsed() > frist {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let v = gesammelt.borrow().clone();
            v
        })
        .expect("glib-Kontext")
}

/// Prozentwerte aus einer Meldungsliste.
pub fn prozente(m: &[qs_typen::Fortschritt]) -> Vec<u8> {
    m.iter()
        .filter_map(|f| match f {
            qs_typen::Fortschritt::Prozent(p) => Some(*p),
            _ => None,
        })
        .collect()
}
