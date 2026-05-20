use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ExchangeConfig {
    pub enabled: bool,
    pub symbols: Option<Vec<String>>,      // spot symbols override
    pub perp_symbols: Option<Vec<String>>, // perp symbols override
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    pub fee: FeeModel,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FeeModel {
    Fixed {
        maker_bps: f64,
        taker_bps: f64,
    },
    Tiered {
        volume_30d_usdt: f64,
        tiers: Vec<FeeTier>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeeTier {
    pub min_volume_usdt: f64,
    pub maker_bps: f64,
    pub taker_bps: f64,
}

impl FeeModel {
    pub fn maker_bps(&self) -> f64 {
        match self {
            FeeModel::Fixed { maker_bps, .. } => *maker_bps,
            FeeModel::Tiered {
                volume_30d_usdt,
                tiers,
            } => select_fee_tier(*volume_30d_usdt, tiers)
                .map(|x| x.maker_bps)
                .unwrap_or(0.0),
        }
    }

    pub fn taker_bps(&self) -> f64 {
        match self {
            FeeModel::Fixed { taker_bps, .. } => *taker_bps,
            FeeModel::Tiered {
                volume_30d_usdt,
                tiers,
            } => select_fee_tier(*volume_30d_usdt, tiers)
                .map(|x| x.taker_bps)
                .unwrap_or(0.0),
        }
    }
}

fn select_fee_tier(volume_30d_usdt: f64, tiers: &[FeeTier]) -> Option<&FeeTier> {
    let mut best: Option<&FeeTier> = None;
    for tier in tiers {
        if volume_30d_usdt >= tier.min_volume_usdt
            && best.is_none_or(|x| tier.min_volume_usdt > x.min_volume_usdt)
        {
            best = Some(tier);
        }
    }
    best.or_else(|| tiers.first())
}

#[cfg(test)]
mod tests {
    use super::{FeeModel, FeeTier};

    #[test]
    fn tiered_fee_selects_highest_matching_tier() {
        let f = FeeModel::Tiered {
            volume_30d_usdt: 5_500_000.0,
            tiers: vec![
                FeeTier {
                    min_volume_usdt: 0.0,
                    maker_bps: 10.0,
                    taker_bps: 12.0,
                },
                FeeTier {
                    min_volume_usdt: 1_000_000.0,
                    maker_bps: 8.0,
                    taker_bps: 9.0,
                },
                FeeTier {
                    min_volume_usdt: 5_000_000.0,
                    maker_bps: 6.0,
                    taker_bps: 7.0,
                },
            ],
        };
        assert!((f.maker_bps() - 6.0).abs() < 1e-9);
        assert!((f.taker_bps() - 7.0).abs() < 1e-9);
    }
}
