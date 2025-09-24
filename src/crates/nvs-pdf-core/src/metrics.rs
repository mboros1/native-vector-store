// Minimal metrics façade: define keys and a measure! macro
// that can be redirected to different sinks in the future.

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub enum MetricKey {
    Decode,
    Fonts,
    Interpret,
    PageTotal,
    Resources,
    Streams,
    Normalize,
}

#[macro_export]
macro_rules! measure {
    ($key:expr, $block:expr) => {{
        #[cfg(feature = "metrics")]
        {
            let _t0 = std::time::Instant::now();
            let __ret = { $block };
            let ns = _t0.elapsed().as_nanos() as u128;
            match $key {
                $crate::metrics::MetricKey::Decode => $crate::stats::add_decode_duration(ns),
                $crate::metrics::MetricKey::Fonts => $crate::stats::add_fonts_duration(ns),
                $crate::metrics::MetricKey::Interpret => $crate::stats::add_interpret_duration(ns),
                $crate::metrics::MetricKey::PageTotal => $crate::stats::add_page_total_duration(ns),
                $crate::metrics::MetricKey::Resources => $crate::stats::add_resources_duration(ns),
                $crate::metrics::MetricKey::Streams => $crate::stats::add_streams_duration(ns),
                $crate::metrics::MetricKey::Normalize => $crate::stats::add_normalize_duration(ns),
            }
            __ret
        }
        #[cfg(not(feature = "metrics"))]
        {
            $block
        }
    }};
}
