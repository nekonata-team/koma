use std::collections::HashSet;

use koma_core::model::PersonalSheetPreset;

use crate::config::SheetPresetConfig;

pub struct SheetPresetRef<'a> {
    pub name: &'a str,
    pub komas: &'a [String],
    pub personal: bool,
}

/// Personal presets take precedence; within each source the first name wins.
pub fn available_sheet_presets<'a>(
    personal: &'a [PersonalSheetPreset],
    configured: &'a [SheetPresetConfig],
) -> impl Iterator<Item = SheetPresetRef<'a>> + 'a {
    let mut seen = HashSet::new();
    personal
        .iter()
        .map(|preset| SheetPresetRef {
            name: &preset.name,
            komas: &preset.komas,
            personal: true,
        })
        .chain(configured.iter().map(|preset| SheetPresetRef {
            name: &preset.name,
            komas: &preset.komas,
            personal: false,
        }))
        .filter(move |preset| seen.insert(preset.name.to_ascii_lowercase()))
}
