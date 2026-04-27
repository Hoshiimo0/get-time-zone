//! 基準となる同一瞬間（既定では UTC の「いま」）に対し、
//! 入力された壁時計の表示と一致する IANA タイムゾーンを列挙する。

use std::io::{self, IsTerminal, Write};

use chrono::{DateTime, NaiveDateTime, NaiveTime, Timelike, Utc};
use chrono_tz::{Tz, TZ_VARIANTS};

#[derive(Debug, Clone)]
enum WallClock {
    /// 日付付きのローカル表示（基準瞬間と同じ瞬間の壁時計として解釈）
    DateTime(NaiveDateTime),
    /// 時刻のみ（基準瞬間における各ゾーンの時・分と照合。秒は無視）
    TimeHm { hour: u32, minute: u32 },
    /// 時刻のみ（秒まで一致）
    TimeHms { hour: u32, minute: u32, second: u32 },
}

fn usage() -> &'static str {
    "get-time-zone — 基準瞬間の壁時計表示から IANA タイムゾーンを推定します\n\
\n\
使い方:\n\
  get-time-zone [オプション] [時刻文字列]\n\
\n\
  時刻文字列を省略すると、標準入力から 1 行読み取ります。\n\
\n\
入力形式（いずれか）:\n\
  YYYY-MM-DD HH:MM[:SS]\n\
  YYYY-MM-DDTHH:MM[:SS]\n\
  HH:MM[:SS]  … 基準瞬間の各タイムゾーンのローカル時刻と照合\n\
\n\
オプション:\n\
  -r, --reference <RFC3339>  基準瞬間（既定: 現在の UTC）。例: 2026-04-28T12:00:00Z\n\
  -h, --help                 このヘルプを表示\n"
}

fn parse_wall_clock(s: &str) -> Result<WallClock, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("入力が空です".into());
    }

    const FMTS_DT: &[&str] = &[
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ];

    for fmt in FMTS_DT {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(WallClock::DateTime(dt));
        }
    }

    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        return Ok(WallClock::TimeHms {
            hour: t.hour(),
            minute: t.minute(),
            second: t.second(),
        });
    }

    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        return Ok(WallClock::TimeHm {
            hour: t.hour(),
            minute: t.minute(),
        });
    }

    Err(format!(
        "解釈できない形式です: {s:?}\n\
対応形式: YYYY-MM-DD HH:MM[:SS], YYYY-MM-DDTHH:MM[:SS], HH:MM[:SS]"
    ))
}

fn parse_reference_rfc3339(s: &str) -> Result<DateTime<Utc>, String> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("RFC3339 として解釈できません ({s:?}): {e}"))
}

fn collect_matches(reference: DateTime<Utc>, wall: &WallClock) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = TZ_VARIANTS
        .iter()
        .filter(|tz| zone_matches(reference, tz, wall))
        .map(|tz| tz.name())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn zone_matches(reference: DateTime<Utc>, tz: &Tz, wall: &WallClock) -> bool {
    let local = reference.with_timezone(tz);
    match wall {
        WallClock::DateTime(naive) => local.naive_local() == *naive,
        WallClock::TimeHm { hour, minute } => {
            let t = local.time();
            t.hour() == *hour && t.minute() == *minute
        }
        WallClock::TimeHms {
            hour,
            minute,
            second,
        } => {
            let t = local.time();
            t.hour() == *hour && t.minute() == *minute && t.second() == *second
        }
    }
}

struct Args {
    reference: DateTime<Utc>,
    wall_input: Option<String>,
}

enum CliError {
    Help,
    Message(String),
}

fn parse_args() -> Result<Args, CliError> {
    let mut args = std::env::args().skip(1);
    let mut reference = Utc::now();
    let mut wall_input: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => return Err(CliError::Help),
            "-r" | "--reference" => {
                let v = args.next().ok_or_else(|| {
                    CliError::Message("--reference の値がありません".into())
                })?;
                reference = parse_reference_rfc3339(&v).map_err(CliError::Message)?;
            }
            s if s.starts_with('-') => {
                return Err(CliError::Message(format!(
                    "不明なオプション: {s}（--help で使い方を表示）"
                )));
            }
            s => {
                if wall_input.is_some() {
                    return Err(CliError::Message(
                        "時刻文字列は 1 つだけ指定してください".into(),
                    ));
                }
                wall_input = Some(s.to_string());
            }
        }
    }

    Ok(Args {
        reference,
        wall_input,
    })
}

fn read_wall_line() -> Result<String, io::Error> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        eprint!("壁時計で見えている日時（または時刻）を入力: ");
        io::stderr().flush()?;
    }
    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(CliError::Help) => {
            print!("{}", usage());
            return Ok(());
        }
        Err(CliError::Message(m)) => return Err(m),
    };

    let line = if let Some(s) = args.wall_input {
        s
    } else {
        read_wall_line().map_err(|e| e.to_string())?
    };

    let wall = parse_wall_clock(&line)?;
    let zones = collect_matches(args.reference, &wall);

    if zones.is_empty() {
        return Err(
            "一致する IANA タイムゾーンがありません（入力形式や基準時刻を確認してください）"
                .into(),
        );
    }

    for z in zones {
        println!("{z}");
    }
    Ok(())
}
