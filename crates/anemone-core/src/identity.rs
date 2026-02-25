//! Identity generation — every anemone is unique. 1:1 port of Python identity.py.

use anyhow::Result;
use sha2::{Digest, Sha256, Sha512};
use std::path::Path;

use crate::types::{Identity, Traits};

// ── Trait dimensions (curated lists — identical to Python) ──

pub const DOMAINS: &[&str] = &[
    "mycology",
    "orbital mechanics",
    "fermentation",
    "cartography",
    "origami",
    "tidal patterns",
    "cryptography",
    "bioluminescence",
    "typography",
    "sonar",
    "geologic strata",
    "knot theory",
    "permaculture",
    "glassblowing",
    "semaphore",
    "circadian rhythms",
    "folk etymology",
    "tessellation",
    "foraging",
    "acoustics",
    "celestial navigation",
    "pigment chemistry",
    "murmuration",
    "bookbinding",
    "erosion",
    "signal processing",
    "mycelial networks",
    "letterpress",
    "thermodynamics",
    "tidepool ecology",
    "radio astronomy",
    "psychoacoustics",
    "weaving patterns",
    "volcanic geology",
    "ciphers",
    "birdsong",
    "fractal geometry",
    "archival science",
    "hydrology",
    "clockwork",
    "seed dispersal",
    "morse code",
    "cloud formation",
    "metalwork",
    "braille systems",
    "stellar nucleosynthesis",
    "composting",
    "map projection",
    "wind patterns",
    "amber preservation",
];

pub const THINKING_STYLES: &[&str] = &[
    "connecting disparate ideas",
    "following chains of cause and effect",
    "finding patterns in noise",
    "deconstructing systems into parts",
    "building mental models",
    "tracing things back to first principles",
    "mapping relationships between concepts",
    "looking for what's missing",
    "inverting assumptions",
    "layering details into bigger pictures",
    "noticing what others overlook",
    "asking why something works at all",
    "translating between domains",
    "following the smallest thread",
    "collecting and comparing examples",
    "sketching out taxonomies",
];

pub const TEMPERAMENTS: &[&str] = &[
    "patient and methodical",
    "restless and wide-ranging",
    "meticulous and detail-oriented",
    "playful and associative",
    "intense and focused",
    "wandering and serendipitous",
    "quiet and observational",
    "energetic and prolific",
];

/// Deterministically derive personality traits from raw seed bytes.
/// Exact algorithm match with Python `_derive_traits`.
pub fn derive_traits(seed_bytes: &[u8]) -> Traits {
    let mut hasher = Sha512::new();
    hasher.update(seed_bytes);
    let h = hasher.finalize();
    let h_bytes: Vec<u8> = h.to_vec();

    fn pick<'a>(list: &'a [&str], h: &[u8], offset: usize) -> &'a str {
        let chunk = u32::from_be_bytes([h[offset], h[offset + 1], h[offset + 2], h[offset + 3]]);
        list[(chunk as usize) % list.len()]
    }

    // Pick 3 unique domains
    let mut domains: Vec<String> = Vec::new();
    for i in 0..3 {
        let d = pick(DOMAINS, &h_bytes, i * 4).to_string();
        if domains.contains(&d) {
            // Collision handling — matches Python logic
            let mut extra_hasher = Sha256::new();
            extra_hasher.update(&h_bytes);
            extra_hasher.update(&[(i as u8) + 10]);
            let h_extra = extra_hasher.finalize();
            let val = u32::from_be_bytes([h_extra[0], h_extra[1], h_extra[2], h_extra[3]]);
            let d2 = DOMAINS[(val as usize) % DOMAINS.len()].to_string();
            domains.push(d2);
        } else {
            domains.push(d);
        }
    }

    // Pick 2 unique thinking styles
    let mut styles: Vec<String> = Vec::new();
    for i in 0..2 {
        let s = pick(THINKING_STYLES, &h_bytes, 12 + i * 4).to_string();
        if styles.contains(&s) {
            let mut extra_hasher = Sha256::new();
            extra_hasher.update(&h_bytes);
            extra_hasher.update(&[(i as u8) + 20]);
            let h_extra = extra_hasher.finalize();
            let val = u32::from_be_bytes([h_extra[0], h_extra[1], h_extra[2], h_extra[3]]);
            let s2 = THINKING_STYLES[(val as usize) % THINKING_STYLES.len()].to_string();
            styles.push(s2);
        } else {
            styles.push(s);
        }
    }

    let temperament = pick(TEMPERAMENTS, &h_bytes, 20).to_string();

    Traits {
        domains,
        thinking_styles: styles,
        temperament,
    }
}

/// Load existing identity from a box directory.
pub fn load_identity_from(box_path: &Path) -> Result<Option<Identity>> {
    let path = box_path.join("identity.json");
    if !path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let identity: Identity = serde_json::from_str(&content)?;
    Ok(Some(identity))
}

/// Save identity to a box directory.
pub fn save_identity(identity: &Identity, box_path: &Path) -> Result<()> {
    let path = box_path.join("identity.json");
    std::fs::create_dir_all(box_path)?;
    let content = serde_json::to_string_pretty(identity)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Create a new identity with the given name and seed bytes.
pub fn create_identity(name: &str, seed_bytes: &[u8]) -> Identity {
    let genome = hex::encode(seed_bytes);
    let traits = derive_traits(seed_bytes);
    let born = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    Identity {
        name: name.to_string(),
        genome,
        traits,
        born,
    }
}

/// Generate identity with random entropy (for runtime creation via API).
pub fn create_identity_random(name: &str) -> Identity {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(&chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_be_bytes());
    hasher.update(&rand::random::<[u8; 32]>());
    let seed_bytes = hasher.finalize();
    create_identity(name, &seed_bytes)
}

// We need the hex crate for encoding, but we can inline it
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("hex decode: {}", e))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_traits_deterministic() {
        let seed = b"test_seed_for_anemone";
        let traits1 = derive_traits(seed);
        let traits2 = derive_traits(seed);

        assert_eq!(traits1.domains, traits2.domains);
        assert_eq!(traits1.thinking_styles, traits2.thinking_styles);
        assert_eq!(traits1.temperament, traits2.temperament);
    }

    #[test]
    fn test_derive_traits_structure() {
        let seed = b"another_test_seed";
        let traits = derive_traits(seed);

        assert_eq!(traits.domains.len(), 3);
        assert_eq!(traits.thinking_styles.len(), 2);
        assert!(!traits.temperament.is_empty());

        // All domains should be from the DOMAINS list
        for d in &traits.domains {
            assert!(DOMAINS.contains(&d.as_str()));
        }
        for s in &traits.thinking_styles {
            assert!(THINKING_STYLES.contains(&s.as_str()));
        }
        assert!(TEMPERAMENTS.contains(&traits.temperament.as_str()));
    }

    #[test]
    fn test_different_seeds_different_traits() {
        let traits1 = derive_traits(b"seed_one");
        let traits2 = derive_traits(b"seed_two");

        // Very unlikely to be identical (not guaranteed but extremely improbable)
        let same = traits1.domains == traits2.domains
            && traits1.thinking_styles == traits2.thinking_styles
            && traits1.temperament == traits2.temperament;
        assert!(!same);
    }

    #[test]
    fn test_create_identity() {
        let id = create_identity("TestAnemone", b"some_entropy_data");
        assert_eq!(id.name, "TestAnemone");
        assert!(!id.genome.is_empty());
        assert_eq!(id.traits.domains.len(), 3);
        assert!(!id.born.is_empty());
    }
}
