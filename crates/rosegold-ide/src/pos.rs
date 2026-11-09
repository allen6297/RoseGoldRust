/// Java `SourcePos`. Offsets are Unicode scalar indexes, which match Java
/// `String` indexes for the ASCII sources the language service is given.

pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

pub fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    if text.is_empty() || offset == 0 {
        return (1, 1);
    }
    let n = offset.min(char_len(text));
    for (i, c) in text.chars().enumerate() {
        if i >= n {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn offset(text: &str, line: u32, col: u32) -> usize {
    if text.is_empty() {
        return 0;
    }
    let chars: Vec<char> = text.chars().collect();
    let line_index = line.saturating_sub(1) as usize;
    let mut i = 0usize;
    let mut current = 0usize;
    while i < chars.len() && current < line_index {
        if chars[i] == '\n' {
            current += 1;
        }
        i += 1;
    }
    let mut line_end = i;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }
    if line_end > i && chars[line_end - 1] == '\r' {
        line_end -= 1;
    }
    let col_off = col.saturating_sub(1) as usize;
    (i + col_off).min(line_end)
}

pub fn clamp_offset(source: &str, offset: usize) -> usize {
    offset.min(char_len(source))
}

pub fn is_ws(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\u{000b}'
            | '\u{000c}'
            | '\r'
            | '\u{001c}'
            | '\u{001d}'
            | '\u{001e}'
            | '\u{001f}'
    ) || ('\u{2000}'..='\u{200a}').contains(&c)
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{3000}')
}
