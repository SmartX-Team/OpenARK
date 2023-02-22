use vine_api::k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};

type ResourceCapacity = Option<::std::collections::BTreeMap<String, Quantity>>;
type Result<T> = ::core::result::Result<T, <f64 as ::core::str::FromStr>::Err>;

pub fn is_affordable(capacity: ResourceCapacity, requirements: ResourceRequirements) -> bool {
    fn is_affordable_atomic(capacity: Option<&Quantity>, requirement: &Quantity) -> Result<bool> {
        match capacity {
            Some(capacity) => {
                let capacity = parse_quantity(capacity)?;
                let requirement = parse_quantity(requirement)?;
                Ok(capacity >= requirement)
            }
            _ => Ok(true),
        }
    }

    requirements
        .limits
        .map(|requirements| {
            requirements.iter().all(|(key, requirement)| {
                let capacity = capacity.as_ref().and_then(|capacity| capacity.get(key));
                is_affordable_atomic(capacity, requirement).unwrap_or(false)
            })
        })
        .unwrap_or(true)
}

fn parse_quantity(quantity: &Quantity) -> Result<f64> {
    let quantity = quantity.0.trim();

    let map: &[(&str, u128)] = &[
        ("m", 1),
        ("Ki", 1_000 * (2 << 10)),
        ("Mi", 1_000 * (2 << 20)),
        ("Gi", 1_000 * (2 << 30)),
        ("Ti", 1_000 * (2 << 40)),
        ("Pi", 1_000 * (2 << 50)),
        ("Ei", 1_000 * (2 << 60)),
        ("K", 1_000 * (10 << 3)),
        ("M", 1_000 * (10 << 6)),
        ("G", 1_000 * (10 << 9)),
        ("T", 1_000 * (10 << 12)),
        ("P", 1_000 * (10 << 15)),
        ("E", 1_000 * (10 << 18)),
    ];
    map.iter()
        .filter_map(|(pattern, weight)| {
            if quantity.ends_with(pattern) {
                Some(
                    quantity[..quantity.len() - 2]
                        .parse()
                        .map(|v: f64| v * *weight as f64),
                )
            } else {
                None
            }
        })
        .next()
        .unwrap_or_else(|| quantity.parse().map(|v: f64| v * 1_000_f64))
}
