//! Печатные формы (арх.: typst как библиотека). Компиляция статического
//! шаблона + данных через виртуальный `data.json`: значения не встраиваются
//! в разметку, поэтому экранирование Typst-синтаксиса не требуется.

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt as _, World};
use typst_layout::PagedDocument;

/// Мир одного документа: `/main.typ` (шаблон) + `/data.json` (данные),
/// шрифты Libertinus из typst-assets (кириллица; печатные формы контура 1 - ru,
/// NFR-01). Пакеты и файловая система недоступны по построению.
pub struct SingleDocWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main: Source,
    data_id: FileId,
    data: Bytes,
}

impl SingleDocWorld {
    pub fn new(template: &str, data_json: Vec<u8>) -> Result<Self, RenderError> {
        let main_id = project_file("main.typ")?;
        let data_id = project_file("data.json")?;

        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|face| Font::iter(Bytes::new(face)))
            .collect();
        let book = FontBook::from_fonts(&fonts);

        Ok(Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            main: Source::new(main_id, template.to_owned()),
            data_id,
            data: Bytes::new(data_json),
        })
    }
}

fn project_file(path: &str) -> Result<FileId, RenderError> {
    let vpath = VirtualPath::new(path).map_err(|err| RenderError::Compile(err.to_string()))?;
    Ok(FileId::new(RootedPath::new(VirtualRoot::Project, vpath)))
}

impl World for SingleDocWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.data_id {
            Ok(self.data.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    /// Дата «сегодня» шаблонам недоступна: все временные значения приходят
    /// в данных уже отформатированными (NFR-03 - время сервера/БД).
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("сериализация данных печатной формы: {0}")]
    Data(#[from] serde_json::Error),
    #[error("компиляция печатной формы: {0}")]
    Compile(String),
    #[error("экспорт PDF: {0}")]
    Export(String),
}

/// Компиляция шаблона с данными в PDF. CPU-bound - вызывающий код обязан
/// уводить в `spawn_blocking`.
pub fn render(template: &str, data: &serde_json::Value) -> Result<Vec<u8>, RenderError> {
    let world = SingleDocWorld::new(template, serde_json::to_vec(data)?)?;

    let document: PagedDocument = typst::compile(&world)
        .output
        .map_err(|errors| RenderError::Compile(join_diagnostics(&errors)))?;

    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| RenderError::Export(join_diagnostics(&errors)))
}

fn join_diagnostics(errors: &typst::diag::EcoVec<typst::diag::SourceDiagnostic>) -> String {
    errors
        .iter()
        .map(|diag| diag.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}
