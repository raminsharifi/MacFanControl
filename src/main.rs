mod app;
mod control;
mod curve;
mod fan;
mod persist;
mod plot;
mod smc;
mod temps;
mod ui;

use std::process::ExitCode;

use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

fn model_name() -> String {
    let chip = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let model = std::process::Command::new("sysctl")
        .args(["-n", "hw.model"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Mac".into());
    match chip {
        Some(chip) => format!("{model} · {chip}"),
        None => model,
    }
}

fn print_help() {
    println!("macfan — control Mac fan speed from the terminal");
    println!();
    println!("usage: macfan [--curve [SPEC] | --list | --auto | --help | --version]");
    println!();
    println!("  (no args)       launch the TUI (sudo required to change speeds)");
    println!("  -c, --curve     launch with a piecewise-linear temperature fan curve");
    println!("                  SPEC is hotspot(°C):fan(%) pairs, e.g. 40:30,60:60,90:100");
    println!("                  or 40c:30%,60c:60%,90c:100% (default if omitted)");
    println!("  --list          print fans and temperatures, then exit");
    println!("  --auto          restore all fans to automatic control, then exit");
    println!("  -V, --version   print version, then exit");
    println!();
    println!("TUI keys: ↑↓ select fan, ←→ ±100 RPM, shift←→ ±500, m manual/auto,");
    println!("          c toggle curve, C edit curve (or edit it on the plot with the mouse; u undo, D reset), a all auto, f full blast,");
    println!("          space toggle linked fans, q quit (restores auto), Q quit keeping settings");
}

#[derive(Debug, PartialEq)]
enum Action {
    Help,
    Version,
    List,
    Auto,
    Tui(Option<curve::Curve>),
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Action::Help);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Action::Version);
    }
    let mut list = false;
    let mut auto = false;
    let mut curve: Option<curve::Curve> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--list" => list = true,
            "--auto" => auto = true,
            "--curve" | "-c" => {
                let mut parts = Vec::new();
                while i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 1;
                    parts.push(args[i].as_str());
                }
                curve = Some(if parts.is_empty() {
                    curve::Curve::default()
                } else {
                    curve::Curve::parse(&parts.join(" "))?
                });
            }
            s if s.starts_with("--curve=") || s.starts_with("-c=") => {
                let (_, spec) = s.split_once('=').unwrap();
                curve = Some(curve::Curve::parse(spec)?);
            }
            _ => return Err(format!("unknown argument '{arg}' (try --help)")),
        }
        i += 1;
    }
    if (list as u8 + auto as u8 + curve.is_some() as u8) > 1 {
        return Err("--list, --auto, and --curve cannot be used together".into());
    }
    if auto {
        Ok(Action::Auto)
    } else if list {
        Ok(Action::List)
    } else {
        Ok(Action::Tui(curve))
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let action = parse_args(args)?;
    if action == Action::Help {
        print_help();
        return Ok(());
    }
    if action == Action::Version {
        println!("macfan {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let smc = smc::Smc::open()?;
    let fans = fan::discover(&smc)?;
    if fans.is_empty() {
        return Err("no fans found (is this a fanless Mac?)".into());
    }

    match action {
        Action::Help | Action::Version => unreachable!(),
        Action::Auto => {
            if !smc::is_root() {
                return Err(
                    "restoring automatic fan control requires root — run: sudo macfan --auto"
                        .into(),
                );
            }
            let problems = control::force_auto(&smc, &fans);
            if problems.is_empty() {
                println!("all fans restored to automatic control");
                return Ok(());
            }
            Err(problems.join("; ").into())
        }
        Action::List => {
            println!("discovering sensors…");
            let temps = temps::Temps::discover(&smc);
            println!("{}", model_name());
            println!();
            for fan in &fans {
                let mode = match fan.mode {
                    fan::FanMode::Manual => "MANUAL",
                    fan::FanMode::System => "SYSTEM",
                    fan::FanMode::Auto => "AUTO",
                };
                println!(
                    "  {:<12} {:>5.0} RPM  target {:>5.0}  range {:.0}–{:.0}  [{mode}]",
                    fan.name, fan.actual, fan.target, fan.min, fan.max,
                );
            }
            println!();
            match &temps.hottest {
                Some((key, t)) => println!(
                    "  temps: avg {:.1}°C, hottest {t:.1}°C ({key}), {} sensors",
                    temps.avg,
                    temps.sensor_count(),
                ),
                None => println!("  temps: no readable sensors"),
            }
            Ok(())
        }
        Action::Tui(curve) => {
            let temps = temps::Temps::discover(&smc);
            let controller = control::Controller::spawn(fans.clone())?;
            let mut terminal = ratatui::init();
            let _ = execute!(std::io::stdout(), EnableMouseCapture);
            let hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = execute!(std::io::stdout(), DisableMouseCapture);
                hook(info);
            }));
            let mut app = app::App::new(smc, fans, temps, model_name(), controller, curve);
            let result = app.run(&mut terminal);
            let _ = execute!(std::io::stdout(), DisableMouseCapture);
            ratatui::restore();
            if app.keep_on_exit() {
                println!("keeping manual fan settings — run `sudo macfan --auto` to restore");
            } else {
                println!("restoring fans to automatic control…");
            }
            use std::io::Write;
            let _ = std::io::stdout().flush();
            drop(app);
            result
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("macfan: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).into()).collect()
    }

    #[test]
    fn cli_args() {
        assert_eq!(parse_args(&s(&[])).unwrap(), Action::Tui(None));
        assert_eq!(parse_args(&s(&["--list"])).unwrap(), Action::List);
        assert_eq!(parse_args(&s(&["--auto"])).unwrap(), Action::Auto);
        assert_eq!(
            parse_args(&s(&["--curve"])).unwrap(),
            Action::Tui(Some(curve::Curve::default()))
        );
        assert_eq!(
            parse_args(&s(&["-c", "40:30,60:60,90:100"])).unwrap(),
            Action::Tui(Some(curve::Curve::default()))
        );
        assert_eq!(
            parse_args(&s(&["--curve=40c:30%,60c:60%,90c:100%"])).unwrap(),
            Action::Tui(Some(curve::Curve::default()))
        );
        assert_eq!(
            parse_args(&s(&["--curve", "40c:30%", "60c:60%", "90c:100%"])).unwrap(),
            Action::Tui(Some(curve::Curve::default()))
        );
        assert_eq!(parse_args(&s(&["--version"])).unwrap(), Action::Version);
        assert_eq!(parse_args(&s(&["-V"])).unwrap(), Action::Version);
        assert!(parse_args(&s(&["--list", "--auto"])).is_err());
        assert!(parse_args(&s(&["--list", "--curve"])).is_err());
        assert!(parse_args(&s(&["--bogus"])).is_err());
    }
}
