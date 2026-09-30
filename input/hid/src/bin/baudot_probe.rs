//! Outil de diagnostic de l'étape 0 : lit les trames du clavier Var, les
//! affiche, et joue le rôle du software (réponse au signal de vie, armement).
//!
//! Usage :
//!   baudot-probe --list      interfaces HID brutes Keychron présentes
//!   baudot-probe             lit et répond au signal de vie
//!   baudot-probe --silent    lit sans jamais répondre (test M8)
//!   baudot-probe --arm       demande l'armement à chaque appui de Var, et le
//!                            lève après la première touche reçue hors Var
//!   baudot-probe --raw       affiche aussi les octets de chaque trame
//!   baudot-probe --ping      lecture seule : interroge le firmware (VIA et
//!                            Keychron) pour vérifier que le canal répond

use std::time::Instant;

use hidapi::HidApi;
use omnikey_core::protocol::{DeviceFrame, FrameError, HostFrame};
use omnikey_core::BaudotEvent;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);

    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Impossible d'initialiser HID : {e}");
            std::process::exit(1);
        }
    };

    let found = omnikey_hid::list_raw_interfaces(&api);
    if has("--list") || found.is_empty() {
        if found.is_empty() {
            eprintln!("Aucune interface HID brute Keychron (VID 3434, page FF60) trouvée.");
            eprintln!("Clavier branché en USB, interrupteur sur « Cable » ?");
            std::process::exit(2);
        }
        for (path, product, pid) in &found {
            println!("{product}  PID {pid:04X}  {path}");
        }
        return;
    }

    let (dev, pid) = match omnikey_hid::open_k3_max(&api) {
        Some((Ok(d), pid)) => (d, pid),
        Some((Err(e), _)) => {
            eprintln!("Ouverture impossible : {e}");
            std::process::exit(3);
        }
        None => {
            eprintln!("Aucun K3 Max branché en USB (seuls d'autres appareils Keychron sont présents).");
            std::process::exit(2);
        }
    };
    let variant = omnikey_hid::k3_max_variant(pid).unwrap_or("?");
    println!("Connecté au K3 Max {variant} (PID {pid:04X}). Ctrl+C pour quitter.");
    if has("--ping") {
        ping(&dev);
        return;
    }
    let (silent, arm, raw) = (has("--silent"), has("--arm"), has("--raw"));
    if silent {
        println!("Mode --silent : aucune réponse au signal de vie. Au bout de 3 s, Var doit devenir neutre.");
    }

    let start = Instant::now();
    let mut tx_seq: u8 = 0;
    let mut last_seq: Option<u8> = None;
    let mut held = false;
    let mut armed = false;
    let mut buf = [0u8; 64];

    let mut send = |dev: &hidapi::HidDevice, f: HostFrame, what: &str| {
        let _ = omnikey_hid::write_frame(dev, &f.encode(tx_seq));
        tx_seq = tx_seq.wrapping_add(1);
        if !what.is_empty() {
            println!("{:>9.3}s  -> {what}", start.elapsed().as_secs_f64());
        }
    };

    loop {
        let n = match dev.read_timeout(&mut buf, 100) {
            Ok(0) => continue,
            Ok(n) => n,
            Err(e) => {
                eprintln!("Lecture interrompue : {e}");
                std::process::exit(4);
            }
        };
        let t = start.elapsed().as_secs_f64();
        let frame = &buf[..n];
        if raw {
            println!("{t:>9.3}s  octets {:02X?}", &frame[..8.min(n)]);
        }
        match DeviceFrame::decode(frame) {
            Err(FrameError::NotBaudot) => println!("{t:>9.3}s  (trame non Baudot, ignorée : {:02X?})", &frame[..4.min(n)]),
            Err(e) => println!("{t:>9.3}s  trame invalide : {e:?}"),
            Ok(DeviceFrame::Heartbeat { seq }) => {
                check_seq(&mut last_seq, seq, t);
                if !silent {
                    send(&dev, HostFrame::HeartbeatAck, "");
                }
                println!("{t:>9.3}s  signal de vie #{seq}{}", if silent { " (sans réponse)" } else { "" });
            }
            Ok(DeviceFrame::Event { seq, event }) => {
                check_seq(&mut last_seq, seq, t);
                println!("{t:>9.3}s  {}", describe(&event));
                match event {
                    BaudotEvent::VarDown => {
                        held = true;
                        armed = false;
                        if arm {
                            send(&dev, HostFrame::SetArm(true), "SetArm(1)");
                        }
                    }
                    BaudotEvent::VarUp => {
                        held = false;
                        armed = arm;
                        if armed {
                            println!("           clavier armé : la prochaine lettre doit arriver ici");
                        }
                    }
                    BaudotEvent::KeyDown { .. } if !held && armed => {
                        armed = false;
                        send(&dev, HostFrame::SetArm(false), "SetArm(0)");
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Requêtes en lecture seule, dont la réponse prouve que le canal HID brut
/// est lisible et inscriptible depuis le PC, sans hook (étape 0, point a).
fn ping(dev: &hidapi::HidDevice) {
    // VIA id_get_protocol_version = 0x01 ; Keychron KC_GET_FIRMWARE_VERSION = 0xA1.
    for (label, cmd) in [("VIA, version du protocole", 0x01u8), ("Keychron, version du firmware", 0xA1)] {
        let mut req = [0u8; 32];
        req[0] = cmd;
        if let Err(e) = omnikey_hid::write_frame(dev, &req) {
            println!("{label} : écriture refusée ({e})");
            continue;
        }
        let mut buf = [0u8; 64];
        match dev.read_timeout(&mut buf, 500) {
            Ok(0) => println!("{label} : pas de réponse en 500 ms"),
            Ok(n) if cmd == 0xA1 => {
                let text: String = buf[1..n].iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
                println!("{label} : {text:?} (octets {:02X?})", &buf[..8.min(n)]);
            }
            Ok(n) => println!("{label} : réponse {:02X?}", &buf[..4.min(n)]),
            Err(e) => println!("{label} : lecture refusée ({e})"),
        }
    }
}

fn check_seq(last: &mut Option<u8>, seq: u8, t: f64) {
    if let Some(prev) = *last {
        if seq != prev.wrapping_add(1) {
            println!("{t:>9.3}s  !! trou de séquence : {prev} puis {seq}");
        }
    }
    *last = Some(seq);
}

fn describe(e: &BaudotEvent) -> String {
    let flags = |upper: bool| if upper { " (majuscule)" } else { "" };
    match *e {
        BaudotEvent::VarDown => "Var appuyée".into(),
        BaudotEvent::VarUp => "Var relâchée".into(),
        BaudotEvent::KeyDown { id, upper } => format!("touche {:>2} appuyée{}", id.0, flags(upper)),
        BaudotEvent::KeyUp { id, upper } => format!("touche {:>2} relâchée{}", id.0, flags(upper)),
    }
}
