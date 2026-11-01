//! The fixture korrid as a process, for workstation proofs (never shipped).
//!
//! ```text
//! fixture-korrid --games N --credentials DIR
//! ```
//!
//! Writes `DIR/KORRID_RPC_CAPABILITY` (mode 0600) and prints
//! `KORRID_ADDRESS=…` and `KORRID_PORTAL_ORIGIN=…` lines on stdout. Then it
//! reads commands on stdin, one per line:
//!
//! | Command | Effect |
//! | --- | --- |
//! | `pad` | add an initialized gamepad `fixture-pad` |
//! | `press CODE` | press and release a gamepad key, e.g. `press 0x130` |
//! | `hold CODE` / `release CODE` | hold or release a key |
//! | `overlay on` / `overlay off` | switch the presentation |
//! | `requests` | print every request received so far, one JSON line each |
//! | `stored ID` | print the preferences stored for a surface |

use korri_shell::{
    catalog,
    fixture::{Behaviour, Fixture},
    verify,
};
use serde_json::json;
use std::io::{BufRead, Write};

fn code(text: &str) -> Option<u16> {
    text.strip_prefix("0x").map_or_else(
        || text.parse().ok(),
        |hex| u16::from_str_radix(hex, 16).ok(),
    )
}

fn main() -> std::io::Result<()> {
    let mut games = 12;
    let mut credentials = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--games" => games = args.next().and_then(|n| n.parse().ok()).expect("--games N"),
            "--credentials" => credentials = args.next(),
            _ => panic!("usage: fixture-korrid --games N --credentials DIR"),
        }
    }
    let (model, examples) = verify::fixture();
    let fixture = Fixture::start(
        model,
        catalog::games(&examples, games),
        Behaviour::default(),
    )?;
    let credentials = credentials.expect("--credentials DIR");
    fixture.write_credential(std::path::Path::new(&credentials))?;
    println!("KORRID_ADDRESS=127.0.0.1:{}", fixture.port());
    println!("KORRID_PORTAL_ORIGIN={}", fixture.origin());
    std::io::stdout().flush()?;
    let key = |code: u16, value: u8| json!({"kind": "input", "deviceId": "fixture-pad", "class": "gamepad", "type": 1, "code": code, "value": value, "timestamp": 0});
    let report = json!({"kind": "input", "deviceId": "fixture-pad", "class": "gamepad", "type": 0, "code": 0, "value": 0, "timestamp": 0});
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["pad"] => fixture.send_input(&Fixture::gamepad_frames("fixture-pad")),
            ["press", value] => fixture.send_input(&Fixture::press_frames(
                "fixture-pad",
                code(value).expect("key code"),
            )),
            ["hold", value] => {
                fixture.send_input(&[key(code(value).expect("key code"), 1), report.clone()])
            }
            ["release", value] => {
                fixture.send_input(&[key(code(value).expect("key code"), 0), report.clone()])
            }
            ["overlay", "on"] => fixture.set_overlay(true),
            ["overlay", "off"] => fixture.set_overlay(false),
            ["requests"] => {
                for (surface, request) in fixture.requests() {
                    println!("{}", json!({"surface": surface, "request": request}));
                }
            }
            ["stored", id] => println!("{}", json!(fixture.stored(id))),
            [] => {}
            _ => eprintln!("fixture-korrid: unknown command {line:?}"),
        }
        std::io::stdout().flush()?;
    }
    // Keep serving after stdin closes, until killed.
    loop {
        std::thread::park();
    }
}
