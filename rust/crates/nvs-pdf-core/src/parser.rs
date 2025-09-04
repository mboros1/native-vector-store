//! Thin compatibility module that re-exports the core PDF primitives and helpers
//! from the modular layout. Existing call sites can continue to use
//! `nvs_pdf_core::parser::*`.

pub use crate::objects::{PdfValue, PdfDoc};
pub use crate::pages::{collect_pages_via_tree, collect_page_object_ids};
pub use crate::content::extract_page_text;

