use raster::app::App;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1);
    let mut app = App::new(path)?;

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
