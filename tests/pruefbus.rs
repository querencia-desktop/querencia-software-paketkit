//! Integrationstests gegen den privaten Prüfbus (packagekitd mit Test-Backend bzw.
//! PackageKit-Attrappe). Kein echter Systembus, keine echten Pakete.

mod gemeinsam;

use std::time::Duration;

use gemeinsam::{ausfuehren_glib, blockiert, prozente, Pruefbus};
use qs_typen::{Dringlichkeit, Fehler, Fortschritt, Paketquelle, Vorgang};

macro_rules! oder_blockiert {
    ($backend:expr) => {
        if let Some(grund) = blockiert($backend) {
            eprintln!("BLOCKED: {grund}");
            return;
        }
    };
}

fn ende(m: &[Fortschritt]) -> &Fortschritt {
    m.last().expect("keine Meldungen")
}

// ------------------------------------------------------- echter packagekitd, Test-Backends

#[test]
fn echt_test_thread_suche() {
    oder_blockiert!("test_thread");
    let pb = Pruefbus::neu("test_thread", None);
    let pk = pb.paketkit(true);
    let treffer = zbus::block_on(pk.suchen("power", false)).unwrap();
    let namen: Vec<_> = treffer.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(namen, ["glib2", "gtk2"]);
    assert!(treffer.iter().all(|p| p.installiert));
    assert_eq!(treffer[0].id, "glib2;2.14.0;i386;fedora");
    assert_eq!(treffer[0].arch, "i386");
}

#[test]
fn echt_test_thread_abbruch_und_laufende_liste() {
    // test_thread sucht ~1 s mit Percentage 0,10,20,…; Cancel → ErrorCode
    // transaction-cancelled + Finished. Abbildung → Fortschritt::Abgebrochen.
    oder_blockiert!("test_thread");
    use qs_paketkit::proxies::{PackageKitProxy, TransactionProxy};
    use qs_paketkit::transaktion::{Abbildung, Lauscher};
    let pb = Pruefbus::neu("test_thread", None);
    let pk = pb.paketkit(true);
    let conn = pk.verbindung().clone();
    let meldungen = zbus::block_on(async {
        let daemon = PackageKitProxy::new(&conn).await.unwrap();
        let pfad = daemon.create_transaction().await.unwrap();
        let mut l = Lauscher::neu(&conn, &pfad).await.unwrap();
        let tx = TransactionProxy::builder(&conn)
            .path(pfad.clone())
            .unwrap()
            .build()
            .await
            .unwrap();
        tx.set_hints(&qs_paketkit::HINWEISE).await.unwrap();
        tx.search_names(0, &["power"]).await.unwrap();
        let laufend = pk.laufende_transaktionen().await.unwrap();
        assert!(
            laufend.contains(&pfad.to_string()),
            "{laufend:?} enthält {pfad}"
        );
        let mut a = Abbildung::neu(true);
        let mut aus = Vec::new();
        let mut abgebrochen = false;
        while !a.ist_fertig() {
            let e = l.naechstes().await;
            aus.extend(a.abbilden(e));
            if !abgebrochen && prozente(&aus).iter().any(|p| *p >= 10 && *p <= 100) {
                tx.cancel().await.unwrap();
                abgebrochen = true;
            }
        }
        aus
    });
    assert_eq!(ende(&meldungen), &Fortschritt::Abgebrochen, "{meldungen:?}");
    assert!(
        meldungen.contains(&Fortschritt::Status("Breche ab …".into())),
        "{meldungen:?}"
    );
}

#[test]
fn echt_test_fail_suche_meldet_fehler() {
    oder_blockiert!("test_fail");
    let pb = Pruefbus::neu("test_fail", None);
    let pk = pb.paketkit(true);
    let f = zbus::block_on(pk.suchen("power", false)).unwrap_err();
    assert_eq!(
        f,
        Fehler::Sonstiges {
            code: "internal-error".into(),
            text: "Error number 1".into()
        }
    );
}

#[test]
fn echt_ohne_polkit_kein_agent_bzw_nicht_berechtigt() {
    // Ohne org.freedesktop.PolicyKit1 auf dem Bus meldet packagekitd not-authorized.
    oder_blockiert!("test_fail");
    let pb = Pruefbus::neu("test_fail", None);
    let m = ausfuehren_glib(
        &pb.paketkit(false),
        Vorgang::Installieren,
        vec!["powertop;1.8-1.fc8;i386;fedora".into()],
        None,
        Duration::from_secs(10),
    );
    assert!(
        matches!(
            m.first(),
            Some(Fortschritt::Gestartet {
                vorgang: Vorgang::Installieren,
                ..
            })
        ),
        "{m:?}"
    );
    assert_eq!(ende(&m), &Fortschritt::Fehler(Fehler::KeinAgent), "{m:?}");
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Installieren,
        vec!["powertop;1.8-1.fc8;i386;fedora".into()],
        None,
        Duration::from_secs(10),
    );
    assert_eq!(
        ende(&m),
        &Fortschritt::Fehler(Fehler::NichtBerechtigt),
        "{m:?}"
    );
}

#[test]
fn echt_polkit_verweigert() {
    oder_blockiert!("test_fail");
    let pb = Pruefbus::neu("test_fail", Some("verweigern"));
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Entfernen,
        vec!["powertop;1.8-1.fc8;i386;fedora".into()],
        None,
        Duration::from_secs(10),
    );
    assert_eq!(
        ende(&m),
        &Fortschritt::Fehler(Fehler::NichtBerechtigt),
        "{m:?}"
    );
}

#[test]
fn echt_mit_polkit_install_simulation_entfernen() {
    // test_fail beendet Install/Remove/Update nach erfolgreicher polkit-Prüfung sofort
    // mit Finished(success) ohne Fortschritt – prüft Weg, Hinweise und Flaggen.
    oder_blockiert!("test_fail");
    let pb = Pruefbus::neu("test_fail", Some("erlauben"));
    let pk = pb.paketkit(true);
    let id = "powertop;1.8-1.fc8;i386;fedora".to_string();
    for v in [
        Vorgang::Installieren,
        Vorgang::Entfernen,
        Vorgang::Aktualisieren,
    ] {
        let m = ausfuehren_glib(
            &pk,
            v.clone(),
            vec![id.clone()],
            None,
            Duration::from_secs(10),
        );
        assert_eq!(ende(&m), &Fortschritt::Fertig, "{v:?}: {m:?}");
        assert!(
            m.contains(&Fortschritt::Status("Warte auf Berechtigung …".into())),
            "{m:?}"
        );
    }
    let sim = zbus::block_on(pk.simulieren(Vorgang::Installieren, vec![id.clone()])).unwrap();
    assert_eq!(sim, qs_typen::Simulation::default());
}

#[test]
fn echt_test_succeed_haengt_frist_greift() {
    // PackageKit 1.3.4: test_succeed ruft pk_backend_job_finished mit falschem Zeiger auf
    // (GLib-CRITICAL im Log); die Transaktion endet nie. Die Frist muss greifen.
    oder_blockiert!("test_succeed");
    let pb = Pruefbus::neu("test_succeed", None);
    let pk = pb.paketkit(true).mit_frist(Some(Duration::from_secs(2)));
    let start = std::time::Instant::now();
    let r = zbus::block_on(pk.suchen("power", false));
    assert!(
        matches!(&r, Err(Fehler::Sonstiges { code, .. }) if code == qs_paketkit::CODE_ZEIT),
        "{r:?} Log: {}",
        pb.daemon_log()
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(
        pb.daemon_log().contains("PK_IS_BACKEND_JOB"),
        "Defekt sichtbar im Log"
    );
}

#[test]
fn echt_daemon_beendet_waehrend_suche() {
    oder_blockiert!("test_thread");
    let mut pb = Pruefbus::neu("test_thread", None);
    let pk = pb.paketkit(true);
    let suche = std::thread::spawn(move || zbus::block_on(pk.suchen("power", false)));
    std::thread::sleep(Duration::from_millis(300));
    pb.daemon_beenden();
    assert_eq!(suche.join().unwrap(), Err(Fehler::DaemonWeg));
}

#[test]
fn echt_test_spawn_hilfsskript_fehlt_blocked() {
    // Dokumentiert den Defekt (BLOCKED für Funktionsprüfungen mit test_spawn):
    // packagekitd sucht /usr/share/PackageKit/helpers/test/search-name.sh, das Paket
    // installiert es unter helpers/test_spawn/. Ohne root nicht behebbar.
    oder_blockiert!("test_spawn");
    let pb = Pruefbus::neu("test_spawn", None);
    let f = zbus::block_on(pb.paketkit(true).suchen("power", false)).unwrap_err();
    match f {
        Fehler::Sonstiges { code, text } => {
            assert_eq!(code, "internal-error");
            assert!(text.contains("helpers/test/search-name.sh"), "{text}");
        }
        f => panic!("unerwartet: {f:?}"),
    }
}

// ------------------------------------------------------- PackageKit-Attrappe

#[test]
fn attrappe_suche_filter_und_name() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let alle = zbus::block_on(pk.suchen("querencia", false)).unwrap();
    assert_eq!(alle.len(), 4);
    let inst = zbus::block_on(pk.suchen("querencia", true)).unwrap();
    assert_eq!(inst.len(), 1);
    assert!(inst[0].installiert && inst[0].name == "querencia-bilder");
    let p = zbus::block_on(pk.paket_nach_name("querencia-bilder"))
        .unwrap()
        .unwrap();
    assert!(p.installiert, "installierte Version bevorzugt");
    assert_eq!(p.version, "0.3.0-1");
    assert_eq!(
        zbus::block_on(pk.paket_nach_name("gibtsnicht")).unwrap(),
        None
    );
    let d = zbus::block_on(pk.suchen_in_beschreibungen("stromverbrauch", false)).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].name, "powertop");
}

#[test]
fn attrappe_details() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let d = zbus::block_on(pk.details(&"gedit;48.1-1;amd64;attrappe".to_string())).unwrap();
    assert_eq!(d.paket.name, "gedit");
    assert_eq!(d.lizenz, "GPL-3.0-or-later");
    assert_eq!(d.groesse_bytes, 1_374_208);
    assert_eq!(d.gruppe, "desktop-gnome");
    assert_eq!(d.url.as_deref(), Some("https://querencia.example/gedit"));
    assert!(d.beschreibung.contains("Attrappe"));
    let f = zbus::block_on(pk.details(&"nix;1;amd64;x".to_string())).unwrap_err();
    assert!(matches!(f, Fehler::PaketNichtGefunden(_)), "{f:?}");
}

#[test]
fn attrappe_aktualisierungen_dringlichkeit_neustart() {
    let pb = Pruefbus::neu("attrappe", None);
    let a = zbus::block_on(pb.paketkit(true).aktualisierungen()).unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a[0].paket.name, "libssl3t64");
    assert_eq!(a[0].dringlichkeit, Dringlichkeit::Sicherheit);
    assert!(a[0].neustart_noetig);
    assert_eq!(a[1].dringlichkeit, Dringlichkeit::Normal);
    assert!(!a[1].neustart_noetig);
    assert_eq!(a[2].dringlichkeit, Dringlichkeit::Fehlerbehebung);
    assert!(!a[2].neustart_noetig, "Neustart der Anwendung zählt nicht");
}

#[test]
fn attrappe_install_fortschritt_0_bis_100() {
    let pb = Pruefbus::neu("attrappe", None);
    let id = "querencia-editor;0.2.0-1;amd64;attrappe".to_string();
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Installieren,
        vec![id.clone()],
        None,
        Duration::from_secs(20),
    );
    assert_eq!(
        m[0],
        Fortschritt::Gestartet {
            vorgang: Vorgang::Installieren,
            pakete: vec![id.clone()]
        }
    );
    assert_eq!(ende(&m), &Fortschritt::Fertig, "{m:?}");
    let p = prozente(&m);
    assert_eq!(p.first(), Some(&0));
    assert_eq!(p.last(), Some(&100));
    assert!(p.windows(2).all(|w| w[0] <= w[1]), "monoton: {p:?}");
    for s in [
        "Lade herunter …",
        "Installiere …",
        "Löse Abhängigkeiten auf …",
    ] {
        assert!(m.contains(&Fortschritt::Status(s.into())), "{s} fehlt");
    }
    assert!(m
        .iter()
        .any(|f| matches!(f, Fortschritt::PaketFortschritt { id: i, .. } if *i == id)));
    assert_eq!(
        m.iter()
            .filter(|f| matches!(f, Fortschritt::Fertig))
            .count(),
        1
    );
}

#[test]
fn attrappe_entfernen_aktualisieren_auffrischen() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let m = ausfuehren_glib(
        &pk,
        Vorgang::Entfernen,
        vec!["powertop;2.15-1;amd64;installed".into()],
        None,
        Duration::from_secs(20),
    );
    assert_eq!(
        ende(&m),
        &Fortschritt::Fertig,
        "Entfernen mit allow_deps=false/autoremove=false: {m:?}"
    );
    assert!(m.contains(&Fortschritt::Status("Entferne …".into())));
    let m = ausfuehren_glib(
        &pk,
        Vorgang::Aktualisieren,
        vec!["libssl3t64;3.5.0-2;amd64;attrappe-security".into()],
        None,
        Duration::from_secs(20),
    );
    assert_eq!(ende(&m), &Fortschritt::Fertig, "{m:?}");
    assert!(m.contains(&Fortschritt::BrauchtNeustart), "{m:?}");
    let m = ausfuehren_glib(
        &pk,
        Vorgang::QuellenAuffrischen,
        vec![],
        None,
        Duration::from_secs(20),
    );
    assert_eq!(ende(&m), &Fortschritt::Fertig, "{m:?}");
    assert!(m.contains(&Fortschritt::Status("Aktualisiere Paketquellen …".into())));
}

#[test]
fn attrappe_abbrechen() {
    let pb = Pruefbus::neu("attrappe", None);
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Installieren,
        vec!["langsam;1.0-1;amd64;attrappe".into()],
        Some(Duration::from_millis(800)),
        Duration::from_secs(20),
    );
    assert_eq!(ende(&m), &Fortschritt::Abgebrochen, "{m:?}");
    assert!(prozente(&m).last().copied().unwrap_or(0) < 100);
}

#[test]
fn attrappe_abbrechen_sofort() {
    // abbrechen() direkt nach ausfuehren(), bevor die Transaktion angelegt ist.
    let pb = Pruefbus::neu("attrappe", None);
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Installieren,
        vec!["langsam;1.0-1;amd64;attrappe".into()],
        Some(Duration::ZERO),
        Duration::from_secs(20),
    );
    assert_eq!(ende(&m), &Fortschritt::Abgebrochen, "{m:?}");
}

#[test]
fn attrappe_sperre_belegt() {
    let pb = Pruefbus::neu("attrappe", None);
    let m = ausfuehren_glib(
        &pb.paketkit(true),
        Vorgang::Installieren,
        vec!["gesperrt;1.0-1;amd64;attrappe".into()],
        None,
        Duration::from_secs(20),
    );
    assert!(m.contains(&Fortschritt::WartetAufSperre), "{m:?}");
    assert!(
        matches!(ende(&m), Fortschritt::Fehler(Fehler::SperreBelegt(t)) if t.contains("apt")),
        "{m:?}"
    );
}

#[test]
fn attrappe_abhaengigkeiten_und_neustart() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let m = ausfuehren_glib(
        &pk,
        Vorgang::Installieren,
        vec!["kaputt;1.0-1;amd64;attrappe".into()],
        None,
        Duration::from_secs(20),
    );
    assert!(
        matches!(
            ende(&m),
            Fortschritt::Fehler(Fehler::AbhaengigkeitenKonflikt(_))
        ),
        "{m:?}"
    );
    let m = ausfuehren_glib(
        &pk,
        Vorgang::Installieren,
        vec!["neustart;1.0-1;amd64;attrappe".into()],
        None,
        Duration::from_secs(20),
    );
    assert!(m.contains(&Fortschritt::BrauchtNeustart));
    assert_eq!(ende(&m), &Fortschritt::Fertig);
    let m = ausfuehren_glib(
        &pk,
        Vorgang::Installieren,
        vec!["nix;1;amd64;x".into()],
        None,
        Duration::from_secs(20),
    );
    assert!(
        matches!(ende(&m), Fortschritt::Fehler(Fehler::PaketNichtGefunden(_))),
        "{m:?}"
    );
}

#[test]
fn attrappe_polkit_verweigert() {
    let pb = Pruefbus::neu("attrappe", Some("verweigern"));
    let m = ausfuehren_glib(
        &pb.paketkit(false),
        Vorgang::Installieren,
        vec!["gedit;48.1-1;amd64;attrappe".into()],
        None,
        Duration::from_secs(20),
    );
    assert_eq!(ende(&m), &Fortschritt::Fehler(Fehler::KeinAgent), "{m:?}");
}

#[test]
fn attrappe_simulation() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let s = zbus::block_on(pk.simulieren(
        Vorgang::Installieren,
        vec!["querencia-editor;0.2.0-1;amd64;attrappe".into()],
    ))
    .unwrap();
    let namen: Vec<_> = s.installieren.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(namen, ["libquerencia-gemein"], "nur zusätzliche Pakete");
    assert!(s.entfernen.is_empty() && s.aktualisieren.is_empty());
    assert_eq!(s.download_bytes, 1_500_000);
    let s = zbus::block_on(pk.simulieren(
        Vorgang::Entfernen,
        vec!["powertop;2.15-1;amd64;installed".into()],
    ))
    .unwrap();
    assert_eq!(s, qs_typen::Simulation::default());
    let f = zbus::block_on(pk.simulieren(
        Vorgang::Entfernen,
        vec!["libquerencia-gemein;0.1.0-1;amd64;attrappe".into()],
    ))
    .unwrap_err();
    assert!(matches!(f, Fehler::AbhaengigkeitenKonflikt(t) if t.contains("allow_deps=false")));
    let s = zbus::block_on(pk.simulieren(
        Vorgang::Aktualisieren,
        vec!["libssl3t64;3.5.0-2;amd64;attrappe-security".into()],
    ))
    .unwrap();
    assert_eq!(s.installieren.len(), 1);
    assert!(s.aktualisieren.is_empty());
    assert_eq!(
        zbus::block_on(pk.simulieren(Vorgang::QuellenAuffrischen, vec![])).unwrap(),
        qs_typen::Simulation::default()
    );
}

#[test]
fn attrappe_daemon_weg_waehrend_installation() {
    let mut pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let (rx, _griff) = pk.ausfuehren_kanal(
        Vorgang::Installieren,
        vec!["langsam;1.0-1;amd64;attrappe".into()],
    );
    // Warten, bis Fortschritt kommt, dann Daemon hart beenden.
    loop {
        match rx.naechster_blockierend() {
            Some(Fortschritt::Prozent(p)) if p >= 10 => break,
            Some(_) => {}
            None => panic!("Kanal vorzeitig zu"),
        }
    }
    pb.daemon_beenden();
    let rest = rx.alle_blockierend(Duration::from_secs(10));
    assert_eq!(
        rest.last(),
        Some(&Fortschritt::Fehler(Fehler::DaemonWeg)),
        "{rest:?}"
    );
    // Nach Neustart des Daemons arbeitet dieselbe Verbindung weiter.
    pb.daemon_starten("attrappe", false);
    assert_eq!(zbus::block_on(pk.suchen("gedit", false)).unwrap().len(), 1);
}

#[test]
fn attrappe_wiederanbindung() {
    let pb = Pruefbus::neu("attrappe", None);
    let pk = pb.paketkit(true);
    let (rx, griff) = pk.ausfuehren_kanal(
        Vorgang::Installieren,
        vec!["langsam;1.0-1;amd64;attrappe".into()],
    );
    while !matches!(
        rx.naechster_blockierend(),
        Some(Fortschritt::Prozent(_)) | None
    ) {}
    let laufend = zbus::block_on(pk.laufende_transaktionen()).unwrap();
    assert_eq!(laufend.len(), 1);
    assert_eq!(Some(laufend[0].clone()), griff.pfad());
    // Zweite Verbindung (wie nach App-Neustart) bindet sich an.
    let pk2 = pb.paketkit(true);
    let (rx2, _g2) = pk2.beobachten_kanal(&laufend[0]);
    let m = rx2.alle_blockierend(Duration::from_secs(15));
    assert_eq!(
        m[0],
        Fortschritt::Gestartet {
            vorgang: Vorgang::Installieren,
            pakete: vec![]
        }
    );
    assert_eq!(m.last(), Some(&Fortschritt::Fertig), "{m:?}");
    assert_eq!(prozente(&m).last(), Some(&100));
    assert!(zbus::block_on(pk.laufende_transaktionen())
        .unwrap()
        .is_empty());
}

#[test]
fn attrappe_umgebungsvariable_qs_pk_bus() {
    let pb = Pruefbus::neu("attrappe", None);
    // Nur in diesem Test gesetzt; andere Tests nutzen mit_adresse.
    std::env::set_var(qs_paketkit::UMGEBUNG_BUS, &pb.adresse);
    let pk = qs_paketkit::PaketKit::neu_blockierend().unwrap();
    std::env::remove_var(qs_paketkit::UMGEBUNG_BUS);
    assert_eq!(zbus::block_on(pk.backend_name()).unwrap(), "attrappe");
}
