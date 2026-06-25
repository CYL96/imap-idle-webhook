use anyhow::Result;
use mailparse::{parse_mail, MailHeaderMap, ParsedMail};
use serde::{Deserialize, Serialize};

const MAX_TEXT_CHARS: usize = 50_000;
const TRUNCATION_SUFFIX: &str = "\n\n[message truncated: original text exceeded 50000 chars]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedEmail {
    pub event: String,
    pub account: String,
    pub folder: String,
    pub uid: u32,
    pub message_id: Option<String>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
    pub text: Option<String>,
    pub raw_rfc822_base64: String,
}

pub fn parse_message(raw: &[u8], account: &str, folder: &str, uid: u32) -> Result<ParsedEmail> {
    let message = parse_mail(raw)?;

    Ok(ParsedEmail {
        event: "email.received".to_owned(),
        account: account.to_owned(),
        folder: folder.to_owned(),
        uid,
        message_id: header(&message, "Message-ID"),
        from: header(&message, "From"),
        to: address_list(&message, "To"),
        cc: address_list(&message, "Cc"),
        subject: header(&message, "Subject"),
        date: header(&message, "Date"),
        text: plain_text(&message),
        raw_rfc822_base64: String::new(),
    })
}

fn header(message: &ParsedMail<'_>, name: &str) -> Option<String> {
    message.headers.get_first_value(name)
}

fn address_list(message: &ParsedMail<'_>, name: &str) -> Vec<String> {
    header(message, name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn plain_text(message: &ParsedMail<'_>) -> Option<String> {
    body_part(message, "text/plain")
        .or_else(|| body_part(message, "text/html").map(|body| html_to_text(&body)))
        .map(truncate_text)
}

fn truncate_text(text: String) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text;
    }

    text.chars()
        .take(MAX_TEXT_CHARS)
        .chain(TRUNCATION_SUFFIX.chars())
        .collect()
}

fn body_part(message: &ParsedMail<'_>, mimetype: &str) -> Option<String> {
    if is_attachment(message) {
        return None;
    }

    if !message.subparts.is_empty() {
        return message
            .subparts
            .iter()
            .find_map(|subpart| body_part(subpart, mimetype));
    }

    if message.ctype.mimetype.eq_ignore_ascii_case(mimetype) {
        return message
            .get_body()
            .ok()
            .filter(|body| !body.trim().is_empty());
    }

    None
}

fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut previous_was_space = false;

    for character in html.chars() {
        match character {
            '<' => {
                in_tag = true;
                if !previous_was_space {
                    text.push(' ');
                    previous_was_space = true;
                }
            }
            '>' => in_tag = false,
            _ if in_tag => {}
            character if character.is_whitespace() => {
                if !previous_was_space {
                    text.push(' ');
                    previous_was_space = true;
                }
            }
            character => {
                text.push(character);
                previous_was_space = false;
            }
        }
    }

    decode_html_entities(text.trim())
}

fn decode_html_entities(html: &str) -> String {
    let mut decoded = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        rest = &rest[start..];

        let Some(end) = rest.find(';') else {
            decoded.push_str(rest);
            return decoded;
        };

        let entity = &rest[1..end];
        if let Some(character) = decode_html_entity(entity) {
            decoded.push(character);
        } else {
            decoded.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }

    decoded.push_str(rest);
    decoded
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "pound" => Some('£'),
        entity if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        entity if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn is_attachment(message: &ParsedMail<'_>) -> bool {
    header(message, "Content-Disposition")
        .map(|value| value.to_ascii_lowercase().starts_with("attachment"))
        .unwrap_or(false)
}
