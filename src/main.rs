use raster::{app::App, config};
use std::path::PathBuf;

const USAGE: &str = "usage: ras [--clean] [-u <init.lua>] [file]";

fn main() -> anyhow::Result<()> {
    let mut file = None;
    let mut init: Option<PathBuf> = None;
    let mut clean = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--clean" => clean = true,
            "-u" => match args.next() {
                Some(p) => init = Some(p.into()),
                None => anyhow::bail!("-u needs a file\n{USAGE}"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if file.is_none() => file = Some(arg),
            _ => anyhow::bail!("too many arguments\n{USAGE}"),
        }
    }

    let mut app = App::new(file)?;
    // `-u` must exist; the default init.lua is optional.
    let init = match (clean, init) {
        (true, _) => None,
        (false, Some(p)) => Some(p),
        (false, None) => config::default_path().filter(|p| p.exists()),
    };
    app.load_config(init.as_deref());

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
