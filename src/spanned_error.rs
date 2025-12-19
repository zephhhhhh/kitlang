//! Provides utilities for building formatted error messages with source code context and highlights.
//!
//! This module is used throughout the compiler to format different errors that may occur, that include
//! source code snippets and highlighting of the exact location of the error, along with messages that
//! describe the error in detail.

use std::fmt::Write;
use std::ops::Range;

use crate::ast::SourceSpan;

use log::error;

#[allow(clippy::wildcard_imports)]
#[cfg(feature = "colour")]
use inline_colorization::*;

#[cfg(feature = "colour")]
#[allow(non_upper_case_globals)]
const prefix_color: &str = color_blue;

#[cfg(feature = "colour")]
#[allow(non_upper_case_globals)]
const highlight_color: &str = color_bright_yellow;

/// Represents a single line in the the "source" view of an error message output.
/// Each source code line can have zero or more 'comment' lines, that can provide more information
/// or highlight the problem area.
#[derive(Debug)]
pub struct SpannedErrorLine {
    /// The actual line in the the source code.
    pub source: String,

    pub line_number: u32,
    /// The index in the source code string that this line starts on.
    pub line_start_global_index: u32,

    pub comment_lines: Vec<String>,
}

impl SpannedErrorLine {
    /// Push a new comment for this line onto the back of the comment list.
    #[inline]
    pub fn add_comment_line(&mut self, comment: &str) {
        self.comment_lines.push(comment.to_string());
    }

    /// The index of the last character in this line in the global source code string.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub const fn end_of_line_index(&self) -> u32 {
        self.line_start_global_index
            .saturating_add(self.source.len() as u32)
    }

    /// How many characters the line number string takes up.
    /// (Used for consistent formatting of line number lengths across lines.)
    #[inline]
    #[must_use]
    pub fn line_number_string_len(&self) -> usize {
        self.line_number.to_string().len()
    }

    /// Get the prefix printed before a line, containing the line number aligned to the right hand
    /// side, with space padding to the left to maintain a consistent width.
    #[inline]
    fn get_prefix_on_source(&self, line_no_width: usize) -> String {
        #[cfg(feature = "colour")]
        {
            format!(
                "{prefix_color} {:>width$} | {color_reset}",
                self.line_number,
                width = line_no_width
            )
        }
        #[cfg(not(feature = "colour"))]
        {
            format!(" {:>width$} | ", self.line_number, width = line_no_width)
        }
    }

    /// Adds a comment to this line, that underlines all the characters that are within the span of
    /// the provided error span.
    #[allow(clippy::cast_possible_truncation)]
    pub fn highlight_from_span(&mut self, span: SourceSpan, highlight: &str, error: bool) {
        if self.end_of_line_index() < span.start {
            return;
        }

        let start = span.start.saturating_sub(self.line_start_global_index);
        let cut_off = self.line_start_global_index.saturating_sub(span.start);
        let remaining = span.end.saturating_sub(span.start).saturating_sub(cut_off);

        if remaining == 0 {
            return;
        }

        let end = start.saturating_add(remaining.min(self.source.len() as u32));
        let only_highlighted_bit = substr_safe(&self.source, start..end);

        if let Some(first_non_whitespace) = only_highlighted_bit.find(|c: char| !c.is_whitespace())
        {
            let final_start = start + first_non_whitespace as u32;

            if final_start >= self.source.len() as u32 || final_start >= end {
                return;
            }

            let mut highlight_str = String::with_capacity(self.source.len());
            highlight_str += &" ".repeat(final_start as usize);
            let actual_str_bit: String = highlight
                .chars()
                .cycle()
                .take(end.saturating_sub(final_start) as usize)
                .collect();

            #[cfg(feature = "colour")]
            {
                let highlight = if error { color_red } else { highlight_color };
                let _ = write!(highlight_str, "{highlight}{actual_str_bit}{color_reset}");
            }
            #[cfg(not(feature = "colour"))]
            {
                highlight_str += &actual_str_bit;
            }

            self.add_comment_line(&highlight_str);
        }
    }

    /// Generate the final output string for this line, including prefixes and all comments.
    #[inline]
    #[must_use]
    pub fn generate_output(&self, line_no_width: usize, blank_prefix: &str) -> String {
        let mut output_string = format!(
            "{}{}\n",
            self.get_prefix_on_source(line_no_width),
            self.source
        );
        for comment in &self.comment_lines {
            output_string += blank_prefix;
            output_string += comment;
            output_string += "\n";
        }
        output_string
    }
}

/// Used for building pretty formatted error messages based on an error span and the source code
/// string that was used when this error originated.
#[must_use]
pub struct SpannedErrorBuilder {
    pub header_lines: Vec<String>,
    pub footer_lines: Vec<String>,

    pub file_name: Option<String>,

    pub show_location: bool,
    pub pad_around_source_view: bool,

    pub error_span: SourceSpan,
    pub lines: Vec<SpannedErrorLine>,

    pub hard_error: bool,
}

impl SpannedErrorBuilder {
    #[inline]
    pub fn new(src: &str, span: SourceSpan) -> Self {
        let lines = get_lines(src, span);
        Self {
            header_lines: Vec::new(),
            footer_lines: Vec::new(),
            file_name: None,
            show_location: true,
            pad_around_source_view: true,

            error_span: span,
            lines,

            hard_error: true,
        }
    }

    /// Generate a highlight comment on all applicable lines that highlight the provided error
    /// span with an underline.
    #[inline]
    pub fn generate_highlight(&mut self) -> &mut Self {
        if self.error_span.is_null_span() {
            return self;
        }
        for line in &mut self.lines {
            line.highlight_from_span(self.error_span, "^", self.hard_error);
        }
        self
    }

    /// Set the file name, used if `show_location` is set to `true`.
    #[inline]
    pub fn set_file_name(&mut self, file_name: impl AsRef<str>) -> &mut Self {
        self.file_name = Some(file_name.as_ref().to_string());
        self
    }

    /// Add blank prefix lines above the source code view and below if there are `footer_lines`
    /// present.
    #[inline]
    pub const fn pad_around_source_view(&mut self, should_pad: bool) -> &mut Self {
        self.pad_around_source_view = should_pad;
        self
    }

    /// Show the location (file name, line number and character index) that the error span starts
    /// at as a header line above the source code view.
    #[inline]
    pub const fn show_location(&mut self, should_show: bool) -> &mut Self {
        self.show_location = should_show;
        self
    }

    /// Add any string as a header line above the source code view.
    #[inline]
    pub fn print_header_line(&mut self, line: impl AsRef<str>) -> &mut Self {
        let header_line = if self.hard_error {
            #[cfg(feature = "colour")]
            {
                format!(
                    "{color_red}Error:{color_bright_white} {}{color_reset}\n",
                    line.as_ref()
                )
            }
            #[cfg(not(feature = "colour"))]
            format!("Error: {}\n", line.as_ref())
        } else {
            format!("{}\n", line.as_ref())
        };

        self.header_lines.push(header_line);
        self
    }

    /// Add any string as a footer line below the source code view.
    #[inline]
    pub fn print_footer_line(&mut self, line: impl AsRef<str>) -> &mut Self {
        self.footer_lines.push(format!("{}\n", line.as_ref()));
        self
    }
}

impl SpannedErrorBuilder {
    /// Constructs a blank prefix line that is padded to maintain the same width as the other
    /// prefixed lines with a specified max line number character width.
    #[inline]
    #[must_use]
    fn get_blank_prefix(line_no_width: usize) -> String {
        #[cfg(feature = "colour")]
        {
            format!(
                "{prefix_color} {} | {color_reset}",
                " ".repeat(line_no_width)
            )
        }
        #[cfg(not(feature = "colour"))]
        format!(" {} | ", " ".repeat(line_no_width))
    }

    /// Generates the final fully formatted output from `self`.
    #[must_use]
    pub fn generate_output(&self) -> String {
        let max_line_no_len = self
            .lines
            .iter()
            .map(SpannedErrorLine::line_number_string_len)
            .max()
            .unwrap_or(1);
        let blank_prefix = Self::get_blank_prefix(max_line_no_len);

        let mut final_output = String::new();

        for header_line in &self.header_lines {
            final_output += header_line;
        }

        if self.show_location
            && let Some(first_line) = self.lines.first()
        {
            let file_name = self
                .file_name
                .as_ref()
                .map(|f| format!("{f}:"))
                .unwrap_or_default();
            let line = first_line.line_number;
            let char_index = self
                .error_span
                .start
                .saturating_sub(first_line.line_start_global_index)
                .saturating_add(1);
            #[cfg(feature = "colour")]
            {
                let _ = writeln!(
                    final_output,
                    "{prefix_color}  --> {color_reset}{file_name}{line}:{char_index}"
                );
            }
            #[cfg(not(feature = "colour"))]
            {
                let _ = writeln!(final_output, "  --> {file_name}{line}:{char_index}");
            }
        }

        if self.error_span.is_null_span() {
            final_output += &blank_prefix;
            final_output += "\n";
        } else {
            if self.pad_around_source_view {
                final_output += &blank_prefix;
                final_output += "\n";
            }

            for line in &self.lines {
                final_output += &line.generate_output(max_line_no_len, &blank_prefix);
            }

            if self.pad_around_source_view && !self.footer_lines.is_empty() {
                final_output += &blank_prefix;
                final_output += "\n";
            }
        }

        for footer_line in &self.footer_lines {
            final_output += footer_line;
        }

        final_output
    }
}

/// Safely substring a string, clamping the range to the valid values.
#[inline]
#[must_use]
fn substr_safe(s: &str, range: Range<u32>) -> &str {
    let (r_min, r_max) = (range.start, range.end);
    if r_max < r_min {
        error!(
            "Custom backtrace: {}",
            std::backtrace::Backtrace::force_capture()
        );
    }
    // let (r_min, r_max) = if range.start <= range.end {
    //     (range.start, range.end)
    // } else {
    //     (range.end, range.start)
    // };
    let r_start = s.len().min(r_min as usize);
    let r_end = s.len().min(r_max as usize);
    &s[r_start..r_end]
}

/// Safely substring a string, clamping the range to the valid values.
/// If the resulting clamped substring ends with '\r', it is removed.
#[inline]
#[must_use]
fn substr_safe_no_trailing_return(s: &str, range: Range<u32>) -> &str {
    let r_start = range.start as usize;
    let r_end = s.len().min(range.end as usize);
    let res: &str = &s[r_start..r_end];
    if res.ends_with('\r') {
        &s[r_start..(r_end.saturating_sub(1))]
    } else {
        res
    }
}

/// Counts the number of newline (`\n`) characters in a string slice.
#[inline]
#[must_use]
fn count_new_lines(str: &str) -> usize {
    str.chars().filter(|c| *c == '\n').count()
}

/// Get the 3 segments from the source string and `self`.
/// # Returns
/// `(before, the_error_span, after)`
#[inline]
#[must_use]
fn get_segments_from_source(source_string: &str, span: SourceSpan) -> (&str, &str, &str) {
    let before_span = substr_safe(source_string, 0..span.start);
    let error_span = substr_safe(source_string, span.start..span.end);
    let after_span = substr_safe(source_string, span.end..u32::MAX);
    (before_span, error_span, after_span)
}

/// Converts a span and a source code string, into a [`Vec`] of [`SpannedErrorLine`]s.
/// This finds all lines that the provided [`SourceSpan`] touches.
/// Along with filling out meta-data about the line such as it's line number and where the line
/// starts within the provided `source_string`.
#[allow(clippy::cast_possible_truncation)]
#[must_use]
fn get_lines(source_string: &str, span: SourceSpan) -> Vec<SpannedErrorLine> {
    const LINE_LOOP_SAFETY: usize = 10000;

    let (raw_before, raw_error, raw_after) = get_segments_from_source(source_string, span);
    let previous_newline_index = raw_before.rfind('\n').map_or(0, |i| i.saturating_add(1));
    let start_line_number = count_new_lines(raw_before).saturating_add(1);

    let mut next_newline_index = raw_after.find('\n').unwrap_or(source_string.len());
    if substr_safe(raw_after, 0..(next_newline_index as u32)).ends_with('\r') {
        next_newline_index = next_newline_index.saturating_sub(1);
    }
    let absolute_nni = next_newline_index + raw_before.len() + raw_error.len();

    let mut buf_lines = Vec::new();

    let all_lines_containing_error = substr_safe(
        source_string,
        (previous_newline_index as u32)..(absolute_nni as u32),
    );

    let mut process_line = all_lines_containing_error;
    let mut global_line_start_index = previous_newline_index;
    let mut line_number = start_line_number;
    for _i in 0..LINE_LOOP_SAFETY {
        if process_line.is_empty() {
            break;
        }

        let next_newline_index = process_line.find('\n').unwrap_or(process_line.len());

        buf_lines.push(SpannedErrorLine {
            source: substr_safe_no_trailing_return(process_line, 0..(next_newline_index as u32))
                .trim_end()
                .to_string(),
            line_number: line_number as u32,
            line_start_global_index: global_line_start_index as u32,
            comment_lines: Vec::new(),
        });

        process_line = substr_safe(
            process_line,
            (next_newline_index as u32).saturating_add(1)..u32::MAX,
        );
        global_line_start_index += next_newline_index;
        line_number += 1;
    }

    buf_lines
}
