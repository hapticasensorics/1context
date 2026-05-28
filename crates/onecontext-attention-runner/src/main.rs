use std::path::PathBuf;

use anyhow::{bail, Result};
use onecontext_attention_runner::{run_attention_filter, run_attention_filter_on_bundle};

fn main() {
    if let Err(error) = real_main() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let args = Args::parse()?;
    if args.help {
        print_usage();
        return Ok(());
    }

    let summary = if let Some(bundle) = &args.bundle {
        run_attention_filter_on_bundle(bundle, args.output.as_deref())?
    } else if let Some(session) = &args.session {
        run_attention_filter(session, args.output.as_deref())?
    } else {
        bail!("use --session or --bundle");
    };
    println!(
        "wrote {} ({} candidates, {} saved states)",
        summary.output_path.display(),
        summary.candidates,
        summary.saved
    );
    if let Some(session_path) = summary.dashboard_session_path {
        println!("dashboard session {}", session_path.display());
    }
    Ok(())
}

#[derive(Debug)]
struct Args {
    session: Option<PathBuf>,
    bundle: Option<PathBuf>,
    output: Option<PathBuf>,
    help: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = std::env::args_os().skip(1);
        let mut session = None;
        let mut bundle = None;
        let mut output = None;
        let mut help = false;

        while let Some(arg) = args.next() {
            if arg == "--session" {
                let Some(value) = args.next() else {
                    bail!("--session requires a path");
                };
                session = Some(PathBuf::from(value));
            } else if arg == "--bundle" {
                let Some(value) = args.next() else {
                    bail!("--bundle requires a READY capture bundle path");
                };
                bundle = Some(PathBuf::from(value));
            } else if arg == "--out" || arg == "--output" {
                let Some(value) = args.next() else {
                    bail!("{} requires a path", arg.to_string_lossy());
                };
                output = Some(PathBuf::from(value));
            } else if arg == "--help" || arg == "-h" {
                help = true;
            } else {
                bail!("unknown argument: {}", arg.to_string_lossy());
            }
        }
        if session.is_some() && bundle.is_some() {
            bail!("use either --session or --bundle, not both");
        }

        Ok(Self {
            session,
            bundle,
            output,
            help,
        })
    }
}

fn print_usage() {
    println!(
        "usage: onecontext-attention-runner [--session <attention-dashboard-session.json> | --bundle <READY capture bundle>] [--out <attention-filter-output.json>]"
    );
}
