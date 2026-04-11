//! Camera compatibility checker — know before you buy.
//!
//! Takes what you know about a camera (app name, FCC ID, SoC,
//! brand/model) and tells you if it's flashable with open-eyes firmware.
//!
//! Usage:
//!   oe-eval --check-camera --app "CloudEdge"
//!   oe-eval --check-camera --soc "Hi3518EV300"
//!   oe-eval --check-camera --fcc "2AZL7-ZS-GX1S"
//!
//! The goal: you're standing in the Amazon aisle (or browsing online),
//! you type in what's on the box, and you get a go/no-go before buying.

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatResult {
    pub verdict: Verdict,
    pub reason: String,
    pub soc: Option<SoCInfo>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Verdict {
    /// Confirmed flashable with open-eyes firmware
    Compatible,
    /// Might work but untested — buy at your own risk
    PossiblyCompatible,
    /// Confirmed incompatible — do not buy
    Incompatible,
    /// Not enough info to determine
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoCInfo {
    pub name: String,
    pub vendor: String,
    pub architecture: String,
    pub openipc: SupportStatus,
    pub thingino: SupportStatus,
    pub flash_method: FlashMethod,
    pub has_npu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SupportStatus {
    Supported,
    Experimental,
    Planned,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FlashMethod {
    /// SD card boot — the consumer path
    SdCard,
    /// Needs UART serial connection (soldering)
    Uart,
    /// Needs SoC chip swap (expert only)
    ChipSwap,
    /// No known flash method
    None,
}

// ── SoC Database ───────────────────────────────────────────────

fn soc_database() -> Vec<SoCInfo> {
    vec![
        // ── Ingenic (Thingino) — the good ones ──
        SoCInfo {
            name: "T20".into(), vendor: "Ingenic".into(),
            architecture: "MIPS".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Supported,
            flash_method: FlashMethod::SdCard,
            has_npu: false,
        },
        SoCInfo {
            name: "T21".into(), vendor: "Ingenic".into(),
            architecture: "MIPS".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Supported,
            flash_method: FlashMethod::SdCard,
            has_npu: false,
        },
        SoCInfo {
            name: "T31".into(), vendor: "Ingenic".into(),
            architecture: "MIPS".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Supported,
            flash_method: FlashMethod::SdCard,
            has_npu: true,
        },
        SoCInfo {
            name: "T40".into(), vendor: "Ingenic".into(),
            architecture: "MIPS".into(),
            openipc: SupportStatus::Experimental,
            thingino: SupportStatus::Planned,
            flash_method: FlashMethod::SdCard, // unless secure boot
            has_npu: true,
        },
        // ── HiSilicon (OpenIPC) — depends on stock firmware ──
        SoCInfo {
            name: "Hi3516EV200".into(), vendor: "HiSilicon".into(),
            architecture: "ARM Cortex-A7".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::SdCard, // if stock is Linux
            has_npu: false,
        },
        SoCInfo {
            name: "Hi3518EV200".into(), vendor: "HiSilicon".into(),
            architecture: "ARM926EJ-S".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::SdCard,
            has_npu: false,
        },
        SoCInfo {
            name: "Hi3518EV300".into(), vendor: "HiSilicon".into(),
            architecture: "ARM Cortex-A7".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::Uart, // LiteOS stock = no SD flash
            has_npu: false,
        },
        // ── Sigmastar ──
        SoCInfo {
            name: "SSC338Q".into(), vendor: "Sigmastar".into(),
            architecture: "ARM Cortex-A7".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::SdCard,
            has_npu: true,
        },
        SoCInfo {
            name: "SSD202".into(), vendor: "Sigmastar".into(),
            architecture: "ARM Cortex-A7".into(),
            openipc: SupportStatus::Supported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::SdCard,
            has_npu: false,
        },
        // ── Anyka — poorly supported ──
        SoCInfo {
            name: "AK3918EV300".into(), vendor: "Anyka".into(),
            architecture: "ARM".into(),
            openipc: SupportStatus::Unsupported,
            thingino: SupportStatus::Unsupported,
            flash_method: FlashMethod::None,
            has_npu: false,
        },
    ]
}

// ── App Database (app name → likely platform → verdict) ────────

fn check_app(app_name: &str) -> CompatResult {
    let app_lower = app_name.to_lowercase();

    if app_lower.contains("cloudedge") || app_lower.contains("meari") || app_lower.contains("ppstrong") {
        return CompatResult {
            verdict: Verdict::Incompatible,
            reason: "CloudEdge/Meari cameras use LiteOS firmware, proprietary P2P protocol, \
                     no RTSP, and are typically sealed shut. They are designed to prevent \
                     user access to their own video. Do not buy.".into(),
            soc: None,
            recommendations: vec![
                "Buy a Wyze Cam v3 instead ($15, Thingino proven, SD card flash)".into(),
                "Or TP-Link Tapo C100 ($15, check SoC before flashing)".into(),
            ],
        };
    }

    if app_lower.contains("wyze") {
        return CompatResult {
            verdict: Verdict::PossiblyCompatible,
            reason: "Wyze cameras use Ingenic SoCs. v2 and v3 are fully supported by Thingino. \
                     v4 has secure boot (incompatible without chip swap). Check the model.".into(),
            soc: None,
            recommendations: vec![
                "Wyze Cam v3: COMPATIBLE (T31, SD card flash, Thingino)".into(),
                "Wyze Cam v2: COMPATIBLE (T20, SD card flash, Thingino)".into(),
                "Wyze Cam v4: INCOMPATIBLE (T40 secure boot, chip swap needed)".into(),
            ],
        };
    }

    if app_lower.contains("tapo") || app_lower.contains("tp-link") {
        return CompatResult {
            verdict: Verdict::PossiblyCompatible,
            reason: "TP-Link Tapo cameras vary by model. Some use flashable SoCs, \
                     some don't. Check the FCC internal photos for the specific SoC.".into(),
            soc: None,
            recommendations: vec![
                "Look up FCC ID at fcc.report, find internal photos, identify SoC".into(),
                "Cross-reference SoC with OpenIPC/Thingino supported hardware list".into(),
            ],
        };
    }

    if app_lower.contains("icsee") || app_lower.contains("xmeye") {
        return CompatResult {
            verdict: Verdict::PossiblyCompatible,
            reason: "iCSee/XMEye cameras often use HiSilicon SoCs with OpenIPC support. \
                     Many expose RTSP. Check the specific SoC — Hi3516EV200 is well supported.".into(),
            soc: None,
            recommendations: vec![
                "Check FCC ID for SoC".into(),
                "HiSilicon Hi3516EV200/300 = likely compatible".into(),
            ],
        };
    }

    if app_lower.contains("tuya") || app_lower.contains("smart life") {
        return CompatResult {
            verdict: Verdict::PossiblyCompatible,
            reason: "Tuya cameras use various SoCs. Some are flashable, some aren't. \
                     The Tuya platform itself is cloud-dependent but the hardware varies.".into(),
            soc: None,
            recommendations: vec![
                "Check FCC ID for SoC before buying".into(),
                "Ingenic or HiSilicon SoCs = likely flashable".into(),
                "Realtek or proprietary SoCs = likely not".into(),
            ],
        };
    }

    CompatResult {
        verdict: Verdict::Unknown,
        reason: format!("Unknown app '{}'. Check the SoC via FCC internal photos.", app_name),
        soc: None,
        recommendations: vec![
            "Find the FCC ID on the camera label".into(),
            "Look up internal photos at fcc.report".into(),
            "Identify the SoC chip markings".into(),
            "Check against OpenIPC/Thingino supported hardware".into(),
        ],
    }
}

// ── SoC lookup ─────────────────────────────────────────────────

fn check_soc(soc_name: &str) -> CompatResult {
    let soc_lower = soc_name.to_lowercase().replace("-", "").replace("_", "").replace(" ", "");
    let db = soc_database();

    for soc in &db {
        let db_lower = soc.name.to_lowercase().replace("-", "").replace("_", "").replace(" ", "");
        if soc_lower.contains(&db_lower) || db_lower.contains(&soc_lower) {
            let verdict = match (&soc.flash_method, &soc.openipc, &soc.thingino) {
                (FlashMethod::SdCard, SupportStatus::Supported, _) => Verdict::Compatible,
                (FlashMethod::SdCard, _, SupportStatus::Supported) => Verdict::Compatible,
                (FlashMethod::SdCard, SupportStatus::Experimental, _) => Verdict::PossiblyCompatible,
                (FlashMethod::Uart, SupportStatus::Supported, _) => Verdict::PossiblyCompatible,
                (FlashMethod::ChipSwap, _, _) => Verdict::Incompatible,
                (FlashMethod::None, _, _) => Verdict::Incompatible,
                _ => Verdict::Unknown,
            };

            let reason = match verdict {
                Verdict::Compatible => format!(
                    "{} {} — SD card flashable, {} support. Buy it.",
                    soc.vendor, soc.name,
                    if soc.thingino == SupportStatus::Supported { "Thingino" } else { "OpenIPC" }
                ),
                Verdict::PossiblyCompatible => format!(
                    "{} {} — supported but may need UART flash (soldering). \
                     Check if the specific camera's stock firmware is Linux-based.",
                    soc.vendor, soc.name
                ),
                Verdict::Incompatible => format!(
                    "{} {} — flash method: {:?}. Not consumer-flashable.",
                    soc.vendor, soc.name, soc.flash_method
                ),
                Verdict::Unknown => format!(
                    "{} {} — support status unclear.",
                    soc.vendor, soc.name
                ),
            };

            return CompatResult {
                verdict,
                reason,
                soc: Some(soc.clone()),
                recommendations: vec![],
            };
        }
    }

    CompatResult {
        verdict: Verdict::Unknown,
        reason: format!("SoC '{}' not in database. Check openipc.org/cameras/vendors.", soc_name),
        soc: None,
        recommendations: vec![
            "Check https://openipc.org/cameras/vendors".into(),
            "Check https://thingino.com".into(),
            "Submit the SoC to the open-eyes compatibility list".into(),
        ],
    }
}

// ── Public API ─────────────────────────────────────────────────

/// Check camera compatibility by app name.
pub fn check_by_app(app_name: &str) -> CompatResult {
    check_app(app_name)
}

/// Check camera compatibility by SoC name.
pub fn check_by_soc(soc_name: &str) -> CompatResult {
    check_soc(soc_name)
}

/// Print a human-readable compatibility report.
pub fn print_report(result: &CompatResult) {
    let icon = match result.verdict {
        Verdict::Compatible => "✅",
        Verdict::PossiblyCompatible => "⚠️",
        Verdict::Incompatible => "❌",
        Verdict::Unknown => "❓",
    };

    println!("{} {}", icon, match result.verdict {
        Verdict::Compatible => "COMPATIBLE — flash it!",
        Verdict::PossiblyCompatible => "POSSIBLY COMPATIBLE — check details",
        Verdict::Incompatible => "INCOMPATIBLE — do not buy",
        Verdict::Unknown => "UNKNOWN — need more info",
    });
    println!();
    println!("{}", result.reason);

    if let Some(ref soc) = result.soc {
        println!();
        println!("SoC: {} {} ({})", soc.vendor, soc.name, soc.architecture);
        println!("OpenIPC: {:?}", soc.openipc);
        println!("Thingino: {:?}", soc.thingino);
        println!("Flash: {:?}", soc.flash_method);
        println!("NPU: {}", if soc.has_npu { "yes" } else { "no" });
    }

    if !result.recommendations.is_empty() {
        println!();
        println!("Recommendations:");
        for rec in &result.recommendations {
            println!("  → {}", rec);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudedge_is_incompatible() {
        let result = check_by_app("CloudEdge");
        assert_eq!(result.verdict, Verdict::Incompatible);
    }

    #[test]
    fn wyze_is_possibly_compatible() {
        let result = check_by_app("Wyze");
        assert_eq!(result.verdict, Verdict::PossiblyCompatible);
    }

    #[test]
    fn t31_is_compatible() {
        let result = check_by_soc("T31");
        assert_eq!(result.verdict, Verdict::Compatible);
        assert!(result.soc.unwrap().thingino == SupportStatus::Supported);
    }

    #[test]
    fn hi3518ev300_is_possibly_compatible() {
        // Supported by OpenIPC but needs UART flash (LiteOS stock)
        let result = check_by_soc("Hi3518EV300");
        assert_eq!(result.verdict, Verdict::PossiblyCompatible);
    }

    #[test]
    fn ak3918ev300_is_incompatible() {
        let result = check_by_soc("AK3918EV300");
        assert_eq!(result.verdict, Verdict::Incompatible);
    }

    #[test]
    fn unknown_soc_returns_unknown() {
        let result = check_by_soc("XYZ9999");
        assert_eq!(result.verdict, Verdict::Unknown);
    }

    #[test]
    fn case_insensitive_app_check() {
        assert_eq!(check_by_app("cloudedge").verdict, Verdict::Incompatible);
        assert_eq!(check_by_app("CLOUDEDGE").verdict, Verdict::Incompatible);
        assert_eq!(check_by_app("CloudEdge").verdict, Verdict::Incompatible);
    }

    #[test]
    fn case_insensitive_soc_check() {
        assert_eq!(check_by_soc("t31").verdict, Verdict::Compatible);
        assert_eq!(check_by_soc("T31").verdict, Verdict::Compatible);
    }
}
