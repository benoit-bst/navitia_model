// Copyright 2017-2018 Kisio Digital and/or its affiliates.
//
// This program is free software: you can redistribute it and/or
// modify it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see
// <http://www.gnu.org/licenses/>.

extern crate csv;
#[macro_use]
extern crate derivative;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate get_corresponding_derive;
#[macro_use]
extern crate log;
extern crate serde;
#[macro_use]
extern crate serde_derive;

extern crate chrono;
#[macro_use]
pub(crate) mod utils;
pub mod collection;
pub mod objects;
pub(crate) mod common_format;
pub mod relations;
pub mod ntfs;
pub mod gtfs;

use std::ops;

use std::collections::{BTreeMap, HashMap};
use collection::{Collection, CollectionWithId, Idx};
use objects::*;
use relations::{IdxSet, ManyToMany, OneToMany, Relation};
use std::result::Result as StdResult;

pub type Error = failure::Error;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Derivative, Serialize, Deserialize, Debug)]
#[derivative(Default)]
pub struct Collections {
    pub contributors: CollectionWithId<Contributor>,
    pub datasets: CollectionWithId<Dataset>,
    pub networks: CollectionWithId<Network>,
    pub commercial_modes: CollectionWithId<CommercialMode>,
    pub lines: CollectionWithId<Line>,
    pub routes: CollectionWithId<Route>,
    pub vehicle_journeys: CollectionWithId<VehicleJourney>,
    pub physical_modes: CollectionWithId<PhysicalMode>,
    pub stop_areas: CollectionWithId<StopArea>,
    pub stop_points: CollectionWithId<StopPoint>,
    pub feed_infos: HashMap<String, String>,
    pub calendars: CollectionWithId<Calendar>,
    pub companies: CollectionWithId<Company>,
    pub comments: CollectionWithId<Comment>,
    pub equipments: CollectionWithId<Equipment>,
    pub transfers: Collection<Transfer>,
    pub trip_properties: CollectionWithId<TripProperty>,
    pub geometries: CollectionWithId<Geometry>,
    pub admin_stations: Collection<AdminStation>,
}

#[derive(GetCorresponding)]
pub struct PtObjects {
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

impl PtObjects {
    pub fn new(c: Collections) -> Result<Self> {
        let forward_vj_to_sp = c.vehicle_journeys
            .iter()
            .map(|(idx, vj)| {
                let sps = vj.stop_times.iter().map(|st| st.stop_point_idx).collect();
                (idx, sps)
            })
            .collect();

        let forward_tr_to_sp = c.transfers
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
        Ok(PtObjects {
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
            collections: c,
        })
    }
}
impl ::serde::Serialize for PtObjects {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        self.collections.serialize(serializer)
    }
}
impl<'de> ::serde::Deserialize<'de> for PtObjects {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        use serde::de::Error;
        ::serde::Deserialize::deserialize(deserializer)
            .and_then(|o| PtObjects::new(o).map_err(D::Error::custom))
    }
}
impl ops::Deref for PtObjects {
    type Target = Collections;
    fn deref(&self) -> &Self::Target {
        &self.collections
    }
}
