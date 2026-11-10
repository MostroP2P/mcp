//! The peer chat of an order: `read_chat` → `mostro-cli order chat` reads the
//! conversation with the counterparty (read-only); `send_chat` → `order chat
//! --message --yes` sends one message to them (state-changing). The
//! counterparty's key comes from the CLI's store or the node's messages, never
//! from the caller.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{flag, order_id, since_minutes};
use crate::error::{Result, invalid};

/// Longest message the CLI sends, in bytes once trimmed.
pub const MAX_MESSAGE_BYTES: usize = 4096;

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ReadChatParams {
    /// The id (UUID) of an order in this identity's local store.
    pub order_id: String,
    /// How far back to read, in minutes, 1 to 10080. The CLI reads the whole
    /// week when omitted.
    #[serde(default)]
    pub since_minutes: Option<u32>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SendChatParams {
    /// The id (UUID) of an order in this identity's local store.
    pub order_id: String,
    /// The text to send to the counterparty, 1 to 4096 bytes once trimmed.
    /// Line breaks and tabs are kept; other control characters are refused.
    pub message: String,
}

pub fn read_args(params: &ReadChatParams) -> Result<Vec<String>> {
    let mut args = chat("order_id", &params.order_id)?;
    if let Some(minutes) = params.since_minutes {
        args.push(flag("since", since_minutes("since_minutes", minutes)?));
    }
    Ok(args)
}

pub fn send_args(params: &SendChatParams) -> Result<Vec<String>> {
    let mut args = chat("order_id", &params.order_id)?;
    args.push(flag("message", message("message", &params.message)?));
    args.push("--yes".to_string());
    Ok(args)
}

fn chat(field: &'static str, id: &str) -> Result<Vec<String>> {
    Ok(vec![
        "order".to_string(),
        "chat".to_string(),
        flag("order", order_id(field, id)?),
    ])
}

/// A chat text: CRLF made LF, trimmed, 1 to [`MAX_MESSAGE_BYTES`] bytes, no
/// control characters but line breaks and tabs, and no invisible character
/// that would make the counterparty read other text than the user approved.
fn message(field: &'static str, text: &str) -> Result<String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    if text.chars().any(is_invisible_format) {
        return Err(invalid(
            field,
            "must not contain invisible or direction-changing characters",
        ));
    }
    if text.is_empty() {
        return Err(invalid(field, "must not be empty"));
    }
    if text.len() > MAX_MESSAGE_BYTES {
        return Err(invalid(
            field,
            format!("must be at most {MAX_MESSAGE_BYTES} bytes"),
        ));
    }
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(invalid(
            field,
            "must not contain control characters other than line breaks and tabs",
        ));
    }
    Ok(text.to_string())
}

/// Zero-width characters, direction marks, embeddings, overrides and
/// isolates, and the byte-order mark.
fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ffdc768a-37d8-429e-bfcc-0968be56b6f9";

    #[test]
    fn reading_names_the_order_and_an_optional_window() {
        let params = ReadChatParams {
            order_id: ID.to_uppercase(),
            since_minutes: None,
        };
        assert_eq!(
            read_args(&params).unwrap(),
            ["order", "chat", &format!("--order={ID}")]
        );
        let params = ReadChatParams {
            since_minutes: Some(60),
            ..params
        };
        assert_eq!(
            read_args(&params).unwrap(),
            ["order", "chat", &format!("--order={ID}"), "--since=60"]
        );
    }

    #[test]
    fn reading_never_sends() {
        let args = read_args(&ReadChatParams {
            order_id: ID.into(),
            since_minutes: None,
        })
        .unwrap();
        assert!(
            !args
                .iter()
                .any(|a| a.starts_with("--message") || a == "--yes")
        );
    }

    #[test]
    fn a_bad_window_or_id_is_refused() {
        for since in [0, 10_081] {
            let params = ReadChatParams {
                order_id: ID.into(),
                since_minutes: Some(since),
            };
            assert!(read_args(&params).is_err(), "{since}");
        }
        assert!(
            read_args(&ReadChatParams {
                order_id: "--help".into(),
                since_minutes: None
            })
            .is_err()
        );
    }

    fn send(message: &str) -> SendChatParams {
        SendChatParams {
            order_id: ID.into(),
            message: message.into(),
        }
    }

    #[test]
    fn sending_passes_the_trimmed_text_as_one_value_and_confirms() {
        assert_eq!(
            send_args(&send("  Paid by Belo.\nCheck it, please.  ")).unwrap(),
            [
                "order",
                "chat",
                &format!("--order={ID}"),
                "--message=Paid by Belo.\nCheck it, please.",
                "--yes"
            ]
        );
    }

    #[test]
    fn a_text_that_looks_like_a_flag_stays_the_message() {
        let args = send_args(&send("--yes")).unwrap();
        assert_eq!(args[3], "--message=--yes");
        assert_eq!(args.len(), 5);
    }

    #[test]
    fn crlf_line_breaks_are_kept_as_line_breaks() {
        let args = send_args(&send("Alias: a.b\r\nCBU: 123")).unwrap();
        assert_eq!(args[3], "--message=Alias: a.b\nCBU: 123");
    }

    #[test]
    fn invisible_format_characters_that_hide_or_reorder_text_are_refused() {
        for bad in ["pay a\u{202e}cba", "a\u{200b}b", "\u{2066}x\u{2069}"] {
            let error = send_args(&send(bad)).unwrap_err().to_string();
            assert!(error.contains("message"), "{bad:?}: {error}");
        }
    }

    #[test]
    fn an_empty_oversized_or_control_text_is_refused() {
        let at_limit = "é".repeat(MAX_MESSAGE_BYTES / 2);
        assert!(send_args(&send(&at_limit)).is_ok());
        for bad in [
            "   ".to_string(),
            "x".repeat(MAX_MESSAGE_BYTES + 1),
            "a\u{0}b".to_string(),
            "a\u{1b}[31mb".to_string(),
        ] {
            let error = send_args(&send(&bad)).unwrap_err().to_string();
            assert!(error.contains("message"), "{error}");
        }
    }
}
