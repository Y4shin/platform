//! Plain-text vs JSON output formatting.

use std::io::Write;

use crate::cli::OutputFormat;

/// A value that can be rendered as both JSON (via `Serialize`) and as a
/// human-readable plain block. Each command result implements this once.
pub trait Renderable: serde::Serialize {
    fn render_plain(&self, w: &mut dyn Write) -> std::io::Result<()>;
}

/// Print `value` to stdout in the requested format. Always appends a final newline.
pub fn emit<T: Renderable>(format: OutputFormat, value: &T) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match format {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut out, value)
                .map_err(|e| std::io::Error::other(format!("json encode: {e}")))?;
            out.write_all(b"\n")?;
        }
        OutputFormat::Plain => {
            value.render_plain(&mut out)?;
        }
    }
    Ok(())
}
