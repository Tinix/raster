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
    /// Anything else.
    Unknown(String),
}

impl Command {
    /// Parse the text typed after `:` (without the colon).
    pub fn parse(input: &str) -> Self {
        match input.trim() {
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
    fn trims_whitespace() {
        assert_eq!(Command::parse("  w "), Command::Write);
    }

    #[test]
    fn unknown_keeps_input() {
        assert_eq!(Command::parse("foo"), Command::Unknown("foo".into()));
    }
}
