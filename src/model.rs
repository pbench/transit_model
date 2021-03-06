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

//! Definition of the navitia transit model.

use crate::{objects::*, Error, Result};
use chrono::NaiveDate;
use derivative::Derivative;
use failure::{bail, format_err};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops;
use std::result::Result as StdResult;
use transit_model_collection::{Collection, CollectionWithId, Id, Idx};
use transit_model_procmacro::*;
use transit_model_relations::{IdxSet, ManyToMany, OneToMany, Relation};

const AIR_PHYSICAL_MODE: &str = "Air";
const BIKE_PHYSICAL_MODE: &str = "Bike";
const BIKE_SHARING_SERVICE_PHYSICAL_MODE: &str = "BikeSharingService";
const BUS_PHYSICAL_MODE: &str = "Bus";
const BUS_RAPID_TRANSIT_PHYSICAL_MODE: &str = "BusRapidTransit";
const CAR_PHYSICAL_MODE: &str = "Car";
const COACH_PHYSICAL_MODE: &str = "Coach";
const FERRY_PHYSICAL_MODE: &str = "Ferry";
const FUNICULAR_PHYSICAL_MODE: &str = "Funicular";
const LOCAL_TRAIN_PHYSICAL_MODE: &str = "LocalTrain";
const LONG_DISTANCE_TRAIN_PHYSICAL_MODE: &str = "LongDistanceTrain";
const METRO_PHYSICAL_MODE: &str = "Metro";
const RAPID_TRANSIT_PHYSICAL_MODE: &str = "RapidTransit";
const TAXI_PHYSICAL_MODE: &str = "Taxi";
const TRAIN_PHYSICAL_MODE: &str = "Train";
const TRAMWAY_PHYSICAL_MODE: &str = "Tramway";
lazy_static! {
    static ref CO2_EMISSIONS: std::collections::HashMap<&'static str, f32> = {
        let mut modes_map = std::collections::HashMap::new();
        modes_map.insert(AIR_PHYSICAL_MODE, 144.6f32);
        modes_map.insert(BIKE_PHYSICAL_MODE, 0f32);
        modes_map.insert(BIKE_SHARING_SERVICE_PHYSICAL_MODE, 0f32);
        // Unknown value
        // modes_map.insert("Boat", 0.0f32);
        modes_map.insert(BUS_PHYSICAL_MODE, 132f32);
        modes_map.insert(BUS_RAPID_TRANSIT_PHYSICAL_MODE, 84f32);
        modes_map.insert(CAR_PHYSICAL_MODE, 184f32);
        modes_map.insert(COACH_PHYSICAL_MODE, 171f32);
        modes_map.insert(FERRY_PHYSICAL_MODE, 279f32);
        modes_map.insert(FUNICULAR_PHYSICAL_MODE, 3f32);
        modes_map.insert(LOCAL_TRAIN_PHYSICAL_MODE, 30.7f32);
        modes_map.insert(LONG_DISTANCE_TRAIN_PHYSICAL_MODE, 3.4f32);
        modes_map.insert(METRO_PHYSICAL_MODE, 3f32);
        modes_map.insert(RAPID_TRANSIT_PHYSICAL_MODE, 6.2f32);
        // Unknown value
        // modes_map.insert("RailShuttle", 0.0f32);
        // Unknown value
        // modes_map.insert("Shuttle", 0.0f32);
        // Unknown value
        // modes_map.insert("SuspendedCableCar", 0.0f32);
        modes_map.insert(TAXI_PHYSICAL_MODE, 184f32);
        modes_map.insert(TRAIN_PHYSICAL_MODE, 11.9f32);
        modes_map.insert(TRAMWAY_PHYSICAL_MODE, 4f32);
        modes_map
    };
}

/// The set of collections representing the model.
#[derive(Derivative, Serialize, Deserialize, Debug)]
#[derivative(Default)]
#[allow(missing_docs)]
pub struct Collections {
    pub contributors: CollectionWithId<Contributor>,
    pub datasets: CollectionWithId<Dataset>,
    pub networks: CollectionWithId<Network>,
    pub commercial_modes: CollectionWithId<CommercialMode>,
    pub lines: CollectionWithId<Line>,
    pub routes: CollectionWithId<Route>,
    pub vehicle_journeys: CollectionWithId<VehicleJourney>,
    pub frequencies: Collection<Frequency>,
    pub physical_modes: CollectionWithId<PhysicalMode>,
    pub stop_areas: CollectionWithId<StopArea>,
    pub stop_points: CollectionWithId<StopPoint>,
    pub stop_locations: CollectionWithId<StopLocation>,
    pub feed_infos: BTreeMap<String, String>,
    pub calendars: CollectionWithId<Calendar>,
    pub companies: CollectionWithId<Company>,
    pub comments: CollectionWithId<Comment>,
    pub equipments: CollectionWithId<Equipment>,
    pub transfers: Collection<Transfer>,
    pub trip_properties: CollectionWithId<TripProperty>,
    pub geometries: CollectionWithId<Geometry>,
    pub admin_stations: Collection<AdminStation>,
    #[serde(skip)]
    pub stop_time_headsigns: HashMap<(Idx<VehicleJourney>, u32), String>,
    #[serde(skip)]
    pub stop_time_ids: HashMap<(Idx<VehicleJourney>, u32), String>,
    #[serde(skip)]
    pub stop_time_comments: HashMap<(Idx<VehicleJourney>, u32), Idx<Comment>>,
    pub prices_v1: Collection<PriceV1>,
    pub od_fares_v1: Collection<ODFareV1>,
    pub fares_v1: Collection<FareV1>,
    pub tickets: CollectionWithId<Ticket>,
    pub ticket_uses: CollectionWithId<TicketUse>,
    pub ticket_prices: Collection<TicketPrice>,
    pub ticket_use_perimeters: Collection<TicketUsePerimeter>,
    pub ticket_use_restrictions: Collection<TicketUseRestriction>,
    pub pathways: CollectionWithId<Pathway>,
    pub levels: CollectionWithId<Level>,
    pub grid_calendars: CollectionWithId<GridCalendar>,
    pub grid_exception_dates: Collection<GridExceptionDate>,
    pub grid_periods: Collection<GridPeriod>,
    pub grid_rel_calendar_line: Collection<GridRelCalendarLine>,
}

impl Collections {
    /// Merge the `Collections` parameter into the current `Collections` by consecutively merging
    /// each collections representing the model.  Fails in case of id collision.
    pub fn try_merge(&mut self, c: Collections) -> Result<()> {
        let Collections {
            contributors,
            datasets,
            networks,
            commercial_modes,
            lines,
            routes,
            mut vehicle_journeys,
            frequencies,
            mut physical_modes,
            mut stop_areas,
            mut stop_points,
            calendars,
            companies,
            comments,
            equipments,
            transfers,
            trip_properties,
            geometries,
            admin_stations,
            stop_time_headsigns,
            stop_time_ids,
            stop_time_comments,
            prices_v1,
            od_fares_v1,
            fares_v1,
            tickets,
            ticket_uses,
            ticket_prices,
            ticket_use_perimeters,
            ticket_use_restrictions,
            pathways,
            levels,
            grid_calendars,
            grid_exception_dates,
            grid_periods,
            grid_rel_calendar_line,
            ..
        } = c;
        self.contributors.try_merge(contributors)?;
        self.datasets.try_merge(datasets)?;
        self.networks.try_merge(networks)?;
        self.commercial_modes.merge(commercial_modes);
        self.lines.try_merge(lines)?;
        self.routes.try_merge(routes)?;
        self.frequencies.merge(frequencies);
        for physical_mode in physical_modes.take() {
            if self.physical_modes.contains_id(&physical_mode.id) {
                // We already check the ID is present, unwrap is safe
                let mut existing_physical_mode =
                    self.physical_modes.get_mut(&physical_mode.id).unwrap();
                existing_physical_mode.co2_emission = match (
                    existing_physical_mode.co2_emission,
                    physical_mode.co2_emission,
                ) {
                    (Some(e), Some(n)) => Some(e.max(n)),
                    (Some(e), None) => Some(e),
                    (None, Some(n)) => Some(n),
                    (None, None) => None,
                }
            } else {
                // We already check the ID is not present, unwrap is safe
                self.physical_modes.push(physical_mode).unwrap();
            }
        }

        self.prices_v1.merge(prices_v1);
        self.od_fares_v1.merge(od_fares_v1);
        self.fares_v1.merge(fares_v1);
        self.tickets.try_merge(tickets)?;
        self.ticket_uses.try_merge(ticket_uses)?;
        self.ticket_prices.merge(ticket_prices);
        self.ticket_use_perimeters.merge(ticket_use_perimeters);
        self.ticket_use_restrictions.merge(ticket_use_restrictions);
        self.pathways.merge(pathways);
        self.levels.merge(levels);
        self.grid_calendars.try_merge(grid_calendars)?;
        self.grid_exception_dates.merge(grid_exception_dates);
        self.grid_periods.merge(grid_periods);
        self.grid_rel_calendar_line.merge(grid_rel_calendar_line);

        fn get_new_idx<T>(
            old_idx: Idx<T>,
            old_idx_to_id: &HashMap<Idx<T>, String>,
            merged_collection: &CollectionWithId<T>,
        ) -> Option<Idx<T>> {
            old_idx_to_id
                .get(&old_idx)
                .and_then(|id| merged_collection.get_idx(id))
        }
        fn idx_to_id<T: Id<T>>(collection: &CollectionWithId<T>) -> HashMap<Idx<T>, String> {
            collection
                .iter()
                .map(|(idx, obj)| (idx, obj.id().into()))
                .collect()
        }

        // update comment idx of collection
        fn update_comment_idx<T: CommentLinks + Id<T>>(
            collection: &mut CollectionWithId<T>,
            c_idx_to_id: &HashMap<Idx<Comment>, String>,
            comments: &CollectionWithId<Comment>,
        ) {
            let mut objs = collection.take();
            for obj in &mut objs {
                *obj.comment_links_mut() = obj
                    .comment_links()
                    .iter()
                    .filter_map(|c_idx| get_new_idx(*c_idx, c_idx_to_id, comments))
                    .collect();
            }

            *collection = CollectionWithId::new(objs).unwrap();
        }

        let sp_idx_to_id = idx_to_id(&stop_points);
        let vj_idx_to_id = idx_to_id(&vehicle_journeys);
        let c_idx_to_id = idx_to_id(&comments);

        self.comments.try_merge(comments)?;
        update_comment_idx(&mut stop_points, &c_idx_to_id, &self.comments);
        update_comment_idx(&mut stop_areas, &c_idx_to_id, &self.comments);

        self.stop_points.try_merge(stop_points)?;
        self.stop_areas.try_merge(stop_areas)?;

        // Update stop point idx in new stop times
        let mut vjs = vehicle_journeys.take();
        for vj in &mut vjs {
            for st in &mut vj.stop_times.iter_mut() {
                if let Some(new_idx) =
                    get_new_idx(st.stop_point_idx, &sp_idx_to_id, &self.stop_points)
                {
                    st.stop_point_idx = new_idx;
                }
            }
        }
        vehicle_journeys = CollectionWithId::new(vjs)?;
        self.vehicle_journeys.try_merge(vehicle_journeys)?;

        fn update_vj_idx<'a, T: Clone>(
            map: &'a HashMap<(Idx<VehicleJourney>, u32), T>,
            vjs: &'a CollectionWithId<VehicleJourney>,
            vj_idx_to_id: &'a HashMap<Idx<VehicleJourney>, String>,
        ) -> impl Iterator<Item = ((Idx<VehicleJourney>, u32), T)> + 'a {
            map.iter()
                .filter_map(move |((old_vj_idx, sequence), value)| {
                    get_new_idx(*old_vj_idx, vj_idx_to_id, vjs)
                        .map(|new_vj_idx| ((new_vj_idx, *sequence), value.clone()))
                })
        }

        // Update vehicle journey idx
        self.stop_time_headsigns.extend(update_vj_idx(
            &stop_time_headsigns,
            &self.vehicle_journeys,
            &vj_idx_to_id,
        ));

        self.stop_time_ids.extend(update_vj_idx(
            &stop_time_ids,
            &self.vehicle_journeys,
            &vj_idx_to_id,
        ));

        let mut new_stop_time_comments = HashMap::new();
        for ((old_vj_idx, sequence), value) in &stop_time_comments {
            let new_vj_idx =
                get_new_idx(*old_vj_idx, &vj_idx_to_id, &self.vehicle_journeys).unwrap();
            let new_c_idx = get_new_idx(*value, &c_idx_to_id, &self.comments).unwrap();
            new_stop_time_comments.insert((new_vj_idx, *sequence), new_c_idx);
        }
        self.stop_time_comments.extend(new_stop_time_comments);
        self.calendars.try_merge(calendars)?;
        self.companies.try_merge(companies)?;
        self.equipments.try_merge(equipments)?;
        self.transfers.merge(transfers);
        self.trip_properties.try_merge(trip_properties)?;
        self.geometries.try_merge(geometries)?;
        self.admin_stations.merge(admin_stations);
        Ok(())
    }

    /// Restrict the validity period of the current `Collections` with the start_date and end_date
    pub fn restrict_period(&mut self, start_date: NaiveDate, end_date: NaiveDate) -> Result<()> {
        let mut calendars = self.calendars.take();
        for calendar in calendars.iter_mut() {
            calendar.dates = calendar
                .dates
                .iter()
                .cloned()
                .filter(|date| *date >= start_date && *date <= end_date)
                .collect();
        }
        let mut data_sets = self.datasets.take();
        for data_set in data_sets.iter_mut() {
            data_set.start_date = cmp::max(start_date, data_set.start_date);
            data_set.end_date = cmp::min(end_date, data_set.end_date);
        }
        self.datasets = CollectionWithId::new(data_sets)?;
        self.calendars = CollectionWithId::new(calendars)?;
        Ok(())
    }

    /// Keep the collections consistent for the new model by purging unreferenced data by
    /// calendars
    pub fn sanitize(&mut self) -> Result<()> {
        fn update_comments_used(
            comments_used: &mut HashSet<String>,
            comment_links: &CommentLinksT,
            comments: &CollectionWithId<Comment>,
        ) {
            comments_used.extend(comment_links.iter().map(|cl| comments[*cl].id.clone()));
        }
        fn update_comments_idx<T>(
            container: &mut Vec<T>,
            comment_old_idx_to_new_idx: &HashMap<Idx<Comment>, Idx<Comment>>,
        ) where
            T: CommentLinks,
        {
            for elt in container.iter_mut() {
                let links = elt.comment_links_mut();
                *links = links
                    .iter()
                    .map(|l| comment_old_idx_to_new_idx[l])
                    .collect::<BTreeSet<_>>();
            }
        }
        fn log_object_removed(object_type: &str, id: &str) {
            log::debug!("{} with ID {} has been removed", object_type, id);
        }
        fn log_predicate<'a, T, F>(object_type: &'a str, mut f: F) -> impl 'a + FnMut(&T) -> bool
        where
            T: Id<T>,
            F: 'a + FnMut(&T) -> bool,
        {
            move |item| {
                if f(item) {
                    true
                } else {
                    log_object_removed(object_type, item.id());
                    false
                }
            }
        }

        self.calendars
            .retain(log_predicate("Calendar", |cal: &Calendar| {
                !cal.dates.is_empty()
            }));

        let mut geometries_used: HashSet<String> = HashSet::new();
        let mut companies_used: HashSet<String> = HashSet::new();
        let mut trip_properties_used: HashSet<String> = HashSet::new();
        let mut route_ids_used: HashSet<String> = HashSet::new();
        let mut stop_points_used: HashSet<String> = HashSet::new();
        let mut data_sets_used: HashSet<String> = HashSet::new();
        let mut physical_modes_used: HashSet<String> = HashSet::new();
        let mut comments_used: HashSet<String> = HashSet::new();
        let mut level_id_used: HashSet<String> = HashSet::new();

        // Keep fallback modes even if not referenced by the model
        physical_modes_used.insert(String::from(BIKE_PHYSICAL_MODE));
        physical_modes_used.insert(String::from(BIKE_SHARING_SERVICE_PHYSICAL_MODE));
        physical_modes_used.insert(String::from(CAR_PHYSICAL_MODE));

        let vj_id_to_old_idx = self.vehicle_journeys.get_id_to_idx().clone();
        let comment_id_to_old_idx = self.comments.get_id_to_idx().clone();
        let stop_point_id_to_old_idx = self.stop_points.get_id_to_idx().clone();

        let vjs: HashMap<String, VehicleJourney> = self
            .vehicle_journeys
            .take()
            .into_iter()
            .filter_map(|vj| {
                if self.calendars.get(&vj.service_id).is_some() {
                    if let Some(geo_id) = &vj.geometry_id {
                        geometries_used.insert(geo_id.clone());
                    }
                    if let Some(prop_id) = &vj.trip_property_id {
                        trip_properties_used.insert(prop_id.clone());
                    }
                    companies_used.insert(vj.company_id.clone());
                    route_ids_used.insert(vj.route_id.clone());
                    for stop_time in &vj.stop_times {
                        stop_points_used
                            .insert(self.stop_points[stop_time.stop_point_idx].id.clone());
                    }
                    data_sets_used.insert(vj.dataset_id.clone());
                    physical_modes_used.insert(vj.physical_mode_id.clone());
                    update_comments_used(&mut comments_used, &vj.comment_links, &self.comments);
                    Some((vj.id.clone(), vj))
                } else {
                    log_object_removed("Vehicle Journey", &vj.id);
                    None
                }
            })
            .collect();
        let mut line_ids_used: HashSet<String> = HashSet::new();
        let mut routes = self
            .routes
            .take()
            .into_iter()
            .filter(|r| {
                if route_ids_used.contains(&r.id) {
                    if let Some(geo_id) = &r.geometry_id {
                        geometries_used.insert(geo_id.clone());
                    }
                    line_ids_used.insert(r.line_id.clone());
                    update_comments_used(&mut comments_used, &r.comment_links, &self.comments);
                    true
                } else {
                    log_object_removed("Route", &r.id);
                    false
                }
            })
            .collect::<Vec<_>>();
        let mut stop_area_ids_used: HashSet<String> = HashSet::new();
        let mut equipments_used: HashSet<String> = HashSet::new();

        let stop_locations = self
            .stop_locations
            .take()
            .into_iter()
            .filter(|sl| {
                if sl.stop_type == StopType::StopEntrance || sl.stop_type == StopType::GenericNode {
                    if let Some(stop_area_id) = &sl.parent_id {
                        stop_area_ids_used.insert(stop_area_id.clone());
                    }
                }
                if sl.stop_type == StopType::BoardingArea {
                    if let Some(stop_point_id) = &sl.parent_id {
                        stop_points_used.insert(stop_point_id.clone());
                        if let Some(stop_area_id) = self
                            .stop_points
                            .get(&stop_point_id)
                            .map(|sp| sp.stop_area_id.clone())
                        {
                            stop_area_ids_used.insert(stop_area_id);
                        }
                    }
                }
                if let Some(level_id) = &sl.level_id {
                    level_id_used.insert(level_id.clone());
                }
                update_comments_used(&mut comments_used, &sl.comment_links, &self.comments);
                true
            })
            .collect::<Vec<_>>();

        let pathways = self
            .pathways
            .take()
            .into_iter()
            .filter(|pw| {
                let mut insert_if_used = |stop_type: &StopType, stop_id: &String| {
                    if *stop_type == StopType::BoardingArea || *stop_type == StopType::Point {
                        stop_points_used.insert(stop_id.clone());
                        if let Some(stop_area_id) = self
                            .stop_points
                            .get(&stop_id)
                            .map(|sp| sp.stop_area_id.clone())
                        {
                            stop_area_ids_used.insert(stop_area_id);
                        }
                    }
                };
                insert_if_used(&pw.from_stop_type, &pw.from_stop_id);
                insert_if_used(&pw.to_stop_type, &pw.to_stop_id);
                true
            })
            .collect::<Vec<_>>();
        self.pathways = CollectionWithId::new(pathways)?;

        let mut stop_points = self
            .stop_points
            .take()
            .into_iter()
            .filter(|sp| {
                if stop_points_used.contains(&sp.id) {
                    stop_area_ids_used.insert(sp.stop_area_id.clone());
                    if let Some(geo_id) = &sp.geometry_id {
                        geometries_used.insert(geo_id.clone());
                    }
                    if let Some(equipment_id) = &sp.equipment_id {
                        equipments_used.insert(equipment_id.clone());
                    }
                    if let Some(level_id) = &sp.level_id {
                        level_id_used.insert(level_id.clone());
                    }
                    update_comments_used(&mut comments_used, &sp.comment_links, &self.comments);
                    true
                } else {
                    log_object_removed("Stop Point", &sp.id);
                    false
                }
            })
            .collect::<Vec<_>>();

        let mut networks_used: HashSet<String> = HashSet::new();
        let mut commercial_modes_used: HashSet<String> = HashSet::new();
        let mut lines = self
            .lines
            .take()
            .into_iter()
            .filter(|l| {
                if line_ids_used.contains(&l.id) {
                    if let Some(geo_id) = &l.geometry_id {
                        geometries_used.insert(geo_id.clone());
                    }
                    networks_used.insert(l.network_id.clone());
                    commercial_modes_used.insert(l.commercial_mode_id.clone());
                    update_comments_used(&mut comments_used, &l.comment_links, &self.comments);
                    true
                } else {
                    log_object_removed("Line", &l.id);
                    false
                }
            })
            .collect::<Vec<_>>();
        let mut contributors_used: HashSet<String> = HashSet::new();
        self.datasets = CollectionWithId::new(
            self.datasets
                .take()
                .into_iter()
                .filter(|d| {
                    if data_sets_used.contains(&d.id) {
                        contributors_used.insert(d.contributor_id.clone());
                        true
                    } else {
                        log_object_removed("Dataset", &d.id);
                        false
                    }
                })
                .collect(),
        )?;
        let mut stop_areas = self
            .stop_areas
            .take()
            .into_iter()
            .filter(|sa| {
                if stop_area_ids_used.contains(&sa.id) {
                    if let Some(geo_id) = &sa.geometry_id {
                        geometries_used.insert(geo_id.clone());
                    }
                    if let Some(level_id) = &sa.level_id {
                        level_id_used.insert(level_id.clone());
                    }
                    update_comments_used(&mut comments_used, &sa.comment_links, &self.comments);
                    true
                } else {
                    log_object_removed("Stop Area", &sa.id);
                    false
                }
            })
            .collect::<Vec<_>>();

        let vj_idx_to_old_id: HashMap<&Idx<VehicleJourney>, &String> =
            vj_id_to_old_idx.iter().map(|(id, idx)| (idx, id)).collect();
        comments_used.extend(self.stop_time_comments.iter().filter_map(
            |((old_vj_idx, _), old_comment_idx)| {
                vj_idx_to_old_id.get(&old_vj_idx).and_then(|&old_vj_id| {
                    if vjs.contains_key(old_vj_id) {
                        Some(self.comments[*old_comment_idx].id.clone())
                    } else {
                        None
                    }
                })
            },
        ));

        self.comments
            .retain(log_predicate("Comment", |comment: &Comment| {
                comments_used.contains(&comment.id)
            }));

        let comment_old_idx_to_new_idx: HashMap<Idx<Comment>, Idx<Comment>> = self
            .comments
            .iter()
            .map(|(new_idx, comment)| (comment_id_to_old_idx[&comment.id], new_idx))
            .collect();

        update_comments_idx(&mut lines, &comment_old_idx_to_new_idx);
        self.lines = CollectionWithId::new(lines)?;
        update_comments_idx(&mut stop_points, &comment_old_idx_to_new_idx);
        self.stop_points = CollectionWithId::new(stop_points)?;
        let stop_point_old_idx_to_new_idx: HashMap<Idx<StopPoint>, Idx<StopPoint>> = self
            .stop_points
            .iter()
            .map(|(new_idx, stop_point)| (stop_point_id_to_old_idx[&stop_point.id], new_idx))
            .collect();
        let mut vjs: Vec<VehicleJourney> = vjs.into_iter().map(|(_, vj)| vj).collect();
        for vj in vjs.iter_mut() {
            for st in vj.stop_times.iter_mut() {
                st.stop_point_idx = stop_point_old_idx_to_new_idx[&st.stop_point_idx];
            }
        }
        update_comments_idx(&mut stop_areas, &comment_old_idx_to_new_idx);
        self.stop_areas = CollectionWithId::new(stop_areas)?;
        update_comments_idx(&mut routes, &comment_old_idx_to_new_idx);
        self.routes = CollectionWithId::new(routes)?;
        update_comments_idx(&mut vjs, &comment_old_idx_to_new_idx);
        let vehicle_journeys_used: HashSet<String> = vjs.iter().map(|vj| vj.id.clone()).collect();
        self.vehicle_journeys = CollectionWithId::new(vjs)?;
        self.stop_locations = CollectionWithId::new(stop_locations)?;

        let vj_old_idx_to_new_idx: HashMap<Idx<VehicleJourney>, Idx<VehicleJourney>> = self
            .vehicle_journeys
            .iter()
            .map(|(new_idx, vj)| (vj_id_to_old_idx[&vj.id], new_idx))
            .collect();
        self.stop_time_comments = self
            .stop_time_comments
            .iter()
            .filter_map(|((old_vj_idx, seq), comment_old_idx)| {
                match (
                    vj_old_idx_to_new_idx.get(old_vj_idx),
                    comment_old_idx_to_new_idx.get(&comment_old_idx),
                ) {
                    (Some(new_vj_idx), Some(new_comment_idx)) => {
                        Some(((*new_vj_idx, *seq), *new_comment_idx))
                    }
                    _ => None,
                }
            })
            .collect();
        self.stop_time_ids = self
            .stop_time_ids
            .iter()
            .filter_map(|((old_vj_id, seq), stop_time_id)| {
                vj_old_idx_to_new_idx
                    .get(&old_vj_id)
                    .map(|new_vj_id| ((*new_vj_id, *seq), stop_time_id.clone()))
            })
            .collect();
        self.stop_time_headsigns = self
            .stop_time_headsigns
            .iter()
            .filter_map(|((old_vj_id, seq), headsign)| {
                vj_old_idx_to_new_idx
                    .get(&old_vj_id)
                    .map(|new_vj_id| ((*new_vj_id, *seq), headsign.clone()))
            })
            .collect();
        self.grid_rel_calendar_line
            .retain(|grid_rel_calendar_line| {
                line_ids_used.contains(&grid_rel_calendar_line.line_id)
                    // If `line_external_code` is used,
                    // it is not possible to sanitize without the exact `line` identifier
                    || (grid_rel_calendar_line.line_id.is_empty() && grid_rel_calendar_line.line_external_code.is_some())
            });
        let grid_calendar_id_used: Vec<_> = self
            .grid_rel_calendar_line
            .values()
            .map(|grid_rel_calendar_line| grid_rel_calendar_line.grid_calendar_id.clone())
            .collect();
        self.grid_calendars.retain(log_predicate(
            "GridCalendar",
            |grid_calendar: &GridCalendar| grid_calendar_id_used.contains(&grid_calendar.id),
        ));
        self.grid_exception_dates.retain(|grid_exception_date| {
            grid_calendar_id_used.contains(&grid_exception_date.grid_calendar_id)
        });
        self.grid_periods
            .retain(|grid_period| grid_calendar_id_used.contains(&grid_period.grid_calendar_id));

        self.networks
            .retain(log_predicate("Network", |network: &Network| {
                networks_used.contains(&network.id)
            }));
        self.trip_properties.retain(log_predicate(
            "Trip Property",
            |trip_property: &TripProperty| trip_properties_used.contains(&trip_property.id),
        ));
        self.geometries
            .retain(log_predicate("Geometry", |geometry: &Geometry| {
                geometries_used.contains(&geometry.id)
            }));
        self.companies
            .retain(log_predicate("Company", |company: &Company| {
                companies_used.contains(&company.id)
            }));
        self.equipments
            .retain(log_predicate("Equipment", |equipment: &Equipment| {
                equipments_used.contains(&equipment.id)
            }));
        self.contributors
            .retain(log_predicate("Contributor", |contributor: &Contributor| {
                contributors_used.contains(&contributor.id)
            }));
        self.commercial_modes.retain(log_predicate(
            "Commercial Mode",
            |commercial_mode: &CommercialMode| commercial_modes_used.contains(&commercial_mode.id),
        ));
        self.physical_modes.retain(log_predicate(
            "Physical Mode",
            |physical_mode: &PhysicalMode| physical_modes_used.contains(&physical_mode.id),
        ));
        self.transfers.retain(|t| {
            stop_points_used.contains(&t.from_stop_id) && stop_points_used.contains(&t.to_stop_id)
        });
        self.frequencies
            .retain(|frequency| vehicle_journeys_used.contains(&frequency.vehicle_journey_id));
        self.levels
            .retain(|level| level_id_used.contains(&level.id));
        Ok(())
    }

    /// Physical mode should contains CO2 emissions. If the values are not present
    /// in the NTFS, some default values will be used.
    pub fn enhance_with_co2(&mut self) {
        let mut physical_modes = self.physical_modes.take();
        for physical_mode in &mut physical_modes {
            if physical_mode.co2_emission.is_none() {
                physical_mode.co2_emission = CO2_EMISSIONS.get(physical_mode.id.as_str()).copied();
            }
        }
        self.physical_modes = CollectionWithId::new(physical_modes).unwrap();
        // Add fallback modes
        for fallback_mode in &[
            BIKE_PHYSICAL_MODE,
            BIKE_SHARING_SERVICE_PHYSICAL_MODE,
            CAR_PHYSICAL_MODE,
        ] {
            if !self.physical_modes.contains_id(fallback_mode) {
                // Can unwrap because we first check that the ID doesn't exist
                self.physical_modes
                    .push(PhysicalMode {
                        id: fallback_mode.to_string(),
                        name: fallback_mode.to_string(),
                        co2_emission: CO2_EMISSIONS.get(fallback_mode).copied(),
                    })
                    .unwrap();
            }
        }
    }

    /// Trip headsign can be derived from the name of the stop point of the
    /// last stop time of the associated trip.
    pub fn enhance_trip_headsign(&mut self) {
        let mut vehicle_journeys = self.vehicle_journeys.take();
        for vehicle_journey in &mut vehicle_journeys {
            if vehicle_journey.headsign.is_none() {
                vehicle_journey.headsign = vehicle_journey
                    .stop_times
                    .last()
                    .map(|stop_time| self.stop_points[stop_time.stop_point_idx].name.clone());
            }
        }
        self.vehicle_journeys = CollectionWithId::new(vehicle_journeys).unwrap();
    }

    /// Many calendars are identical and can be deduplicate
    pub fn calendar_deduplication(&mut self) {
        let mut calendars_used: Vec<Calendar> = vec![];
        let mut vehicle_journeys = self.vehicle_journeys.take();
        vehicle_journeys.sort_unstable_by(|vj1, vj2| vj1.service_id.cmp(&vj2.service_id));
        for vehicle_journey in &mut vehicle_journeys {
            if let Some(calendar) = self.calendars.get(&vehicle_journey.service_id) {
                if let Some(dup_calendar) =
                    calendars_used.iter().find(|c| c.dates == calendar.dates)
                {
                    vehicle_journey.service_id = dup_calendar.id.clone();
                } else {
                    calendars_used.push(calendar.clone());
                }
            }
        }
        self.calendars
            .retain(|calendar| calendars_used.contains(calendar));
        self.vehicle_journeys = CollectionWithId::new(vehicle_journeys).unwrap();
    }

    /// If the route name is empty, it is derived from the most frequent
    /// `stop_area` origin and `stop_area` destination of all the associated
    /// trips.  The `stop_area` name is used to create the following `String`:
    /// `[most frequent origin] - [most frequent destination]`
    ///
    /// If 2 stops have equal frequency, the biggest `stop_area` (biggest number
    /// of `stop_point`) is chosen.
    ///
    /// If still equality between multiple `stop_area`, then alphabetical order
    /// of `stop_area`'s name is used.
    ///
    /// `route.destination_id` is also replaced with the destination stop area
    /// found with the above rules.
    pub fn enhance_route_names(&mut self) {
        fn find_best_origin_destination<'a>(
            route_id: &'a str,
            collections: &'a Collections,
        ) -> Result<(&'a StopArea, &'a StopArea)> {
            fn select_stop_areas<F>(
                collections: &Collections,
                route_id: &str,
                select_stop_point_in_vj: F,
            ) -> Vec<Idx<StopArea>>
            where
                F: Fn(&VehicleJourney) -> Idx<StopPoint>,
            {
                collections
                    .vehicle_journeys
                    .values()
                    .filter(|vj| vj.route_id == route_id)
                    .filter(|vj| !vj.stop_times.is_empty())
                    .map(select_stop_point_in_vj)
                    .map(|sp_idx| &collections.stop_points[sp_idx])
                    .map(|stop_point| &stop_point.stop_area_id)
                    .filter_map(|sa_id| collections.stop_areas.get_idx(sa_id))
                    .collect()
            }
            fn group_by_frequencies(
                stop_areas: Vec<Idx<StopArea>>,
            ) -> HashMap<Idx<StopArea>, usize> {
                stop_areas
                    .into_iter()
                    .fold(HashMap::new(), |mut frequencies, sa_idx| {
                        *frequencies.entry(sa_idx).or_insert(0) += 1;
                        frequencies
                    })
            }
            fn find_indexes_with_max_frequency(
                frequencies: HashMap<Idx<StopArea>, usize>,
            ) -> Vec<Idx<StopArea>> {
                if frequencies.is_empty() {
                    return Vec::new();
                }
                let mut max_frequency = *frequencies.values().next().unwrap();
                let mut max_indexes = Vec::new();
                for (idx, frequency) in frequencies {
                    if frequency > max_frequency {
                        max_frequency = frequency;
                        max_indexes = vec![idx];
                    } else if frequency == max_frequency {
                        max_indexes.push(idx);
                    }
                }
                max_indexes
            }
            fn find_biggest_stop_area<'a>(
                stop_area_indexes: Vec<Idx<StopArea>>,
                collections: &'a Collections,
            ) -> Vec<&'a StopArea> {
                if stop_area_indexes.is_empty() {
                    return Vec::new();
                }
                if stop_area_indexes.len() == 1 {
                    return vec![&collections.stop_areas[stop_area_indexes[0]]];
                }
                let mut max_sp_number = 0;
                let mut biggest_stop_areas = Vec::new();
                for sa_idx in stop_area_indexes {
                    let stop_area = &collections.stop_areas[sa_idx];
                    let sp_number = collections
                        .stop_points
                        .values()
                        .filter(|stop_point| stop_point.stop_area_id == stop_area.id)
                        .count();
                    if sp_number > max_sp_number {
                        max_sp_number = sp_number;
                        biggest_stop_areas = vec![stop_area];
                    } else if sp_number == max_sp_number {
                        biggest_stop_areas.push(stop_area);
                    }
                }
                biggest_stop_areas
            }
            fn find_first_by_alphabetical_order<'a>(
                mut stop_areas: Vec<&'a StopArea>,
            ) -> Option<&'a StopArea> {
                stop_areas.sort_by_key(|stop_area| &stop_area.name);
                stop_areas.get(0).cloned()
            }
            fn find_stop_area_for<'a, F>(
                collections: &'a Collections,
                route_id: &'a str,
                select_stop_point_in_vj: F,
            ) -> Option<&'a StopArea>
            where
                F: Fn(&VehicleJourney) -> Idx<StopPoint>,
            {
                let stop_areas: Vec<Idx<StopArea>> =
                    select_stop_areas(collections, route_id, select_stop_point_in_vj);
                let by_frequency: HashMap<Idx<StopArea>, usize> = group_by_frequencies(stop_areas);
                let most_frequent_stop_areas = find_indexes_with_max_frequency(by_frequency);
                let biggest_stop_areas =
                    find_biggest_stop_area(most_frequent_stop_areas, collections);
                find_first_by_alphabetical_order(biggest_stop_areas)
            }

            let origin_stop_area =
                find_stop_area_for(collections, route_id, |vj| vj.stop_times[0].stop_point_idx);
            let destination_stop_area = find_stop_area_for(collections, route_id, |vj| {
                vj.stop_times[vj.stop_times.len() - 1].stop_point_idx
            });

            if let (Some(origin_stop_area), Some(destination_stop_area)) =
                (origin_stop_area, destination_stop_area)
            {
                Ok((origin_stop_area, destination_stop_area))
            } else {
                bail!("Failed to generate a `name` for route {}", route_id)
            }
        }

        let mut routes = self.routes.take();
        for mut route in &mut routes {
            if route.name.is_empty() {
                let (origin, destination) =
                    skip_fail!(find_best_origin_destination(&route.id, &self));
                route.destination_id = Some(destination.id.clone());
                route.name = format!("{} - {}", origin.name, destination.name);
            }
        }
        self.routes = CollectionWithId::new(routes).unwrap();
    }
}

/// The navitia transit model.
#[derive(GetCorresponding)]
pub struct Model {
    collections: Collections,

    // original relations
    networks_to_lines: OneToMany<Network, Line>,
    commercial_modes_to_lines: OneToMany<CommercialMode, Line>,
    lines_to_routes: OneToMany<Line, Route>,
    routes_to_vehicle_journeys: OneToMany<Route, VehicleJourney>,
    physical_modes_to_vehicle_journeys: OneToMany<PhysicalMode, VehicleJourney>,
    stop_areas_to_stop_points: OneToMany<StopArea, StopPoint>,
    contributors_to_datasets: OneToMany<Contributor, Dataset>,
    datasets_to_vehicle_journeys: OneToMany<Dataset, VehicleJourney>,
    companies_to_vehicle_journeys: OneToMany<Company, VehicleJourney>,
    vehicle_journeys_to_stop_points: ManyToMany<VehicleJourney, StopPoint>,
    transfers_to_stop_points: ManyToMany<Transfer, StopPoint>,
    calendars_to_vehicle_journeys: OneToMany<Calendar, VehicleJourney>,

    // shortcuts
    #[get_corresponding(weight = "1.9")]
    routes_to_stop_points: ManyToMany<Route, StopPoint>,
    #[get_corresponding(weight = "1.9")]
    physical_modes_to_stop_points: ManyToMany<PhysicalMode, StopPoint>,
    #[get_corresponding(weight = "1.9")]
    physical_modes_to_routes: ManyToMany<PhysicalMode, Route>,
    #[get_corresponding(weight = "1.9")]
    datasets_to_stop_points: ManyToMany<Dataset, StopPoint>,
    #[get_corresponding(weight = "1.9")]
    datasets_to_routes: ManyToMany<Dataset, Route>,
    #[get_corresponding(weight = "1.9")]
    datasets_to_physical_modes: ManyToMany<Dataset, PhysicalMode>,
}

impl Model {
    /// Constructs a model from the given `Collections`.  Fails in
    /// case of incoherence, as invalid external references.
    ///
    /// # Examples
    ///
    /// ```
    /// # use transit_model::model::*;
    /// # fn run() -> transit_model::Result<()> {
    /// let _: Model = Model::new(Collections::default())?;
    /// # Ok(())
    /// # }
    /// # fn main() { run().unwrap() }
    /// ```
    ///
    /// ```
    /// # use transit_model::model::*;
    /// # use transit_model_collection::Collection;
    /// # use transit_model::objects::Transfer;
    /// let mut collections = Collections::default();
    /// // This transfer is invalid as there is no stop points in collections
    /// collections.transfers = Collection::from(Transfer {
    ///     from_stop_id: "invalid".into(),
    ///     to_stop_id: "also_invalid".into(),
    ///     min_transfer_time: None,
    ///     real_min_transfer_time: None,
    ///     equipment_id: None,
    /// });
    /// assert!(Model::new(collections).is_err());
    /// ```
    pub fn new(mut c: Collections) -> Result<Self> {
        fn apply_generic_business_rules(collections: &mut Collections) {
            collections.enhance_with_co2();
            collections.enhance_trip_headsign();
            collections.enhance_route_names();
        }
        apply_generic_business_rules(&mut c);

        let forward_vj_to_sp = c
            .vehicle_journeys
            .iter()
            .map(|(idx, vj)| {
                let sps = vj.stop_times.iter().map(|st| st.stop_point_idx).collect();
                (idx, sps)
            })
            .collect();

        let forward_tr_to_sp = c
            .transfers
            .iter()
            .map(|(idx, tr)| {
                let mut stop_points = IdxSet::default();
                stop_points.insert(c.stop_points.get_idx(&tr.from_stop_id).ok_or_else(|| {
                    format_err!("Invalid id: transfer.from_stop_id={:?}", tr.from_stop_id)
                })?);
                stop_points.insert(c.stop_points.get_idx(&tr.to_stop_id).ok_or_else(|| {
                    format_err!("Invalid id: transfer.to_stop_id={:?}", tr.to_stop_id)
                })?);
                Ok((idx, stop_points))
            })
            .collect::<StdResult<BTreeMap<_, _>, Error>>()?;
        let vehicle_journeys_to_stop_points = ManyToMany::from_forward(forward_vj_to_sp);
        let routes_to_vehicle_journeys =
            OneToMany::new(&c.routes, &c.vehicle_journeys, "routes_to_vehicle_journeys")?;
        let physical_modes_to_vehicle_journeys = OneToMany::new(
            &c.physical_modes,
            &c.vehicle_journeys,
            "physical_modes_to_vehicle_journeys",
        )?;
        let datasets_to_vehicle_journeys = OneToMany::new(
            &c.datasets,
            &c.vehicle_journeys,
            "datasets_to_vehicle_journeys",
        )?;
        Ok(Model {
            routes_to_stop_points: ManyToMany::from_relations_chain(
                &routes_to_vehicle_journeys,
                &vehicle_journeys_to_stop_points,
            ),
            physical_modes_to_stop_points: ManyToMany::from_relations_chain(
                &physical_modes_to_vehicle_journeys,
                &vehicle_journeys_to_stop_points,
            ),
            physical_modes_to_routes: ManyToMany::from_relations_sink(
                &physical_modes_to_vehicle_journeys,
                &routes_to_vehicle_journeys,
            ),
            datasets_to_stop_points: ManyToMany::from_relations_chain(
                &datasets_to_vehicle_journeys,
                &vehicle_journeys_to_stop_points,
            ),
            datasets_to_routes: ManyToMany::from_relations_sink(
                &datasets_to_vehicle_journeys,
                &routes_to_vehicle_journeys,
            ),
            datasets_to_physical_modes: ManyToMany::from_relations_sink(
                &datasets_to_vehicle_journeys,
                &physical_modes_to_vehicle_journeys,
            ),
            transfers_to_stop_points: ManyToMany::from_forward(forward_tr_to_sp),
            datasets_to_vehicle_journeys,
            routes_to_vehicle_journeys,
            vehicle_journeys_to_stop_points,
            physical_modes_to_vehicle_journeys,
            networks_to_lines: OneToMany::new(&c.networks, &c.lines, "networks_to_lines")?,
            commercial_modes_to_lines: OneToMany::new(
                &c.commercial_modes,
                &c.lines,
                "commercial_modes_to_lines",
            )?,
            lines_to_routes: OneToMany::new(&c.lines, &c.routes, "lines_to_routes")?,
            stop_areas_to_stop_points: OneToMany::new(
                &c.stop_areas,
                &c.stop_points,
                "stop_areas_to_stop_points",
            )?,
            contributors_to_datasets: OneToMany::new(
                &c.contributors,
                &c.datasets,
                "contributors_to_datasets",
            )?,
            companies_to_vehicle_journeys: OneToMany::new(
                &c.companies,
                &c.vehicle_journeys,
                "companies_to_vehicle_journeys",
            )?,
            calendars_to_vehicle_journeys: OneToMany::new(
                &c.calendars,
                &c.vehicle_journeys,
                "calendars_to_vehicle_journeys",
            )?,
            collections: c,
        })
    }

    /// Consumes collections,
    ///
    /// # Examples
    ///
    /// ```
    /// # use transit_model::model::*;
    /// # use std::collections::HashMap;
    /// # fn run() -> transit_model::Result<()> {
    /// let model: Model = Model::new(Collections::default())?;
    /// let mut collections = model.into_collections();
    ///  collections
    ///    .feed_infos
    ///    .insert("foo".to_string(), "bar".to_string());
    /// let feeds: Vec<(_, _)> = collections.feed_infos.into_iter().collect();
    /// assert_eq!(
    ///    vec![("foo".to_string(), "bar".to_string())],
    ///    feeds
    /// );
    /// # Ok(())
    /// # }
    /// # fn main() { run().unwrap() }
    /// ```
    pub fn into_collections(self) -> Collections {
        self.collections
    }
}
impl ::serde::Serialize for Model {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        self.collections.serialize(serializer)
    }
}
impl<'de> ::serde::Deserialize<'de> for Model {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        use serde::de::Error;
        ::serde::Deserialize::deserialize(deserializer)
            .and_then(|o| Model::new(o).map_err(D::Error::custom))
    }
}
impl ops::Deref for Model {
    type Target = Collections;
    fn deref(&self) -> &Self::Target {
        &self.collections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod merge {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn physical_mode_co2_emission_max() {
            let physical_mode1 = PhysicalMode {
                id: String::from(BUS_PHYSICAL_MODE),
                name: String::from("Bus"),
                co2_emission: Some(21f32),
            };
            let physical_mode2 = PhysicalMode {
                id: String::from(BUS_PHYSICAL_MODE),
                name: String::from("Bus"),
                co2_emission: Some(42f32),
            };
            let mut collections = Collections::default();
            collections.physical_modes.push(physical_mode1).unwrap();
            let mut collections_to_merge = Collections::default();
            collections_to_merge
                .physical_modes
                .push(physical_mode2)
                .unwrap();
            collections.try_merge(collections_to_merge).unwrap();
            let bus_mode = collections.physical_modes.get(BUS_PHYSICAL_MODE).unwrap();
            assert_eq!(42f32, bus_mode.co2_emission.unwrap());
        }

        #[test]
        fn physical_mode_co2_emission_one_missing() {
            let physical_mode1 = PhysicalMode {
                id: String::from(BUS_PHYSICAL_MODE),
                name: String::from("Bus"),
                co2_emission: None,
            };
            let physical_mode2 = PhysicalMode {
                id: String::from(BUS_PHYSICAL_MODE),
                name: String::from("Bus"),
                co2_emission: Some(42f32),
            };
            let mut collections = Collections::default();
            collections.physical_modes.push(physical_mode1).unwrap();
            let mut collections_to_merge = Collections::default();
            collections_to_merge
                .physical_modes
                .push(physical_mode2)
                .unwrap();
            collections.try_merge(collections_to_merge).unwrap();
            let bus_mode = collections.physical_modes.get(BUS_PHYSICAL_MODE).unwrap();
            assert_eq!(42f32, bus_mode.co2_emission.unwrap());
        }
    }

    mod enhance_with_co2 {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn enhance_with_default() {
            let mut collections = Collections::default();
            collections
                .physical_modes
                .push(PhysicalMode {
                    id: String::from(BUS_PHYSICAL_MODE),
                    name: String::from("Bus"),
                    ..Default::default()
                })
                .unwrap();
            collections.enhance_with_co2();

            let bus_mode = collections.physical_modes.get(BUS_PHYSICAL_MODE).unwrap();
            assert_eq!(132f32, bus_mode.co2_emission.unwrap());
        }

        #[test]
        fn preserve_existing() {
            let mut collections = Collections::default();
            collections
                .physical_modes
                .push(PhysicalMode {
                    id: String::from(BUS_PHYSICAL_MODE),
                    name: String::from("Bus"),
                    co2_emission: Some(42.0f32),
                })
                .unwrap();
            collections.enhance_with_co2();

            let bus_mode = collections.physical_modes.get(BUS_PHYSICAL_MODE).unwrap();
            assert_eq!(42.0f32, bus_mode.co2_emission.unwrap());
        }

        #[test]
        fn add_fallback_modes() {
            let mut collections = Collections::default();
            collections.enhance_with_co2();

            assert_eq!(3, collections.physical_modes.len());
            let bike_mode = collections.physical_modes.get(BIKE_PHYSICAL_MODE).unwrap();
            assert_eq!(0.0f32, bike_mode.co2_emission.unwrap());
            let walk_mode = collections
                .physical_modes
                .get(BIKE_SHARING_SERVICE_PHYSICAL_MODE)
                .unwrap();
            assert_eq!(0.0f32, walk_mode.co2_emission.unwrap());
            let car_mode = collections.physical_modes.get(CAR_PHYSICAL_MODE).unwrap();
            assert_eq!(184.0f32, car_mode.co2_emission.unwrap());
        }
    }

    mod enhance_trip_headsign {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn enhance() {
            let mut collections = Collections::default();
            collections
                .stop_points
                .push(StopPoint {
                    id: String::from("stop_point_id"),
                    name: String::from("Stop Name"),
                    ..Default::default()
                })
                .unwrap();
            let stop_time = StopTime {
                stop_point_idx: collections.stop_points.get_idx("stop_point_id").unwrap(),
                sequence: 0,
                arrival_time: Time::new(0, 0, 0),
                departure_time: Time::new(0, 0, 0),
                boarding_duration: 0,
                alighting_duration: 0,
                pickup_type: 0,
                drop_off_type: 0,
                datetime_estimated: false,
                local_zone_id: Some(0),
                precision: None,
            };
            collections
                .vehicle_journeys
                .push(VehicleJourney {
                    id: String::from("vehicle_journey_id_1"),
                    stop_times: vec![stop_time],
                    ..Default::default()
                })
                .unwrap();
            collections
                .vehicle_journeys
                .push(VehicleJourney {
                    id: String::from("vehicle_journey_id_2"),
                    headsign: Some(String::from("Headsign")),
                    ..Default::default()
                })
                .unwrap();
            collections.enhance_trip_headsign();
            let vehicle_journey = collections
                .vehicle_journeys
                .get("vehicle_journey_id_1")
                .unwrap();
            assert_eq!("Stop Name", vehicle_journey.headsign.as_ref().unwrap());
            let vehicle_journey = collections
                .vehicle_journeys
                .get("vehicle_journey_id_2")
                .unwrap();
            assert_eq!("Headsign", vehicle_journey.headsign.as_ref().unwrap());
        }
    }

    mod calendar_deduplication {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn enhance() {
            let mut collections = Collections::default();

            let mut service_1 = Calendar::new(String::from("service_1"));
            service_1.dates.insert(NaiveDate::from_ymd(2019, 10, 01));
            service_1.dates.insert(NaiveDate::from_ymd(2019, 10, 02));
            service_1.dates.insert(NaiveDate::from_ymd(2019, 10, 03));
            service_1.dates.insert(NaiveDate::from_ymd(2019, 10, 10));
            collections.calendars.push(service_1).unwrap();

            let mut service_2 = Calendar::new(String::from("service_2"));
            service_2.dates.insert(NaiveDate::from_ymd(2019, 10, 01));
            service_2.dates.insert(NaiveDate::from_ymd(2019, 10, 02));
            service_2.dates.insert(NaiveDate::from_ymd(2019, 10, 03));
            service_2.dates.insert(NaiveDate::from_ymd(2019, 10, 10));
            collections.calendars.push(service_2).unwrap();

            let mut service_3 = Calendar::new(String::from("service_3"));
            service_3.dates.insert(NaiveDate::from_ymd(2019, 10, 01));
            service_3.dates.insert(NaiveDate::from_ymd(2019, 10, 03));
            service_3.dates.insert(NaiveDate::from_ymd(2019, 10, 10));
            collections.calendars.push(service_3).unwrap();

            collections
                .vehicle_journeys
                .push(VehicleJourney {
                    id: String::from("vehicle_journey_id_1"),
                    service_id: String::from("service_1"),
                    ..Default::default()
                })
                .unwrap();

            collections
                .vehicle_journeys
                .push(VehicleJourney {
                    id: String::from("vehicle_journey_id_2"),
                    service_id: String::from("service_2"),
                    ..Default::default()
                })
                .unwrap();

            collections
                .vehicle_journeys
                .push(VehicleJourney {
                    id: String::from("vehicle_journey_id_3"),
                    service_id: String::from("service_3"),
                    ..Default::default()
                })
                .unwrap();

            collections.calendar_deduplication();

            let vehicle_journey = collections
                .vehicle_journeys
                .get("vehicle_journey_id_2")
                .unwrap();
            assert_eq!("service_1", vehicle_journey.service_id);

            let vehicle_journey = collections
                .vehicle_journeys
                .get("vehicle_journey_id_3")
                .unwrap();
            assert_eq!("service_3", vehicle_journey.service_id);

            let calendar = collections.calendars.get("service_2");
            assert_eq!(None, calendar);
        }
    }

    mod enhance_route_names {
        use super::*;
        use pretty_assertions::assert_eq;

        fn stop_areas() -> CollectionWithId<StopArea> {
            CollectionWithId::new(
                (1..9)
                    .map(|index| StopArea {
                        id: format!("stop_area:{}", index),
                        name: format!("Stop Area {}", index),
                        ..Default::default()
                    })
                    .collect(),
            )
            .unwrap()
        }

        fn stop_points() -> CollectionWithId<StopPoint> {
            CollectionWithId::new(
                (1..9)
                    .map(|index| StopPoint {
                        id: format!("stop_point:{}", index),
                        stop_area_id: format!("stop_area:{}", index),
                        ..Default::default()
                    })
                    .collect(),
            )
            .unwrap()
        }

        fn collections() -> Collections {
            let mut collections = Collections::default();
            collections.stop_areas = stop_areas();
            collections.stop_points = stop_points();
            collections
                .routes
                .push(Route {
                    id: String::from("route_id"),
                    name: String::new(),
                    ..Default::default()
                })
                .unwrap();
            collections
        }

        fn create_vehicle_journey_with(
            trip_id: &str,
            stop_point_ids: Vec<&str>,
            collections: &Collections,
        ) -> VehicleJourney {
            let stop_time_at = |stop_point_id: &str| StopTime {
                stop_point_idx: collections.stop_points.get_idx(stop_point_id).unwrap(),
                sequence: 0,
                arrival_time: Time::new(0, 0, 0),
                departure_time: Time::new(0, 0, 0),
                boarding_duration: 0,
                alighting_duration: 0,
                pickup_type: 0,
                drop_off_type: 0,
                datetime_estimated: false,
                local_zone_id: None,
                precision: None,
            };
            let stop_times: Vec<_> = stop_point_ids.into_iter().map(stop_time_at).collect();
            VehicleJourney {
                id: String::from(trip_id),
                codes: KeysValues::default(),
                object_properties: KeysValues::default(),
                comment_links: CommentLinksT::default(),
                route_id: String::from("route_id"),
                physical_mode_id: String::new(),
                dataset_id: String::new(),
                service_id: String::new(),
                headsign: None,
                short_name: None,
                block_id: None,
                company_id: String::new(),
                trip_property_id: None,
                geometry_id: None,
                stop_times,
            }
        }

        #[test]
        fn generate_route_name() {
            let mut collections = collections();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:1",
                    vec!["stop_point:1", "stop_point:2"],
                    &collections,
                ))
                .unwrap();
            collections.enhance_route_names();
            let route = collections.routes.get("route_id").unwrap();
            assert_eq!("Stop Area 1 - Stop Area 2", route.name);
        }

        #[test]
        fn most_frequent_origin_destination() {
            let mut collections = collections();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:1",
                    vec!["stop_point:1", "stop_point:2"],
                    &collections,
                ))
                .unwrap();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:2",
                    vec!["stop_point:1", "stop_point:3"],
                    &collections,
                ))
                .unwrap();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:3",
                    vec!["stop_point:2", "stop_point:3"],
                    &collections,
                ))
                .unwrap();
            collections.enhance_route_names();
            let route = collections.routes.get("route_id").unwrap();
            assert_eq!("Stop Area 1 - Stop Area 3", route.name);
        }

        #[test]
        fn same_frequency_then_biggest_stop_area() {
            let mut collections = collections();
            // Make 'stop_area:1' the biggest stop area by number of stop points
            collections
                .stop_points
                .get_mut("stop_point:2")
                .unwrap()
                .stop_area_id = String::from("stop_area:1");
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:1",
                    vec!["stop_point:1", "stop_point:3"],
                    &collections,
                ))
                .unwrap();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:2",
                    vec!["stop_point:3", "stop_point:2"],
                    &collections,
                ))
                .unwrap();
            collections.enhance_route_names();
            let route = collections.routes.get("route_id").unwrap();
            assert_eq!("Stop Area 1 - Stop Area 1", route.name);
        }

        #[test]
        fn same_frequency_same_size_stop_area_then_first_aphabetical_order() {
            let mut collections = collections();
            // Make 'stop_area:1' the biggest stop area by number of stop points
            collections
                .stop_points
                .get_mut("stop_point:2")
                .unwrap()
                .stop_area_id = String::from("stop_area:1");
            // Make 'stop_area:3' as big as 'stop_area:1'
            collections
                .stop_points
                .get_mut("stop_point:4")
                .unwrap()
                .stop_area_id = String::from("stop_area:3");
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:1",
                    vec!["stop_point:1", "stop_point:3"],
                    &collections,
                ))
                .unwrap();
            collections
                .vehicle_journeys
                .push(create_vehicle_journey_with(
                    "trip:2",
                    vec!["stop_point:4", "stop_point:2"],
                    &collections,
                ))
                .unwrap();
            collections.enhance_route_names();
            let route = collections.routes.get("route_id").unwrap();
            // 'Stop Area 1' is before 'Stop Area 3' in alphabetical order
            assert_eq!("Stop Area 1 - Stop Area 1", route.name);
        }
    }
}
