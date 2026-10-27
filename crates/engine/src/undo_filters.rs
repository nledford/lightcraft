//! What a step did to the person's filter, for undo and redo to carry back and forth.
//!
//! Deleting a keyword, renaming it or a person, removing a library folder: each changes the filter that names it, in use
//! or put aside by Enable Filters. The filter is session state, not catalog state, so the step's inverse op cannot
//! restore it. `Session::run_command` therefore compares the person's filter before and after every
//! command that committed a step, and the step's undo entry keeps the difference as a [`FilterChange`]. No command
//! records anything itself, so a command that changes the filter next needs nothing here.
//!
//! Undo moves the filter from what the step left to what it found, and the entry it hands to redo does the reverse.
//! Only what the step changed is carried, group by group (the fields one filter chip clears together, see
//! [`filter_chips`]), and a group only moves while it still holds what the step left: a keyword or folder chosen
//! since is not replaced, and the rest of the filter is never touched. What the source shown now owns is left out
//! ([`crate::LibrarySource::reconcile`]), as for every other way into the filter.
//!
//! A filter the person built after the step is left alone, and the one the step put aside is not brought back beside
//! it (setting a filter while filters are off forgets the one put aside, `library.toggleFilters`): nothing is carried
//! once the filter was thrown away since (replaced, emptied, or the one put aside forgotten by setting another: the
//! filter's `replaced` count), nor into a filter the person has again after the step dropped the put-aside one.

use lightcraft_catalog::{Catalog, Filter};
use serde_json::{Map, Value};

use crate::filter_state::{Carried, Snapshot};
use crate::{Session, filter_chips};

/// Moving the person's filter from the state a step left it in (`from`) to the one it found (`to`).
#[derive(Clone, Debug)]
pub(crate) struct FilterChange {
    from: Snapshot,
    to: Snapshot,
    /// [`crate::filter_state::FilterState`]'s `replaced` count after the step.
    replaced: u64,
}

impl FilterChange {
    fn reversed(&self) -> Self {
        Self { from: self.to.clone(), to: self.from.clone(), replaced: self.replaced }
    }
}

/// The fields of a filter by their names in commands (a few at their default are left out).
fn fields(f: &Filter) -> Map<String, Value> {
    match serde_json::to_value(f) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// Fields that change together: those one chip of either filter clears (`label` and `labels`, `only` and `onlyNone`…),
/// and every other field of either on its own.
fn groups(from: &Map<String, Value>, to: &Map<String, Value>, chips: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut groups = chips;
    for key in from.keys().chain(to.keys()) {
        if !groups.iter().flatten().any(|k| k == key) {
            groups.push(vec![key.clone()]);
        }
    }
    groups
}

/// Move the groups of `filter` that the step changed (`from` → `to`) and that still hold what the step left.
pub(crate) fn carry(filter: &mut Filter, from: &Filter, to: &Filter, cat: &Catalog) {
    let (mut now, left, found) = (fields(filter), fields(from), fields(to));
    let chips = filter_chips(from, cat).into_iter().chain(filter_chips(to, cat));
    let chips = chips.filter_map(|c| c.clear.as_object().map(|o| o.keys().cloned().collect())).collect();
    for group in groups(&left, &found, chips) {
        let changed = group.iter().any(|k| left.get(k) != found.get(k));
        if changed && group.iter().all(|k| now.get(k) == left.get(k)) {
            for k in &group {
                match found.get(k) {
                    Some(v) => now.insert(k.clone(), v.clone()),
                    None => now.remove(k),
                };
            }
        }
    }
    if let Ok(moved) = serde_json::from_value(Value::Object(now)) {
        *filter = moved;
    }
}

impl Session {
    /// A command that changed the person's filter ran: if it also committed a step, keep what it did to the filter
    /// with the step, so undo can bring it back. `before` is the filter before its first change, `commits` is
    /// [`Session::commits`] before it ran: what shows a step was committed.
    pub(crate) fn note_filter_change(&mut self, before: Snapshot, commits: u64) {
        if self.commits == commits {
            return;
        }
        let after = self.filters.snapshot();
        if before == after {
            return;
        }
        let replaced = self.filters.replaced();
        if let Some(step) = self.undo.last_mut() {
            step.filters = Some(FilterChange { from: after, to: before, replaced });
        }
    }

    /// Apply a step's filter change (undo or redo); returns the change that takes it back, for the other history stack.
    pub(crate) fn carry_filters(&mut self, change: Option<&FilterChange>) -> Option<FilterChange> {
        let change = change?;
        if self.filters.replaced() == change.replaced {
            let (from, to) = (&change.from, &change.to);
            let cat = &self.catalog;
            self.filters.carry(self.source, |filter, aside| {
                // the step dropped the put-aside filter it emptied, and the person has a filter again: built since,
                // so left alone (the put-aside one is not brought back beside it)
                if !from.aside && to.aside && *filter != Filter::default() {
                    return Carried::InPlace;
                }
                carry(filter, &from.filter, &to.filter, cat);
                // the step had dropped the put-aside filter after emptying it: drop it again once nothing is left in it
                if aside && from.aside && !to.aside && *filter == to.filter {
                    Carried::Drop
                // ... or it had dropped it and the person has no filter now: put it aside again
                } else if !aside && !from.aside && to.aside {
                    Carried::PutAside { even_if_empty: to.filter == Filter::default() }
                } else {
                    Carried::InPlace
                }
            });
        }
        Some(change.reversed())
    }
}
