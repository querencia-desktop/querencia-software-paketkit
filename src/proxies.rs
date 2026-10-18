//! zbus-Proxys für `org.freedesktop.PackageKit` und `org.freedesktop.PackageKit.Transaction`.
//!
//! Signale werden nicht über die Proxy-Methoden empfangen, sondern über einen
//! [`zbus::MessageStream`] mit Pfadregel (siehe [`crate::transaktion`]), damit auch die
//! `PropertiesChanged`-Signale (Percentage, Status) in derselben Reihenfolge ankommen.

use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

/// Dienstname von PackageKit.
pub const DIENST: &str = "org.freedesktop.PackageKit";
/// Objektpfad des Daemons.
pub const PFAD: &str = "/org/freedesktop/PackageKit";
/// Schnittstelle einer Transaktion.
pub const TRANSAKTION: &str = "org.freedesktop.PackageKit.Transaction";

/// Hauptschnittstelle des Daemons.
#[proxy(
    interface = "org.freedesktop.PackageKit",
    default_service = "org.freedesktop.PackageKit",
    default_path = "/org/freedesktop/PackageKit"
)]
pub trait PackageKit {
    /// Legt eine Transaktion an und liefert ihren Objektpfad.
    fn create_transaction(&self) -> zbus::Result<OwnedObjectPath>;
    /// Objektpfade aller laufenden Transaktionen.
    fn get_transaction_list(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    /// `PkAuthorizeEnum`: 0 unbekannt, 1 ja, 2 nein, 3 interaktiv.
    fn can_authorize(&self, action_id: &str) -> zbus::Result<u32>;
    /// Name des Backends (`apt`, `test_thread`, …).
    #[zbus(property)]
    fn backend_name(&self) -> zbus::Result<String>;
    /// Distributionskennung, z. B. `ubuntu;26.04;x86_64`.
    #[zbus(property)]
    fn distro_id(&self) -> zbus::Result<String>;
    /// Unterstützte Rollen (Bitfeld).
    #[zbus(property)]
    fn roles(&self) -> zbus::Result<u64>;
}

/// Eine Transaktion.
#[proxy(
    interface = "org.freedesktop.PackageKit.Transaction",
    default_service = "org.freedesktop.PackageKit"
)]
pub trait Transaction {
    /// Hinweise (`locale=`, `interactive=`, `background=`, …).
    fn set_hints(&self, hints: &[&str]) -> zbus::Result<()>;
    /// Suche in Paketnamen.
    fn search_names(&self, filter: u64, values: &[&str]) -> zbus::Result<()>;
    /// Suche in Beschreibungen.
    fn search_details(&self, filter: u64, values: &[&str]) -> zbus::Result<()>;
    /// Paketnamen auflösen.
    fn resolve(&self, filter: u64, packages: &[&str]) -> zbus::Result<()>;
    /// Details holen.
    fn get_details(&self, package_ids: &[&str]) -> zbus::Result<()>;
    /// Aktualisierungen holen.
    fn get_updates(&self, filter: u64) -> zbus::Result<()>;
    /// Details zu Aktualisierungen holen.
    fn get_update_detail(&self, package_ids: &[&str]) -> zbus::Result<()>;
    /// Pakete installieren.
    fn install_packages(&self, transaction_flags: u64, package_ids: &[&str]) -> zbus::Result<()>;
    /// Pakete entfernen.
    fn remove_packages(
        &self,
        transaction_flags: u64,
        package_ids: &[&str],
        allow_deps: bool,
        autoremove: bool,
    ) -> zbus::Result<()>;
    /// Pakete aktualisieren.
    fn update_packages(&self, transaction_flags: u64, package_ids: &[&str]) -> zbus::Result<()>;
    /// Paketquellen auffrischen.
    fn refresh_cache(&self, force: bool) -> zbus::Result<()>;
    /// Abbrechen.
    fn cancel(&self) -> zbus::Result<()>;
    /// Gesamtfortschritt 0..=100, 101 = unbekannt.
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u32>;
    /// `PkStatusEnum`.
    #[zbus(property)]
    fn status(&self) -> zbus::Result<u32>;
    /// `PkRoleEnum`.
    #[zbus(property)]
    fn role(&self) -> zbus::Result<u32>;
    /// Darf gerade abgebrochen werden?
    #[zbus(property)]
    fn allow_cancel(&self) -> zbus::Result<bool>;
}
