//! Model pricing catalog and token cost estimation.
//!
//! `catalog.json` is the 2026-09-25 price snapshot. Every rate is an integer
//! number of pico currency units per token, preserving exact calculations.
//! Unknown models and gateways deliberately have no estimate.

use chrono::{Datelike, FixedOffset, TimeZone, Timelike};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

mod regions;
pub use regions::PriceRegions;

const PICOS_PER_UNIT: f64 = 1_000_000_000_000.0;

/// Log counts: input includes cache writes, output includes reasoning, and cache
/// reads are separate. The exclusive kinds are derived only once at this edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PriceTokens {
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelCost {
    pub currency: String,
    pub amount: f64,
    pub amount_pico: i128,
}

impl ModelCost {
    fn new(currency: &str, amount_pico: i128) -> Self {
        Self {
            currency: currency.into(),
            amount: amount_pico as f64 / PICOS_PER_UNIT,
            amount_pico,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    #[default]
    International,
    China,
}

impl Region {
    pub fn currency(self) -> &'static str {
        match self {
            Self::International => "USD",
            Self::China => "CNY",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenKind {
    CacheWrite,
    Input,
    Reasoning,
    Output,
    CacheRead,
}

const KINDS: [TokenKind; 5] = [
    TokenKind::CacheWrite,
    TokenKind::Input,
    TokenKind::Reasoning,
    TokenKind::Output,
    TokenKind::CacheRead,
];

#[derive(Clone, Copy, Debug, Default)]
struct KindTokens([u128; 5]);

impl From<&PriceTokens> for KindTokens {
    fn from(tokens: &PriceTokens) -> Self {
        let write = tokens.cache_write.min(tokens.tokens_in);
        let thought = tokens.reasoning.min(tokens.tokens_out);
        Self([
            write.into(),
            (tokens.tokens_in - write).into(),
            thought.into(),
            (tokens.tokens_out - thought).into(),
            tokens.cache_read.into(),
        ])
    }
}

impl KindTokens {
    fn prompt(self) -> u128 {
        self.0[0] + self.0[1] + self.0[4]
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rates {
    input: u64,
    cache_write: u64,
    cache_read: u64,
    output: u64,
}

impl Rates {
    fn rate(&self, kind: TokenKind) -> u64 {
        match kind {
            TokenKind::CacheWrite => self.cache_write,
            TokenKind::Input => self.input,
            TokenKind::CacheRead => self.cache_read,
            TokenKind::Output | TokenKind::Reasoning => self.output,
        }
    }

    fn amount(&self, kinds: KindTokens) -> Option<i128> {
        let mut total = 0_u128;
        for (kind, tokens) in KINDS.into_iter().zip(kinds.0) {
            total = total.checked_add(tokens.checked_mul(self.rate(kind).into())?)?;
        }
        i128::try_from(total).ok()
    }
}

#[derive(Debug, Deserialize)]
struct Tier {
    from: u64,
    rates: Rates,
}

#[derive(Debug, Deserialize)]
struct Price {
    rates: Rates,
    tiers: Vec<Tier>,
}

impl Price {
    fn tier(&self, kinds: KindTokens) -> Option<usize> {
        self.tiers
            .iter()
            .rposition(|tier| kinds.prompt() >= tier.from.into())
    }

    fn rates(&self, tier: Option<usize>) -> &Rates {
        tier.map(|index| &self.tiers[index].rates)
            .unwrap_or(&self.rates)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub vendor: String,
    prices: HashMap<Region, Price>,
    pub global: bool,
    pub peak_hours: bool,
    /// The context window the vendor publishes, in tokens.
    #[serde(default)]
    pub context_window: Option<u64>,
}

impl Model {
    /// The list a call on `region` is priced by: that platform's, or the one
    /// list of a global vendor. A regional model a platform does not sell has
    /// no price there (agent-hud-native-open `ModelCatalog.Model.price(in:)`).
    fn price(&self, region: Region) -> Option<(&Price, Region)> {
        self.prices
            .get(&region)
            .map(|price| (price, region))
            .or_else(|| {
                self.global
                    .then(|| self.prices.get(&Region::International))
                    .flatten()
                    .map(|price| (price, Region::International))
            })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    checked_on: String,
    rate_unit: String,
    models: HashMap<String, Model>,
    aliases: HashMap<String, String>,
    vendor_services: HashMap<String, String>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog: Catalog = serde_json::from_str(include_str!("catalog.json"))
            .expect("the checked-in source price catalog must be valid");
        assert_eq!(catalog.rate_unit, "pico-currency-per-token");
        catalog
    })
}

pub fn checked_on() -> &'static str {
    &catalog().checked_on
}

/// Exact source normalization, not a display-name or family-name heuristic.
/// In particular, `openai/gpt-5.5` stays gateway-qualified and is unpriced.
pub fn normalized_model_name(model: &str) -> Option<String> {
    let mut name = model.split('#').next()?.to_lowercase();
    if name.ends_with("[1m]") {
        name.truncate(name.len() - 4);
    }
    let bytes = name.as_bytes();
    let compact_date = bytes
        .len()
        .checked_sub(9)
        .filter(|at| bytes[*at] == b'-' && bytes[*at + 1..].iter().all(u8::is_ascii_digit));
    let dashed_date = bytes.len().checked_sub(11).filter(|at| {
        bytes[*at] == b'-'
            && bytes[*at + 5] == b'-'
            && bytes[*at + 8] == b'-'
            && bytes[*at + 1..*at + 5].iter().all(u8::is_ascii_digit)
            && bytes[*at + 6..*at + 8].iter().all(u8::is_ascii_digit)
            && bytes[*at + 9..].iter().all(u8::is_ascii_digit)
    });
    if let Some(at) = compact_date.or(dashed_date) {
        name.truncate(at);
    }
    (!name.is_empty()).then_some(name)
}

pub fn name_of(consumer: &str) -> Option<String> {
    let (_, model) = consumer.split_once("-model:")?;
    normalized_model_name(model)
}

fn listed(consumer: &str) -> Option<&'static Model> {
    let name = name_of(consumer)?;
    let catalog = catalog();
    let name = catalog.aliases.get(&name).unwrap_or(&name);
    catalog.models.get(name)
}

/// A route is accepted only when the original catalog identifies it as that
/// vendor's own API/plan. Azure, Bedrock, Vertex, OpenRouter and proxies are not
/// priced with a vendor's list merely because their model has a familiar name.
pub fn model_for(consumer: &str) -> Option<&'static Model> {
    let model = listed(consumer)?;
    if let Some((_, route)) = consumer.split_once('#') {
        if catalog().vendor_services.get(route) != Some(&model.vendor)
            && !(route == "deepseek-official" && model.vendor == "DeepSeek")
        {
            return None;
        }
    }
    Some(model)
}

#[derive(Clone, Copy)]
struct Listing {
    model: &'static Model,
    price: &'static Price,
    region: Region,
}

fn listing(consumer: &str, region: Region) -> Option<Listing> {
    let model = model_for(consumer)?;
    let (price, region) = model.price(region)?;
    Some(Listing {
        model,
        price,
        region,
    })
}

/// Beijing working hours, as in the source. Public holidays are not known to
/// this snapshot and therefore still count as peak when they fall on a weekday.
pub fn is_peak(at_ms: i64) -> bool {
    let Some(date) = chrono::Utc.timestamp_millis_opt(at_ms).single() else {
        return false;
    };
    let local = date.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
    local.weekday().num_days_from_monday() < 5
        && ((9..12).contains(&local.hour()) || (14..18).contains(&local.hour()))
}

fn factor(model: &Model, at_ms: Option<i64>) -> i128 {
    if model.peak_hours && at_ms.is_some_and(is_peak) {
        2
    } else {
        1
    }
}

/// `tiered` is true for a single call, false for a pre-summed bucket whose prompt is unknown.
pub(crate) fn cost_at(
    source: &str,
    model: &str,
    tokens: &PriceTokens,
    tiered: bool,
    region: Region,
    at_ms: Option<i64>,
) -> Option<ModelCost> {
    consumer_cost(
        &format!("{source}-model:{model}"),
        tokens,
        tiered,
        region,
        at_ms,
    )
}

/// The prompt sizes from which `model`, used through `source`, is priced on
/// each higher tier of the international list; None when it has no list price.
/// A call is in the last tier whose start its prompt reaches.
pub(crate) fn tier_starts(source: &str, model: &str) -> Option<Vec<u64>> {
    let listing = listing(&format!("{source}-model:{model}"), Region::International)?;
    Some(listing.price.tiers.iter().map(|tier| tier.from).collect())
}

/// The tier a call whose prompt (fresh input, cache writes and cache reads)
/// is `prompt` tokens is on, given `tier_starts`: the last start it reaches,
/// None for the base rates.
pub(crate) fn tier_of(starts: &[u64], prompt: u64) -> Option<usize> {
    starts.iter().rposition(|from| prompt >= *from)
}

/// The context window a call ran in (agent-hud-native-open
/// `ModelCatalog.contextWindow(agentId:reported:largestSeen:)`): what the
/// client reported, else the published window. Claude Code does not report
/// it, and a Claude model runs with 200K or 1M, so a model this Mac has seen
/// hold more than its published window counts as 1M.
pub(crate) fn context_window(
    source: &str,
    model: &str,
    reported: Option<u64>,
    largest_seen: Option<u64>,
) -> Option<u64> {
    if let Some(reported) = reported.filter(|window| *window > 0) {
        return Some(reported);
    }
    let consumer = format!("{source}-model:{model}");
    let published = listed(&consumer)
        .and_then(|model| model.context_window)
        .or_else(|| consumer.starts_with("claude-model:").then_some(200_000))?;
    Some(if largest_seen.unwrap_or(0) > published {
        published.max(1_000_000)
    } else {
        published
    })
}

/// What calls that all fall in `tier` (an index into `tier_starts`, None for
/// the base rates) cost together on the international list at off-peak
/// rates. Within one tier the price is linear, so the sum of the calls'
/// tokens costs what the calls cost one by one.
pub(crate) fn cost_in_tier(
    source: &str,
    model: &str,
    tokens: &PriceTokens,
    tier: Option<usize>,
) -> Option<ModelCost> {
    let listing = listing(&format!("{source}-model:{model}"), Region::International)?;
    let amount = listing.price.rates(tier).amount(KindTokens::from(tokens))?;
    Some(ModelCost::new(listing.region.currency(), amount))
}

pub fn consumer_cost(
    consumer: &str,
    tokens: &PriceTokens,
    tiered: bool,
    region: Region,
    at_ms: Option<i64>,
) -> Option<ModelCost> {
    let listing = listing(consumer, region)?;
    let kinds = KindTokens::from(tokens);
    let tier = tiered.then(|| listing.price.tier(kinds)).flatten();
    let amount = listing
        .price
        .rates(tier)
        .amount(kinds)?
        .checked_mul(factor(listing.model, at_ms))?;
    Some(ModelCost::new(listing.region.currency(), amount))
}
