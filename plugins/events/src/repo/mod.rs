//! The events plugin's data layer. Each repository is the only path to its
//! tables and is typed on the permission witness `P`: read methods require
//! `Has<EventsRead>`, write methods additionally require `Has<EventsWrite>`, so a
//! context proven to hold only `events:read` cannot even *name* a mutating method
//! (a compile-time gate). Per-instance access is enforced in SQL via
//! `platform.user_can_access`, with public events bypassing the ACL on read.

mod event;

pub use event::{EventRepo, EventUpdate, EventView, NewEvent};
