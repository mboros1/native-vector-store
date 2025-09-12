mod lexer;
mod state;
mod xobject;
mod emit;
mod ops;

use crate::objects::PdfDoc;
use crate::resources::PageResources;
use anyhow::Result;

pub use emit::append_bytes_as_text;
pub use ops::interpret_text_with_resources;

// Public entry: interpret a single concatenated content buffer with page resources
pub fn interpret_contents(doc: &PdfDoc, res: &PageResources, content: &[u8]) -> Result<String> {
    ops::interpret_text_with_resources(doc, &res.xobjects, &res.fonts, content)
}
