use crate::error::{Error, ErrorKind, Result};

pub(super) struct SkillMetadata {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) version: Option<String>,
}

pub(super) fn parse(text: &str) -> Result<SkillMetadata> {
    let mut lines = text.lines().peekable();
    if lines.next() != Some("---") {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "SKILL.md must start with YAML front matter",
        ));
    }
    let mut name = None;
    let mut description = None;
    let mut version = None;
    let mut closed = false;
    while let Some(line) = lines.next() {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let parsed = scalar(value.trim(), &mut lines);
            match key.trim() {
                "name" if !parsed.is_empty() => name = Some(parsed),
                "description" if !parsed.is_empty() => description = Some(parsed),
                "version" if !parsed.is_empty() => version = Some(parsed),
                _ => {}
            }
        }
    }
    if !closed {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "SKILL.md front matter is not closed",
        ));
    }
    Ok(SkillMetadata {
        name: name.ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill name is missing"))?,
        description: description
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "skill description is missing"))?,
        version,
    })
}

fn scalar(value: &str, lines: &mut std::iter::Peekable<std::str::Lines<'_>>) -> String {
    let Some(style) = yaml_block_style(value) else {
        return unquote(value).to_owned();
    };
    let mut parts = Vec::new();
    while lines
        .peek()
        .is_some_and(|next| next.is_empty() || next.starts_with(' ') || next.starts_with('\t'))
    {
        if let Some(next) = lines.next() {
            let next = next.trim();
            if !next.is_empty() {
                parts.push(next);
            }
        }
    }
    if style == '>' {
        parts.join(" ")
    } else {
        parts.join("\n")
    }
}

fn yaml_block_style(value: &str) -> Option<char> {
    ['>', '|'].into_iter().find(|style| {
        value
            .strip_prefix(*style)
            .is_some_and(|suffix| suffix.is_empty() || suffix == "-" || suffix == "+")
    })
}

fn unquote(value: &str) -> &str {
    let quoted = value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|inner| inner.strip_suffix('\''))
        });
    match quoted {
        Some(inner) => inner,
        None => value,
    }
}
