use crate::fs::relativize_path;
use crate::jupyter::JupyterIndex;
use crate::message::diff::calculate_print_width;
use crate::message::text::{MessageCodeFrame, RuleCodeAndBody};
use crate::message::{
    group_messages_by_filename, Emitter, EmitterContext, Message, MessageWithLocation,
};
use colored::Colorize;
use ruff_python_ast::source_code::OneIndexed;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::num::NonZeroUsize;

#[derive(Default)]
pub struct GroupedEmitter {
    show_fix_status: bool,
    show_source: bool,
}

impl GroupedEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.show_fix_status = show_fix_status;
        self
    }

    #[must_use]
    pub fn with_show_source(mut self, show_source: bool) -> Self {
        self.show_source = show_source;
        self
    }
}

impl Emitter for GroupedEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for (filename, messages) in group_messages_by_filename(messages) {
            // Compute the maximum number of digits in the row and column, for messages in
            // this file.

            let mut max_row_length = OneIndexed::MIN;
            let mut max_column_length = OneIndexed::MIN;

            for message in &messages {
                max_row_length = max_row_length.max(message.start_location.row);
                max_column_length = max_column_length.max(message.start_location.column);
            }

            let row_length = calculate_print_width(max_row_length);
            let column_length = calculate_print_width(max_column_length);

            // Print the filename.
            writeln!(writer, "{}:", relativize_path(filename).underline())?;

            // Print each message.
            for message in messages {
                write!(
                    writer,
                    "{}",
                    DisplayGroupedMessage {
                        jupyter_index: context.jupyter_index(message.filename()),
                        message,
                        show_fix_status: self.show_fix_status,
                        show_source: self.show_source,
                        row_length,
                        column_length,
                    }
                )?;
            }
            writeln!(writer)?;
        }

        Ok(())
    }
}

struct DisplayGroupedMessage<'a> {
    message: MessageWithLocation<'a>,
    show_fix_status: bool,
    show_source: bool,
    row_length: NonZeroUsize,
    column_length: NonZeroUsize,
    jupyter_index: Option<&'a JupyterIndex>,
}

impl Display for DisplayGroupedMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let MessageWithLocation {
            message,
            start_location,
        } = &self.message;

        write!(
            f,
            "  {row_padding}",
            row_padding =
                " ".repeat(self.row_length.get() - calculate_print_width(start_location.row).get())
        )?;

        // Check if we're working on a jupyter notebook and translate positions with cell accordingly
        let (row, col) = if let Some(jupyter_index) = self.jupyter_index {
            write!(
                f,
                "cell {cell}{sep}",
                cell = jupyter_index.row_to_cell[start_location.row.get()],
                sep = ":".cyan()
            )?;
            (
                jupyter_index.row_to_row_in_cell[start_location.row.get()] as usize,
                start_location.column.get(),
            )
        } else {
            (start_location.row.get(), start_location.column.get())
        };

        writeln!(
            f,
            "{row}{sep}{col}{col_padding} {code_and_body}",
            sep = ":".cyan(),
            col_padding = " ".repeat(
                self.column_length.get() - calculate_print_width(start_location.column).get()
            ),
            code_and_body = RuleCodeAndBody {
                message_kind: &message.kind,
                show_fix_status: self.show_fix_status
            },
        )?;

        if self.show_source {
            use std::fmt::Write;
            let mut padded = PadAdapter::new(f);
            write!(padded, "{}", MessageCodeFrame { message })?;
        }

        writeln!(f)?;

        Ok(())
    }
}

/// Adapter that adds a '  ' at the start of every line without the need to copy the string.
/// Inspired by Rust's `debug_struct()` internal implementation that also uses a `PadAdapter`.
struct PadAdapter<'buf> {
    buf: &'buf mut (dyn std::fmt::Write + 'buf),
    on_newline: bool,
}

impl<'buf> PadAdapter<'buf> {
    fn new(buf: &'buf mut (dyn std::fmt::Write + 'buf)) -> Self {
        Self {
            buf,
            on_newline: true,
        }
    }
}

impl std::fmt::Write for PadAdapter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for s in s.split_inclusive('\n') {
            if self.on_newline {
                self.buf.write_str("  ")?;
            }

            self.on_newline = s.ends_with('\n');
            self.buf.write_str(s)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::GroupedEmitter;
    use insta::assert_snapshot;

    #[test]
    fn default() {
        let mut emitter = GroupedEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = GroupedEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
