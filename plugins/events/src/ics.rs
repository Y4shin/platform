//! A small hand-rolled iCalendar (RFC 5545) writer — enough for read-only export
//! of events to calendar apps (Google/Apple/Outlook). Renders a `VCALENDAR` with
//! one `VEVENT` per event; text values are escaped and lines use CRLF endings.
//! (Line folding at 75 octets is intentionally omitted — the values we emit are
//! short, and every mainstream client accepts unfolded lines.)

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The fields of one event needed to render a `VEVENT`.
#[derive(Debug, Clone)]
pub struct IcsEvent {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub updated_at: DateTime<Utc>,
}

const PRODID: &str = "-//Junius//Events//EN";

/// Render a `VCALENDAR` document (CRLF-terminated) for `events` with the given
/// calendar `name` (`X-WR-CALNAME`).
#[must_use]
pub fn render_calendar(name: &str, events: &[IcsEvent]) -> String {
    let mut out = String::new();
    line(&mut out, "BEGIN:VCALENDAR");
    line(&mut out, "VERSION:2.0");
    line(&mut out, &format!("PRODID:{PRODID}"));
    line(&mut out, "CALSCALE:GREGORIAN");
    line(&mut out, "METHOD:PUBLISH");
    line(&mut out, &format!("X-WR-CALNAME:{}", escape_text(name)));
    for ev in events {
        render_event(&mut out, ev);
    }
    line(&mut out, "END:VCALENDAR");
    out
}

fn render_event(out: &mut String, ev: &IcsEvent) {
    line(out, "BEGIN:VEVENT");
    // A stable, globally-unique id for the event.
    line(out, &format!("UID:{}@junius.events", ev.id));
    line(out, &format!("DTSTAMP:{}", fmt_utc(ev.updated_at)));
    if ev.all_day {
        line(
            out,
            &format!("DTSTART;VALUE=DATE:{}", fmt_date(ev.starts_at)),
        );
        // All-day DTEND is exclusive; default to the day after start when unset.
        let end = ev.ends_at.unwrap_or(ev.starts_at);
        let end = end + chrono::Duration::days(1);
        line(out, &format!("DTEND;VALUE=DATE:{}", fmt_date(end)));
    } else {
        line(out, &format!("DTSTART:{}", fmt_utc(ev.starts_at)));
        if let Some(end) = ev.ends_at {
            line(out, &format!("DTEND:{}", fmt_utc(end)));
        }
    }
    line(out, &format!("SUMMARY:{}", escape_text(&ev.title)));
    if let Some(desc) = &ev.description {
        line(out, &format!("DESCRIPTION:{}", escape_text(desc)));
    }
    if let Some(loc) = &ev.location {
        line(out, &format!("LOCATION:{}", escape_text(loc)));
    }
    line(out, "END:VEVENT");
}

/// Append `content` plus the iCalendar CRLF line break.
fn line(out: &mut String, content: &str) {
    out.push_str(content);
    out.push_str("\r\n");
}

/// `YYYYMMDDTHHMMSSZ` (UTC).
fn fmt_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

/// `YYYYMMDD` (for all-day `VALUE=DATE`).
fn fmt_date(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%d").to_string()
}

/// Escape a text value per RFC 5545 §3.3.11 (backslash, semicolon, comma,
/// newline). CRLFs collapse to the literal `\n` escape.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> IcsEvent {
        IcsEvent {
            id: Uuid::nil(),
            title: "Launch; Party, v2".to_string(),
            description: Some("Line1\nLine2".to_string()),
            location: Some("HQ".to_string()),
            starts_at: Utc.with_ymd_and_hms(2026, 6, 1, 18, 30, 0).unwrap(),
            ends_at: Some(Utc.with_ymd_and_hms(2026, 6, 1, 20, 0, 0).unwrap()),
            all_day: false,
            updated_at: Utc.with_ymd_and_hms(2026, 5, 25, 9, 0, 0).unwrap(),
        }
    }

    #[test]
    fn renders_vcalendar_with_escaping_and_crlf() {
        let doc = render_calendar("My Feed", &[sample()]);
        assert!(doc.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(doc.ends_with("END:VCALENDAR\r\n"));
        assert!(doc.contains("DTSTART:20260601T183000Z\r\n"));
        assert!(doc.contains("DTEND:20260601T200000Z\r\n"));
        // Special characters escaped; newline → \n.
        assert!(doc.contains("SUMMARY:Launch\\; Party\\, v2\r\n"));
        assert!(doc.contains("DESCRIPTION:Line1\\nLine2\r\n"));
        assert!(doc.contains("UID:00000000-0000-0000-0000-000000000000@junius.events\r\n"));
    }

    #[test]
    fn all_day_uses_date_values() {
        let mut ev = sample();
        ev.all_day = true;
        ev.ends_at = None;
        let doc = render_calendar("F", &[ev]);
        assert!(doc.contains("DTSTART;VALUE=DATE:20260601\r\n"));
        // Exclusive end → the next day.
        assert!(doc.contains("DTEND;VALUE=DATE:20260602\r\n"));
    }
}
