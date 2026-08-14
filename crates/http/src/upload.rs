//! Разбор загружаемых файлов multipart - общий для всех маршрутов, которые
//! кладут содержимое в бакет `dossiers` (FR-401, FR-905, п. 88, п. 91).
//!
//! Почему разбор общий, а не по месту. Бакет досье живет под Object Lock
//! в режиме compliance на пять лет (INV-042, ADR-0004), и права `s3:DeleteObject`
//! у приложения нет намеренно: объект, попавший туда, не удалит ни api,
//! ни администратор хранилища. Значит цена лишнего `put` - не мусор, а занятое
//! на пять лет место на том же диске, где лежит Postgres. Поэтому все, что можно
//! отсеять до записи, отсеивается здесь: чужая часть формы, неизвестный формат,
//! размер сверх потолка, имя файла с путем внутри.
//!
//! Чего этот модуль намеренно не делает: он не знает, чья заявка и не закрыт ли
//! прием. Проверка прав, принадлежности и сроков - забота вызывающего, и она
//! обязана стоять ДО вызова [`take_file`], иначе отсев начнется уже после того,
//! как объект лег в бакет.

use axum::body::Bytes;

use crate::error::ApiError;
use crate::request::Multipart;

/// Потолок на один прикладываемый файл.
///
/// Отдельный от глобального лимита тела (20 МБ, `lib.rs`): тот один на весь
/// запрос и потому втрое выше здравого размера скана. Ориентир - лист А4,
/// отсканированный в 300 dpi: цветной JPEG ~2 МБ, многостраничный PDF договора
/// с приложениями - единицы мегабайт. 10 МБ покрывают их с запасом и вдвое
/// уменьшают то, что один запрос способен уложить в WORM.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// Потолок длины имени файла: столько же, сколько принимает большинство
/// файловых систем, и столько же, сколько принимала прежняя обрезка в
/// `applications.rs`.
const MAX_FILENAME_CHARS: usize = 255;

/// Разобранная часть формы, готовая к записи в хранилище.
pub struct UploadedFile {
    /// Имя без разделителей пути и управляющих символов - его безопасно
    /// подставлять и в ключ объекта, и в `Content-Disposition` при выдаче
    pub filename: String,
    /// Канонический тип из белого списка, а не строка со слов клиента:
    /// в метаданные и в заголовок ответа уходит именно он
    pub content_type: &'static str,
    pub bytes: Bytes,
}

/// Debug пишется вручную, а не выводится: содержимое скана - это сведения
/// о заявителе, и в логах ему делать нечего (NFR-07). Наружу идет только
/// то, по чему разбирают отказ.
impl std::fmt::Debug for UploadedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadedFile")
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.bytes.len())
            .finish()
    }
}

/// Формат, допустимый в досье.
struct Format {
    content_type: &'static str,
    /// Расширения имени, соответствующие типу; первое - каноническое
    extensions: &'static [&'static str],
    /// Сигнатуры начала файла (годится любая из перечисленных)
    signatures: &'static [&'static [u8]],
}

/// Белый список форматов досье.
///
/// Состав определяется тем, что вообще попадает в тендерное досье: это сканы
/// бумажных документов и печатные формы приложений. Отсюда PDF (печатные формы
/// системы и многостраничные сканы) и три растровых формата, которые выдают
/// сканеры и камеры телефонов: JPEG, PNG, TIFF. Ничего исполняемого, архивов
/// и офисных документов с макросами в перечне нет - в досье они не нужны,
/// а храниться будут пять лет без возможности удалить.
///
/// HEIC/HEIF не включен сознательно: его сигнатура стоит не в начале файла,
/// дешевой проверкой первых байтов он не опознается, а сканеры его не выдают.
const FORMATS: &[Format] = &[
    Format {
        content_type: "application/pdf",
        extensions: &["pdf"],
        signatures: &[b"%PDF-"],
    },
    Format {
        content_type: "image/jpeg",
        extensions: &["jpg", "jpeg"],
        signatures: &[&[0xFF, 0xD8, 0xFF]],
    },
    Format {
        content_type: "image/png",
        extensions: &["png"],
        signatures: &[&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]],
    },
    Format {
        content_type: "image/tiff",
        extensions: &["tif", "tiff"],
        // Порядок байтов в TIFF задает сам файл: "II" - little-endian,
        // "MM" - big-endian
        signatures: &[b"II\x2A\x00", b"MM\x00\x2A"],
    },
];

/// Забирает из формы ровно одну часть с именем `field_name` и проверяет ее.
///
/// Остальные части не читаются в память и тем более не попадают в хранилище:
/// на один запрос в бакет обязан уходить ровно один объект. Прежний разбор
/// в `contracts.rs`/`acts.rs` клал в бакет каждую часть, а в БД записывал ключ
/// только последней - все прочие оставались несносимыми сиротами.
///
/// Место для квоты на пользователя - здесь же, рядом с потолком размера: объем,
/// уложенный участником в досье за сутки, считать и отказывать сверх него.
/// Сейчас квоты нет намеренно, ограничением частоты на маршрутах загрузки
/// занимается отдельная задача.
pub async fn take_file(
    multipart: &mut Multipart,
    field_name: &str,
    fallback_filename: &str,
    max_bytes: usize,
) -> Result<UploadedFile, ApiError> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::Validation(err.to_string()))?
    {
        if field.name() != Some(field_name) {
            continue;
        }

        let filename = named_or(field.file_name().unwrap_or_default(), fallback_filename);
        let declared = field.content_type().unwrap_or_default().to_owned();

        // Чтение кусками, а не `field.bytes()`: потолок обязан сработать
        // до того, как файл целиком окажется в памяти процесса
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|err| ApiError::Validation(err.to_string()))?
        {
            if buffer.len() + chunk.len() > max_bytes {
                return Err(ApiError::PayloadTooLarge(format!(
                    "файл больше {} МБ",
                    max_bytes / (1024 * 1024)
                )));
            }
            buffer.extend_from_slice(&chunk);
        }

        return inspect(filename, &declared, buffer);
    }

    Err(ApiError::Validation(format!(
        "часть '{field_name}' отсутствует"
    )))
}

/// Сверка заявленного типа с именем и содержимым.
fn inspect(filename: String, declared: &str, bytes: Vec<u8>) -> Result<UploadedFile, ApiError> {
    if bytes.is_empty() {
        return Err(ApiError::Validation("файл пуст".to_owned()));
    }

    // Тип содержимого приходит со слов клиента, поэтому проверок три:
    // сам тип должен быть в перечне, расширение имени - соответствовать типу,
    // а первые байты - нести его сигнатуру. Порознь каждую обходят тривиально
    let media_type = declared
        .split(';')
        .next()
        .unwrap_or(declared)
        .trim()
        .to_ascii_lowercase();
    let format = FORMATS
        .iter()
        .find(|format| format.content_type == media_type)
        .ok_or_else(|| {
            ApiError::UnsupportedMediaType(format!(
                "в досье принимаются только {}; заявлен '{media_type}'",
                allowed_types()
            ))
        })?;

    if let Some(extension) = extension_of(&filename)
        && !format.extensions.contains(&extension.as_str())
    {
        return Err(ApiError::Validation(format!(
            "расширение '.{extension}' не соответствует типу '{}'",
            format.content_type
        )));
    }

    if !format
        .signatures
        .iter()
        .any(|signature| bytes.starts_with(signature))
    {
        return Err(ApiError::Validation(format!(
            "содержимое файла не похоже на '{}'",
            format.content_type
        )));
    }

    // Имя без расширения получает каноническое: в досье файл потом ищут
    // глазами, и «attachment» без хвоста не подсказывает, чем его открывать
    let filename = match extension_of(&filename) {
        Some(_) => filename,
        None => match format.extensions.first() {
            Some(extension) => with_extension(&filename, extension),
            None => filename,
        },
    };

    Ok(UploadedFile {
        filename,
        content_type: format.content_type,
        bytes: Bytes::from(bytes),
    })
}

/// Очистка имени файла от всего, что меняет смысл строки-приемника.
///
/// Имя приходит от клиента и попадает и в ключ объекта хранилища, и в заголовок
/// `Content-Disposition` при выдаче. Разделители пути, кавычки, точка с запятой
/// и управляющие символы там означают не букву имени, а границу поля или
/// каталога. Возвращает пустую строку, если после очистки ничего не осталось.
pub fn sanitize_filename(name: &str) -> String {
    // Каталоги отбрасываются целиком: имени файла они не принадлежат,
    // а «..» в ключе объекта - уже путешествие по бакету
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '"' | '\'' | ';' | ':' | '*' | '?' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .take(MAX_FILENAME_CHARS)
        .collect();
    cleaned.trim_matches(['.', ' ', '_']).to_owned()
}

/// Очищенное имя либо запасное, если от исходного ничего не осталось.
fn named_or(name: &str, fallback: &str) -> String {
    let cleaned = sanitize_filename(name);
    if cleaned.is_empty() {
        sanitize_filename(fallback)
    } else {
        cleaned
    }
}

/// Расширение в нижнем регистре; `None` - имя без расширения.
fn extension_of(filename: &str) -> Option<String> {
    let (stem, extension) = filename.rsplit_once('.')?;
    if stem.is_empty() || extension.is_empty() {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

/// Приписывает расширение, не выходя за потолок длины имени.
fn with_extension(name: &str, extension: &str) -> String {
    let keep = MAX_FILENAME_CHARS.saturating_sub(extension.len() + 1);
    let base: String = name.chars().take(keep).collect();
    format!("{base}.{extension}")
}

fn allowed_types() -> String {
    FORMATS
        .iter()
        .map(|format| format.content_type)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest as _;
    use axum::http::{Request, header};

    use super::*;

    const BOUNDARY: &str = "tou-rent-test-boundary";
    const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];

    fn part(name: &str, filename: Option<&str>, content_type: &str, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        let disposition = match filename {
            Some(filename) => {
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"")
            }
            None => format!("Content-Disposition: form-data; name=\"{name}\""),
        };
        out.extend_from_slice(disposition.as_bytes());
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(b"\r\n");
        out
    }

    /// Разбор формы тем же экстрактором, что и в бою.
    async fn take(parts: &[Vec<u8>], max_bytes: usize) -> Result<UploadedFile, ApiError> {
        let mut body: Vec<u8> = parts.concat();
        body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());

        let request = Request::builder()
            .method("POST")
            .uri("/upload")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .expect("request");

        let mut multipart = Multipart::from_request(request, &())
            .await
            .expect("multipart");
        take_file(&mut multipart, "file", "attachment", max_bytes).await
    }

    #[tokio::test]
    async fn scan_of_allowed_format_passes() {
        let file = take(
            &[part("file", Some("scan.pdf"), "application/pdf", PDF)],
            MAX_FILE_BYTES,
        )
        .await
        .expect("файл принят");

        assert_eq!(file.filename, "scan.pdf");
        assert_eq!(file.content_type, "application/pdf");
        assert_eq!(file.bytes.len(), PDF.len());
    }

    /// Тип из белого списка - не то же, что тип со слов клиента: архив
    /// в досье не нужен, а удалить его оттуда потом нельзя.
    #[tokio::test]
    async fn foreign_media_type_is_refused() {
        let error = take(
            &[part(
                "file",
                Some("dossier.zip"),
                "application/zip",
                b"PK\x03\x04",
            )],
            MAX_FILE_BYTES,
        )
        .await
        .expect_err("формат вне перечня");

        assert!(
            matches!(error, ApiError::UnsupportedMediaType(_)),
            "{error}"
        );
    }

    /// Заявленный тип не принимается на слово: первые байты обязаны его
    /// подтвердить, иначе в досье лежит что угодно под видом PDF.
    #[tokio::test]
    async fn content_must_match_declared_type() {
        let error = take(
            &[part(
                "file",
                Some("scan.pdf"),
                "application/pdf",
                b"PK\x03\x04 not a pdf",
            )],
            MAX_FILE_BYTES,
        )
        .await
        .expect_err("сигнатура не совпала");

        assert!(matches!(error, ApiError::Validation(_)), "{error}");
    }

    #[tokio::test]
    async fn extension_must_match_declared_type() {
        let error = take(
            &[part("file", Some("scan.exe"), "application/pdf", PDF)],
            MAX_FILE_BYTES,
        )
        .await
        .expect_err("расширение не совпало");

        assert!(matches!(error, ApiError::Validation(_)), "{error}");
    }

    /// Потолок на файл срабатывает раньше, чем тело целиком окажется
    /// в памяти, и отдельно от глобального лимита тела запроса.
    #[tokio::test]
    async fn file_over_cap_is_refused() {
        let error = take(&[part("file", Some("scan.pdf"), "application/pdf", PDF)], 8)
            .await
            .expect_err("файл больше потолка");

        assert!(matches!(error, ApiError::PayloadTooLarge(_)), "{error}");
    }

    /// Забирается ровно одна названная часть: прежний разбор складывал
    /// в бакет каждую, а в БД записывал ключ только последней.
    #[tokio::test]
    async fn only_the_named_field_is_taken() {
        let file = take(
            &[
                part("cover", Some("cover.jpg"), "image/jpeg", JPEG),
                part("file", Some("scan.pdf"), "application/pdf", PDF),
                part("extra", Some("extra.jpg"), "image/jpeg", JPEG),
            ],
            MAX_FILE_BYTES,
        )
        .await
        .expect("файл принят");

        assert_eq!(file.filename, "scan.pdf");
        assert_eq!(file.content_type, "application/pdf");
    }

    #[tokio::test]
    async fn missing_field_is_refused() {
        let error = take(
            &[part("cover", Some("cover.jpg"), "image/jpeg", JPEG)],
            MAX_FILE_BYTES,
        )
        .await
        .expect_err("нужной части нет");

        assert!(matches!(error, ApiError::Validation(_)), "{error}");
    }

    #[tokio::test]
    async fn empty_file_is_refused() {
        let error = take(
            &[part("file", Some("scan.pdf"), "application/pdf", b"")],
            MAX_FILE_BYTES,
        )
        .await
        .expect_err("пустой файл");

        assert!(matches!(error, ApiError::Validation(_)), "{error}");
    }

    /// Путь внутри имени - это ключ объекта в чужом каталоге бакета
    /// и подмена границы поля в `Content-Disposition`.
    #[tokio::test]
    async fn path_and_quotes_are_stripped_from_filename() {
        let file = take(
            &[part(
                "file",
                Some("../../etc/passwd.pdf"),
                "application/pdf",
                PDF,
            )],
            MAX_FILE_BYTES,
        )
        .await
        .expect("файл принят");

        assert_eq!(file.filename, "passwd.pdf");
    }

    #[tokio::test]
    async fn long_filename_is_truncated() {
        let long = format!("{}.pdf", "a".repeat(400));
        let file = take(
            &[part("file", Some(&long), "application/pdf", PDF)],
            MAX_FILE_BYTES,
        )
        .await
        .expect("файл принят");

        assert!(
            file.filename.chars().count() <= MAX_FILENAME_CHARS,
            "имя длиной {}",
            file.filename.chars().count()
        );
        assert!(file.filename.ends_with(".pdf"), "{}", file.filename);
    }

    /// Часть без имени файла получает запасное имя с каноническим
    /// расширением: в досье потом смотрят глазами.
    #[tokio::test]
    async fn nameless_part_gets_fallback_name() {
        let file = take(
            &[part("file", None, "application/pdf", PDF)],
            MAX_FILE_BYTES,
        )
        .await
        .expect("файл принят");

        assert_eq!(file.filename, "attachment.pdf");
    }

    #[test]
    fn sanitize_strips_control_characters_and_directories() {
        assert_eq!(sanitize_filename("C:\\temp\\скан.pdf"), "скан.pdf");
        assert_eq!(sanitize_filename("a\"b\r\nc.pdf"), "a_b__c.pdf");
        assert_eq!(sanitize_filename(".."), "");
        assert_eq!(sanitize_filename(""), "");
    }
}
