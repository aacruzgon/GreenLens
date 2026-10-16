//! Membership proofs per route ([contracts 1], roles and access; architecture 4).
//!
//! [`Access<R, T>`] reads the `{id}` capture as an `Id<T>`, finds its garden, and asks
//! `domain::authz` for a `Membership<R>`; [`ConvAccess<R, T>`] does the same for a conversation
//! and the records that belong to one. The role is a type parameter, so the role a route needs
//! is written in the handler's signature, and the repository call it makes only compiles with a
//! proof of at least that role. A non-member, or an id that names nothing, is `404`; a member
//! below `R` is `403` with `required_role`.

use std::{future::Future, sync::Arc};

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};

use domain::{
    authz::{self, ConversationOwner, CurrentUser, Membership, MinRole},
    id::{ConversationId, Id, Table, table},
    ports::{GardenResource, Store, StoreError},
};

use crate::{error::ApiError, extract, extract::auth::AuthUser, state::AppState};

/// Where a record's garden is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locate {
    /// The id is the garden.
    Garden(domain::id::GardenId),
    /// The store looks up the record's garden, skipping records being deleted.
    Resource(GardenResource),
}

/// A table whose records belong to a garden, so a route can be scoped by its `{id}`.
/// Implemented here for the domain's table markers (a local trait on foreign types is allowed by
/// the orphan rule).
pub trait Scoped: Table + Sized + Send + Sync {
    /// How to find the garden of `id`.
    fn locate(id: Id<Self>) -> Locate;
}

impl Scoped for table::Garden {
    fn locate(id: Id<Self>) -> Locate {
        Locate::Garden(id)
    }
}
impl Scoped for table::Plant {
    fn locate(id: Id<Self>) -> Locate {
        Locate::Resource(GardenResource::Plant(id))
    }
}
impl Scoped for table::Scan {
    fn locate(id: Id<Self>) -> Locate {
        Locate::Resource(GardenResource::Scan(id))
    }
}
impl Scoped for table::Task {
    fn locate(id: Id<Self>) -> Locate {
        Locate::Resource(GardenResource::Task(id))
    }
}
impl Scoped for table::Memory {
    fn locate(id: Id<Self>) -> Locate {
        Locate::Resource(GardenResource::Memory(id))
    }
}

/// The caller, the record in `{id}`, and a proof of at least role `R` in its garden.
///
/// - `R: MinRole` is the minimum role, checked at run time against the stored role and carried
///   in the proof's type.
/// - `T: Scoped` is the table of the `{id}` capture (`Access<Editor, table::Scan>` reads a
///   `ScanId`).
#[derive(Debug, Clone)]
pub struct Access<R: MinRole, T: Scoped> {
    /// The caller.
    pub user: CurrentUser,
    /// The proof.
    pub member: Membership<R>,
    /// The record.
    pub id: Id<T>,
}

impl<S, R, T> FromRequestParts<S> for Access<R, T>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    R: MinRole,
    T: Scoped,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        let id: Id<T> = extract::path_param(parts, state, "id").await?;
        let app = Arc::<AppState>::from_ref(state);
        let store: &dyn Store = &*app.ports.store;
        let member = match T::locate(id) {
            Locate::Garden(garden) => authz::check_membership::<R>(store, &user, garden).await?,
            Locate::Resource(resource) => authz::check_access::<R>(store, &user, resource).await?,
        };
        Ok(Self { user, member, id })
    }
}

/// A table whose records belong to a conversation: the conversation itself, its pending
/// actions and its messages.
///
/// The method returns `impl Future + Send` rather than being an `async fn`: an `async fn` in a
/// trait has no `Send` bound on its future, and the extractor that awaits it must be `Send`
/// (axum's `FromRequestParts` future is `+ Send`). Implementations may still be written as
/// `async fn`, because their futures are `Send`.
pub trait ConvScoped: Table + Sized + Send + Sync {
    /// The conversation `id` belongs to, or `None` when it names nothing.
    fn conversation(
        store: &dyn Store,
        id: Id<Self>,
    ) -> impl Future<Output = Result<Option<ConversationId>, StoreError>> + Send;
}

impl ConvScoped for table::Conversation {
    // No lookup: a ready future instead of an `async fn` with nothing to await.
    fn conversation(
        _: &dyn Store,
        id: ConversationId,
    ) -> impl Future<Output = Result<Option<ConversationId>, StoreError>> + Send {
        std::future::ready(Ok(Some(id)))
    }
}
impl ConvScoped for table::PendingAction {
    async fn conversation(
        store: &dyn Store,
        id: Id<Self>,
    ) -> Result<Option<ConversationId>, StoreError> {
        store.pending_action_conversation(id).await
    }
}
impl ConvScoped for table::Message {
    async fn conversation(
        store: &dyn Store,
        id: Id<Self>,
    ) -> Result<Option<ConversationId>, StoreError> {
        store.message_conversation(id).await
    }
}

/// The caller, the record in `{id}`, and a proof that the caller owns its conversation and is a
/// member of its garden with at least `R` ([contracts 8], access). Anyone else gets `404`.
#[derive(Debug, Clone)]
pub struct ConvAccess<R: MinRole, T: ConvScoped> {
    /// The caller.
    pub user: CurrentUser,
    /// The proof.
    pub owner: ConversationOwner<R>,
    /// The record.
    pub id: Id<T>,
}

impl<S, R, T> FromRequestParts<S> for ConvAccess<R, T>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
    R: MinRole,
    T: ConvScoped,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        let id: Id<T> = extract::path_param(parts, state, "id").await?;
        let app = Arc::<AppState>::from_ref(state);
        let store: &dyn Store = &*app.ports.store;
        let conversation = T::conversation(store, id)
            .await?
            .ok_or_else(ApiError::not_found)?;
        let owner = authz::check_conversation::<R>(store, &user, conversation).await?;
        Ok(Self { user, owner, id })
    }
}
