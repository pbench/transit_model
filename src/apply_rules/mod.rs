// Copyright (C) 2017 Kisio Digital and/or its affiliates.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU Affero General Public License as published by the
// Free Software Foundation, version 3.

// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.

// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>

//! See function apply_rules

mod complementary_code;
mod network_consolidation;
mod property_rule;

use crate::{
    objects::{Line, VehicleJourney},
    utils::Report,
    Model, Result,
};
use log::info;
use std::{collections::HashMap, fs, path::PathBuf};
use transit_model_relations::IdxSet;

/// Applying rules
///
/// - `complementary_code_rules_files` Csv files containing codes to add for certain objects
/// - `property_rules_files` Csv files containing rules applied on properties
/// - `networks_consolidation_file` Json file containing rules for grouping networks
pub fn apply_rules(
    model: Model,
    complementary_code_rules_files: Vec<PathBuf>,
    property_rules_files: Vec<PathBuf>,
    networks_consolidation_file: Option<PathBuf>,
    report_path: PathBuf,
) -> Result<Model> {
    let lines_by_network: HashMap<String, IdxSet<Line>> = model
        .networks
        .iter()
        .filter_map(|(idx, obj)| {
            let lines = model.get_corresponding_from_idx(idx);
            if lines.is_empty() {
                None
            } else {
                Some((obj.id.clone(), lines))
            }
        })
        .collect();

    let vjs_by_line: HashMap<String, IdxSet<VehicleJourney>> = model
        .lines
        .iter()
        .filter_map(|(idx, obj)| {
            let vjs = model.get_corresponding_from_idx(idx);
            if vjs.is_empty() {
                None
            } else {
                Some((obj.id.clone(), vjs))
            }
        })
        .collect();

    let mut collections = model.into_collections();

    let mut report = Report::default();

    if let Some(networks_consolidation_file) = networks_consolidation_file {
        info!("Applying network consolidation rules");
        collections = network_consolidation::apply_rules(
            networks_consolidation_file,
            &lines_by_network,
            collections,
            &mut report,
        )?;
    }

    info!("Applying complementary code rules");
    complementary_code::apply_rules(
        complementary_code_rules_files,
        &mut collections,
        &mut report,
    )?;

    info!("Applying property rules");
    property_rule::apply_rules(
        property_rules_files,
        &mut collections,
        &vjs_by_line,
        &mut report,
    )?;

    let serialized_report = serde_json::to_string_pretty(&report)?;
    fs::write(report_path, serialized_report)?;

    collections.sanitize()?;
    Model::new(collections)
}
