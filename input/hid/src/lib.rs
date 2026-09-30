//! Accès au clavier Var par HID brut (D2 B3).
//!
//! Aucun hook : on ne lit que l'interface vendor-defined du clavier, sur
//! laquelle il n'émet rien hors du maintien de Var et de l'armement.

use std::time::Duration;

use hidapi::{HidApi, HidDevice, HidResult};
use omnikey_core::protocol::{DeviceFrame, FrameError, HostFrame, SeqTracker};
use omnikey_core::BaudotEvent;

/// Keychron.
pub const VENDOR_ID: u16 = 0x3434;
/// Interface HID brute de QMK (partagée avec VIA).
pub const RAW_USAGE_PAGE: u16 = 0xFF60;
pub const RAW_USAGE: u16 = 0x61;

/// PID des K3 Max (fork Keychron, `keyboards/keychron/k3_max/*/info.json`).
pub const K3_MAX_PIDS: [(u16, &str); 6] = [
    (0x0A30, "ANSI RGB"),
    (0x0A31, "ISO RGB"),
    (0x0A32, "JIS RGB"),
    (0x0A33, "ANSI White"),
    (0x0A34, "ISO White"),
    (0x0A35, "JIS White"),
];

pub fn k3_max_variant(pid: u16) -> Option<&'static str> {
    K3_MAX_PIDS.iter().find(|(p, _)| *p == pid).map(|(_, v)| *v)
}

/// Interfaces HID brutes Keychron présentes : (chemin, produit, PID).
pub fn list_raw_interfaces(api: &HidApi) -> Vec<(String, String, u16)> {
    api.device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID && d.usage_page() == RAW_USAGE_PAGE && d.usage() == RAW_USAGE)
        .map(|d| {
            (
                d.path().to_string_lossy().into_owned(),
                d.product_string().unwrap_or("?").to_string(),
                d.product_id(),
            )
        })
        .collect()
}

/// Ouvre l'interface HID brute du premier K3 Max branché en USB. Les autres
/// appareils Keychron (dongle Keychron Link notamment) sont écartés.
pub fn open_k3_max(api: &HidApi) -> Option<(HidResult<HidDevice>, u16)> {
    let info = api.device_list().find(|d| {
        d.vendor_id() == VENDOR_ID
            && d.usage_page() == RAW_USAGE_PAGE
            && d.usage() == RAW_USAGE
            && k3_max_variant(d.product_id()).is_some()
    })?;
    Some((info.open_device(api), info.product_id()))
}

/// Envoie une trame de 32 octets. hidapi attend un numéro de rapport en
/// tête ; l'interface QMK n'en a pas, d'où le 0.
pub fn write_frame(dev: &HidDevice, frame: &[u8; 32]) -> HidResult<usize> {
    let mut buf = [0u8; 33];
    buf[1..].copy_from_slice(frame);
    dev.write(&buf)
}

/// Ce que la source transmet au reste du software.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    Event(BaudotEvent),
    /// Trame(s) perdue(s) : le Core doit revenir au repos (D2 B1).
    SequenceGap,
    /// Clavier dont le firmware parle une autre version du protocole.
    VersionMismatch(u8),
}

/// Destinataire des événements du clavier. `incoming` retourne les trames à
/// renvoyer au clavier (armement).
pub trait Sink {
    fn connected(&mut self, variant: &'static str);
    fn incoming(&mut self, msg: Incoming) -> Vec<HostFrame>;
    fn disconnected(&mut self);
}

/// Sert un clavier ouvert jusqu'à sa déconnexion : répond au signal de vie,
/// vérifie les séquences, transmet les événements. Le clavier n'est annoncé
/// (`Sink::connected`) qu'à sa première trame Baudot : un K3 Max au firmware
/// d'origine a le même identifiant USB mais n'en émet jamais.
pub fn serve(dev: &HidDevice, variant: &'static str, sink: &mut impl Sink) {
    let mut announced = false;
    serve_inner(dev, variant, sink, &mut announced);
    if announced {
        sink.disconnected();
    }
}

fn serve_inner(dev: &HidDevice, variant: &'static str, sink: &mut impl Sink, announced: &mut bool) {
    let mut seq = SeqTracker::default();
    let mut tx_seq: u8 = 0;
    let mut send = |f: HostFrame| {
        let r = write_frame(dev, &f.encode(tx_seq));
        tx_seq = tx_seq.wrapping_add(1);
        r
    };
    // Réponse immédiate : le clavier se considère sans software jusqu'à la
    // première réponse, inutile d'attendre son prochain signal de vie.
    if send(HostFrame::HeartbeatAck).is_err() {
        return;
    }
    let mut buf = [0u8; 64];
    loop {
        let n = match dev.read_timeout(&mut buf, 250) {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => return,
        };
        let frame = DeviceFrame::decode(&buf[..n]);
        if !*announced && frame.is_ok() {
            *announced = true;
            sink.connected(variant);
        }
        let replies = match frame {
            Err(FrameError::NotBaudot) | Err(FrameError::Malformed) => continue,
            Err(FrameError::VersionMismatch(v)) => sink.incoming(Incoming::VersionMismatch(v)),
            Ok(DeviceFrame::Heartbeat { seq: s }) => {
                seq.accept(s);
                vec![HostFrame::HeartbeatAck]
            }
            Ok(DeviceFrame::Event { seq: s, event }) => {
                let mut out = Vec::new();
                if !seq.accept(s) {
                    out.extend(sink.incoming(Incoming::SequenceGap));
                }
                out.extend(sink.incoming(Incoming::Event(event)));
                out
            }
        };
        for f in replies {
            if send(f).is_err() {
                return;
            }
        }
    }
}

/// Boucle sans fin : attend un K3 Max, le sert, recommence après retrait.
pub fn run(sink: &mut impl Sink) -> ! {
    loop {
        if let Ok(api) = HidApi::new() {
            if let Some((Ok(dev), pid)) = open_k3_max(&api) {
                serve(&dev, k3_max_variant(pid).unwrap_or("?"), sink);
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
}
