//! `qs_paketkit` – die D-Bus-Schicht von „Querencia Software“ zu PackageKit.
//!
//! Implementiert [`qs_typen::Paketquelle`] über zbus 5 gegen `org.freedesktop.PackageKit`
//! auf dem Systembus (oder einem privaten Prüfbus, Umgebungsvariable `QS_PK_BUS`).
//!
//! * Lesen: `SearchNames`, `SearchDetails`, `Resolve`, `GetDetails`, `GetUpdates` +
//!   `GetUpdateDetail` – ohne Rechte.
//! * Schreiben: `InstallPackages`, `RemovePackages` (immer `allow_deps=false`,
//!   `autoremove=false`), `UpdatePackages`, `RefreshCache`; Simulation über die
//!   `SIMULATE`-Flagge; `Cancel`; `GetTransactionList`.
//! * Fortschritt: Signale `Package`, `Details`, `UpdateDetail`, `ItemProgress`,
//!   `ErrorCode`, `RequireRestart`, `Finished`, `Destroy` und die Eigenschaften
//!   `Percentage`/`Status` werden in [`qs_typen::Fortschritt`] abgebildet; Statuswerte
//!   erscheinen als deutscher Text, Fehlercodes als [`qs_typen::Fehler`].
//! * Ein Daemon-Neustart während einer Transaktion (`NameOwnerChanged`) wird als
//!   [`qs_typen::Fehler::DaemonWeg`] gemeldet.
//!
//! Einstieg: [`PaketKit::neu`] (async) oder [`PaketKit::neu_blockierend`].
//!
//! ```no_run
//! use qs_typen::Paketquelle;
//! let pk = qs_paketkit::PaketKit::neu_blockierend().unwrap();
//! let treffer = zbus::block_on(pk.suchen("querencia", false)).unwrap();
//! for p in treffer {
//!     println!("{} {} {}", p.name, p.version, if p.installiert { "installiert" } else { "" });
//! }
//! ```

pub mod agent;
pub mod enums;
pub mod proxies;
pub mod transaktion;

mod paketkit;

pub use agent::Agentpruefung;
pub use paketkit::{
    details_aus_map, AbbruchGriff, Abruf, FortschrittEmpfaenger, PaketKit, CODE_ZEIT, FRIST_LESEN,
    HINWEISE, UMGEBUNG_BUS,
};
pub use qs_typen;
pub use zbus;
