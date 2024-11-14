// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use web_sys::Event;

use crate::event::EventCast;
use crate::runtime::{EventId, StateId, Then};
use crate::state::Hook;

pub(super) struct ContextBase<'a, T = ()> {
    eid: EventId,
    event: &'a Event,
    states: T,
}

impl<'event> ContextBase<'event> {
    pub fn new(eid: EventId, event: &'event Event) -> Self {
        ContextBase {
            eid,
            event,
            states: (),
        }
    }
}

pub trait ContextHelper<'a> {
    type Reborrow<'b>: ContextHelper<'b> + 'b
    where
        Self: 'b;

    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>;

    fn reborrow<'b>(&'b mut self) -> Self::Reborrow<'b>;
}

impl ContextHelper<'_> for () {
    type Reborrow<'b> = ();

    fn with_state<S, F>(&self, _: StateId, _: F) -> Option<Then>
    where
        F: Fn(&mut S) -> Option<Then>,
    {
        None
    }

    fn reborrow<'b>(&'b mut self) -> () {
        ()
    }
}

impl<'a, T, U> ContextHelper<'a> for (&'a Hook<T>, U)
where
    T: 'static,
    U: ContextHelper<'a>,
{
    type Reborrow<'b> = (&'b Hook<T>, U::Reborrow<'b>) where Self: 'b;

    fn with_state<S, F>(&self, sid: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>,
    {
        use std::any::TypeId;

        // There might be conflicts on the hashes here, but that's okay
        // as we are going to rely on unique nature of `StateId`.
        //
        // Ideally the first condition will be evaluated at compile time
        // and this whole branch is gone if `T` isn't the same type as `S`.
        if TypeId::of::<T>() == TypeId::of::<S>() && self.0.id == sid {
            let state_ptr = self.0.as_ptr() as *mut S;

            return then(unsafe { &mut *state_ptr });
        }

        self.1.with_state(sid, then)
    }

    fn reborrow<'b>(&'b mut self) -> Self::Reborrow<'b> {
        (self.0, self.1.reborrow())
    }
}

pub trait Context {
    type Attached<'b, S>: Context + 'b
    where
        S: 'static,
        Self: 'b;

    fn eid(&self) -> EventId;

    fn event<E>(&self) -> &E
    where
        E: EventCast;

    fn attach<'b, S>(&'b mut self, hook: &'b Hook<S>) -> Self::Attached<'b, S>
    where
        S: 'static,
        Self: 'b;

    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>;
}

impl<'a, T> Context for ContextBase<'a, T>
where
    T: ContextHelper<'a>,
{
    type Attached<'b, S> = ContextBase<'b, (&'b Hook<S>, T::Reborrow<'b>)>
    where
        S: 'static,
        Self: 'b;

    fn eid(&self) -> EventId {
        self.eid
    }

    fn event<E>(&self) -> &E
    where
        E: EventCast,
    {
        unsafe { &*(&self.event as *const _ as *const E) }
    }

    fn attach<'b, S>(&'b mut self, hook: &'b Hook<S>) -> Self::Attached<'b, S>
    where
        S: 'static,
        Self: 'b,
    {
        ContextBase {
            eid: self.eid,
            event: self.event,
            states: (hook, self.states.reborrow()),
        }
    }

    fn with_state<S, F>(&self, id: StateId, then: F) -> Option<Then>
    where
        S: 'static,
        F: Fn(&mut S) -> Option<Then>,
    {
        self.states.with_state(id, then)
    }
}
