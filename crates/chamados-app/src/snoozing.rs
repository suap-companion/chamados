//! Adiar um chamado: de quando ele volta. Datas sem biblioteca de calendário (só o calendário
//! gregoriano e um deslocamento fixo de fuso, que basta para o Brasil sem horário de verão), em
//! milissegundos desde 1970 (UTC).

use crate::{zip::civil, AppError};

const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
/// Fuso padrão: UTC-03:00 (Brasília; Natal).
pub const DEFAULT_OFFSET_MINUTES: i32 = -180;
/// O maior adiamento aceito: um ano.
const LONGEST: u64 = 366 * DAY;

fn bad(message: impl Into<String>) -> AppError {
    AppError::Message(message.into())
}

/// Os atalhos de data (os do Spark): quando o chamado volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Daqui a 3 horas.
    Later,
    /// Amanhã às 8h.
    Tomorrow,
    /// No próximo sábado, às 9h.
    Weekend,
    /// Na próxima segunda, às 8h.
    NextWeek,
}

/// O fuso `text` em minutos a leste de UTC: `-03:00`, `+0530`, `-3`, `Z` ou `UTC`.
pub fn parse_offset(text: &str) -> Result<i32, AppError> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("z") || text.eq_ignore_ascii_case("utc") {
        return Ok(0);
    }
    let wrong = || {
        bad(format!(
            "fuso inválido {text:?}: use por exemplo -03:00, +05:30 ou UTC"
        ))
    };
    let (sign, rest) = match text.chars().next() {
        Some('-') => (-1, &text[1..]),
        Some('+') => (1, &text[1..]),
        _ => return Err(wrong()),
    };
    let (hours, minutes) = match rest.split_once(':') {
        Some(parts) => parts,
        None if rest.len() == 4 => rest.split_at(2),
        None => (rest, "0"),
    };
    let numbers = (hours.parse::<i32>(), minutes.parse::<i32>());
    match numbers {
        (Ok(hours), Ok(minutes)) if (0..=14).contains(&hours) && (0..60).contains(&minutes) => {
            Ok(sign * (hours * 60 + minutes))
        }
        _ => Err(wrong()),
    }
}

/// `UTC-03:00`.
pub fn format_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    format!(
        "UTC{sign}{:02}:{:02}",
        minutes.abs() / 60,
        minutes.abs() % 60
    )
}

/// Uma duração como `30m`, `3h`, `2d`, `1w` ou somadas (`1d12h`), em milissegundos.
pub fn parse_duration(text: &str) -> Result<u64, AppError> {
    let text = text.trim();
    let wrong = || {
        bad(format!(
            "duração inválida {text:?}: use por exemplo 30m, 3h, 2d, 1w ou 1d12h"
        ))
    };
    let (mut total, mut number, mut digits) = (0_u64, 0_u64, false);
    for character in text.chars() {
        if let Some(digit) = character.to_digit(10) {
            number = number
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(digit)))
                .ok_or_else(wrong)?;
            digits = true;
            continue;
        }
        let unit = match character.to_ascii_lowercase() {
            'm' => MINUTE,
            'h' => HOUR,
            'd' => DAY,
            'w' => 7 * DAY,
            _ => return Err(wrong()),
        };
        if !digits {
            return Err(wrong());
        }
        total = number
            .checked_mul(unit)
            .and_then(|part| total.checked_add(part))
            .ok_or_else(wrong)?;
        (number, digits) = (0, false);
    }
    if digits || total == 0 {
        return Err(wrong());
    }
    match total <= LONGEST {
        true => Ok(total),
        false => Err(bad("o adiamento pode ser de no máximo um ano")),
    }
}

/// Dias desde 1970-01-01 até a data (calendário gregoriano; algoritmo de Howard Hinnant).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let day_of_year = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn days_in_month(year: i64, month: u32) -> u32 {
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    match month {
        2 => 28 + u32::from(leap),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Um momento local (`AAAA-MM-DD`, `AAAA-MM-DD HH:MM` ou com `T`) no fuso `offset`, em ms desde 1970.
/// Só a data vale 8h.
pub fn parse_local(text: &str, offset: i32) -> Result<u64, AppError> {
    let text = text.trim();
    let wrong = || {
        bad(format!(
            "data inválida {text:?}: use AAAA-MM-DD ou AAAA-MM-DD HH:MM"
        ))
    };
    let (date, time) = text.split_once([' ', 'T']).unwrap_or((text, "08:00"));
    let parts: Vec<&str> = date.split('-').collect();
    let clock: Vec<&str> = time.trim().split(':').collect();
    let numbers = |parts: &[&str]| -> Option<Vec<u32>> {
        parts.iter().map(|part| part.parse().ok()).collect()
    };
    let (Some(day), Some(hm)) = (numbers(&parts), numbers(&clock)) else {
        return Err(wrong());
    };
    let (&[year, month, day], &[hour, minute]) = (&day[..], &hm[..]) else {
        return Err(wrong());
    };
    let valid = (1..=12).contains(&month)
        && (1..=days_in_month(i64::from(year), month)).contains(&day)
        && hour < 24
        && minute < 60
        && year >= 1970;
    if !valid {
        return Err(wrong());
    }
    let days = days_from_civil(i64::from(year), month, day);
    let local = days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60;
    u64::try_from((local - i64::from(offset) * 60) * 1_000).map_err(|_| wrong())
}

/// O momento de um atalho, contado a partir de `now` (ms desde 1970), no fuso `offset`.
pub fn at(now: u64, offset: i32, when: When) -> u64 {
    if when == When::Later {
        return now + 3 * HOUR;
    }
    let local = (now / 1_000) as i64 + i64::from(offset) * 60;
    let today = local.div_euclid(86_400);
    let weekday = (today + 4).rem_euclid(7); // 0 = domingo (1970-01-01 foi quinta)
    let (days_ahead, hour) = match when {
        When::Tomorrow => (1, 8),
        When::Weekend => (
            match (6 - weekday + 7) % 7 {
                0 => 7,
                days => days,
            },
            9,
        ),
        _ => (
            match (1 - weekday + 7) % 7 {
                0 => 7,
                days => days,
            },
            8,
        ),
    };
    let target = (today + days_ahead) * 86_400 + hour * 3_600;
    ((target - i64::from(offset) * 60) * 1_000) as u64
}

/// `DD/MM/AAAA HH:MM` no fuso `offset`.
pub fn format_local(moment: u64, offset: i32) -> String {
    let local = (moment / 1_000) as i64 + i64::from(offset) * 60;
    let (year, month, day, hour, minute, _) = civil(local.max(0) as u64);
    format!("{day:02}/{month:02}/{year:04} {hour:02}:{minute:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-14 (quarta) 15:30 em UTC-03:00 = 18:30 UTC.
    fn wednesday() -> u64 {
        parse_local("2026-10-14 15:30", -180).unwrap()
    }

    #[test]
    fn offsets_are_read_in_the_usual_spellings() {
        for (text, minutes) in [
            ("-03:00", -180),
            ("+0530", 330),
            ("-3", -180),
            ("+5:30", 330),
            (" Z ", 0),
            ("utc", 0),
            ("+14", 840),
        ] {
            assert_eq!(parse_offset(text).unwrap(), minutes, "{text}");
        }
        for text in ["", "3", "-15:00", "-03:60", "-ab", "+1:xx", "x"] {
            assert!(
                parse_offset(text)
                    .unwrap_err()
                    .to_string()
                    .contains("fuso inválido"),
                "{text}"
            );
        }
        assert_eq!(format_offset(-180), "UTC-03:00");
        assert_eq!(format_offset(330), "UTC+05:30");
        assert_eq!(format_offset(0), "UTC+00:00");
    }

    #[test]
    fn durations_add_up_and_refuse_nonsense() {
        assert_eq!(parse_duration("30m").unwrap(), 30 * MINUTE);
        assert_eq!(parse_duration("3H").unwrap(), 3 * HOUR);
        assert_eq!(parse_duration("2d").unwrap(), 2 * DAY);
        assert_eq!(parse_duration("1w").unwrap(), 7 * DAY);
        assert_eq!(parse_duration(" 1d12h ").unwrap(), DAY + 12 * HOUR);
        for text in [
            "",
            "h",
            "3",
            "0m",
            "3x",
            "1d2",
            "99999999999999999999d",
            "999999999999999999d",
        ] {
            assert!(
                parse_duration(text)
                    .unwrap_err()
                    .to_string()
                    .contains("duração inválida"),
                "{text}"
            );
        }
        let long = parse_duration("367d").unwrap_err().to_string();
        assert!(long.contains("no máximo um ano"), "{long}");
        assert!(parse_duration("366d").is_ok());
    }

    #[test]
    fn local_moments_are_converted_to_utc() {
        // 2026-10-14 15:30 em UTC-03:00 é 18:30 UTC.
        assert_eq!(format_local(wednesday(), 0), "14/10/2026 18:30");
        assert_eq!(format_local(wednesday(), -180), "14/10/2026 15:30");
        assert_eq!(parse_local("2026-10-14T15:30", -180).unwrap(), wednesday());
        // Só a data: 8h.
        assert_eq!(
            format_local(parse_local("2026-10-20", -180).unwrap(), -180),
            "20/10/2026 08:00"
        );
        // Bissexto e fim de mês.
        assert!(parse_local("2028-02-29", 0).is_ok() && parse_local("2100-02-29", 0).is_err());
        assert!(
            parse_local("2026-04-31", 0).is_err() && parse_local("2026-12-31 23:59", 0).is_ok()
        );
        for text in [
            "",
            "amanhã",
            "2026-13-01",
            "2026-00-10",
            "2026-10-00",
            "2026-10-14 24:00",
            "2026-10-14 10:60",
            "2026-10",
            "2026-10-14 10",
            "1969-12-31",
            "x-y-z",
            "2026-10-14 a:b",
        ] {
            assert!(
                parse_local(text, 0)
                    .unwrap_err()
                    .to_string()
                    .contains("data inválida"),
                "{text}"
            );
        }
        // Antes de 1970 em UTC (a data local existe, o instante não).
        assert!(parse_local("1970-01-01 00:00", 60).is_err());
    }

    #[test]
    fn the_shortcuts_land_where_spark_would_put_them() {
        let (now, offset) = (wednesday(), -180);
        let show = |when| format_local(at(now, offset, when), offset);
        assert_eq!(show(When::Later), "14/10/2026 18:30");
        assert_eq!(show(When::Tomorrow), "15/10/2026 08:00");
        assert_eq!(show(When::Weekend), "17/10/2026 09:00");
        assert_eq!(show(When::NextWeek), "19/10/2026 08:00");

        // No sábado, o fim de semana é o seguinte; no domingo, o sábado que vem; na segunda, a outra segunda.
        let on = |text: &str, when| {
            format_local(at(parse_local(text, offset).unwrap(), offset, when), offset)
        };
        assert_eq!(on("2026-10-17 10:00", When::Weekend), "24/10/2026 09:00");
        assert_eq!(on("2026-10-18 10:00", When::Weekend), "24/10/2026 09:00");
        assert_eq!(on("2026-10-19 10:00", When::NextWeek), "26/10/2026 08:00");
        assert_eq!(on("2026-10-18 23:59", When::NextWeek), "19/10/2026 08:00");
        // O dia vem do fuso, não de UTC: 23h de terça em UTC-03:00 já é quarta em UTC.
        assert_eq!(on("2026-10-13 23:00", When::Tomorrow), "14/10/2026 08:00");
        let utc = parse_local("2026-10-13 23:00", 0).unwrap();
        assert_eq!(
            format_local(at(utc, 0, When::Tomorrow), 0),
            "14/10/2026 08:00"
        );
        assert!(at(now, offset, When::Tomorrow) > now);
    }
}
