use std::io::{Cursor, Write};

use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

pub(crate) fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    build_zip_with(entries, CompressionMethod::Deflated)
}

pub(crate) fn build_zip_with(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(method);
    for (name, data) in entries {
        writer.start_file(*name, options).expect("start_file");
        writer.write_all(data).expect("write_all");
    }
    writer.finish().expect("finish").into_inner()
}

pub(crate) const W_NS: &str =
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

pub(crate) fn document_xml(body: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {W_NS}><w:body>{body}</w:body></w:document>")
}

pub(crate) fn para(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}
