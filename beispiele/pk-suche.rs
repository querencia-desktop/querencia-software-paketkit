//! Beispiel: Suche und Details von der Kommandozeile – zum manuellen Nachprüfen.
//!
//! ```text
//! cargo run --offline --example pk-suche -- querencia          # SearchNames
//! cargo run --offline --example pk-suche -- --installiert gtk  # nur installierte
//! cargo run --offline --example pk-suche -- --details gedit    # Resolve + GetDetails
//! cargo run --offline --example pk-suche -- --aktualisierungen # GetUpdates
//! cargo run --offline --example pk-suche -- --laufend          # GetTransactionList
//! QS_PK_BUS=unix:path=/tmp/qs-pk-$USER/sock cargo run --offline --example pk-suche -- power
//! ```
//! Alle Aufrufe sind lesend. Es wird nichts installiert, entfernt oder aktualisiert.

use qs_paketkit::PaketKit;
use qs_typen::Paketquelle;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("Nutzung: pk-suche [--installiert] TEXT | --details NAME | --aktualisierungen | --laufend");
        std::process::exit(2);
    }
    let pk = match PaketKit::neu_blockierend() {
        Ok(pk) => pk,
        Err(f) => {
            eprintln!("Keine Verbindung zu PackageKit: {f}");
            std::process::exit(1);
        }
    };
    let ergebnis = zbus::block_on(async {
        println!(
            "Backend: {}",
            pk.backend_name()
                .await
                .unwrap_or_else(|f| format!("? ({f})"))
        );
        match args[0].as_str() {
            "--details" => {
                let name = args.get(1).cloned().unwrap_or_default();
                let Some(p) = pk.paket_nach_name(&name).await? else {
                    println!("Kein Paket „{name}“.");
                    return Ok(());
                };
                let d = pk.details(&p.id).await?;
                println!(
                    "{}\n  Version: {}  Arch: {}  {}",
                    d.paket.name,
                    d.paket.version,
                    d.paket.arch,
                    if d.paket.installiert {
                        "installiert"
                    } else {
                        "verfügbar"
                    }
                );
                println!("  ID: {}", d.paket.id);
                println!("  Zusammenfassung: {}", d.paket.zusammenfassung);
                println!(
                    "  Lizenz: {}  Gruppe: {}  Größe: {} Bytes",
                    d.lizenz, d.gruppe, d.groesse_bytes
                );
                println!("  URL: {}", d.url.unwrap_or_default());
                println!("  Beschreibung:\n{}", einruecken(&d.beschreibung));
                Ok(())
            }
            "--aktualisierungen" => {
                let liste = pk.aktualisierungen().await?;
                println!("{} Aktualisierung(en)", liste.len());
                for a in liste {
                    println!(
                        "  {:<40} {:?}{}",
                        a.paket.id,
                        a.dringlichkeit,
                        if a.neustart_noetig {
                            "  (Neustart nötig)"
                        } else {
                            ""
                        }
                    );
                }
                Ok(())
            }
            "--laufend" => {
                for p in pk.laufende_transaktionen().await? {
                    println!("  {p}");
                }
                Ok(())
            }
            _ => {
                let nur_installiert = args[0] == "--installiert";
                let text = if nur_installiert {
                    args.get(1).cloned().unwrap_or_default()
                } else {
                    args[0].clone()
                };
                let treffer = pk.suchen(&text, nur_installiert).await?;
                println!("{} Treffer für „{text}“", treffer.len());
                for p in treffer {
                    println!(
                        "  {} {:<30} {:<16} {}",
                        if p.installiert { "[x]" } else { "[ ]" },
                        p.name,
                        p.version,
                        p.zusammenfassung
                    );
                }
                Ok(())
            }
        }
    });
    if let Err(f) = ergebnis {
        let f: qs_typen::Fehler = f;
        eprintln!("Fehler: {f} ({f:?})");
        std::process::exit(1);
    }
}

fn einruecken(text: &str) -> String {
    text.lines()
        .map(|z| format!("    {z}"))
        .collect::<Vec<_>>()
        .join("\n")
}
