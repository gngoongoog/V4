//! Coordinated execution facade.
//! Secondary brains remain independently testable in execution_legacy.rs, while the
//! `system` brain acts as the main coordinator and consumes their directional evidence.
#[path = "execution_legacy.rs"]
mod legacy;

pub use legacy::{brain_for_magic, magic, BrainConfig, Signal, BRAINS};
use crate::{studio::{FrameBar, Genome}, Config, Telemetry};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct AdvisorSpec {
    id: &'static str,
    timeframe: &'static str,
    weight: f64,
}

const ADVISORS: [AdvisorSpec; 12] = [
    AdvisorSpec { id: "trend", timeframe: "M5", weight: 1.4 },
    AdvisorSpec { id: "momentum", timeframe: "M1", weight: 1.0 },
    AdvisorSpec { id: "structure", timeframe: "M1", weight: 1.3 },
    AdvisorSpec { id: "zones", timeframe: "M5", weight: 1.1 },
    AdvisorSpec { id: "gann", timeframe: "M5", weight: 0.7 },
    AdvisorSpec { id: "wyckoff", timeframe: "M5", weight: 1.1 },
    AdvisorSpec { id: "dom", timeframe: "M1", weight: 0.8 },
    AdvisorSpec { id: "profile", timeframe: "M5", weight: 0.7 },
    AdvisorSpec { id: "gann_dom_profile", timeframe: "M5", weight: 0.6 },
    AdvisorSpec { id: "ces", timeframe: "M1", weight: 0.9 },
    AdvisorSpec { id: "kte", timeframe: "M1", weight: 1.0 },
    AdvisorSpec { id: "regime", timeframe: "M5", weight: 1.2 },
];

/// The selected trading profile remains the trigger generator. Specialist brains are a
/// transparent confirmation/veto layer: they can block a weak/conflicted coordinated
/// order, but they do not silently invent a main-brain entry when the selected profile
/// itself has no eligible signal. SOLO execution bypasses this function because each
/// secondary brain is evaluated directly by the runtime under its own identity.
pub fn evaluate(
    config: &BrainConfig,
    telemetry: &Telemetry,
    global: &Config,
    frames: &BTreeMap<String, Vec<FrameBar>>,
    genome: Option<&Genome>,
    now: u64,
) -> Signal {
    let mut primary = legacy::evaluate(config, telemetry, global, frames, genome, now);
    if config.id != "system" || !primary.ready || !matches!(primary.side, -1 | 1) {
        return primary;
    }

    let mut supporting = 0.0f64;
    let mut opposing = 0.0f64;
    let mut support_names = Vec::new();
    let mut oppose_names = Vec::new();
    let mut neutral = 0usize;

    for spec in ADVISORS {
        let mut advisor = config.clone();
        advisor.id = spec.id.into();
        advisor.enabled = false;
        advisor.timeframe = spec.timeframe.into();
        advisor.report_id.clear();
        let signal = legacy::evaluate(&advisor, telemetry, global, frames, None, now);
        if signal.ready && signal.side == primary.side {
            supporting += spec.weight;
            support_names.push(spec.id);
        } else if signal.ready && signal.side == -primary.side {
            opposing += spec.weight;
            oppose_names.push(spec.id);
        } else {
            neutral += 1;
        }
    }

    // The coordinator is deliberately a veto layer over the selected trading mode.
    // A single specialist cannot cancel a valid mode trigger; material conflicting
    // evidence must outweigh support. This keeps Sniper responsive while still using
    // the broader brain set instead of treating every specialist as an autonomous bot.
    let conflicted = opposing >= 2.0 && opposing > supporting + 0.5;
    let evidence = format!(
        "Coordinator: support {:.1} [{}] | oppose {:.1} [{}] | neutral {}",
        supporting,
        if support_names.is_empty() { "none".into() } else { support_names.join(",") },
        opposing,
        if oppose_names.is_empty() { "none".into() } else { oppose_names.join(",") },
        neutral
    );
    if conflicted {
        primary.ready = false;
        primary.reason = format!("{} | {} | BLOCKED: specialist opposition dominates", primary.reason, evidence);
    } else {
        primary.reason = format!("{} | {} | coordinated trigger retained", primary.reason, evidence);
    }
    primary
}
