//! Heuristik: Gibt es in der Sitzung einen polkit-Authentifizierungsagenten?
//!
//! Hintergrund (G2-I 1.4, G2-K): polkit kennt keine D-Bus-Abfrage „welche Agenten sind
//! registriert“. `EnumerateTemporaryAuthorizations` liefert nur bestehende
//! Kurzzeit-Freigaben und sagt nichts über Agenten aus. Ohne Agent beantwortet polkitd
//! eine `CheckAuthorization` mit `ALLOW_USER_INTERACTION` sofort mit
//! `is_authorized=false, is_challenge=true`; PackageKit meldet daraufhin `not-authorized`
//! – denselben Code wie bei einer echten Ablehnung durch den Benutzer.
//!
//! Deshalb schaut diese Heuristik in `/proc` nach einem laufenden Agentenprozess des
//! eigenen Benutzers (Name enthält „polkit“ und „agent“, oder bekannte Agenten wie
//! `lxpolkit`, `mate-polkit`, `xfce-polkit`). Findet sie keinen, wird `not-authorized` als
//! [`qs_typen::Fehler::KeinAgent`] gemeldet, sonst als `NichtBerechtigt`. Für Tests lässt
//! sich das Ergebnis mit [`Agentpruefung::Fest`] vorgeben.

use std::fs;

/// Wie die Agentenprüfung erfolgt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Agentpruefung {
    /// Prozessliste des eigenen Benutzers durchsuchen (Standard).
    #[default]
    Automatisch,
    /// Festes Ergebnis (für Tests und für Oberflächen, die es besser wissen).
    Fest(bool),
}

/// Bekannte Agenten, deren Name nicht beide Wörter „polkit“ und „agent“ enthält.
const BEKANNTE_AGENTEN: &[&str] = &[
    "lxpolkit",
    "lxqt-policykit-agent",
    "mate-polkit",
    "xfce-polkit",
    "polkit-gnome-authentication-agent-1",
    "polkit-kde-authentication-agent-1",
    "querencia-polkit-agent",
    "hyprpolkitagent",
    "pantheon-agent-polkit",
    "ukui-polkit",
    "deepin-polkit-agent",
];

/// Ist `name` (Prozessname oder Dateiname der ausführbaren Datei) ein polkit-Agent?
pub fn ist_agentname(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n == "polkitd" || n.starts_with("polkitd") {
        return false;
    }
    if n.contains("polkit") && n.contains("agent") {
        return true;
    }
    // `comm` ist auf 15 Zeichen gekürzt – Präfixvergleich in beide Richtungen.
    BEKANNTE_AGENTEN
        .iter()
        .any(|b| b.starts_with(&n) && n.len() >= 8 || n.starts_with(b))
}

/// Durchsucht `/proc` nach einem Agentenprozess des eigenen Benutzers.
pub fn agent_vorhanden() -> bool {
    let eigene_uid = eigene_uid();
    let Ok(eintraege) = fs::read_dir("/proc") else {
        return false;
    };
    for e in eintraege.flatten() {
        let name = e.file_name();
        let Some(pid) = name
            .to_str()
            .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
        else {
            continue;
        };
        let pfad = format!("/proc/{pid}");
        if let Some(uid) = eigene_uid {
            if uid_von(&pfad) != Some(uid) {
                continue;
            }
        }
        if let Ok(comm) = fs::read_to_string(format!("{pfad}/comm")) {
            if ist_agentname(comm.trim()) {
                return true;
            }
        }
        if let Ok(exe) = fs::read_link(format!("{pfad}/exe")) {
            if let Some(basis) = exe.file_name().and_then(|s| s.to_str()) {
                if ist_agentname(basis) {
                    return true;
                }
            }
        }
    }
    false
}

fn eigene_uid() -> Option<u32> {
    uid_von("/proc/self")
}

fn uid_von(prozpfad: &str) -> Option<u32> {
    let status = fs::read_to_string(format!("{prozpfad}/status")).ok()?;
    status
        .lines()
        .find(|z| z.starts_with("Uid:"))
        .and_then(|z| z.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
}

/// Wertet eine [`Agentpruefung`] aus.
pub fn pruefen(art: Agentpruefung) -> bool {
    match art {
        Agentpruefung::Automatisch => agent_vorhanden(),
        Agentpruefung::Fest(wert) => wert,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agentnamen() {
        assert!(ist_agentname("querencia-polkit-agent"));
        assert!(ist_agentname("querencia-polki")); // comm, 15 Zeichen
        assert!(ist_agentname("polkit-gnome-au"));
        assert!(ist_agentname("lxpolkit"));
        assert!(ist_agentname("mate-polkit"));
        assert!(ist_agentname("polkit-kde-authentication-agent-1"));
        assert!(!ist_agentname("polkitd"));
        assert!(!ist_agentname("bash"));
        assert!(!ist_agentname("packagekitd"));
        assert!(!ist_agentname("pk"));
    }

    #[test]
    fn eigene_uid_lesbar() {
        assert!(eigene_uid().is_some());
        // Darf nicht abstürzen, Ergebnis hängt von der Sitzung ab.
        let _ = agent_vorhanden();
        assert!(pruefen(Agentpruefung::Fest(true)));
        assert!(!pruefen(Agentpruefung::Fest(false)));
    }
}
