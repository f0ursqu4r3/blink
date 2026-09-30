//! Port of `src/lib/timing.ts`. The browser Resource Timing reader has no
//! counterpart: the native transport measures each phase itself.

use crate::model::ResponseTiming;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingPhaseId {
    Dns,
    Connect,
    Tls,
    Wait,
    Download,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimingPhase {
    pub id: TimingPhaseId,
    pub label: &'static str,
    pub ms: f64,
    /// Start, from the request start.
    pub offset_ms: f64,
}

/// The phases in order, each starting when the one before it ends.
pub fn timing_phases(timing: &ResponseTiming) -> Vec<TimingPhase> {
    let entries = [
        (TimingPhaseId::Dns, "DNS lookup", timing.dns_ms),
        (
            TimingPhaseId::Connect,
            if timing.tls_ms.is_none() {
                "Connect"
            } else {
                "TCP connect"
            },
            timing.connect_ms,
        ),
        (TimingPhaseId::Tls, "TLS handshake", timing.tls_ms),
        (TimingPhaseId::Wait, "Waiting (TTFB)", Some(timing.wait_ms)),
        (
            TimingPhaseId::Download,
            "Content download",
            Some(timing.download_ms),
        ),
    ];
    let mut offset_ms = 0.0;
    let mut phases = Vec::new();
    for (id, label, ms) in entries {
        let Some(ms) = ms else { continue };
        phases.push(TimingPhase {
            id,
            label,
            ms,
            offset_ms,
        });
        offset_ms += ms;
    }
    phases
}

/// `Math.round`: halves round up, toward positive infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `Number.prototype.toFixed`: the exact value, with ties rounded up.
fn to_fixed(value: f64, digits: usize) -> String {
    let scaled = value * 10f64.powi(digits as i32);
    if scaled.fract() == 0.5 {
        return format!("{:.*}", digits, (scaled + 0.5) / 10f64.powi(digits as i32));
    }
    format!("{value:.digits$}")
}

pub fn format_ms(ms: f64) -> String {
    if ms < 1.0 {
        return format!("{} ms", to_fixed(ms, 2));
    }
    if ms < 10.0 {
        return format!("{} ms", to_fixed(ms, 1));
    }
    if ms < 1000.0 {
        return format!("{} ms", js_round(ms));
    }
    format!("{} s", to_fixed(ms / 1000.0, 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_each_phase_after_the_one_before() {
        let phases = timing_phases(&ResponseTiming {
            dns_ms: Some(5.0),
            connect_ms: Some(20.0),
            tls_ms: None,
            wait_ms: 100.0,
            download_ms: 10.0,
        });
        assert_eq!(
            phases
                .iter()
                .map(|p| (p.id, p.offset_ms))
                .collect::<Vec<_>>(),
            [
                (TimingPhaseId::Dns, 0.0),
                (TimingPhaseId::Connect, 5.0),
                (TimingPhaseId::Wait, 25.0),
                (TimingPhaseId::Download, 125.0),
            ]
        );
        assert_eq!(phases[1].label, "Connect");
    }

    #[test]
    fn labels_tcp_apart_when_tls_is_known() {
        let phases = timing_phases(&ResponseTiming {
            dns_ms: None,
            connect_ms: Some(3.0),
            tls_ms: Some(4.0),
            wait_ms: 1.0,
            download_ms: 1.0,
        });
        assert!(phases.iter().any(|p| p.label == "TCP connect"));
        assert_eq!(
            phases
                .iter()
                .find(|p| p.id == TimingPhaseId::Tls)
                .map(|p| p.offset_ms),
            Some(3.0)
        );
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_ms(0.123), "0.12 ms");
        assert_eq!(format_ms(4.56), "4.6 ms");
        assert_eq!(format_ms(123.4), "123 ms");
        assert_eq!(format_ms(2345.0), "2.35 s");
    }

    #[test]
    fn rounds_ties_up_as_to_fixed_does() {
        assert_eq!(format_ms(0.125), "0.13 ms");
        assert_eq!(format_ms(1.005), "1.0 ms");
        assert_eq!(format_ms(12.5), "13 ms");
    }
}
