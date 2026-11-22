//! Aperçu des PDF : pages rendues en images PNG par Windows
//! (`Windows.Data.Pdf`), sans bibliothèque PDF embarquée. Ailleurs, le
//! fichier s'ouvre avec l'application par défaut.

/// Pages rendues au plus (les suivantes : ouvrir le fichier).
#[cfg_attr(not(windows), allow(dead_code))]
pub const MAX_PAGES: u32 = 30;

/// Aperçu possible sur ce système.
pub const SUPPORTED: bool = cfg!(windows);

/// Pages du PDF en PNG (largeur `width` px), et nombre total de pages.
#[cfg(windows)]
pub fn render_pages(bytes: &[u8], width: u32) -> anyhow::Result<(Vec<Vec<u8>>, u32)> {
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

    // Fil de travail : appartement multithread (déjà fait : sans effet).
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };

    let source = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&source)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.FlushAsync()?.join()?;
    writer.DetachStream()?;
    source.Seek(0)?;

    let document = PdfDocument::LoadFromStreamAsync(&source)?.join()?;
    let total = document.PageCount()?;
    let mut pages = Vec::new();
    for i in 0..total.min(MAX_PAGES) {
        let page = document.GetPage(i)?;
        let options = PdfPageRenderOptions::new()?;
        options.SetDestinationWidth(width)?;
        let out = InMemoryRandomAccessStream::new()?;
        page.RenderWithOptionsToStreamAsync(&out, &options)?.join()?;
        let size = out.Size()? as u32;
        let reader = DataReader::CreateDataReader(&out.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size)?.join()?;
        let mut png = vec![0u8; size as usize];
        reader.ReadBytes(&mut png)?;
        pages.push(png);
    }
    Ok((pages, total))
}

#[cfg(not(windows))]
pub fn render_pages(_bytes: &[u8], _width: u32) -> anyhow::Result<(Vec<Vec<u8>>, u32)> {
    anyhow::bail!("aperçu PDF indisponible sur ce système")
}
