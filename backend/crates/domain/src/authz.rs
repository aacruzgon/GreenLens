//! Authorization as types ([contracts 1], roles and access; architecture 4).
//!
//! A garden-scoped repository call takes a [`Membership<R>`]: a proof that a user is a member of
//! a garden with at least role `R`. The proof can only be produced by the checks in this module
//! ([`check_membership`], [`check_access`], [`check_conversation`]), which ask the store's
//! [`AccessRepo`] for the stored role. Its constructor is private to `domain`, it has no
//! `Default` and no `Deserialize`, and its fields are private, so a handler in another crate
//! cannot build one by hand. "Editor or above" is therefore checked where the handler is
//! written: a handler that holds a `Membership<Viewer>` and calls a method that takes
//! `Membership<Editor>` does not compile.
//!
//! ```compile_fail,E0308
//! use domain::authz::{Editor, Membership, Viewer};
//! fn create_plant(_: Membership<Editor>) {}
//! fn handler(proof: Membership<Viewer>) {
//!     create_plant(proof); // expected `Membership<Editor>`, found `Membership<Viewer>`
//! }
//! ```
//!
//! A downgrade is free and an upgrade doesn't exist at compile time:
//!
//! ```compile_fail,E0277
//! use domain::authz::{Editor, Membership, Owner};
//! fn handler(proof: Membership<Editor>) -> Membership<Owner> {
//!     proof.at_least::<Owner>() // `Editor: AtLeast<Owner>` is not satisfied
//! }
//! ```
//!
//! Nor can a proof be built outside `domain`:
//!
//! ```compile_fail,E0624
//! use domain::authz::{Membership, Owner};
//! use domain::wire::Role;
//! fn forge(user: domain::id::UserId, garden: domain::id::GardenId) -> Membership<Owner> {
//!     Membership::new(user, garden, Role::Owner) // associated function `new` is private
//! }
//! ```
//!
//! The non-member case is always `404 not_found`; a member whose role is too low gets
//! `403 forbidden` with `required_role`.
//!
//! **Why the check lives here and not in `store`.** `store` depends on `domain`, never the
//! other way, so `domain` cannot name `store` to grant it a constructor. A `pub` constructor
//! hidden behind a "sealed" trait would still be callable by any crate. Keeping the constructor
//! private and the check in `domain` makes every proof come from one function that reads the
//! role through a port. The remaining way to forge one is to write an `AccessRepo` that lies,
//! which is a visible trait implementation, not an accident in a handler; only `store` and test
//! fakes implement it.

use std::marker::PhantomData;

use crate::{
    error::DomainError,
    id::{AuthId, ConversationId, GardenId, UserId},
    ports::{AccessRepo, AuthError, AuthVerifier, GardenResource, IdentityRepo, StoreError},
    wire::{Role, Timestamp},
};

mod sealed {
    pub trait Sealed {}
}

/// A minimum role as a type. Sealed: exactly the four markers below.
pub trait MinRole: sealed::Sealed + Copy + std::fmt::Debug + Eq + Send + Sync + 'static {
    /// The role.
    const ROLE: Role;
}

/// Read and chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Viewer;
/// Viewer plus scan feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Reviewer;
/// Scan, answer, manage tasks and plants, map, write tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Editor;
/// Manage the garden and its members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Owner;

impl sealed::Sealed for Viewer {}
impl sealed::Sealed for Reviewer {}
impl sealed::Sealed for Editor {}
impl sealed::Sealed for Owner {}
impl MinRole for Viewer {
    const ROLE: Role = Role::Viewer;
}
impl MinRole for Reviewer {
    const ROLE: Role = Role::Reviewer;
}
impl MinRole for Editor {
    const ROLE: Role = Role::Editor;
}
impl MinRole for Owner {
    const ROLE: Role = Role::Owner;
}

/// `Self` is at least `R`: the compile-time "or above". Implemented for every pair where
/// `Self::ROLE >= R::ROLE`, and only those, so `Membership<Owner>` downgrades to
/// `Membership<Editor>` and never the other way.
pub trait AtLeast<R: MinRole>: MinRole {}

macro_rules! at_least {
    ($($hi:ty => [$($lo:ty),*]);* $(;)?) => {
        $($(impl AtLeast<$lo> for $hi {})*)*
    };
}
at_least! {
    Viewer => [Viewer];
    Reviewer => [Viewer, Reviewer];
    Editor => [Viewer, Reviewer, Editor];
    Owner => [Viewer, Reviewer, Editor, Owner];
}

/// A signed-in user: produced only by [`authenticate`], from a verified token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    id: UserId,
    auth_id: AuthId,
    token_expires_at: Timestamp,
}

impl CurrentUser {
    /// The user.
    pub fn id(&self) -> UserId {
        self.id
    }

    /// The provider's id.
    pub fn auth_id(&self) -> &AuthId {
        &self.auth_id
    }

    /// When the session token expires.
    pub fn token_expires_at(&self) -> Timestamp {
        self.token_expires_at
    }
}

/// Proof that `user` is a member of `garden` with a role of at least `R`. `Copy`: it is two
/// ULIDs and a role, passed by value to repository calls. The `PhantomData<fn() -> R>` marker
/// holds no `R`, so the proof is `Send + Sync + 'static` whatever `R` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Membership<R: MinRole> {
    user: UserId,
    garden: GardenId,
    role: Role,
    min: PhantomData<fn() -> R>,
}

impl<R: MinRole> Membership<R> {
    /// Private to `domain`: only the checks below build proofs.
    const fn new(user: UserId, garden: GardenId, role: Role) -> Self {
        Self {
            user,
            garden,
            role,
            min: PhantomData,
        }
    }

    /// The member.
    pub const fn user(&self) -> UserId {
        self.user
    }

    /// The garden.
    pub const fn garden(&self) -> GardenId {
        self.garden
    }

    /// The member's actual role, which may be above `R`.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// The compile-time downgrade: an owner's proof works wherever an editor's is asked for.
    pub const fn at_least<R2: MinRole>(self) -> Membership<R2>
    where
        R: AtLeast<R2>,
    {
        Membership::new(self.user, self.garden, self.role)
    }

    /// The run-time upgrade, for a call whose role depends on the request (removing a member is
    /// allowed for an owner or the member themselves): checks the actual role.
    pub fn require<R2: MinRole>(self) -> Result<Membership<R2>, AccessError> {
        if self.role >= R2::ROLE {
            Ok(Membership::new(self.user, self.garden, self.role))
        } else {
            Err(AccessError::Forbidden { required: R2::ROLE })
        }
    }
}

/// Proof that `membership.user()` owns `conversation`, which belongs to
/// `membership.garden()` ([contracts 8], access). Anyone else gets `404`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationOwner<R: MinRole> {
    membership: Membership<R>,
    conversation: ConversationId,
}

impl<R: MinRole> ConversationOwner<R> {
    /// The owner's membership.
    pub const fn membership(&self) -> Membership<R> {
        self.membership
    }

    /// The conversation.
    pub const fn conversation(&self) -> ConversationId {
        self.conversation
    }

    /// The compile-time downgrade.
    pub const fn at_least<R2: MinRole>(self) -> ConversationOwner<R2>
    where
        R: AtLeast<R2>,
    {
        ConversationOwner {
            membership: self.membership.at_least(),
            conversation: self.conversation,
        }
    }

    /// The run-time upgrade: chat write tools and cards need an editor.
    pub fn require<R2: MinRole>(self) -> Result<ConversationOwner<R2>, AccessError> {
        Ok(ConversationOwner {
            membership: self.membership.require()?,
            conversation: self.conversation,
        })
    }
}

/// Why access was refused, or the lookup failed.
#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    /// Not a member, not the conversation's owner, or no such record: always `404`.
    #[error("not found")]
    NotFound,
    /// A member below the required role: `403` with `required_role`.
    #[error("requires the {required} role")]
    Forbidden {
        /// The required role.
        required: Role,
    },
    /// The token was refused: `401`.
    #[error("unauthenticated")]
    Unauthenticated(#[source] AuthError),
    /// The lookup failed.
    #[error("access lookup failed")]
    Store(#[from] StoreError),
}

impl AccessError {
    /// The rule failure, when it is one.
    pub fn domain(&self) -> Option<DomainError> {
        match self {
            Self::NotFound => Some(DomainError::NotFound),
            Self::Forbidden { required } => Some(DomainError::Forbidden {
                required: *required,
            }),
            Self::Unauthenticated(_) | Self::Store(_) => None,
        }
    }
}

/// Verifies a bearer token and maps its subject to a user, creating the user on first sight
/// (architecture 4, identity: an upsert on `auth_id`, racing the webhook safely).
pub async fn authenticate(
    verifier: &dyn AuthVerifier,
    identity: &dyn IdentityRepo,
    bearer: &str,
    now: Timestamp,
) -> Result<CurrentUser, AccessError> {
    let token = verifier
        .verify(bearer, now)
        .await
        .map_err(AccessError::Unauthenticated)?;
    // A tombstoned account (deleted, its session token still valid) is refused by the store
    // with `NotFound`: `401`, never a new user.
    let id = match identity
        .upsert_user(&token.auth_id, token.display_name.clone())
        .await
    {
        Err(StoreError::NotFound) => {
            return Err(AccessError::Unauthenticated(AuthError::AccountDeleted));
        }
        other => other?,
    };
    Ok(CurrentUser {
        id,
        auth_id: token.auth_id,
        token_expires_at: token.expires_at,
    })
}

/// The membership check for a garden id from the request ([database 4], membership: a direct
/// read of `member_of:[user, garden]`).
pub async fn check_membership<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    garden: GardenId,
) -> Result<Membership<R>, AccessError> {
    check_membership_of(access, user.id, garden).await
}

/// The same check for a user whose identity comes from a verified auth webhook rather than a
/// session token: the account-deletion cascade started by `user.deleted` (architecture 5,
/// deletion), which has no [`CurrentUser`]. The role is still read from the store, so the proof
/// still says what the store says.
pub async fn check_membership_of<R: MinRole>(
    access: &dyn AccessRepo,
    user: UserId,
    garden: GardenId,
) -> Result<Membership<R>, AccessError> {
    let role = access
        .role_in(user, garden)
        .await?
        .ok_or(AccessError::NotFound)?;
    Membership::<Viewer>::new(user, garden, role).require()
}

/// The membership check for a record id from the request (a plant, scan, task, memory, invite
/// or pending action): the record's garden is looked up first, skipping records being deleted,
/// so a record the caller may not see and a missing one both give `404`.
pub async fn check_access<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    resource: GardenResource,
) -> Result<Membership<R>, AccessError> {
    let garden = access
        .garden_of(resource)
        .await?
        .ok_or(AccessError::NotFound)?;
    check_membership(access, user, garden).await
}

/// The conversation check ([contracts 8]): the caller owns the conversation **and** is a member
/// of its garden with at least `R`; anyone else gets `404`.
pub async fn check_conversation<R: MinRole>(
    access: &dyn AccessRepo,
    user: &CurrentUser,
    conversation: ConversationId,
) -> Result<ConversationOwner<R>, AccessError> {
    let owner = access
        .conversation_owner(conversation)
        .await?
        .ok_or(AccessError::NotFound)?;
    if owner.user != user.id {
        return Err(AccessError::NotFound);
    }
    let membership = check_membership(access, user, owner.garden).await?;
    Ok(ConversationOwner {
        membership,
        conversation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ConversationOwnerRow, GardenResource};

    struct Fake {
        role: Option<Role>,
        owner: UserId,
    }

    const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5G";
    const USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5W";
    const OTHER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5V";

    #[async_trait::async_trait]
    impl AccessRepo for Fake {
        async fn role_in(&self, _: UserId, _: GardenId) -> Result<Option<Role>, StoreError> {
            Ok(self.role)
        }
        async fn garden_of(&self, _: GardenResource) -> Result<Option<GardenId>, StoreError> {
            Ok(Some(GARDEN.parse().unwrap()))
        }
        async fn conversation_owner(
            &self,
            _: ConversationId,
        ) -> Result<Option<ConversationOwnerRow>, StoreError> {
            Ok(Some(ConversationOwnerRow {
                user: self.owner,
                garden: GARDEN.parse().unwrap(),
            }))
        }
        async fn pending_action_conversation(
            &self,
            _: crate::id::PendingActionId,
        ) -> Result<Option<ConversationId>, StoreError> {
            Ok(None)
        }
    }

    fn user() -> CurrentUser {
        CurrentUser {
            id: USER.parse().unwrap(),
            auth_id: "user_2abc".parse().unwrap(),
            token_expires_at: "2026-10-18T15:00:00Z".parse().unwrap(),
        }
    }

    fn run<T>(f: impl std::future::Future<Output = T>) -> T {
        futures::executor::block_on(f)
    }

    #[test]
    fn non_members_get_not_found_and_low_roles_forbidden() {
        let garden: GardenId = GARDEN.parse().unwrap();
        let none = Fake {
            role: None,
            owner: USER.parse().unwrap(),
        };
        assert!(matches!(
            run(check_membership::<Viewer>(&none, &user(), garden)),
            Err(AccessError::NotFound)
        ));
        let viewer = Fake {
            role: Some(Role::Viewer),
            owner: USER.parse().unwrap(),
        };
        assert!(matches!(
            run(check_membership::<Editor>(&viewer, &user(), garden)),
            Err(AccessError::Forbidden {
                required: Role::Editor
            })
        ));
        let owner = Fake {
            role: Some(Role::Owner),
            owner: USER.parse().unwrap(),
        };
        let proof = run(check_membership::<Owner>(&owner, &user(), garden)).unwrap();
        let editor: Membership<Editor> = proof.at_least();
        assert_eq!(editor.role(), Role::Owner);
    }

    #[test]
    fn conversations_are_private() {
        let someone_else = Fake {
            role: Some(Role::Owner),
            owner: OTHER.parse().unwrap(),
        };
        let conv: ConversationId = "01JAX3K9T6R2B8V0QZ4M7D1E5C".parse().unwrap();
        assert!(matches!(
            run(check_conversation::<Viewer>(&someone_else, &user(), conv)),
            Err(AccessError::NotFound)
        ));
        let mine = Fake {
            role: Some(Role::Reviewer),
            owner: USER.parse().unwrap(),
        };
        let proof = run(check_conversation::<Viewer>(&mine, &user(), conv)).unwrap();
        assert!(proof.require::<Editor>().is_err());
    }

    #[test]
    fn proofs_are_send_sync_copy() {
        fn assert_traits<T: Send + Sync + Copy + 'static>() {}
        assert_traits::<Membership<Owner>>();
        assert_traits::<ConversationOwner<Viewer>>();
    }
}
