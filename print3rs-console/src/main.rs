mod commands;
mod logging;

use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tracing;
use winnow::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let (mut readline, mut writer) = Readline::new(String::from("> "))?;

    while let ReadlineEvent::Line(line) = readline.readline().await? {
        match commands::parse_command.parse(&line) {
            Ok(command) => writer.write(format!("{command:?}\n").as_bytes()).await?,
            Err(e) => {
                writer
                    .write(format!("invalid command! Error: {e:?}\n").as_bytes())
                    .await?
            }
        };
        readline.add_history_entry(line);
    }

    Ok(())
}
