//! Um escritor de ZIP mínimo (sem compressão, "stored"), só para o que o `chamados` exporta: poucos
//! arquivos, nomes em UTF-8, nada de ZIP64. Evita uma dependência para um formato de poucas linhas.

use crate::AppError;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_DIRECTORY: u32 = 0x0605_4b50;
/// Versão 2.0: a mínima que sabe de arquivos "stored" e nomes UTF-8.
const VERSION: u16 = 20;
/// Bit 11: os nomes estão em UTF-8.
const UTF8_NAMES: u16 = 0x0800;

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Monta um ZIP em memória.
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Entry>,
    dos_time: u16,
    dos_date: u16,
}

/// Data e hora (UTC) de `seconds` desde 1970: ano, mês, dia, hora, minuto, segundo.
pub(crate) fn civil(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let rest = (seconds % 86_400) as u32;
    // Dias desde 1970-01-01 para o calendário gregoriano (algoritmo de Howard Hinnant).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_part + 2) / 5 + 1) as u32;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day, rest / 3_600, rest % 3_600 / 60, rest % 60)
}

/// `AAAA-MM-DDTHH:MM:SSZ`.
pub(crate) fn iso_utc(seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = civil(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn too_big(what: &str) -> AppError {
    AppError::Message(format!(
        "o ZIP não comporta {what} (limite de 4 GB e 65535 arquivos)"
    ))
}

/// `value` como `u32`, ou o erro de "não comporta" quando passa de `limit`.
fn within(value: usize, limit: usize, what: &str) -> Result<u32, AppError> {
    match value <= limit {
        true => Ok(value as u32),
        false => Err(too_big(what)),
    }
}

/// O maior tamanho que os campos de 32 bits do ZIP guardam.
const MAX_BYTES: usize = u32::MAX as usize;

impl ZipWriter {
    /// `modified` (segundos desde 1970) é a data gravada em cada arquivo; antes de 1980 vira 1980.
    pub fn new(modified: u64) -> Self {
        let (year, month, day, hour, minute, second) = civil(modified);
        let (year, month, day) = match year >= 1980 {
            true => (year, month, day),
            false => (1980, 1, 1),
        };
        Self {
            out: Vec::new(),
            entries: Vec::new(),
            dos_time: ((hour << 11) | (minute << 5) | (second / 2)) as u16,
            dos_date: (((year - 1980) as u32) << 9 | (month << 5) | day) as u16,
        }
    }

    fn u16(&mut self, value: u16) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    /// Acrescenta o arquivo `name` (com `/` entre pastas) com o conteúdo `data`.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), AppError> {
        let size = within(data.len(), MAX_BYTES, "um arquivo desse tamanho")?;
        let offset = within(self.out.len(), MAX_BYTES, "tanto conteúdo")?;
        let name_len = within(name.len(), usize::from(u16::MAX), "um nome tão longo")? as u16;
        if self.entries.len() >= usize::from(u16::MAX) {
            return Err(too_big("mais arquivos"));
        }
        let crc = crc32(data);
        self.u32(LOCAL_HEADER);
        self.u16(VERSION);
        self.u16(UTF8_NAMES);
        self.u16(0); // sem compressão
        self.u16(self.dos_time);
        self.u16(self.dos_date);
        self.u32(crc);
        self.u32(size);
        self.u32(size);
        self.u16(name_len);
        self.u16(0);
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(data);
        self.entries.push(Entry {
            name: name.to_owned(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Fecha o ZIP (diretório central) e devolve os bytes.
    pub fn finish(mut self) -> Result<Vec<u8>, AppError> {
        let start = within(self.out.len(), MAX_BYTES, "tanto conteúdo")?;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            self.u32(CENTRAL_HEADER);
            self.u16(VERSION);
            self.u16(VERSION);
            self.u16(UTF8_NAMES);
            self.u16(0);
            self.u16(self.dos_time);
            self.u16(self.dos_date);
            self.u32(entry.crc);
            self.u32(entry.size);
            self.u32(entry.size);
            self.u16(entry.name.len() as u16);
            self.u16(0); // extra
            self.u16(0); // comentário
            self.u16(0); // disco
            self.u16(0); // atributos internos
            self.u32(0); // atributos externos
            self.u32(entry.offset);
            self.out.extend_from_slice(entry.name.as_bytes());
        }
        let size = within(self.out.len(), MAX_BYTES, "tanto conteúdo")? - start;
        self.u32(END_OF_DIRECTORY);
        self.u16(0);
        self.u16(0);
        self.u16(entries.len() as u16);
        self.u16(entries.len() as u16);
        self.u32(size);
        self.u32(start);
        self.u16(0);
        Ok(self.out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn le16(bytes: &[u8], at: usize) -> usize {
        usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
    }

    fn le32(bytes: &[u8], at: usize) -> usize {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
    }

    /// Lê um ZIP "stored" pelo diretório central e confere o CRC de cada arquivo.
    pub(crate) fn read(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
        let end = zip.len() - 22;
        assert_eq!(le32(zip, end), END_OF_DIRECTORY as usize);
        let (count, mut at) = (le16(zip, end + 10), le32(zip, end + 16));
        let mut files = Vec::new();
        for _ in 0..count {
            assert_eq!(le32(zip, at), CENTRAL_HEADER as usize);
            let (crc, size, name_len) =
                (le32(zip, at + 16), le32(zip, at + 24), le16(zip, at + 28));
            let local = le32(zip, at + 42);
            let name = String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap();
            assert_eq!(le32(zip, local), LOCAL_HEADER as usize);
            let data_at = local + 30 + le16(zip, local + 26) + le16(zip, local + 28);
            let data = zip[data_at..data_at + size].to_vec();
            assert_eq!(crc32(&data) as usize, crc);
            files.push((name, data));
            at += 46 + name_len;
        }
        files
    }

    #[test]
    fn the_crc_matches_the_reference_value() {
        // O valor de referência do CRC-32 de "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn dates_are_computed_in_utc() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400 + 86_399), (2000, 2, 29, 23, 59, 59));
        assert_eq!(iso_utc(1_791_900_000), "2026-10-13T14:00:00Z");
    }

    #[test]
    fn a_zip_holds_its_files_with_utf8_names() {
        let mut zip = ZipWriter::new(1_791_900_000);
        zip.add("chamado.md", "olá, mundo\n".as_bytes()).unwrap();
        zip.add("anexos/relatório.txt", b"").unwrap();
        zip.add("anexos/b.bin", &[0, 1, 2, 255]).unwrap();
        let bytes = zip.finish().unwrap();
        let files = read(&bytes);
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["chamado.md", "anexos/relatório.txt", "anexos/b.bin"]
        );
        assert_eq!(files[0].1, "olá, mundo\n".as_bytes());
        assert_eq!(files[2].1, [0, 1, 2, 255]);
        // Data DOS: 2026-10-13 14:00:00.
        let (time, date) = (le16(&bytes, 10), le16(&bytes, 12));
        assert_eq!(
            (
                time >> 11,
                (time >> 5) & 63,
                (date >> 9) + 1980,
                (date >> 5) & 15,
                date & 31
            ),
            (14, 0, 2026, 10, 13)
        );
        // Um ZIP vazio é só o diretório central.
        assert_eq!(ZipWriter::new(0).finish().unwrap().len(), 22);
        // Antes de 1980 o ZIP não tem como dizer a data: vira 1980-01-01.
        let mut old = ZipWriter::new(0);
        old.add("a", b"x").unwrap();
        let bytes = old.finish().unwrap();
        assert_eq!((le16(&bytes, 12) >> 9) + 1980, 1980);
    }

    #[test]
    fn the_size_limits_are_checked() {
        assert_eq!(within(10, 10, "x").unwrap(), 10);
        let refused = within(11, 10, "tanto").unwrap_err().to_string();
        assert!(refused.contains("não comporta tanto"), "{refused}");
        assert_eq!(MAX_BYTES, 4_294_967_295);
    }

    #[test]
    fn what_the_format_cannot_hold_is_refused() {
        let mut zip = ZipWriter::new(0);
        assert!(zip.add(&"n".repeat(70_000), b"x").is_err());
        let mut many = ZipWriter::new(0);
        for index in 0..usize::from(u16::MAX) {
            many.add(&index.to_string(), b"").unwrap();
        }
        let refused = many.add("mais um", b"").unwrap_err().to_string();
        assert!(refused.contains("não comporta"), "{refused}");
    }
}
