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

use std::collections::{BTreeMap, BTreeSet};
use collection::{CollectionWithId, Id, Idx};
use Result;
use failure::ResultExt;

pub type IdxSet<T> = BTreeSet<Idx<T>>;

pub trait Relation {
    type From;
    type To;
    fn get_from(&self) -> IdxSet<Self::From>;
    fn get_corresponding_forward(&self, from: &IdxSet<Self::From>) -> IdxSet<Self::To>;
    fn get_corresponding_backward(&self, from: &IdxSet<Self::To>) -> IdxSet<Self::From>;
}

pub struct OneToMany<T, U> {
    one_to_many: BTreeMap<Idx<T>, IdxSet<U>>,
    many_to_one: BTreeMap<Idx<U>, Idx<T>>,
}

impl<T, U> OneToMany<T, U>
where
    T: Id<T>,
    U: Id<U> + Id<T>,
{
    fn new_impl(one: &CollectionWithId<T>, many: &CollectionWithId<U>) -> Result<Self> {
        let mut one_to_many = BTreeMap::default();
        let mut many_to_one = BTreeMap::default();
        for (many_idx, obj) in many.iter() {
            let one_id = <U as Id<T>>::id(obj);
            let one_idx = one.get_idx(one_id)
                .ok_or_else(|| format_err!("id={:?} not found", one_id))?;
            many_to_one.insert(many_idx, one_idx);
            one_to_many
                .entry(one_idx)
                .or_insert_with(IdxSet::default)
                .insert(many_idx);
        }
        Ok(OneToMany {
            one_to_many,
            many_to_one,
        })
    }
    pub fn new(
        one: &CollectionWithId<T>,
        many: &CollectionWithId<U>,
        rel_name: &str,
    ) -> Result<Self> {
        Ok(Self::new_impl(one, many).with_context(|_| format!("Error indexing {}", rel_name))?)
    }
}

impl<T, U> Relation for OneToMany<T, U> {
    type From = T;
    type To = U;
    fn get_from(&self) -> IdxSet<T> {
        self.one_to_many.keys().cloned().collect()
    }
    fn get_corresponding_forward(&self, from: &IdxSet<T>) -> IdxSet<U> {
        get_corresponding(&self.one_to_many, from)
    }
    fn get_corresponding_backward(&self, from: &IdxSet<U>) -> IdxSet<T> {
        from.iter()
            .filter_map(|from_idx| self.many_to_one.get(from_idx))
            .cloned()
            .collect()
    }
}

pub struct ManyToMany<T, U> {
    forward: BTreeMap<Idx<T>, IdxSet<U>>,
    backward: BTreeMap<Idx<U>, IdxSet<T>>,
}

impl<T, U> ManyToMany<T, U> {
    pub fn from_forward(forward: BTreeMap<Idx<T>, IdxSet<U>>) -> Self {
        let mut backward = BTreeMap::default();
        forward
            .iter()
            .flat_map(|(&from_idx, obj)| obj.iter().map(move |&to_idx| (from_idx, to_idx)))
            .for_each(|(from_idx, to_idx)| {
                backward
                    .entry(to_idx)
                    .or_insert_with(IdxSet::default)
                    .insert(from_idx);
            });
        ManyToMany { forward, backward }
    }
    pub fn from_relations_chain<R1, R2>(r1: &R1, r2: &R2) -> Self
    where
        R1: Relation<From = T>,
        R2: Relation<From = R1::To, To = U>,
    {
        let forward = r1.get_from()
            .into_iter()
            .map(|idx| {
                let from = Some(idx).into_iter().collect();
                let tmp = r1.get_corresponding_forward(&from);
                (idx, r2.get_corresponding_forward(&tmp))
            })
            .collect();
        Self::from_forward(forward)
    }
    pub fn from_relations_sink<R1, R2>(r1: &R1, r2: &R2) -> Self
    where
        R1: Relation<From = T>,
        R2: Relation<From = U, To = R1::To>,
    {
        let forward = r1.get_from()
            .into_iter()
            .map(|idx| {
                let from = Some(idx).into_iter().collect();
                let tmp = r1.get_corresponding_forward(&from);
                (idx, r2.get_corresponding_backward(&tmp))
            })
            .collect();
        Self::from_forward(forward)
    }
}

impl<T, U> Relation for ManyToMany<T, U> {
    type From = T;
    type To = U;
    fn get_from(&self) -> IdxSet<T> {
        self.forward.keys().cloned().collect()
    }
    fn get_corresponding_forward(&self, from: &IdxSet<T>) -> IdxSet<U> {
        get_corresponding(&self.forward, from)
    }
    fn get_corresponding_backward(&self, from: &IdxSet<U>) -> IdxSet<T> {
        get_corresponding(&self.backward, from)
    }
}

fn get_corresponding<T, U>(map: &BTreeMap<Idx<T>, IdxSet<U>>, from: &IdxSet<T>) -> IdxSet<U> {
    from.iter()
        .filter_map(|from_idx| map.get(from_idx))
        .flat_map(|indices| indices.iter().cloned())
        .collect()
}
