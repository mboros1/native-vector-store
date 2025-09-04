use std::cell::RefCell;

#[derive(Default, Clone)]
pub struct ExtractStatsInner {
    // Store nanoseconds for precision on very fast operations
    pub decode_ns: u128,
    pub fonts_ns: u128,
    pub interpret_ns: u128,
    pub page_total_ns: u128,
    pub resources_ns: u128,
    pub streams_ns: u128,
    pub normalize_ns: u128,
}

thread_local! {
    static STATS: RefCell<ExtractStatsInner> = RefCell::new(ExtractStatsInner::default());
}

pub fn reset() { STATS.with(|s| *s.borrow_mut() = ExtractStatsInner::default()); }
pub fn add_decode_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().decode_ns += ns); } }
pub fn add_fonts_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().fonts_ns += ns); } }
pub fn add_interpret_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().interpret_ns += ns); } }
pub fn add_page_total_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().page_total_ns += ns); } }
pub fn add_resources_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().resources_ns += ns); } }
pub fn add_streams_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().streams_ns += ns); } }
pub fn add_normalize_duration(ns: u128) { if ns>0 { STATS.with(|s| s.borrow_mut().normalize_ns += ns); } }
pub fn snapshot() -> ExtractStatsInner { STATS.with(|s| s.borrow().clone()) }
