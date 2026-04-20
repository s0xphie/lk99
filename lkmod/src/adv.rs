// Source map and game-specific logic for adv.b (Adventure).
// This will mirror the structure of sourcemap.rs for LostKng.b, but for adv.b.

// TODO: Fill in with actual regions, subregions, and notables for adv.b.

#[derive(Clone)]
pub struct Region {
    pub id: usize,
    pub name: &'static str,
    pub raw_start: usize,
    pub raw_end: usize,
    pub bf_ops: usize,
    pub sections: usize,
    pub description: &'static str,
    pub sub_regions: Vec<SubRegion>,
    pub notable: Vec<Notable>,
}

#[derive(Clone)]
pub struct SubRegion {
    pub name: &'static str,
    pub raw_start: usize,
    pub raw_end: usize,
    pub sections: usize,
    pub dots: usize,
    pub description: &'static str,
}

#[derive(Clone)]
pub struct Notable {
    pub category: &'static str,
    pub label: &'static str,
    pub raw_pos: usize,
    pub detail: &'static str,
}

// Placeholder: implement with real data for adv.b
pub fn build_source_map_adv() -> Vec<Region> {
    vec![]
}
