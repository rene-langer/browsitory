//! The headers of a `git format-patch` mail (#658): who wrote the change, when, and its message,
//! read the way `git am` (through `git mailinfo`) reads them, for "Apply as commits".
//!
//! Only what `git format-patch` and libgit2 write is understood: `From:`, `Date:` and `Subject:`
//! headers (folded lines joined, RFC 2047 encoded words decoded), the `[PATCH …]` prefix
//! dropped from the subject, and the message body up to the `---` line before the diff.

/// What a mail says about its commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the epoch and the offset from UTC in minutes, when `Date:` could be read.
    pub time: Option<(i64, i32)>,
    /// The subject without its `[PATCH n/m]` prefix.
    pub subject: String,
    /// The message body (without the subject), trailing blank lines removed.
    pub body: String,
}

impl Mail {
    /// The commit message: the subject, then the body after a blank line.
    pub fn message(&self) -> String {
        if self.body.is_empty() {
            format!("{}\n", self.subject)
        } else {
            format!("{}\n\n{}\n", self.subject, self.body)
        }
    }
}

/// `part`'s mail headers, or `None` when it isn't a mail (a plain diff, or a mail without a
/// `From:` header naming its author).
pub fn parse(part: &[u8]) -> Option<Mail> {
    let text = String::from_utf8_lossy(part);
    let mut lines = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut first = true;
    for line in lines.by_ref() {
        if first {
            first = false;
            // The mbox separator `From <id> Mon Sep 17 00:00:00 2001`.
            if line.starts_with("From ") {
                continue;
            }
        }
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            let (_, value) = headers.last_mut()?;
            value.push(' ');
            value.push_str(line.trim());
            continue;
        }
        let (name, value) = line.split_once(':')?;
        if name.is_empty() || name.contains(char::is_whitespace) {
            return None;
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| decode_words(v))
    };
    let (author_name, author_email) = parse_address(&header("from")?)?;
    let subject = strip_prefix(&header("subject").unwrap_or_default());
    let time = header("date").and_then(|d| parse_date(&d));
    let mut body: Vec<&str> = Vec::new();
    for line in lines {
        if line == "---" || line.starts_with("diff --git ") || line.starts_with("Index: ") {
            break;
        }
        body.push(line);
    }
    while body.last().is_some_and(|l| l.trim().is_empty()) {
        body.pop();
    }
    while body.first().is_some_and(|l| l.trim().is_empty()) {
        body.remove(0);
    }
    Some(Mail {
        author_name,
        author_email,
        time,
        subject,
        body: body.join("\n"),
    })
}

/// `Name <email>`, `"Name" <email>` or a bare address. An empty name becomes the address, as a
/// signature needs one.
fn parse_address(value: &str) -> Option<(String, String)> {
    let (name, email) = match (value.rfind('<'), value.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            (value[..open].trim(), value[open + 1..close].trim())
        }
        _ => ("", value.trim()),
    };
    if email.is_empty() {
        return None;
    }
    let name = name
        .strip_prefix('"')
        .and_then(|n| n.strip_suffix('"'))
        .unwrap_or(name)
        .replace("\\\"", "\"");
    let name = if name.trim().is_empty() {
        email.to_string()
    } else {
        name.trim().to_string()
    };
    Some((name, email.to_string()))
}

/// The subject without the leading `[PATCH …]` groups `git format-patch` adds.
fn strip_prefix(subject: &str) -> String {
    let mut rest = subject.trim();
    while let Some(after) = rest.strip_prefix('[') {
        match after.find(']') {
            Some(end) => rest = after[end + 1..].trim_start(),
            None => break,
        }
    }
    rest.to_string()
}

/// Decodes RFC 2047 encoded words (`=?UTF-8?q?…?=`, `=?UTF-8?b?…?=`); whitespace between two
/// encoded words is dropped. Anything that doesn't decode is kept as it is.
fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut after_word = false;
    while !rest.is_empty() {
        let Some(start) = rest.find("=?") else {
            out.push_str(rest);
            break;
        };
        let (before, word_start) = rest.split_at(start);
        let decoded = encoded_word(word_start);
        match decoded {
            Some((text, used)) => {
                if !(after_word && before.trim().is_empty()) {
                    out.push_str(before);
                }
                out.push_str(&text);
                rest = &word_start[used..];
                after_word = true;
            }
            None => {
                out.push_str(before);
                out.push_str("=?");
                rest = &word_start[2..];
                after_word = false;
            }
        }
    }
    out
}

/// The text of the encoded word at the start of `s` and how many bytes it took.
fn encoded_word(s: &str) -> Option<(String, usize)> {
    let inner = s.strip_prefix("=?")?;
    let (charset, inner) = inner.split_once('?')?;
    let (encoding, inner) = inner.split_once('?')?;
    let end = inner.find("?=")?;
    let payload = &inner[..end];
    let used = 2 + charset.len() + 1 + encoding.len() + 1 + end + 2;
    let bytes = match encoding {
        "Q" | "q" => decode_q(payload)?,
        "B" | "b" => decode_base64(payload)?,
        _ => return None,
    };
    let charset = charset.split('*').next().unwrap_or(charset);
    let text =
        if charset.eq_ignore_ascii_case("iso-8859-1") || charset.eq_ignore_ascii_case("latin1") {
            bytes.iter().map(|&b| char::from(b)).collect()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
    Some((text, used))
}

fn decode_q(payload: &str) -> Option<Vec<u8>> {
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => out.push(b' '),
            b'=' => {
                let hex = payload.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            other => out.push(other),
        }
        i += 1;
    }
    Some(out)
}

fn decode_base64(payload: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in payload.bytes().filter(|&c| c != b'=') {
        buffer = (buffer << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// An RFC 2822 date (`Tue, 10 Nov 2026 12:34:56 +0100`): seconds since the epoch and the
/// offset in minutes.
fn parse_date(value: &str) -> Option<(i64, i32)> {
    let value = match value.split_once(',') {
        Some((_, rest)) => rest,
        None => value,
    };
    let mut parts = value.split_whitespace();
    let day: i64 = parts.next()?.parse().ok()?;
    let month = parts.next()?.to_ascii_lowercase();
    let month = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .position(|m| month.starts_with(m))? as i64
        + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next().map_or(Some(0), |s| s.parse().ok())?;
    let zone = parts.next().unwrap_or("+0000");
    let (sign, digits) = match zone.as_bytes().first()? {
        b'+' => (1, &zone[1..]),
        b'-' => (-1, &zone[1..]),
        _ => (1, "0000"),
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let offset =
        sign * (digits[..2].parse::<i32>().ok()? * 60 + digits[2..].parse::<i32>().ok()?);
    if !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some((local - i64::from(offset) * 60, offset))
}

/// Days from 1970-01-01 to the given date in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_format_patch_mail() {
        let mail = parse(
            b"From 1234567890123456789012345678901234567890 Mon Sep 17 00:00:00 2001\n\
              From: Ada Lovelace <ada@example.com>\n\
              Date: Tue, 10 Nov 2026 12:34:56 +0100\n\
              Subject: [PATCH 2/3] Fix the\n thing\n\
              \n\
              Longer text.\n\
              \n\
              Second paragraph.\n\
              ---\n f | 1 +\n\ndiff --git a/f b/f\n",
        )
        .unwrap();
        assert_eq!(mail.author_name, "Ada Lovelace");
        assert_eq!(mail.author_email, "ada@example.com");
        assert_eq!(mail.subject, "Fix the thing");
        assert_eq!(mail.body, "Longer text.\n\nSecond paragraph.");
        assert_eq!(mail.time, Some((1_794_310_496, 60)));
        assert_eq!(
            mail.message(),
            "Fix the thing\n\nLonger text.\n\nSecond paragraph.\n"
        );
    }

    #[test]
    fn a_plain_diff_is_not_a_mail() {
        assert_eq!(parse(b"diff --git a/f b/f\n--- a/f\n+++ b/f\n"), None);
        assert_eq!(parse(b"Subject: no author\n\nbody\n"), None);
    }

    #[test]
    fn decodes_encoded_words() {
        assert_eq!(
            decode_words("=?UTF-8?q?J=C3=BCrgen_M?= =?UTF-8?q?=C3=BCller?= <j@x>"),
            "Jürgen Müller <j@x>"
        );
        assert_eq!(decode_words("=?utf-8?b?SsO8cmdlbg==?="), "Jürgen");
        assert_eq!(decode_words("=?ISO-8859-1?Q?J=FCrgen?="), "Jürgen");
        assert_eq!(decode_words("plain =? text"), "plain =? text");
    }

    #[test]
    fn reads_addresses() {
        assert_eq!(
            parse_address("\"Doe, Jane\" <jane@x>"),
            Some(("Doe, Jane".into(), "jane@x".into()))
        );
        assert_eq!(
            parse_address("jane@x"),
            Some(("jane@x".into(), "jane@x".into()))
        );
        assert_eq!(parse_address("<>"), None);
    }

    #[test]
    fn reads_dates() {
        assert_eq!(parse_date("Thu, 1 Jan 1970 00:00:00 +0000"), Some((0, 0)));
        assert_eq!(parse_date("1 Jan 1970 01:00:00 -0130"), Some((9000, -90)));
        assert_eq!(parse_date("not a date"), None);
        assert_eq!(parse_date("Thu, 1 Jan 1970 25:00:00 +0000"), None);
    }

    #[test]
    fn strips_the_patch_prefix() {
        assert_eq!(strip_prefix("[PATCH v2 1/2] [RFC] Do it"), "Do it");
        assert_eq!(strip_prefix("No prefix"), "No prefix");
    }
}
