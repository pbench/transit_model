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

use crate::{
    model::Collections,
    objects::{Coord, Geometry, Line, VehicleJourney},
    utils::{Report, ReportType},
    Result,
};
use csv;
use failure::ResultExt;
use geo_types::Geometry as GeoGeometry;
use lazy_static::lazy_static;
use log::{info, warn};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    str::FromStr,
};
use transit_model_collection::{CollectionWithId, Idx};
use transit_model_relations::IdxSet;
use wkt::{self, conversion::try_into_geometry};

#[derive(Deserialize, Debug, Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Hash)]
#[serde(rename_all = "snake_case")]
enum ObjectType {
    Line,
    Route,
    StopPoint,
    StopArea,
    PhysicalMode,
    CommercialMode,
}

impl ObjectType {
    fn as_str(self) -> &'static str {
        match self {
            ObjectType::Line => "line",
            ObjectType::Route => "route",
            ObjectType::StopPoint => "stop_point",
            ObjectType::StopArea => "stop_area",
            ObjectType::PhysicalMode => "physical_mode",
            ObjectType::CommercialMode => "commercial_mode",
        }
    }
}

#[derive(Deserialize, Debug, Ord, Eq, PartialOrd, PartialEq, Clone)]
struct PropertyRule {
    object_type: ObjectType,
    object_id: String,
    property_name: String,
    property_old_value: Option<String>,
    property_value: String,
}

fn read_property_rules_files<P: AsRef<Path>>(
    rule_files: Vec<P>,
    report: &mut Report,
) -> Result<Vec<PropertyRule>> {
    info!("Reading property rules.");
    let mut properties: BTreeMap<(ObjectType, String, String), BTreeSet<PropertyRule>> =
        BTreeMap::default();
    for rule_path in rule_files {
        let path = rule_path.as_ref();
        let mut rdr = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_path(&path)
            .with_context(ctx_from_path!(path))?;
        for p in rdr.deserialize() {
            let p: PropertyRule = match p {
                Ok(val) => val,
                Err(e) => {
                    report.add_warning(
                        format!("Error reading {:?}: {}", path.file_name().unwrap(), e),
                        ReportType::InvalidFile,
                    );
                    continue;
                }
            };

            properties
                .entry((p.object_type, p.object_id.clone(), p.property_name.clone()))
                .or_insert_with(BTreeSet::new)
                .insert(p);
        }
    }

    let properties = properties
        .into_iter()
        .filter(|((object_type, object_id, property_name), property)| {
            if !PROPERTY_UPDATER.contains_key(&(*object_type, property_name)) && (*object_type, property_name) != (ObjectType::Line, &"physical_mode_id".to_string()) {
                report.add_warning(
                    format!(
                        "object_type={}, object_id={}: unknown property_name {} defined",
                        object_type.as_str(), object_id, property_name,
                    ),
                    ReportType::UnknownPropertyName,
                );
                return false;
            }

            if property.len() > 1 {
                report.add_warning(
                    format!(
                        "object_type={}, object_id={}: multiple values specified for the property {}",
                        object_type.as_str(), object_id, property_name
                    ),
                    ReportType::MultipleValue,
                );
                return false;
            }
            true
        })
        .flat_map(|(_, p)| p)
        .collect();

    Ok(properties)
}

fn property_old_value_do_not_match(report: &mut Report, p: &PropertyRule) {
    report.add_warning(
        format!(
            "object_type={}, object_id={}, property_name={}: property_old_value does not match the value found in the data",
            p.object_type.as_str(),
            p.object_id,
            p.property_name
        ),
        ReportType::OldPropertyValueDoesNotMatch,
    )
}

fn update_prop<T: Clone + From<String> + Into<Option<String>>>(
    p: &PropertyRule,
    field: &mut T,
    report: &mut Report,
) {
    let any_prop = Some("*".to_string());
    if p.property_old_value == any_prop || p.property_old_value == field.clone().into() {
        *field = T::from(p.property_value.clone());
    } else {
        property_old_value_do_not_match(report, p);
    }
}

fn update_stringable_option<T: FromStr + ToString + Clone>(
    p: &PropertyRule,
    field: &mut Option<T>,
    report: &mut Report,
    err_msg: &str,
) {
    let any_prop = Some("*".to_string());
    let field_cmp = field.clone().map(|f| f.to_string());

    if p.property_old_value == any_prop || p.property_old_value == field_cmp {
        if let Ok(i) = T::from_str(&p.property_value) {
            *field = Some(i);
        } else {
            report.add_warning(
                format!(
                    "object_type={}, object_id={}, property_name={}: {}",
                    p.object_type.as_str(),
                    p.object_id,
                    p.property_name,
                    err_msg
                ),
                ReportType::NonConvertibleString,
            );
        }
    } else {
        property_old_value_do_not_match(report, p);
    }
}

fn update_object_id<T>(
    p: &PropertyRule,
    field: &mut String,
    report: &mut Report,
    collection: &CollectionWithId<T>,
) {
    let any_prop = Some("*".to_string());
    if p.property_old_value == any_prop || p.property_old_value.as_ref() == Some(&field) {
        if collection.get(&p.property_value).is_some() {
            *field = p.property_value.clone();
        } else {
            report.add_warning(
                format!(
                    "object_type={}, object_id={}, property_name={}, property_value={}: object not found",
                    p.object_type.as_str(),
                    p.object_id,
                    p.property_name,
                    p.property_value,
                ),
                ReportType::ObjectNotFound,
            );
        }
    } else {
        property_old_value_do_not_match(report, p);
    }
}

fn update_physical_mode(
    p: &PropertyRule,
    line: &Line,
    report: &mut Report,
    collections: &mut Collections,
    vjs_by_line: &HashMap<String, IdxSet<VehicleJourney>>,
) {
    let any_prop = "*";
    if let Some(pov) = p.property_old_value.as_ref() {
        if !collections.physical_modes.contains_id(pov) && *pov != any_prop {
            report.add_warning(
                format!(
                    "object_type={}, object_id={}, property_name={}, property_old_value={}: physical mode not found",
                    p.object_type.as_str(),
                    p.object_id,
                    p.property_name,
                    pov
                ),
                ReportType::ObjectNotFound,
            );
            return;
        }

        if let Some(vjs) = vjs_by_line.get(&line.id) {
            let vjs_by_mode: Vec<Idx<VehicleJourney>> = vjs
                .iter()
                .filter_map(|idx| {
                    if *pov == any_prop
                        || collections.vehicle_journeys[*idx].physical_mode_id == *pov
                    {
                        Some(*idx)
                    } else {
                        None
                    }
                })
                .collect();

            if vjs_by_mode.is_empty() {
                report.add_warning(
                    format!(
                        "object_type={}, object_id={}: no vehicle journeys with physical mode {}",
                        p.object_type.as_str(),
                        p.object_id,
                        pov
                    ),
                    ReportType::ObjectNotFound,
                );
                return;
            }
            if !collections.physical_modes.contains_id(&p.property_value) {
                report.add_warning(
                    format!(
                        "object_type={}, object_id={}, property_name={}, property_value={}: physical mode not found",
                        p.object_type.as_str(),
                        p.object_id,
                        p.property_name,
                        p.property_value
                    ),
                    ReportType::ObjectNotFound,
                );
                return;
            }

            for vj_idx in vjs_by_mode {
                collections
                    .vehicle_journeys
                    .index_mut(vj_idx)
                    .physical_mode_id = p.property_value.clone();
            }
        }
    }
}

fn wkt_to_geo(wkt: &str, report: &mut Report, p: &PropertyRule) -> Option<GeoGeometry<f64>> {
    if let Ok(wkt) = wkt::Wkt::from_str(wkt) {
        if let Ok(geo) = try_into_geometry(&wkt.items[0]) {
            Some(geo)
        } else {
            warn!("impossible to convert empty point");
            None
        }
    } else {
        report.add_warning(
            format!(
                "object_type={}, object_id={}: invalid geometry",
                p.object_type.as_str(),
                p.object_id,
            ),
            ReportType::GeometryNotValid,
        );
        None
    }
}

fn get_geometry_id(
    wkt: &str,
    collection: &mut CollectionWithId<Geometry>,
    p: &PropertyRule,
    report: &mut Report,
) -> Option<String> {
    if let Some(geo) = wkt_to_geo(wkt, report, p) {
        let id = p.object_type.as_str().to_owned() + ":" + &p.object_id;
        let mut obj = collection.get_or_create_with(&id, || Geometry {
            id: id.to_string(),
            geometry: geo.clone(),
        });
        if obj.geometry != geo {
            obj.geometry = geo.clone();
        }
        return Some(id);
    }

    None
}

fn update_geometry(
    p: &mut PropertyRule,
    field: &mut Option<String>,
    geometries: &mut CollectionWithId<Geometry>,
    report: &mut Report,
) {
    match (p.property_old_value.as_ref(), field.as_ref()) {
        (Some(pov), Some(geo_id)) if *pov != "*" => {
            let pov_geo = match wkt_to_geo(&pov, report, &p) {
                Some(pov_geo) => pov_geo,
                None => return,
            };
            let route_geo = match geometries.get(geo_id) {
                Some(geo) => &geo.geometry,
                None => {
                    // this should not happen
                    report.add_warning(
                        format!(
                            "object_type={}, object_id={}: geometry {} not found",
                            p.object_type.as_str(),
                            p.object_id,
                            geo_id
                        ),
                        ReportType::ObjectNotFound,
                    );
                    return;
                }
            };

            if &pov_geo != route_geo {
                update_prop(&p, field, report);
                return;
            }
            p.property_old_value = Some(geo_id.to_string())
        }
        (Some(pov), None) if *pov != "*" => {
            update_prop(&p, field, report);
            return;
        }
        (None, Some(_)) => {
            update_prop(&p, field, report);
            return;
        }
        (_, _) => {}
    }

    if let Some(id) = get_geometry_id(&p.property_value, geometries, &p, report) {
        p.property_value = id;
        update_prop(&p, field, report);
    }
}

fn wkt_to_coord(
    wkt: &str,
    report: &mut Report,
    p: &PropertyRule,
    property_label: &str,
) -> Option<Coord> {
    let pov_geo = wkt_to_geo(wkt, report, &p)?;

    match pov_geo {
        GeoGeometry::Point(p) => Some(Coord {
            lon: p.x(),
            lat: p.y(),
        }),
        _ => {
            report.add_warning(
                format!(
                    "object_type={}, object_id={}, property_name={}, {}={}: WKT should be POINT",
                    p.object_type.as_str(),
                    p.object_id,
                    p.property_name,
                    property_label,
                    wkt,
                ),
                ReportType::ObjectNotFound,
            );
            None
        }
    }
}

fn update_position(p: &mut PropertyRule, field: &mut Coord, report: &mut Report) {
    if let Some(pov) = p.property_old_value.as_ref() {
        if *pov != "*" {
            let p_old_value_coord = match wkt_to_coord(&pov, report, &p, "property_old_value") {
                Some(pov_geo) => pov_geo,
                None => return,
            };

            if *field != p_old_value_coord {
                property_old_value_do_not_match(report, p);
                return;
            }
        }

        let p_value_coord = match wkt_to_coord(&p.property_value, report, &p, "property_value") {
            Some(pov_geo) => pov_geo,
            None => return,
        };

        *field = p_value_coord;
    }
}

type FnUpdater =
    Box<dyn Fn(&mut Collections, &mut PropertyRule, &mut Report) -> bool + Send + Sync>;

lazy_static! {
    static ref PROPERTY_UPDATER: HashMap<(ObjectType, &'static str), FnUpdater> = {
        let mut m: HashMap<(ObjectType, &'static str), FnUpdater> = HashMap::new();
        m.insert(
            (ObjectType::Route, "route_name"),
            Box::new(|c, p, r| {
                c.routes.get_mut(&p.object_id).map_or(false, |mut route| {
                    update_prop(p, &mut route.name, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Route, "direction_type"),
            Box::new(|c, p, r| {
                c.routes.get_mut(&p.object_id).map_or(false, |mut route| {
                    update_prop(p, &mut route.direction_type, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Route, "destination_id"),
            Box::new(|c, p, r| {
                c.routes.get_mut(&p.object_id).map_or(false, |mut route| {
                    update_prop(p, &mut route.destination_id, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Route, "route_geometry"),
            Box::new(|c, p, r| {
                let geometries = &mut c.geometries;
                c.routes.get_mut(&p.object_id).map_or(false, |mut route| {
                    update_geometry(p, &mut route.geometry_id, geometries, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_name"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.name, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_code"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.code, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "forward_line_name"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.forward_name, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "backward_line_name"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.backward_name, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "forward_direction"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.forward_direction, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "backward_direction"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.backward_direction, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_geometry"),
            Box::new(|c, p, r| {
                let geometries = &mut c.geometries;
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_geometry(p, &mut obj.geometry_id, geometries, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_sort_order"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_stringable_option(
                        p,
                        &mut obj.sort_order,
                        r,
                        "property_value should be an integer",
                    );
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_color"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_stringable_option(
                        p,
                        &mut obj.color,
                        r,
                        "property_value is an invalid RGB",
                    );
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "line_text_color"),
            Box::new(|c, p, r| {
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_stringable_option(
                        p,
                        &mut obj.text_color,
                        r,
                        "property_value is an invalid RGB",
                    );
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "commercial_mode_id"),
            Box::new(|c, p, r| {
                let cms = &c.commercial_modes;
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_object_id(p, &mut obj.commercial_mode_id, r, cms);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::Line, "network_id"),
            Box::new(|c, p, r| {
                let cms = &c.networks;
                c.lines.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_object_id(p, &mut obj.network_id, r, cms);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::StopPoint, "stop_name"),
            Box::new(|c, p, r| {
                c.stop_points
                    .get_mut(&p.object_id)
                    .map_or(false, |mut obj| {
                        update_prop(p, &mut obj.name, r);
                        true
                    })
            }),
        );
        m.insert(
            (ObjectType::StopArea, "stop_name"),
            Box::new(|c, p, r| {
                c.stop_areas.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_prop(p, &mut obj.name, r);
                    true
                })
            }),
        );
        m.insert(
            (ObjectType::StopPoint, "stop_position"),
            Box::new(|c, p, r| {
                c.stop_points
                    .get_mut(&p.object_id)
                    .map_or(false, |mut obj| {
                        update_position(p, &mut obj.coord, r);
                        true
                    })
            }),
        );
        m.insert(
            (ObjectType::StopArea, "stop_position"),
            Box::new(|c, p, r| {
                c.stop_areas.get_mut(&p.object_id).map_or(false, |mut obj| {
                    update_position(p, &mut obj.coord, r);
                    true
                })
            }),
        );

        m.insert(
            (ObjectType::PhysicalMode, "co2_emission"),
            Box::new(|c, p, r| {
                c.physical_modes
                    .get_mut(&p.object_id)
                    .map_or(false, |mut obj| {
                        update_stringable_option(
                            p,
                            &mut obj.co2_emission,
                            r,
                            "property_value should be a float",
                        );
                        true
                    })
            }),
        );

        m.insert(
            (ObjectType::PhysicalMode, "physical_mode_name"),
            Box::new(|c, p, r| {
                c.physical_modes
                    .get_mut(&p.object_id)
                    .map_or(false, |mut obj| {
                        update_prop(p, &mut obj.name, r);
                        true
                    })
            }),
        );

        m.insert(
            (ObjectType::CommercialMode, "commercial_mode_name"),
            Box::new(|c, p, r| {
                c.commercial_modes
                    .get_mut(&p.object_id)
                    .map_or(false, |mut obj| {
                        update_prop(p, &mut obj.name, r);
                        true
                    })
            }),
        );

        m
    };
}

pub fn apply_rules<P: AsRef<Path>>(
    rule_files: Vec<P>,
    mut collections: &mut Collections,
    vjs_by_line: &HashMap<String, IdxSet<VehicleJourney>>,
    mut report: &mut Report,
) -> Result<()> {
    let properties = read_property_rules_files(rule_files, &mut report)?;

    let lines = collections.lines.clone();
    for mut p in properties {
        let mut obj_found = true;
        if let Some(func) = PROPERTY_UPDATER.get(&(p.object_type, &p.property_name.clone())) {
            obj_found = func(&mut collections, &mut p, &mut report);
        } else if (p.object_type, p.property_name.as_ref())
            == (ObjectType::Line, "physical_mode_id")
        {
            obj_found = lines.get(&p.object_id).map_or(false, |obj| {
                update_physical_mode(&p, &obj, &mut report, &mut collections, &vjs_by_line);
                true
            });
        }

        if !obj_found {
            report.add_warning(
                format!(
                    "{} {} not found in the data",
                    p.object_type.as_str(),
                    p.object_id
                ),
                ReportType::ObjectNotFound,
            );
        }
    }

    Ok(())
}
