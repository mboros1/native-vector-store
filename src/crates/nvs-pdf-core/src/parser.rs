//! Thin compatibility module that re-exports the core PDF primitives and helpers
//! from the modular layout. Existing call sites can continue to use
//! `nvs_pdf_core::parser::*`.

pub use crate::content::extract_page_text;
pub use crate::objects::{PdfDoc, PdfValue};
pub use crate::pages::{collect_page_object_ids, collect_pages_via_tree};
