mod app;
mod cli;

fn main() -> anyhow::Result<()> {
    app::run()
}
