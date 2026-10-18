//! Lesende Integrationstests gegen den ECHTEN PackageKit auf dem Systembus.
//! `#[ignore]`: nur über `pruefgeruest/echt-lesend.sh` (bzw. `--ignored`).
//! Ausschließlich SearchNames, Resolve, GetDetails, GetUpdates(+Detail), GetTransactionList.

use qs_paketkit::PaketKit;
use qs_typen::Paketquelle;

fn pk() -> PaketKit {
    assert!(
        std::env::var(qs_paketkit::UMGEBUNG_BUS).is_err(),
        "QS_PK_BUS muss leer sein"
    );
    PaketKit::neu_blockierend().expect("Systembus/PackageKit")
}

#[test]
#[ignore = "echter Daemon, nur lesend: pruefgeruest/echt-lesend.sh"]
fn echt_suchen_querencia() {
    let pk = pk();
    // Querencia-Pakete sind (noch) nicht in den Ubuntu-Quellen: leere Liste ist korrekt.
    let t = zbus::block_on(pk.suchen("querencia", false)).unwrap();
    println!("querencia: {} Treffer", t.len());
    let t = zbus::block_on(pk.suchen("gedit", false)).unwrap();
    assert!(t.iter().any(|p| p.name == "gedit"), "{t:?}");
    let inst = zbus::block_on(pk.suchen("bash", true)).unwrap();
    assert!(
        inst.iter().any(|p| p.name == "bash" && p.installiert),
        "{inst:?}"
    );
}

#[test]
#[ignore = "echter Daemon, nur lesend: pruefgeruest/echt-lesend.sh"]
fn echt_details() {
    let pk = pk();
    let p = zbus::block_on(pk.paket_nach_name("bash"))
        .unwrap()
        .expect("bash");
    assert!(p.installiert, "installierte Version bevorzugt: {p:?}");
    let d = zbus::block_on(pk.details(&p.id)).unwrap();
    assert_eq!(d.paket.name, "bash");
    assert!(!d.beschreibung.is_empty());
    assert!(d.groesse_bytes > 0);
    println!("{d:#?}");
}

#[test]
#[ignore = "echter Daemon, nur lesend: pruefgeruest/echt-lesend.sh"]
fn echt_aktualisierungen_und_laufend() {
    let pk = pk();
    let a = zbus::block_on(pk.aktualisierungen()).unwrap();
    println!("{} Aktualisierungen", a.len());
    for x in &a {
        println!(
            "  {} {:?} neustart={}",
            x.paket.id, x.dringlichkeit, x.neustart_noetig
        );
    }
    let l = zbus::block_on(pk.laufende_transaktionen()).unwrap();
    println!("laufend: {l:?}");
}
