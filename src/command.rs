//! Command: ex-commands typed after `:` in Command mode.

/// A parsed ex-command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `:w` — write the buffer.
    Write,
    /// `:q` — quit, refusing if there are unsaved changes.
    Quit,
    /// `:q!` / `:qa!` — quit and discard changes.
    ForceQuit,
    /// `:wq` / `:x` — write, then quit.
    WriteQuit,
    /// `:set {args}` / `:se {args}` — set or show options (`nu`, `ts=4`).
    Set(String),
    /// `:colorscheme {name}` / `:colo {name}` — switch theme (empty: show it).
    Colorscheme(String),
    /// Anything else.
    Unknown(String),
}

impl Command {
    /// Parse the text typed after `:` (without the colon).
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        let (name, args) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        match name {
            "set" | "se" => return Command::Set(args.trim().to_owned()),
            "colorscheme" | "colo" => return Command::Colorscheme(args.trim().to_owned()),
            _ => {}
        }
        match input {
            "w" | "write" => Command::Write,
            "q" | "quit" | "qa" => Command::Quit,
            "q!" | "quit!" | "qa!" => Command::ForceQuit,
            "wq" | "x" | "xit" => Command::WriteQuit,
            other => Command::Unknown(other.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands() {
        assert_eq!(Command::parse("w"), Command::Write);
        assert_eq!(Command::parse("q"), Command::Quit);
        assert_eq!(Command::parse("q!"), Command::ForceQuit);
        assert_eq!(Command::parse("qa!"), Command::ForceQuit);
        assert_eq!(Command::parse("wq"), Command::WriteQuit);
        assert_eq!(Command::parse("x"), Command::WriteQuit);
    }

    #[test]
    fn parses_set() {
        assert_eq!(Command::parse("set nu"), Command::Set("nu".into()));
        assert_eq!(
            Command::parse("se  ts=4 nonu "),
            Command::Set("ts=4 nonu".into())
        );
        assert_eq!(Command::parse("set"), Command::Set(String::new()));
        assert_eq!(Command::parse("setx"), Command::Unknown("setx".into()));
    }

    #[test]
    fn parses_colorscheme() {
        assert_eq!(
            Command::parse("colorscheme raster-violet"),
            Command::Colorscheme("raster-violet".into())
        );
        assert_eq!(
            Command::parse("colo default"),
            Command::Colorscheme("default".into())
        );
        assert_eq!(Command::parse("colo"), Command::Colorscheme(String::new()));
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(Command::parse("  w "), Command::Write);
    }

    #[test]
    fn unknown_keeps_input() {
        assert_eq!(Command::parse("foo"), Command::Unknown("foo".into()));
    }
}
